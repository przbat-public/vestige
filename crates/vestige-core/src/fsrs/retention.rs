//! Canonical semantics of `retention_strength` — the one meaning every writer
//! of that column and every reader that classifies a memory must agree on.
//!
//! # Definition
//!
//! `retention_strength` is the probability, in `[0, 1]`, that the memory is
//! retrievable *right now*. It is a derived quantity, never an accumulator:
//!
//! ```text
//! retention_strength = RETRIEVAL_WEIGHT * R(now) + STORAGE_WEIGHT * min(S / STORAGE_SATURATION, 1)
//! ```
//!
//! where `R(now)` is FSRS-6 retrievability at the current instant and `S` is the
//! memory's storage strength (how well encoded it is). A memory that has just
//! been successfully retrieved has `R ≈ 1`; a memory nobody retrieves again
//! decays along the forgetting curve and reaches `Dormant`, then `Silent`.
//!
//! # Composition order
//!
//! 1. Recompute the time-based retrievability (`boosted_retrievability`).
//! 2. Recompose the composite with [`composite_retention`]. Decay *overwrites*
//!    whatever an earlier writer stored — it never accumulates on top of it.
//! 3. Apply at most one event modulation: [`reinforce_retention`] for an access
//!    or a promotion, [`downscale_retention`] for NREM3 downscaling.
//! 4. Clamp through [`clamp_retention`], the only place a value may enter or
//!    leave the unit interval.
//!
//! # Why one module
//!
//! The 2026-09-19 audit found the column written by five paths with five copies
//! of the formula (`review.rs` search-hit / review / NREM3 / promote, the
//! autonomic promotion in `maintenance.rs`, `temporal.rs` decay) and three
//! copies of the Active/Dormant/Silent/Unavailable thresholds
//! (`storage/sqlite/consolidation.rs`, `search_unified/pipeline/scoring.rs`,
//! `memory_unified/helpers.rs`). A memory therefore looked Active or Dormant —
//! and passed or failed `min_retention` — depending on which writer ran last,
//! because the writers did not share a unit, a range or a composition order.
//! Writers that still live in raw SQL implement an additive modulation whose
//! semantics is exactly [`reinforce_retention`] (`MIN(1.0, x + boost)`); they are
//! listed in the audit report so they can be routed here too.

use chrono::{DateTime, Utc};

use super::{DEFAULT_DECAY, retrievability_with_decay};
use crate::neuroscience::memory_states::MemoryState;

/// Lower bound of the unit interval `retention_strength` lives in.
pub const RETENTION_MIN: f64 = 0.0;

/// Upper bound of the unit interval `retention_strength` lives in.
pub const RETENTION_MAX: f64 = 1.0;

/// Weight of the time-based retrievability term in the composite.
///
/// 0.7 keeps "can I recall it now" dominant while still letting a
/// well-encoded memory (`storage_strength` at saturation) score 0.3 even
/// after the forgetting curve has flattened out.
pub const RETRIEVAL_WEIGHT: f64 = 0.7;

/// Weight of the encoding-strength term in the composite.
pub const STORAGE_WEIGHT: f64 = 0.3;

/// Storage strength at which the encoding term saturates (`min(S / 10, 1)`).
pub const STORAGE_SATURATION: f64 = 10.0;

/// Residue kept by an event-level weakening (demotion or NREM3 downscale).
///
/// Weakening is not deletion: `gc` is the only destructive path, so a memory
/// that loses strength keeps a non-zero floor instead of collapsing to 0.
pub const WEAKENING_FLOOR: f64 = 0.05;

/// Lower bound (exclusive) of the `Active` band.
pub const ACTIVE_MIN_RETENTION: f64 = 0.7;

/// Lower bound (exclusive) of the `Dormant` band.
pub const DORMANT_MIN_RETENTION: f64 = 0.3;

/// Lower bound (exclusive) of the `Silent` band.
pub const SILENT_MIN_RETENTION: f64 = 0.1;

/// Seconds per day, used to turn a timestamp delta into FSRS elapsed days.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Clamp a retention value into the canonical unit interval.
///
/// `NaN` collapses to [`RETENTION_MIN`] rather than propagating: a value that
/// cannot be interpreted as a probability must never pin a memory to `Active`
/// (SQLite would persist the `NaN` as `NULL`, which every downstream
/// comparison then reads as "not retrievable" anyway — silently).
#[inline]
pub fn clamp_retention(value: f64) -> f64 {
    if value.is_nan() {
        RETENTION_MIN
    } else {
        value.clamp(RETENTION_MIN, RETENTION_MAX)
    }
}

/// Compose the canonical retention from an encoding strength and a
/// retrievability, both sanitized here so no caller can leak a non-finite
/// value into the column.
#[inline]
pub fn composite_retention(storage_strength: f64, retrievability: f64) -> f64 {
    // `min(S / 10, 1)`, with a non-finite strength treated as "unknown
    // encoding" (0) instead of poisoning the sum with NaN.
    let storage_term = if storage_strength.is_nan() {
        RETENTION_MIN
    } else {
        (storage_strength / STORAGE_SATURATION).clamp(RETENTION_MIN, RETENTION_MAX)
    };
    clamp_retention(RETRIEVAL_WEIGHT * retrievability + STORAGE_WEIGHT * storage_term)
}

/// FSRS-6 retrievability with the emotional-memory stability boost applied —
/// the exact curve `apply_decay` writes, exposed so the decay pass and the
/// read-time reconciliation cannot drift apart.
///
/// Non-finite inputs are sanitized: a `NaN` stability or `w20` cannot be
/// interpreted as a schedule, so the memory decays to the storage-strength
/// floor instead of writing `NaN` into `retrieval_strength`.
pub fn boosted_retrievability(
    stability: f64,
    sentiment_magnitude: f64,
    elapsed_days: f64,
    w20: f64,
) -> f64 {
    let stability = if stability.is_finite() && stability > 0.0 {
        stability
    } else {
        0.0
    };
    let sentiment = if sentiment_magnitude.is_finite() {
        sentiment_magnitude.max(0.0)
    } else {
        0.0
    };
    let w20 = if w20.is_finite() && w20 > 0.0 {
        w20
    } else {
        DEFAULT_DECAY
    };

    // Emotional memories decay slower (up to 1.5x stability), matching the
    // historical `stability * (1.0 + magnitude * 0.5)` used by both the ingest
    // path and `apply_decay`.
    retrievability_with_decay(stability * (1.0 + sentiment * 0.5), elapsed_days, w20)
}

/// The canonical retention for a memory, `elapsed_days` after its last
/// successful retrieval.
pub fn canonical_retention_at(
    storage_strength: f64,
    stability: f64,
    sentiment_magnitude: f64,
    elapsed_days: f64,
    w20: f64,
) -> f64 {
    composite_retention(
        storage_strength,
        boosted_retrievability(stability, sentiment_magnitude, elapsed_days, w20),
    )
}

/// The canonical retention for a memory at a wall-clock instant.
///
/// This is the value `apply_decay` persists and the value
/// `Storage::reconciled_retention` recomputes on demand; because it depends
/// only on the memory's own FSRS ingredients, it cannot be inflated by the
/// order in which event writers touched the row.
pub fn canonical_retention(
    storage_strength: f64,
    stability: f64,
    sentiment_magnitude: f64,
    last_accessed: DateTime<Utc>,
    now: DateTime<Utc>,
    w20: f64,
) -> f64 {
    let elapsed_days = (now - last_accessed).num_seconds() as f64 / SECONDS_PER_DAY;
    canonical_retention_at(
        storage_strength,
        stability,
        sentiment_magnitude,
        elapsed_days,
        w20,
    )
}

/// Additive modulation: a retrieval, a promotion or a demotion moves the
/// current value by `boost` and the result is clamped to the unit interval.
///
/// This is the semantics of the SQL writers that add a flat delta
/// (`MIN(1.0, retention_strength + b)`); a non-finite boost is ignored rather
/// than allowed to saturate the memory at 1.0.
#[inline]
pub fn reinforce_retention(current: f64, boost: f64) -> f64 {
    let boost = if boost.is_finite() {
        boost
    } else {
        RETENTION_MIN
    };
    clamp_retention(clamp_retention(current) + boost)
}

/// Multiplicative modulation: NREM3 downscaling weakens an unreplayed memory
/// by `factor` without ever erasing it.
///
/// A downscale is monotone non-increasing by construction. The naive
/// `MAX(floor, x * factor)` is not: once `x` sits below the floor, multiplying
/// *raises* it (0.01 * 0.95 -> 0.05), inverting the direction of the pass.
#[inline]
pub fn downscale_retention(current: f64, factor: f64) -> f64 {
    let current = clamp_retention(current);
    let factor = if factor.is_finite() {
        factor.clamp(RETENTION_MIN, RETENTION_MAX)
    } else {
        RETENTION_MAX
    };
    (current * factor).max(WEAKENING_FLOOR).min(current)
}

/// The lifecycle state a reconciled retention value denotes.
///
/// Bands are exclusive on their lower bound (`0.7` is Dormant, not Active),
/// matching the historical classification so the persisted lifecycle, the
/// accessibility multiplier and any `min_retention` policy move together.
#[inline]
pub fn memory_state_for(retention_strength: f64) -> MemoryState {
    let retention = clamp_retention(retention_strength);
    if retention > ACTIVE_MIN_RETENTION {
        MemoryState::Active
    } else if retention > DORMANT_MIN_RETENTION {
        MemoryState::Dormant
    } else if retention > SILENT_MIN_RETENTION {
        MemoryState::Silent
    } else {
        MemoryState::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_stays_in_unit_interval_for_garbage_inputs() {
        for probe in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1.0e308,
            -1.0e308,
            0.5,
        ] {
            for value in [
                composite_retention(probe, 1.0),
                composite_retention(1.0, probe),
                composite_retention(probe, probe),
                canonical_retention_at(probe, probe, probe, probe, probe),
                reinforce_retention(probe, 0.02),
                downscale_retention(probe, probe),
            ] {
                assert!(
                    value.is_finite() && (RETENTION_MIN..=RETENTION_MAX).contains(&value),
                    "composition escaped 0..=1 for probe {probe}: {value}"
                );
            }
        }
    }

    #[test]
    fn downscale_never_raises_a_value_that_sits_below_the_floor() {
        assert_eq!(downscale_retention(0.01, 0.95), 0.01);
        assert!((downscale_retention(0.5, 0.95) - 0.475).abs() < 1e-12);
        assert_eq!(downscale_retention(0.5, f64::NAN), 0.5);
    }

    #[test]
    fn bands_are_exclusive_on_their_lower_bound() {
        assert_eq!(memory_state_for(1.0), MemoryState::Active);
        assert_eq!(memory_state_for(0.71), MemoryState::Active);
        assert_eq!(memory_state_for(0.7), MemoryState::Dormant);
        assert_eq!(memory_state_for(0.3), MemoryState::Silent);
        assert_eq!(memory_state_for(0.1), MemoryState::Unavailable);
        assert_eq!(memory_state_for(f64::NAN), MemoryState::Unavailable);
    }

    #[test]
    fn a_fresh_memory_is_active_and_decays_with_time() {
        // Ingest shape: storage_strength 1.0, FSRS "Good" initial stability.
        let fresh = canonical_retention_at(1.0, 2.3065, 0.0, 0.0, DEFAULT_DECAY);
        assert!(fresh > ACTIVE_MIN_RETENTION, "fresh memory: {fresh}");
        let month = canonical_retention_at(1.0, 2.3065, 0.0, 30.0, DEFAULT_DECAY);
        assert!(month < fresh);
        assert_eq!(memory_state_for(month), MemoryState::Dormant);
    }
}

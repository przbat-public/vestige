//! Regression tests for the canonical `retention_strength` semantics
//! (`crate::fsrs`), pinned against the 2026-09-19 audit finding P1.6.
//!
//! The finding: the column had five writers with incompatible meanings, so a
//! memory looked Active or Dormant — and passed or failed `min_retention` —
//! depending on which writer ran last. The verifier's correction is what these
//! tests encode: `apply_decay` overwrites the additive bumps, so the real defect
//! is the *pin* between consolidation runs (a cache that says "1.0 / Active"
//! until the next pass) and the write-order dependence of the lifecycle state.
//!
//! The tests exercise the public `Storage` surface and the `fsrs` contract, not
//! the implementation: each assertion names the invariant it pins and the writer
//! it would fail for.

use chrono::{Duration, Utc};
use rusqlite::params;
use tempfile::tempdir;

use crate::fsrs::{
    ACTIVE_MIN_RETENTION, DEFAULT_DECAY, RETENTION_MAX, RETENTION_MIN, Rating, canonical_retention,
    clamp_retention, composite_retention, downscale_retention, memory_state_for,
    reinforce_retention,
};
use crate::memory::IngestInput;
use crate::neuroscience::memory_states::MemoryState;

use super::Storage;

fn create_test_storage() -> Storage {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    Storage::new(Some(db_path)).unwrap()
}

fn ingest(storage: &Storage, content: &str) -> String {
    storage
        .ingest(IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap()
        .id
}

/// Simulate elapsed time. FSRS derives everything from `last_accessed`, so
/// moving it into the past is exactly equivalent to waiting — and keeps the
/// suite fast. Only the test writes this column directly.
fn backdate_last_accessed(storage: &Storage, id: &str, days: f64) {
    let when = (Utc::now() - Duration::seconds((days * 86_400.0) as i64)).to_rfc3339();
    storage
        .writer
        .lock()
        .unwrap()
        .execute(
            "UPDATE knowledge_nodes SET last_accessed = ?1 WHERE id = ?2",
            params![when, id],
        )
        .unwrap();
}

fn stored_retention(storage: &Storage, id: &str) -> f64 {
    storage.get_node(id).unwrap().unwrap().retention_strength
}

fn persisted_state(storage: &Storage, id: &str) -> String {
    storage
        .get_memory_state(id)
        .unwrap()
        .expect("a recorded access must have created a lifecycle row")
        .state
}

/// Order of the retention bands, so a test can assert a memory never climbs
/// back up the ladder without being retrieved.
fn band_rank(state: MemoryState) -> u8 {
    match state {
        MemoryState::Active => 3,
        MemoryState::Dormant => 2,
        MemoryState::Silent => 1,
        MemoryState::Unavailable => 0,
    }
}

/// (a) Every writer leaves `retention_strength` inside `0..=1`.
///
/// Pins: the ingest writer (`nodes.rs` INSERT), the search-hit writer
/// (`review::strengthen_batch_on_access`), the review writer
/// (`review::mark_reviewed`), promote/demote (`review::fsrs_user_feedback`),
/// the NREM3 writer (`review::downscale_retention_batch`) and the decay writer
/// (`temporal::apply_decay`).
///
/// The garbage-input half is the part that fails on the pre-reconciliation
/// formula: `0.7 * R + 0.3 * min(S/10, 1)` has no clamp and propagates `NaN`
/// (which SQLite then stores as `NULL`, i.e. "not retrievable" for every
/// comparison). `composite_retention` collapses non-finite inputs instead.
#[test]
fn retention_stays_in_unit_interval_for_every_writer() {
    let storage = create_test_storage();
    let id = ingest(&storage, "unit interval probe");

    let assert_unit = |writer: &str| {
        let value = stored_retention(&storage, &id);
        assert!(
            value.is_finite() && (RETENTION_MIN..=RETENTION_MAX).contains(&value),
            "{writer} left retention_strength = {value}, outside 0..=1"
        );
    };

    assert_unit("ingest (nodes.rs INSERT)");

    storage.strengthen_batch_on_access(&[id.as_str()]).unwrap();
    assert_unit("search hit (review::strengthen_batch_on_access)");

    storage.mark_reviewed(&id, Rating::Good).unwrap();
    assert_unit("review (review::mark_reviewed)");

    storage.promote_memory(&id).unwrap();
    assert_unit("promote (review::fsrs_user_feedback)");

    storage.demote_memory(&id).unwrap();
    assert_unit("demote (review::fsrs_user_feedback)");

    storage
        .downscale_retention_batch(&[id.as_str()], 0.95)
        .unwrap();
    assert_unit("NREM3 downscale (review::downscale_retention_batch)");

    backdate_last_accessed(&storage, &id, 500.0);
    storage.apply_decay().unwrap();
    assert_unit("decay (temporal::apply_decay)");

    // The composition itself is total: no probe can leave the unit interval.
    for probe in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1.0e308,
        -1.0e308,
    ] {
        for value in [
            clamp_retention(probe),
            composite_retention(probe, 1.0),
            composite_retention(1.0, probe),
            composite_retention(probe, probe),
            canonical_retention(
                probe,
                probe,
                probe,
                Utc::now() - Duration::days(30),
                Utc::now(),
                probe,
            ),
            reinforce_retention(probe, 0.02),
            downscale_retention(probe, probe),
        ] {
            assert!(
                value.is_finite() && (RETENTION_MIN..=RETENTION_MAX).contains(&value),
                "composition escaped 0..=1 for probe {probe}: {value}"
            );
        }
    }

    // The one modulation that could silently reverse direction: a multiplicative
    // weakening must never *raise* a value that already sits below the floor.
    assert_eq!(downscale_retention(0.01, 0.95), 0.01);
}

/// (b) A memory with no successful retrieval decays monotonically and reaches
/// Dormant, and its lifecycle state follows the value rather than the writer.
///
/// Pins: the access writer (`storage/sqlite/states.rs::record_memory_access`),
/// which used to stamp `state = 'active'` on every touch, and consolidation's
/// state step, which used to compute transitions and throw them away — so a
/// memory whose retention had already fallen to Dormant kept an `active` row
/// until something accessed it again.
///
/// Fails before the change: run 1 asserts `state == "dormant"` while the stored
/// row still says `"active"`.
#[test]
fn unretrieved_memory_decays_monotonically_to_dormant() {
    let storage = create_test_storage();
    let id = ingest(&storage, "a memory nobody ever retrieves again");

    // The access creates the lifecycle row; the memory is fresh, so Active is
    // the honest band (R ≈ 1) even under the reconciled rule.
    storage.record_memory_access(&id).unwrap();
    assert_eq!(persisted_state(&storage, &id), MemoryState::Active.as_str());

    let mut previous = stored_retention(&storage, &id);
    let mut previous_band = MemoryState::Active;
    let mut reached_dormant = false;

    for run in 1..=4 {
        backdate_last_accessed(&storage, &id, (run * 30) as f64);
        storage.run_consolidation().unwrap();

        let value = stored_retention(&storage, &id);
        assert!(
            value <= previous + 1e-9,
            "run {run}: retention rose from {previous} to {value} for a memory with no successful retrieval"
        );
        previous = value;

        let band = memory_state_for(value);
        assert!(
            band_rank(band) <= band_rank(previous_band),
            "run {run}: band climbed from {previous_band:?} to {band:?} without a retrieval"
        );
        previous_band = band;
        reached_dormant |= band == MemoryState::Dormant;

        assert_eq!(
            persisted_state(&storage, &id),
            band.as_str(),
            "run {run}: lifecycle state must be the band of the reconciled value, not whatever the last writer left"
        );
    }

    assert!(
        reached_dormant,
        "a memory that is never retrieved again must reach Dormant (final band: {previous_band:?})"
    );
}

/// (c) A promoted memory outranks an equivalent unretrieved one of the same age.
///
/// Pins: the read-time reconciliation (`temporal::reconciled_retention`) and the
/// single band function (`fsrs::memory_state_for`) that the lifecycle state and
/// the search pipeline's accessibility multiplier are derived from. Promotion
/// survives the decay overwrite through the FSRS review it runs
/// (`review::fsrs_user_feedback`), and the ranking component must therefore rank
/// it above a peer whose only difference is that nothing ever retrieved it.
///
/// Fails before the change: the unretrieved peer keeps `state = "active"` (write
/// ordering) even though its reconciled value is Silent/Dormant, so the state —
/// and with it the accessibility multiplier a ranker applies — disagrees with
/// the reconciled value.
#[test]
fn promoted_memory_outranks_an_equivalent_unretrieved_one_at_the_same_age() {
    let storage = create_test_storage();

    // Same age, same shape: two memories created at the same instant. The
    // contents must be semantically distinct — consolidation merges duplicate
    // memories, which would delete one of the two peers this test compares.
    let unretrieved = ingest(&storage, "the kettle in the kitchen boils water slowly");
    let promoted = ingest(&storage, "kubernetes pod eviction thresholds in staging");
    storage.record_memory_access(&unretrieved).unwrap();
    storage.record_memory_access(&promoted).unwrap();

    // Both were created 60 days ago and neither has since been retrieved …
    backdate_last_accessed(&storage, &unretrieved, 60.0);
    backdate_last_accessed(&storage, &promoted, 60.0);

    // … except the one the user marked as helpful.
    storage.promote_memory(&promoted).unwrap();

    storage.run_consolidation().unwrap();

    assert!(
        storage.get_node(&unretrieved).unwrap().is_some()
            && storage.get_node(&promoted).unwrap().is_some(),
        "precondition: consolidation must not have merged the two peers"
    );

    let unretrieved_value = storage
        .reconciled_retention(&unretrieved)
        .unwrap()
        .expect("node exists");
    let promoted_value = storage
        .reconciled_retention(&promoted)
        .unwrap()
        .expect("node exists");
    assert!(
        promoted_value > unretrieved_value,
        "the promotion must survive the decay pass: {promoted_value} vs {unretrieved_value}"
    );

    let unretrieved_band = memory_state_for(unretrieved_value);
    let promoted_band = memory_state_for(promoted_value);
    assert!(
        promoted_band.accessibility_multiplier() > unretrieved_band.accessibility_multiplier(),
        "the promoted memory must outrank the unretrieved peer ({promoted_band:?} vs {unretrieved_band:?})"
    );

    assert_eq!(
        persisted_state(&storage, &unretrieved),
        unretrieved_band.as_str(),
        "the unretrieved memory's lifecycle must follow the reconciled value"
    );
    assert_eq!(
        persisted_state(&storage, &promoted),
        promoted_band.as_str(),
        "the promoted memory's lifecycle must follow the reconciled value"
    );
}

/// A fresh memory is Active and a memory past its forgetting curve is not —
/// the two ends of the canonical semantic, so the tests above cannot pass by
/// classifying everything as Dormant.
#[test]
fn reconciled_value_is_active_when_fresh_and_dormant_after_a_month() {
    let storage = create_test_storage();
    let id = ingest(&storage, "canonical band probe");

    let fresh = storage.reconciled_retention(&id).unwrap().unwrap();
    assert!(
        fresh > ACTIVE_MIN_RETENTION,
        "a just-encoded memory is retrievable now: {fresh}"
    );
    assert_eq!(memory_state_for(fresh), MemoryState::Active);

    // The same shape a decay pass writes, one month of no retrieval later.
    let month = canonical_retention(
        1.0,
        2.3065,
        0.0,
        Utc::now() - Duration::days(30),
        Utc::now(),
        DEFAULT_DECAY,
    );
    assert!(month < fresh);
    assert_eq!(memory_state_for(month), MemoryState::Dormant);
}

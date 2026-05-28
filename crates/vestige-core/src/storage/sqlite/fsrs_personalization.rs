//! Persistence of FSRS-6 personalized weights and runtime application.
//!
//! The audit found a gap: `optimize_w20_if_ready` (in `consolidation.rs`) ran
//! a golden-section search on real access history and wrote the result into
//! `fsrs_config(key = 'w20', ...)`, but nothing ever read that value back —
//! the in-memory [`FSRSScheduler`] stayed on `FSRS6_WEIGHTS[20]` forever and
//! the personalized decay was effectively dead code.
//!
//! This module closes the loop:
//!
//! 1. [`Storage::save_personalized_w20`] writes the optimizer's output under a
//!    dedicated key (`w20_personalized`) so it is easy to distinguish "user
//!    override actually computed" from the migration-seeded default in `w20`.
//! 2. [`Storage::load_personalized_w20`] reads it back, returning `None` when
//!    no override has ever been written.
//! 3. [`Storage::apply_personalized_weights`] swaps the scheduler's weights in
//!    place. It is called from [`Storage::new`] so a freshly opened DB picks
//!    up previously-optimized weights, and from the consolidation pipeline so
//!    a newly computed `w20` takes effect immediately without a restart.
//!
//! Why a separate key instead of overwriting `w20`?
//!
//! The schema migration seeds `fsrs_config` with `('w20', 0.1542, …)`, which
//! is the default `FSRS6_WEIGHTS[20]`. If we stored the optimizer output in
//! the same row, the boot-time loader could not tell "the user has 73 reviews
//! and we've never optimized" from "we optimized and got the exact default
//! back." Using a separate `w20_personalized` row keeps semantics clean and
//! is forward-compatible with adding the rest of the 21 weights later
//! (`w0_personalized`, `w1_personalized`, …).

use chrono::Utc;
use rusqlite::params;

use crate::fsrs::{FSRSParameters, FSRSScheduler};

use super::{Result, Storage, StorageError};

/// `fsrs_config` row key for the personalized forgetting-curve decay (w20).
/// Kept separate from the seeded `w20` so absence really means "no override".
pub(crate) const W20_PERSONALIZED_KEY: &str = "w20_personalized";

impl Storage {
    /// Load the previously persisted personalized w20, if any.
    ///
    /// Returns `Ok(None)` when the optimizer has never produced a value for
    /// this DB. Callers should fall back to the FSRS-6 default in that case.
    pub fn load_personalized_w20(&self) -> Result<Option<f64>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        // `query_row` returns `QueryReturnedNoRows` on miss; we map any error
        // — including poisoned locks at the rusqlite layer — to `None` here
        // because a missing personalized weight is *not* a failure mode the
        // caller needs to surface.
        let value: Option<f64> = reader
            .query_row(
                "SELECT value FROM fsrs_config WHERE key = ?1",
                params![W20_PERSONALIZED_KEY],
                |row| row.get(0),
            )
            .ok();
        Ok(value)
    }

    /// Persist a personalized w20 produced by the optimizer.
    ///
    /// The value is clamped to FSRS-6's valid decay range `[0.01, 1.0]`. The
    /// optimizer in `consolidation.rs::optimize_w20_if_ready` already keeps
    /// the search inside that bracket, but we re-clamp here so external
    /// callers (e.g. a hypothetical CLI override) cannot poison the scheduler
    /// with a value that crashes `retrievability_with_decay`.
    pub fn save_personalized_w20(&self, w20: f64) -> Result<()> {
        let clamped = w20.clamp(0.01, 1.0);
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT OR REPLACE INTO fsrs_config (key, value, updated_at)
             VALUES (?1, ?2, ?3)",
            params![W20_PERSONALIZED_KEY, clamped, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Pull the persisted personalized w20 (if any) and apply it to the
    /// in-memory scheduler. Returns `true` if a swap happened, `false` if
    /// there was no override to apply.
    ///
    /// Idempotent: applying twice with the same persisted value is a no-op
    /// from the algorithm's point of view (we still rebuild the scheduler
    /// instance, but no caller can observe a difference).
    pub fn apply_personalized_weights(&self) -> Result<bool> {
        let Some(w20) = self.load_personalized_w20()? else {
            return Ok(false);
        };
        let mut scheduler = self
            .scheduler
            .lock()
            .map_err(|_| StorageError::Init("Scheduler lock poisoned".into()))?;
        let mut params = scheduler.params().clone();
        params.weights[20] = w20;
        *scheduler = FSRSScheduler::new(params);
        Ok(true)
    }

    /// Test/observability helper — snapshot the scheduler's current weight
    /// vector. Exposed so callers (and tests) don't need to hold the
    /// scheduler lock or know about `FSRSParameters` internals.
    pub fn scheduler_weights_snapshot(&self) -> [f64; 21] {
        // Lock poisoning here is never recoverable from a caller's
        // perspective; mirror the rest of the storage layer and panic
        // loudly. The lock guards a tiny in-memory struct.
        let scheduler = self
            .scheduler
            .lock()
            .expect("Scheduler lock poisoned at scheduler_weights_snapshot");
        scheduler.params().weights
    }

    /// Companion accessor for the desired retention rate. Currently used in
    /// tests; left `pub` so dashboard / debugging tools can introspect
    /// without poking at the scheduler mutex directly.
    pub fn scheduler_params_snapshot(&self) -> FSRSParameters {
        let scheduler = self
            .scheduler
            .lock()
            .expect("Scheduler lock poisoned at scheduler_params_snapshot");
        scheduler.params().clone()
    }
}

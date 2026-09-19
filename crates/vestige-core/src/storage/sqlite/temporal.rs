//! Temporal queries and FSRS decay.
//!
//! - [`Storage::query_at_time`]: bitemporal lookup — what was true at a given
//!   instant (`valid_from <= t <= valid_until`).
//! - [`Storage::query_time_range`]: rows created/modified inside a window.
//! - [`Storage::apply_decay`]: the batched FSRS-6 retrievability pass that
//!   runs every consolidation cycle. Uses personalized `w20` and the
//!   emotional-memory stability boost.
//! - [`Storage::reconciled_retention`]: the same value recomputed on demand for
//!   a single memory, for readers that must not depend on which writer touched
//!   the cached column last (lifecycle state, ranking tie-breaks).
//!
//! Decay is the *overwriting* writer of `retention_strength`: it recomposes the
//! canonical value from the memory's own FSRS ingredients (`crate::fsrs`) and
//! never accumulates on top of an event. Every other writer modulates that
//! value additively or multiplicatively, in the same unit and range.
//!
//! `get_fsrs_w20` is kept local — it's only read by [`Storage::apply_decay`]
//! and [`Storage::reconciled_retention`] today (the writer side lives in
//! `consolidation`).

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

use crate::fsrs::{
    DEFAULT_DECAY, boosted_retrievability, canonical_retention, composite_retention,
};
use crate::memory::KnowledgeNode;

use super::{Result, Storage, StorageError};

impl Storage {
    /// Query memories valid at a specific time
    pub fn query_at_time(
        &self,
        point_in_time: DateTime<Utc>,
        limit: i32,
    ) -> Result<Vec<KnowledgeNode>> {
        let timestamp = point_in_time.to_rfc3339();

        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM knowledge_nodes
             WHERE (valid_from IS NULL OR valid_from <= ?1)
             AND (valid_until IS NULL OR valid_until >= ?1)
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;

        let nodes = stmt.query_map(params![timestamp, limit], Self::row_to_node)?;

        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }

    /// Query memories created/modified in a time range
    pub fn query_time_range(
        &self,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        limit: i32,
    ) -> Result<Vec<KnowledgeNode>> {
        let start_str = start.map(|dt| dt.to_rfc3339());
        let end_str = end.map(|dt| dt.to_rfc3339());

        let (query, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match (&start_str, &end_str) {
            (Some(s), Some(e)) => (
                "SELECT * FROM knowledge_nodes
                 WHERE created_at >= ?1 AND created_at <= ?2
                 ORDER BY created_at DESC
                 LIMIT ?3",
                vec![
                    Box::new(s.clone()) as Box<dyn rusqlite::ToSql>,
                    Box::new(e.clone()) as Box<dyn rusqlite::ToSql>,
                    Box::new(limit) as Box<dyn rusqlite::ToSql>,
                ],
            ),
            (Some(s), None) => (
                "SELECT * FROM knowledge_nodes
                 WHERE created_at >= ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
                vec![
                    Box::new(s.clone()) as Box<dyn rusqlite::ToSql>,
                    Box::new(limit) as Box<dyn rusqlite::ToSql>,
                ],
            ),
            (None, Some(e)) => (
                "SELECT * FROM knowledge_nodes
                 WHERE created_at <= ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
                vec![
                    Box::new(e.clone()) as Box<dyn rusqlite::ToSql>,
                    Box::new(limit) as Box<dyn rusqlite::ToSql>,
                ],
            ),
            (None, None) => (
                "SELECT * FROM knowledge_nodes
                 ORDER BY created_at DESC
                 LIMIT ?1",
                vec![Box::new(limit) as Box<dyn rusqlite::ToSql>],
            ),
        };

        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(query)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let nodes = stmt.query_map(params_refs.as_slice(), Self::row_to_node)?;

        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }

    /// Mark a memory as no-longer-valid by setting `valid_until = now()`.
    ///
    /// This is the storage-level primitive behind the `temporal invalidate`
    /// MCP action. Idempotent on the data plane: a second call writes the same
    /// timestamp again — callers handle the "already expired" short-circuit
    /// before reaching this method.
    ///
    /// Returns `Ok(true)` if a row was actually updated, `Ok(false)` if the
    /// memory_id didn't match any node (caller should surface a clear error).
    pub fn set_valid_until(&self, id: &str, when: DateTime<Utc>) -> Result<bool> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let n = writer.execute(
            "UPDATE knowledge_nodes SET valid_until = ?1 WHERE id = ?2",
            params![when.to_rfc3339(), id],
        )?;
        Ok(n > 0)
    }

    /// Apply FSRS-6 decay to all memories using batched pagination to avoid OOM.
    ///
    /// Uses the real FSRS-6 retrievability formula: R = (1 + factor * t / S)^(-w20)
    /// with personalized w20 from fsrs_config table. Sentiment boost extends
    /// effective stability for emotional memories.
    ///
    /// The write is an *overwrite* of the canonical composite
    /// (`fsrs::retention`), not a decrement: whatever an event writer added to
    /// the row since the previous pass is discarded here, which is what bounds
    /// every additive bump to one consolidation window.
    pub fn apply_decay(&self) -> Result<i32> {
        // Read personalized w20 from config (falls back to default 0.1542)
        let w20 = self.fsrs_w20();

        const BATCH_SIZE: i64 = 500;
        let now = Utc::now();
        let mut count = 0i32;
        let mut offset = 0i64;

        loop {
            // Read batch using reader
            let batch: Vec<(String, String, f64, f64, f64, f64)> = {
                let reader = self
                    .reader
                    .lock()
                    .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
                reader
                    .prepare(
                        "SELECT id, last_accessed, storage_strength, retrieval_strength,
                                sentiment_magnitude, stability
                         FROM knowledge_nodes
                         ORDER BY id
                         LIMIT ?1 OFFSET ?2",
                    )?
                    .query_map(params![BATCH_SIZE, offset], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    })?
                    .filter_map(|r| r.ok())
                    .collect()
            };

            if batch.is_empty() {
                break;
            }

            let batch_len = batch.len() as i64;

            // Write batch using writer transaction
            {
                let mut writer = self
                    .writer
                    .lock()
                    .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
                // BEGIN IMMEDIATE: this pass writes rows it just read, which is exactly the
                // shape that loses writes to SQLITE_BUSY_SNAPSHOT when the CLI commits beside
                // the server (busy_timeout does not cover the upgrade).
                let tx = super::helpers::begin_write_transaction(&mut writer)?;

                for (id, last_accessed, storage_strength, _, sentiment_mag, stability) in &batch {
                    let last = DateTime::parse_from_rfc3339(last_accessed)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or(now);

                    let days_since = (now - last).num_seconds() as f64 / 86400.0;

                    if days_since > 0.0 {
                        // Sentiment boost + FSRS-6 retrievability, then the
                        // canonical composite — the same two steps every other
                        // writer and reader uses, so `retrieval_strength` and
                        // `retention_strength` can never be composed from
                        // different curves.
                        let new_retrieval =
                            boosted_retrievability(*stability, *sentiment_mag, days_since, w20);
                        let new_retention = composite_retention(*storage_strength, new_retrieval);

                        tx.execute(
                            "UPDATE knowledge_nodes SET retrieval_strength = ?1, retention_strength = ?2 WHERE id = ?3",
                            params![new_retrieval, new_retention, id],
                        )?;

                        count += 1;
                    }
                }

                tx.commit()?;
            }
            offset += batch_len;
        }

        Ok(count)
    }

    /// The reconciled retention value for one memory, recomputed right now.
    ///
    /// `retention_strength` in the row is a *cache*: [`Storage::apply_decay`]
    /// overwrites it and event writers modulate it (`+0.02` per search hit,
    /// `±delta` on promote/demote). This recomputes the canonical value from the
    /// memory's own FSRS ingredients, so a reader that must not depend on write
    /// ordering — the lifecycle state, a ranking tie-break, a `min_retention`
    /// policy — sees the same probability no matter which writer ran last.
    ///
    /// Returns `Ok(None)` when the id has no node.
    pub fn reconciled_retention(&self, id: &str) -> Result<Option<f64>> {
        let w20 = self.fsrs_w20();
        let row: Option<(f64, f64, f64, String)> = {
            let reader = self
                .reader
                .lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            reader
                .query_row(
                    "SELECT storage_strength, stability, sentiment_magnitude, last_accessed
                     FROM knowledge_nodes WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?
        };

        Ok(row.map(
            |(storage_strength, stability, sentiment_magnitude, last_accessed)| {
                let last = DateTime::parse_from_rfc3339(&last_accessed)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                canonical_retention(
                    storage_strength,
                    stability,
                    sentiment_magnitude,
                    last,
                    Utc::now(),
                    w20,
                )
            },
        ))
    }

    /// Personalized forgetting-curve decay (`w20`), falling back to the FSRS-6
    /// default when the config row is missing or unreadable.
    pub(super) fn fsrs_w20(&self) -> f64 {
        self.get_fsrs_w20().unwrap_or(DEFAULT_DECAY)
    }

    /// Read personalized w20 from fsrs_config table
    fn get_fsrs_w20(&self) -> Result<f64> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        reader
            .query_row(
                "SELECT value FROM fsrs_config WHERE key = 'w20'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| StorageError::Init(format!("Failed to read w20: {}", e)))
    }
}

//! Maintenance and autonomic-loop repository for [`super::Storage`].
//!
//! Background hygiene operations that keep the database within its
//! quality targets. Not on a hot path; runs from `consolidate`, dream
//! cycles, the dashboard "system" admin actions, and graceful shutdown.
//!
//! - **Durability** — [`Storage::wal_checkpoint`] forces a TRUNCATE
//!   checkpoint so the WAL/SHM never balloons between runs;
//!   [`Storage::backup_to`] uses `VACUUM INTO` for a single consistent
//!   snapshot file (no SQL injection: control-byte filtering + quote
//!   escaping, because `VACUUM INTO` does not accept parameters).
//! - **Retention floor** — [`Storage::gc_below_retention`] sweeps
//!   memories whose retention has decayed below the floor and that are
//!   older than `min_age_days`; it cleans the vector index in the same
//!   pass so HNSW never holds dangling node IDs.
//! - **Autonomic promotion** — [`Storage::auto_promote_frequent_access`]
//!   rewards memories accessed ≥3× in 24 h (Testing Effect / desirable
//!   difficulty), capped at 0.95 to keep ceiling room for genuine
//!   `mark_memory_useful` promotions.
//! - **Waking tags** — `set_waking_tag` / `clear_waking_tags` /
//!   `get_waking_tagged_memories` implement Stickgold's
//!   "tag-and-replay" account of NREM consolidation: ingest-time
//!   surprise marks a memory for preferential dream replay, the
//!   DreamEngine consumes the tag, then `clear_waking_tags` drains it.

use chrono::{Duration, Utc};
use rusqlite::params;

use crate::memory::KnowledgeNode;

use super::{Result, Storage, StorageError};

impl Storage {
    /// Flush WAL to the main database file for a clean shutdown.
    /// Safe to call at any time; a no-op if WAL is already empty.
    pub fn wal_checkpoint(&self) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// Create a consistent backup using VACUUM INTO
    pub fn backup_to(&self, path: &std::path::Path) -> Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| StorageError::Init("Invalid backup path encoding".to_string()))?;
        // Validate path: reject control characters (except tab) for defense-in-depth
        if path_str.bytes().any(|b| b < 0x20 && b != b'\t') {
            return Err(StorageError::Init(
                "Backup path contains invalid characters".to_string(),
            ));
        }
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        // VACUUM INTO doesn't support parameterized queries; escape single quotes
        reader.execute_batch(&format!("VACUUM INTO '{}'", path_str.replace('\'', "''")))?;
        Ok(())
    }

    /// Hard-delete memories below `threshold` retention that are older than
    /// `min_age_days`. **Destructive and irreversible** — the rows are `DELETE`d, no
    /// tombstone is written and nothing here can be undone.
    ///
    /// This primitive is for *explicit* garbage collection only, where the caller has
    /// shown the user the candidates first (the `gc` MCP tool and the dashboard panel
    /// both default to a dry run, which never reaches this method). It must never be
    /// called from an automatic path: consolidation used to call it on every cycle
    /// and silently deleted user memories, which is why that call site is gone (see
    /// `consolidation.rs`, step 16).
    ///
    /// The pass leaves the vector index in the state `delete_node` leaves it:
    /// eviction failures are reported, and the sidecar is rewritten so the ids this
    /// pass collected cannot come back from `vestige.hnsw` on the next boot.
    pub fn gc_below_retention(&self, threshold: f64, min_age_days: i64) -> Result<i64> {
        let cutoff = (Utc::now() - Duration::days(min_age_days)).to_rfc3339();

        // The candidate set and the DELETE have to be one atomic step. Read
        // through the reader and deleted through the writer, the predicate was
        // evaluated twice: a row that crossed the threshold in between was
        // deleted without ever being collected (its vector stayed in HNSW),
        // and a collected row could survive the DELETE while its vector was
        // still evicted — a live memory silently dropped out of semantic
        // search. `BEGIN IMMEDIATE` holds the write lock across both
        // statements, so the cleanup below acts on exactly the rows removed.
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let tx = super::helpers::begin_write_transaction(&mut writer)?;

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        let doomed_ids: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM knowledge_nodes WHERE retention_strength < ?1 AND created_at < ?2",
            )?;
            stmt.query_map(params![threshold, cutoff], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect()
        };

        let deleted = tx.execute(
            "DELETE FROM knowledge_nodes WHERE retention_strength < ?1 AND created_at < ?2",
            params![threshold, cutoff],
        )? as i64;
        tx.commit()?;
        drop(writer);

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        if deleted > 0 {
            match self.vector_index.lock() {
                Ok(mut index) => {
                    for id in &doomed_ids {
                        if let Err(e) = index.remove(id) {
                            tracing::warn!(
                                node_id = %id,
                                error = %e,
                                "vector index eviction failed during GC — the collected memory \
                                 can still be returned by semantic search until the index is rebuilt"
                            );
                        }
                    }
                }
                Err(_) => tracing::warn!(
                    collected = doomed_ids.len(),
                    "vector index lock poisoned — the memories collected by GC were not evicted"
                ),
            }

            // Persist immediately, like `delete_node`: leaving the rewrite to a
            // later pass is what let collected ids survive in the sidecar.
            if let Err(e) = self.persist_vector_index() {
                tracing::warn!(
                    error = %e,
                    "could not rewrite the vector index sidecar after GC — \
                     next startup rebuilds from SQLite instead of the sidecar"
                );
            }
        }

        Ok(deleted)
    }

    /// Check for auto-promote candidates: memories accessed 3+ times in last 24h
    pub fn auto_promote_frequent_access(&self) -> Result<i64> {
        let twenty_four_hours_ago = (Utc::now() - Duration::hours(24)).to_rfc3339();
        let now = Utc::now().to_rfc3339();

        // Find memories with 3+ accesses in last 24h
        let candidates: Vec<String> = {
            let reader = self
                .reader
                .lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            let mut stmt = reader.prepare(
                "SELECT node_id, COUNT(*) as access_count
                 FROM memory_access_log
                 WHERE accessed_at >= ?1
                 GROUP BY node_id
                 HAVING access_count >= 3",
            )?;
            stmt.query_map(params![twenty_four_hours_ago], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect()
        };

        if candidates.is_empty() {
            return Ok(0);
        }

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let mut promoted = 0i64;
        for id in &candidates {
            let rows = writer.execute(
                "UPDATE knowledge_nodes SET
                    retrieval_strength = MIN(1.0, retrieval_strength + 0.10),
                    retention_strength = MIN(1.0, retention_strength + 0.05),
                    last_accessed = ?1
                WHERE id = ?2 AND retrieval_strength < 0.95",
                params![now, id],
            )?;
            if rows > 0 {
                promoted += 1;
            }
        }

        Ok(promoted)
    }

    /// Set waking tag on a memory (marks it for preferential dream replay)
    pub fn set_waking_tag(&self, memory_id: &str) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "UPDATE knowledge_nodes SET waking_tag = TRUE, waking_tag_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), memory_id],
        )?;
        Ok(())
    }

    /// Clear waking tags (called after dream processes them)
    pub fn clear_waking_tags(&self) -> Result<i64> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let cleared = writer.execute(
            "UPDATE knowledge_nodes SET waking_tag = FALSE, waking_tag_at = NULL WHERE waking_tag = TRUE",
            [],
        )? as i64;
        Ok(cleared)
    }

    /// Get waking-tagged memories for preferential dream replay
    pub fn get_waking_tagged_memories(&self, limit: i32) -> Result<Vec<KnowledgeNode>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM knowledge_nodes WHERE waking_tag = TRUE ORDER BY waking_tag_at DESC LIMIT ?1"
        )?;
        let nodes = stmt.query_map(params![limit], Self::row_to_node)?;
        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }
}

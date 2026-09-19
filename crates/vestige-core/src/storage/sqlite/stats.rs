//! Top-level memory statistics for the dashboard and `system_status` tool.
//!
//! `get_stats` returns a single aggregate snapshot: total nodes, nodes due
//! for review, average retention / storage / retrieval strength, oldest and
//! newest memory timestamps, and embedding coverage. It uses the reader
//! connection pair so it never blocks an in-flight write.
//!
//! Per-tag and time-bucketed statistics live in `history.rs`; per-state and
//! transition counts live in `states.rs`. This module is only the "overall"
//! view the UI's `/api/stats` endpoint consumes.

use chrono::{DateTime, Utc};
use rusqlite::params;

use super::Storage;
use crate::memory::MemoryStats;
use crate::storage::Result;

impl Storage {
    /// Get top-level memory statistics for the dashboard / `system_status`.
    pub fn get_stats(&self) -> Result<MemoryStats> {
        let now = Utc::now().to_rfc3339();
        let reader = self.acquire_reader()?;

        let total: i64 =
            reader.query_row("SELECT COUNT(*) FROM knowledge_nodes", [], |row| row.get(0))?;

        let due: i64 = reader.query_row(
            "SELECT COUNT(*) FROM knowledge_nodes WHERE next_review <= ?1",
            params![now],
            |row| row.get(0),
        )?;

        let avg_retention: f64 = reader.query_row(
            "SELECT COALESCE(AVG(retention_strength), 0) FROM knowledge_nodes",
            [],
            |row| row.get(0),
        )?;

        let avg_storage: f64 = reader.query_row(
            "SELECT COALESCE(AVG(storage_strength), 1) FROM knowledge_nodes",
            [],
            |row| row.get(0),
        )?;

        let avg_retrieval: f64 = reader.query_row(
            "SELECT COALESCE(AVG(retrieval_strength), 1) FROM knowledge_nodes",
            [],
            |row| row.get(0),
        )?;

        let oldest: Option<String> = reader
            .query_row("SELECT MIN(created_at) FROM knowledge_nodes", [], |row| {
                row.get(0)
            })
            .ok();

        let newest: Option<String> = reader
            .query_row("SELECT MAX(created_at) FROM knowledge_nodes", [], |row| {
                row.get(0)
            })
            .ok();

        let nodes_with_embeddings: i64 = reader.query_row(
            "SELECT COUNT(*) FROM knowledge_nodes WHERE has_embedding = 1",
            [],
            |row| row.get(0),
        )?;

        // Report the tag the *current* embedding function writes — model id plus
        // space version and prefix regime, e.g. `nomic-embed-text-v1.5+v2+prefix`
        // — instead of a hard-coded model id. The old literal named the v1-style
        // tag no matter which space the stored vectors came from, so
        // `system_status.embeddingModel` (and `/api/system-stats`, and the CLI's
        // `Embedding Model` line) could not reveal that a store still holds
        // pre-v2 vectors; the version/regime is exactly what an operator needs
        // to decide whether `regenerate_embeddings` is still owed.
        #[cfg(feature = "embeddings")]
        let embedding_model: Option<String> = if nodes_with_embeddings > 0 {
            Some(crate::embeddings::embedding_model_tag().to_string())
        } else {
            None
        };
        // Without the embedding function there is no current tag to report, and
        // naming a model this build cannot compute a vector for would be a lie.
        #[cfg(not(feature = "embeddings"))]
        let embedding_model: Option<String> = if nodes_with_embeddings > 0 {
            Some("unavailable (built without the `embeddings` feature)".to_string())
        } else {
            None
        };

        Ok(MemoryStats {
            total_nodes: total,
            nodes_due_for_review: due,
            average_retention: avg_retention,
            average_storage_strength: avg_storage,
            average_retrieval_strength: avg_retrieval,
            oldest_memory: oldest.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            newest_memory: newest.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            nodes_with_embeddings,
            embedding_model,
        })
    }
}

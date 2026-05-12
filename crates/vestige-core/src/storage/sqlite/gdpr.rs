//! GDPR Article 17 erasure paths.
//!
//! Hard-deletion of a memory and every trace it left behind: connections,
//! embeddings, access logs, state transitions. Two entry points:
//!
//! - [`Storage::right_to_erasure`]: single memory by id.
//! - [`Storage::erase_by_tag`]: bulk wipe of everything carrying a tag.
//!
//! Anything that ever stores a `knowledge_nodes.id` as a foreign key MUST be
//! deleted here. If you add a new memory-keyed table to a migration, also add
//! a cascading `DELETE` below — otherwise GDPR audits will catch us with
//! orphan rows referencing erased subjects.

use rusqlite::params;

use super::{Result, Storage, StorageError};

impl Storage {
    /// GDPR Article 17 "Right to Erasure" — remove a memory and ALL associated
    /// traces: connections, embeddings, access logs, insights that reference it.
    /// Returns the number of artifacts erased (node + connections + embeddings + logs).
    pub fn right_to_erasure(&self, id: &str) -> Result<i64> {
        let mut erased = 0i64;
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;

        erased += writer.execute(
            "DELETE FROM memory_connections WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )? as i64;

        erased += writer.execute(
            "DELETE FROM node_embeddings WHERE node_id = ?1",
            params![id],
        )? as i64;

        erased += writer.execute("DELETE FROM access_log WHERE node_id = ?1", params![id])? as i64;

        erased += writer.execute(
            "DELETE FROM memory_states WHERE memory_id = ?1",
            params![id],
        )? as i64;

        let node_deleted =
            writer.execute("DELETE FROM knowledge_nodes WHERE id = ?1", params![id])?;
        erased += node_deleted as i64;

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        if node_deleted > 0
            && let Ok(mut index) = self.vector_index.lock()
        {
            let _ = index.remove(id);
        }

        Ok(erased)
    }

    /// Erase all memories matching a tag pattern (GDPR bulk erasure).
    /// Returns (memories_erased, total_artifacts_erased).
    pub fn erase_by_tag(&self, tag: &str) -> Result<(i64, i64)> {
        let ids: Vec<String> = {
            let reader = self
                .reader
                .lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            let pattern = format!("%\"{}%", tag.to_lowercase());
            let mut stmt =
                reader.prepare("SELECT id FROM knowledge_nodes WHERE LOWER(tags) LIKE ?1")?;
            stmt.query_map(params![pattern], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect()
        };

        let count = ids.len() as i64;
        let mut total_artifacts = 0i64;
        for id in &ids {
            total_artifacts += self.right_to_erasure(id)?;
        }

        Ok((count, total_artifacts))
    }
}

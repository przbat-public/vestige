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
    /// traces: connections, embeddings, access logs, states, and insights that
    /// reference it. Returns the number of artifacts erased
    /// (node + connections + embeddings + logs + states + insights).
    ///
    /// The deletes run inside a single `BEGIN IMMEDIATE` transaction: without
    /// it, a failure half-way left the node stripped of its embeddings, logs
    /// and states but still present, and the partial erasure was reported as a
    /// success. Free pages are reclaimed and the HNSW sidecar rewritten
    /// afterwards — otherwise the erased vector survives in `vestige.hnsw`.
    pub fn right_to_erasure(&self, id: &str) -> Result<i64> {
        let erased = self.erasure_rows(id)?;
        self.post_erasure_cleanup();
        Ok(erased)
    }

    /// Transactional part of [`Storage::right_to_erasure`] — every row that
    /// references `id`. Split out so `erase_by_tag` can batch N erasures into
    /// one transaction each and one cleanup for the whole set.
    fn erasure_rows(&self, id: &str) -> Result<i64> {
        let mut erased = 0i64;
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let tx = super::helpers::begin_write_transaction(&mut writer)?;

        erased += tx.execute(
            "DELETE FROM memory_connections WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )? as i64;

        erased += tx.execute(
            "DELETE FROM node_embeddings WHERE node_id = ?1",
            params![id],
        )? as i64;

        // Historical typo: the table is `memory_access_log` (see
        // migrations/sql.rs and helpers::log_access). Every other site uses
        // the correct name; this DELETE used to silently break the entire
        // GDPR erasure path with `no such table: access_log` before any
        // node row was actually removed. ON DELETE CASCADE on the FK would
        // mop up the access rows anyway, but the explicit DELETE is kept
        // here so the returned `erased` count reflects all artifacts.
        erased += tx.execute(
            "DELETE FROM memory_access_log WHERE node_id = ?1",
            params![id],
        )? as i64;

        erased += tx.execute(
            "DELETE FROM memory_states WHERE memory_id = ?1",
            params![id],
        )? as i64;

        // `insights.source_memories` is a JSON array of node ids with no
        // foreign key, so the cascade never reaches it: an erased subject
        // stayed referenced by every insight that pointed at it. Drop those
        // insights in the same transaction.
        erased += tx.execute(
            "DELETE FROM insights
             WHERE EXISTS (SELECT 1 FROM json_each(insights.source_memories) WHERE value = ?1)",
            params![id],
        )? as i64;

        let node_deleted = tx.execute("DELETE FROM knowledge_nodes WHERE id = ?1", params![id])?;
        erased += node_deleted as i64;

        tx.commit()?;

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        if node_deleted > 0
            && let Ok(mut index) = self.vector_index.lock()
        {
            let _ = index.remove(id);
        }

        Ok(erased)
    }

    /// Reclaim free pages and refresh the HNSW sidecar after an erasure.
    ///
    /// Best-effort by design: the erasure itself is already committed, and a
    /// failure to reclaim pages or rewrite the cache must not be reported as a
    /// failed erasure.
    fn post_erasure_cleanup(&self) {
        // `incremental_vacuum` only does work when the database runs with
        // `auto_vacuum = INCREMENTAL` (V14); elsewhere it is a no-op.
        if let Ok(writer) = self.writer.lock() {
            let _ = writer.execute_batch("PRAGMA incremental_vacuum;");
        }

        #[cfg(feature = "vector-search")]
        if let Err(e) = self.persist_vector_index() {
            tracing::warn!(
                error = %e,
                "could not refresh the vector index sidecar after erasure — \
                 the erased vector may survive in vestige.hnsw until the next persist"
            );
        }
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
            total_artifacts += self.erasure_rows(id)?;
        }
        // One page-reclaim + sidecar rewrite for the whole batch instead of one
        // per id.
        self.post_erasure_cleanup();

        Ok((count, total_artifacts))
    }
}

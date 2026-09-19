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

use super::{Result, Storage, StorageError, normalize_tags};

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

    /// Make the erasure physical, then refresh the HNSW sidecar.
    ///
    /// Reporting a memory as erased while its text is still readable in the
    /// database file is the failure this guards against: GDPR audits read raw
    /// bytes, not query results. Best-effort by design — the erasure itself is
    /// already committed, and a failure to reclaim pages or rewrite the cache
    /// must not be reported as a failed erasure.
    fn post_erasure_cleanup(&self) {
        if let Ok(writer) = self.writer.lock() {
            // The AFTER DELETE trigger only leaves a tombstone in the FTS
            // index: the erased terms stay in `knowledge_fts_data` (and are
            // still returned by `optimize`-less raw reads) until the segments
            // are merged. Merging is expensive, which is exactly why it is
            // reserved for this path.
            let _ = writer
                .execute_batch("INSERT INTO knowledge_fts(knowledge_fts) VALUES('optimize');");
            // `incremental_vacuum` only does work when the database runs with
            // `auto_vacuum = INCREMENTAL` (V14); elsewhere it is a no-op.
            let _ = writer.execute_batch("PRAGMA incremental_vacuum;");
            // The WAL holds pre-delete page images until a checkpoint, so
            // `strings vestige.db-wal` could still recover the erased text
            // after every query agreed the row was gone.
            let _ = writer.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
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

    /// Erase all memories carrying exactly `tag` (GDPR bulk erasure).
    /// Returns (memories_erased, total_artifacts_erased).
    ///
    /// Membership is an exact element match over the stored JSON array, not a
    /// substring match. The previous `LOWER(tags) LIKE '%"<tag>%'` pattern had
    /// no closing quote and escaped no wildcards, so erasing `code` also
    /// erased `codebase` — irreversible over-deletion in the one code path
    /// where over-deletion cannot be undone.
    pub fn erase_by_tag(&self, tag: &str) -> Result<(i64, i64)> {
        // Canonicalize exactly like the writers do (`normalize_tags` also
        // lowercases and folds spaces/underscores to `-`), otherwise a caller
        // passing "Code" would silently match nothing.
        let normalized = normalize_tags(&[tag.to_string()]);
        let Some(tag) = normalized.first() else {
            return Ok((0, 0));
        };

        let ids: Vec<String> = {
            let reader = self
                .reader
                .lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            let mut stmt = reader.prepare(
                "SELECT id FROM knowledge_nodes
                 WHERE EXISTS (SELECT 1 FROM json_each(knowledge_nodes.tags) WHERE value = ?1)",
            )?;
            stmt.query_map(params![tag], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect()
        };

        let count = ids.len() as i64;
        let mut total_artifacts = 0i64;
        for id in &ids {
            total_artifacts += self.erasure_rows(id)?;
        }
        if count > 0 {
            // One page-reclaim + sidecar rewrite for the whole batch instead of
            // one per id.
            self.post_erasure_cleanup();
        }

        Ok((count, total_artifacts))
    }
}

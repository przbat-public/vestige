//! Append-only content history (`memory_revisions`).
//!
//! Every operation that changes what a memory *says* — `create`, `edit`,
//! `supersede`, `invalidate`, `quarantine` — appends a row here. The point is
//! the one thing the schema could not answer before: what did we believe
//! earlier, and when did it change. The life-cycle tables
//! (`state_transitions`, `consolidation_history`, `dream_history`,
//! `retention_snapshots`) record scheduling, and scheduling is not content.
//!
//! **The write is never free-standing.** A revision committed without its
//! change (or the change without its revision) is worse than no history at all:
//! the first makes the timeline lie about a change that did not happen, the
//! second loses the very change the timeline exists to show. Callers therefore
//! pass a [`Transaction`] they already hold, and the row commits or rolls back
//! with the operation it describes.
//!
//! `recorded_at` is the *storage* clock, not the caller's `DateTime::now()`:
//! history rows are the one place where a client-supplied timestamp would let
//! a later writer silently rewrite the past.

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};

use super::records::{MemoryRevision, RevisionKind};
use super::{Result, Storage};
// Only the test-only helpers below need it: a writer that touches a row that
// is not there must say so instead of reporting success.
#[cfg(test)]
use super::StorageError;

impl Storage {
    /// Append one revision row inside a caller-owned transaction.
    ///
    /// Returns the new row id. See the module docs for why this takes a
    /// `Transaction` instead of opening its own.
    ///
    /// `actor` is recorded verbatim when supplied. It is `None` on every path
    /// in this crate today — the call sites that know which agent or user is
    /// acting live one layer up (MCP tools), and wave 2 threads them through
    /// rather than guessing here.
    #[allow(
        clippy::too_many_arguments,
        reason = "One positional argument per column of `memory_revisions`; bundling them into a struct would hide at the call site which field is which, and the reason/old/new triple is exactly what a reader of the timeline needs to see."
    )]
    pub(super) fn record_revision(
        tx: &Transaction<'_>,
        node_id: &str,
        kind: RevisionKind,
        old_content: Option<&str>,
        new_content: Option<&str>,
        reason: Option<&str>,
        actor: Option<&str>,
    ) -> Result<i64> {
        tx.execute(
            "INSERT INTO memory_revisions
                (node_id, recorded_at, kind, old_content, new_content, reason, actor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                node_id,
                Utc::now().to_rfc3339(),
                kind.as_str(),
                old_content,
                new_content,
                reason,
                actor,
            ],
        )?;
        Ok(tx.last_insert_rowid())
    }

    /// Delete every revision of a node inside a caller-owned transaction.
    ///
    /// Used by erasure and by node deletion. `memory_revisions.node_id` has no
    /// foreign key by design (see `migrations/sql.rs`, V17), so nothing deletes
    /// these rows implicitly — if this call is dropped from a deletion path,
    /// the erased memory's text stays readable in the history table.
    pub(super) fn delete_revisions_for(tx: &Transaction<'_>, node_id: &str) -> Result<usize> {
        Ok(tx.execute(
            "DELETE FROM memory_revisions WHERE node_id = ?1",
            params![node_id],
        )?)
    }

    /// A memory's content history, newest first.
    ///
    /// `limit` is clamped to at least 1: a caller that passes 0 or a negative
    /// value is asking for *some* history, and an empty page would be
    /// indistinguishable from a memory that has no history at all.
    ///
    /// Ties on `recorded_at` are broken by `id DESC` — an edit that lands in
    /// the same millisecond as the `create` it follows must still sort after
    /// it, or the timeline reads backwards.
    pub fn get_memory_revisions(&self, node_id: &str, limit: i64) -> Result<Vec<MemoryRevision>> {
        let reader = self.acquire_reader()?;
        let mut stmt = reader.prepare(
            "SELECT * FROM memory_revisions
             WHERE node_id = ?1
             ORDER BY recorded_at DESC, id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![node_id, limit.max(1)], Self::row_to_memory_revision)?;

        let mut revisions = Vec::new();
        for row in rows {
            revisions.push(row?);
        }
        Ok(revisions)
    }

    /// The newest revision of a node, if it has any.
    ///
    /// Shares the ordering of [`Self::get_memory_revisions`] so the two can
    /// never disagree about which revision is newest.
    pub fn get_latest_revision(&self, node_id: &str) -> Result<Option<MemoryRevision>> {
        let reader = self.acquire_reader()?;
        let revision = reader
            .query_row(
                "SELECT * FROM memory_revisions
                 WHERE node_id = ?1
                 ORDER BY recorded_at DESC, id DESC
                 LIMIT 1",
                params![node_id],
                Self::row_to_memory_revision,
            )
            .optional()?;
        Ok(revision)
    }

    /// Count a node's revisions straight from the table.
    ///
    /// Exists so a test can assert "exactly one revision" against the stored
    /// rows rather than against a read path that might itself filter.
    #[cfg(test)]
    pub(super) fn count_revisions(&self, node_id: &str) -> Result<i64> {
        let reader = self.acquire_reader()?;
        Ok(reader.query_row(
            "SELECT COUNT(*) FROM memory_revisions WHERE node_id = ?1",
            params![node_id],
            |row| row.get(0),
        )?)
    }

    /// Count revision rows anywhere in the table whose text contains `needle`.
    ///
    /// Erasure has to be checked against the whole table, not against one
    /// node's rows: an orphaned revision left behind by a failed delete belongs
    /// to no node, so a per-node read would report a clean erasure while the
    /// text sat in the file.
    #[cfg(test)]
    pub(super) fn count_revisions_containing(&self, needle: &str) -> Result<i64> {
        let reader = self.acquire_reader()?;
        Ok(reader.query_row(
            "SELECT COUNT(*) FROM memory_revisions
             WHERE COALESCE(old_content, '') LIKE '%' || ?1 || '%'
                OR COALESCE(new_content, '') LIKE '%' || ?1 || '%'",
            params![needle],
            |row| row.get(0),
        )?)
    }

    /// Read a node's stored `recorded_at` without going through
    /// [`Self::row_to_node`]'s `created_at` fallback.
    ///
    /// The mapper is deliberately tolerant of a NULL column, so a test that
    /// wants to prove the column is populated (or unchanged) must look at the
    /// column itself.
    #[cfg(test)]
    pub(super) fn raw_recorded_at(&self, id: &str) -> Result<Option<String>> {
        let reader = self.acquire_reader()?;
        let value = reader
            .query_row(
                "SELECT recorded_at FROM knowledge_nodes WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.flatten())
    }

    /// Overwrite a node's stored `recorded_at`.
    ///
    /// Test-only, and deliberately *not* a production writer: the immutability
    /// contract ("no update path may move `recorded_at`") is only testable if a
    /// test can move it first and then watch a search, a strengthening pass and
    /// a consolidation run fail to move it back. Returns `NotFound` for an
    /// unknown id so a typo cannot pass vacuously.
    #[cfg(test)]
    pub(super) fn set_recorded_at_for_test(&self, id: &str, value: &str) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let n = writer.execute(
            "UPDATE knowledge_nodes SET recorded_at = ?1 WHERE id = ?2",
            params![value, id],
        )?;
        if n == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

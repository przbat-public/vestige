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

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Transaction, params};

use super::records::{MemoryRevision, RevisionKind};
use super::{Result, Storage};
// Only the test-only helpers below need it: a writer that touches a row that
// is not there must say so instead of reporting success.
#[cfg(test)]
use super::StorageError;

/// How much of one revision's `old_content` / `new_content` a rendered timeline
/// carries, in characters.
///
/// An edit stores two full copies of a memory's text, and a history page holds
/// up to a hundred of them, so a timeline that renders everything verbatim stops
/// being a timeline and becomes a payload. The cap is public — and every reader
/// that applies it also reports it, together with a per-field `…Truncated`
/// marker — so a client renders the truncation from the payload instead of
/// hard-coding the number and drifting away from the server.
pub const REVISION_CONTENT_CHAR_LIMIT: usize = 500;

/// The revision kinds that take a memory back: after one of these, the store no
/// longer asserted the memory was the current picture.
///
/// Named here because the *same* list decides both what a reader may be shown as
/// believed at a past instant (`Storage::retracted_node_ids_at_or_before`) and
/// what the timeline calls a retraction. A second copy of it in a caller is how
/// those two answers start disagreeing.
pub const RETRACTION_KINDS: [RevisionKind; 2] = [RevisionKind::Invalidate, RevisionKind::Supersede];

impl Storage {
    /// Append one revision row inside a caller-owned transaction.
    ///
    /// Returns the new row id. See the module docs for why this takes a
    /// `Transaction` instead of opening its own.
    ///
    /// `actor` is recorded verbatim when supplied. It arrives on
    /// [`crate::memory::IngestInput::actor`], because the layer that knows which
    /// agent and which conversation is writing is the tool layer above this
    /// crate; a caller that does not know passes `None`, which leaves the
    /// column NULL rather than inventing an identity. Reads never consult it —
    /// it exists so a human auditing the timeline can see who wrote a version.
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

    /// Which of `candidates` the store had already taken back at or before `at`.
    ///
    /// This is the half of "what did we believe at instant T" that the node row
    /// cannot answer on its own. `recorded_at <= T` says a memory existed; it
    /// does not say whether it was still the current picture, and today's
    /// `valid_until` is the wrong clock for that question — a memory superseded
    /// *after* T was the belief *at* T even though its `valid_until` lies in T's
    /// future, and one invalidated *before* T was already gone even though the
    /// revision that says so is the only place that fact survives.
    ///
    /// Read from the revisions rather than from `valid_until` on purpose: a
    /// validity bound can also be anchored at write time as a claim about the
    /// world ("this holds until Friday"), which is not a retraction and must not
    /// be scored as one. Only an `invalidate` / `supersede` row is the store
    /// saying "we no longer assert this".
    ///
    /// A row whose `recorded_at` cannot be parsed counts as a retraction: this
    /// decides whether a memory may be presented as believed, and a timestamp
    /// nobody can read is not evidence that it was.
    pub fn retracted_node_ids_at_or_before(
        &self,
        at: DateTime<Utc>,
        candidates: &[String],
    ) -> Result<std::collections::HashSet<String>> {
        let mut retracted = std::collections::HashSet::new();
        if candidates.is_empty() {
            return Ok(retracted);
        }

        let kind_list = RETRACTION_KINDS
            .iter()
            .map(|kind| format!("'{}'", kind.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=candidates.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(", ");
        // The `kind` list is built from the closed enum above, never from input,
        // so the only bound parameters are the candidate ids themselves.
        let sql = format!(
            "SELECT node_id, recorded_at FROM memory_revisions
             WHERE kind IN ({kind_list}) AND node_id IN ({placeholders})"
        );

        let reader = self.acquire_reader()?;
        let mut stmt = reader.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = candidates
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (node_id, recorded_at) = row?;
            let was_retracted = DateTime::parse_from_rfc3339(&recorded_at)
                .map(|stamp| stamp.with_timezone(&Utc) <= at)
                .unwrap_or(true);
            if was_retracted {
                retracted.insert(node_id);
            }
        }
        Ok(retracted)
    }

    /// How many revisions a node has, in total.
    ///
    /// A page of history is not the history: a caller that renders the newest
    /// `limit` steps has to be able to say how many older steps it left out, or
    /// a truncated timeline reads as a complete one.
    pub fn count_revisions(&self, node_id: &str) -> Result<i64> {
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

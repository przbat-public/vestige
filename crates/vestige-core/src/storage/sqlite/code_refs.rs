//! Storage for `code_refs` — a memory's anchors into code, and their verdicts.
//!
//! Three jobs, kept apart on purpose:
//!
//! - **Write** ([`Storage::insert_code_ref`]): one row per anchor, inside the
//!   same transaction as the memory it belongs to. An anchor that commits alone
//!   would describe a memory that does not exist; a memory that commits alone
//!   would silently lose the citation it was written with.
//! - **Read** ([`Storage::code_refs_for_nodes`]): the search path fetches anchors
//!   for a page of results in one query. Per-node lookups would put a round trip
//!   inside a loop the read path runs on every search.
//! - **Audit** ([`Storage::audit_code_anchors`]): re-resolve and re-record.
//!   Report-only — it updates `verdict`/`resolved_at` and never touches the
//!   reference itself, because inventing a new pointer is worse than reporting a
//!   broken one.
//!
//! `code_refs.node_id` has no foreign key (V19 explains why), so deletion is
//! explicit: [`Storage::delete_code_refs_for`] is called from the GDPR erasure
//! path, where an anchor left behind would be a record of which files the
//! subject's memories touched — erasure that leaves that is not erasure.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{Transaction, params};

use super::records::CodeRef;
use super::{Result, Storage};

/// How many anchors one consolidation pass re-checks.
///
/// Bounded because the audit opens a repository per distinct `repo_remote` and
/// walks a tree per anchor; a store with tens of thousands of anchors must not
/// turn a background cycle into an unbounded filesystem scan. The next cycle
/// picks up where this one stopped, because rows are ordered by how long ago
/// they were last checked (`resolved_at IS NULL` first).
pub const CODE_ANCHOR_AUDIT_BATCH: i64 = 500;

use crate::code_refs::CodeAnchorAudit;

impl Storage {
    /// Insert one anchor for `node_id` inside a caller-owned transaction.
    ///
    /// `resolved_at` is written as given: a verdict with no timestamp would make
    /// "checked recently" indistinguishable from "checked at some point", which
    /// is the same collapse the record-time column exists to prevent.
    pub(super) fn insert_code_ref(
        tx: &Transaction<'_>,
        node_id: &str,
        anchor: &crate::code_refs::IngestAnchor,
    ) -> Result<i64> {
        tx.execute(
            "INSERT INTO code_refs
                (node_id, repo_remote, commit_sha, path, symbol, hint_line, content_hash,
                 resolved_at, verdict)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                node_id,
                anchor.anchor.repo_remote,
                anchor.anchor.commit_sha,
                anchor.anchor.path,
                anchor.anchor.symbol,
                anchor.anchor.hint_line,
                anchor.anchor.content_hash,
                anchor.resolved_at.map(|at| at.to_rfc3339()),
                anchor.verdict.as_str(),
            ],
        )?;
        Ok(tx.last_insert_rowid())
    }

    /// Delete every anchor of a node inside a caller-owned transaction.
    ///
    /// Called by erasure and by node deletion. Nothing deletes these rows
    /// implicitly, so dropping this call makes "the memory was erased" false in
    /// the one way an auditor checks: the file names survive.
    pub(super) fn delete_code_refs_for(tx: &Transaction<'_>, node_id: &str) -> Result<usize> {
        Ok(tx.execute("DELETE FROM code_refs WHERE node_id = ?1", params![node_id])?)
    }

    /// Attach anchors to a node that already exists — the update/merge path.
    ///
    /// A memory that absorbs new text carries the citations that came with it,
    /// and losing them is the silent loss this table exists to end. The node's
    /// earlier anchors are left alone on purpose: they describe text the memory
    /// still holds in its content history, and a stale verdict is what tells a
    /// reader they no longer apply. Deleting them here would be the automatic
    /// repair this feature refuses to make.
    pub fn attach_code_anchors(
        &self,
        node_id: &str,
        anchors: &[crate::code_refs::IngestAnchor],
    ) -> Result<usize> {
        if anchors.is_empty() {
            return Ok(0);
        }
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| super::StorageError::Init("Writer lock poisoned".into()))?;
        let tx = super::helpers::begin_write_transaction(&mut writer)?;
        let mut attached = 0usize;
        for anchor in anchors {
            Self::insert_code_ref(&tx, node_id, anchor)?;
            attached += 1;
        }
        tx.commit()?;
        Ok(attached)
    }

    /// A memory's anchors, oldest first.
    pub fn code_refs_for(&self, node_id: &str) -> Result<Vec<CodeRef>> {
        let reader = self.acquire_reader()?;
        let mut stmt =
            reader.prepare("SELECT * FROM code_refs WHERE node_id = ?1 ORDER BY id ASC")?;
        let rows = stmt.query_map(params![node_id], Self::row_to_code_ref)?;
        let mut refs = Vec::new();
        for row in rows {
            refs.push(row?);
        }
        Ok(refs)
    }

    /// Anchors for a page of search results, keyed by node id.
    ///
    /// One query for the whole page, and nodes with no anchors are simply absent
    /// from the map rather than present with an empty vector — the caller's
    /// question is "does this result carry an anchor?", and an empty list would
    /// answer it with the same shape as a row that failed to load.
    pub fn code_refs_for_nodes(
        &self,
        node_ids: &[String],
    ) -> Result<HashMap<String, Vec<CodeRef>>> {
        let mut by_node: HashMap<String, Vec<CodeRef>> = HashMap::new();
        if node_ids.is_empty() {
            return Ok(by_node);
        }

        let reader = self.acquire_reader()?;
        // A temporary table would be tidier, but this is a bounded page (search
        // results, at most a hundred) and building the IN list keeps the whole
        // read inside one statement, which matters because the reader lock is
        // held for the duration either way.
        let placeholders = std::iter::repeat_n("?", node_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT * FROM code_refs WHERE node_id IN ({placeholders}) ORDER BY node_id, id"
        );
        let mut stmt = reader.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = node_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let rows = stmt.query_map(params.as_slice(), Self::row_to_code_ref)?;
        for row in rows {
            let code_ref = row?;
            by_node
                .entry(code_ref.node_id.clone())
                .or_default()
                .push(code_ref);
        }
        Ok(by_node)
    }

    /// The anchors an audit should check next: never-checked first, then the
    /// ones checked longest ago.
    pub fn code_refs_for_audit(&self, limit: i64) -> Result<Vec<CodeRef>> {
        let reader = self.acquire_reader()?;
        let mut stmt = reader.prepare(
            "SELECT * FROM code_refs
             ORDER BY resolved_at IS NOT NULL, resolved_at ASC, id ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.max(1)], Self::row_to_code_ref)?;
        let mut refs = Vec::new();
        for row in rows {
            refs.push(row?);
        }
        Ok(refs)
    }

    /// Record the outcome of one check.
    ///
    /// `content_hash` is written only when `Some`, and the audit passes `None`
    /// for every verdict except `fresh`: a stale anchor that adopted the new
    /// text would report `fresh` on the next pass, which would erase the very
    /// rot the audit exists to surface.
    pub fn record_code_ref_verdict(
        &self,
        id: i64,
        verdict: crate::code_refs::AnchorVerdict,
        resolved_at: Option<DateTime<Utc>>,
        content_hash: Option<&str>,
    ) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| super::StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "UPDATE code_refs
             SET verdict = ?1,
                 resolved_at = ?2,
                 content_hash = COALESCE(?3, content_hash)
             WHERE id = ?4",
            params![
                verdict.as_str(),
                resolved_at.map(|at| at.to_rfc3339()),
                content_hash,
                id,
            ],
        )?;
        Ok(())
    }

    /// Re-resolve up to `limit` anchors and record what the check found.
    ///
    /// Report-only, and that is a design decision rather than an omission:
    /// nothing here writes a path, a revision or a symbol. Counts come back so
    /// the dream/consolidation cycle can report rot instead of silently
    /// absorbing it.
    ///
    /// A resolution that cannot open a repository is recorded as `unchecked`
    /// with the reason, not skipped: "we tried and could not verify" is
    /// information, and a skipped row would keep its old `fresh` forever.
    pub fn audit_code_anchors(&self, limit: i64) -> Result<CodeAnchorAudit> {
        let anchors = self.code_refs_for_audit(limit)?;
        let roots = crate::code_refs::default_repo_roots();
        let mut audit = CodeAnchorAudit::default();

        for code_ref in anchors {
            let resolution = code_ref.anchor.resolve_with(&roots);
            audit.count(resolution.verdict);

            // The freshly computed hash is written back only when the check
            // produced one *and* the verdict is `fresh`. For `stale` the anchor
            // keeps the hash of the text the memory was written about, so the
            // next audit still reports `stale` — a rot that repairs itself on
            // the second look is not reported at all.
            let hash_to_write = match resolution.verdict {
                crate::code_refs::AnchorVerdict::Fresh => resolution
                    .content_hash
                    .as_deref()
                    .or(code_ref.anchor.content_hash.as_deref()),
                _ => None,
            };
            self.record_code_ref_verdict(
                code_ref.id,
                resolution.verdict,
                resolution.resolved_at,
                hash_to_write,
            )?;
        }

        Ok(audit)
    }

    /// Total anchors in the store. Test helper and audit summary.
    pub fn code_ref_count(&self) -> Result<i64> {
        let reader = self.acquire_reader()?;
        Ok(reader.query_row("SELECT COUNT(*) FROM code_refs", [], |row| row.get(0))?)
    }

    /// Anchors by verdict, for the maintenance report.
    pub fn code_ref_verdict_counts(&self) -> Result<HashMap<String, i64>> {
        let reader = self.acquire_reader()?;
        let mut stmt =
            reader.prepare("SELECT verdict, COUNT(*) FROM code_refs GROUP BY verdict")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counts = HashMap::new();
        for row in rows {
            let (verdict, count) = row?;
            counts.insert(verdict, count);
        }
        Ok(counts)
    }

    pub(super) fn row_to_code_ref(row: &rusqlite::Row) -> rusqlite::Result<CodeRef> {
        let verdict: String = row.get("verdict")?;
        let resolved_at: Option<String> = row.get("resolved_at")?;
        let resolved_at = match resolved_at {
            // An unparseable timestamp would silently become "checked now" on
            // the next audit, pushing the row to the back of the queue; surface
            // it instead.
            Some(value) => Some(Storage::parse_timestamp(&value, "resolved_at")?),
            None => None,
        };

        Ok(CodeRef {
            id: row.get("id")?,
            node_id: row.get("node_id")?,
            anchor: crate::code_refs::CodeAnchor {
                repo_remote: row.get("repo_remote")?,
                commit_sha: row.get("commit_sha")?,
                path: row.get("path")?,
                symbol: row.get("symbol")?,
                hint_line: row.get("hint_line")?,
                content_hash: row.get("content_hash")?,
            },
            resolved_at,
            verdict: crate::code_refs::AnchorVerdict::parse(&verdict).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("unknown code_refs.verdict '{verdict}'"),
                    )),
                )
            })?,
        })
    }

    /// Number of anchors of a node whose path is `path`. Test helper: erasure is
    /// checked against the table, not against a read path that might filter.
    #[cfg(test)]
    pub(super) fn count_code_refs_with_path(&self, path: &str) -> Result<i64> {
        let reader = self.acquire_reader()?;
        Ok(reader.query_row(
            "SELECT COUNT(*) FROM code_refs WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?)
    }
}

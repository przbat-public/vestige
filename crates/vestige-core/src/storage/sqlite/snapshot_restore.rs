//! Import a `VACUUM INTO` snapshot — the artefact the `backup` tool writes — back into a
//! live store.
//!
//! `backup` produces a consistent `.db` snapshot, but until now nothing could read it
//! back: `restore` (the MCP tool, the CLI and the `vestige-restore` binary) only
//! understands the JSON export, so the snapshot was a dead end. A backup that cannot be
//! restored is a promise rather than a safety net — and the automation trigger happily
//! reported the store as protected.
//!
//! The import is a **merge**, not a replacement: rows are `INSERT OR REPLACE`d by id, so
//! memories that exist only in the snapshot come back while everything else in the live
//! store is left alone. Embeddings travel with the snapshot only when their dimension
//! matches what this build produces; otherwise the imported rows are marked
//! `has_embedding = 0` so the existing `regenerate_embeddings` path re-embeds them
//! instead of leaving a vector from a different model in place.

use std::path::Path;

use rusqlite::{Connection, params};

use super::Storage;
use super::error::{Result, StorageError};

/// What a snapshot import did, for the caller to report.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRestoreReport {
    /// Rows merged into the live store.
    pub nodes_imported: usize,
    /// Rows the snapshot contained (equal to or greater than `nodes_imported`).
    pub nodes_in_snapshot: usize,
    /// Vectors copied because their dimension matches this build.
    pub embeddings_imported: usize,
    /// Imported memories marked for re-embedding because their vector did not travel.
    pub embeddings_reset: usize,
    /// Schema version recorded inside the snapshot.
    pub snapshot_schema_version: u32,
    /// Schema version of the live store.
    pub live_schema_version: u32,
}

impl Storage {
    /// Merge a `VACUUM INTO` snapshot into this store.
    ///
    /// Returns a report; the caller decides how to surface it. Refuses a snapshot written
    /// by a newer binary (its schema may contain columns this build cannot interpret) and
    /// says so, rather than importing a partial row set.
    pub fn restore_from_snapshot(&self, path: &Path) -> Result<SnapshotRestoreReport> {
        let report = {
            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;

            // ATTACH cannot run inside a transaction, and we are not in one here.
            writer.execute(
                "ATTACH DATABASE ?1 AS snapshot",
                params![path.to_string_lossy()],
            )?;

            let outcome = import_snapshot(&writer);

            // Best effort: a failed DETACH must not mask the import result, but the
            // connection must not keep the snapshot attached either.
            if let Err(e) = writer.execute_batch("DETACH DATABASE snapshot") {
                tracing::warn!(error = %e, "could not detach the snapshot database");
            }

            outcome?
        };

        // Rebuild the in-memory vector index outside the writer lock: the loader takes the
        // reader lock, and taking writer-then-reader here would invert the order every
        // other combined-lock site uses.
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        if let Err(e) = self.load_embeddings_into_index() {
            tracing::warn!(error = %e, "vector index rebuild after snapshot import failed");
        }

        Ok(report)
    }
}

/// Import the snapshot already attached as `snapshot`.
fn import_snapshot(conn: &Connection) -> Result<SnapshotRestoreReport> {
    let live_schema_version = schema_version(conn, "main")?;
    let snapshot_schema_version = schema_version(conn, "snapshot")?;

    if snapshot_schema_version > live_schema_version {
        return Err(StorageError::Init(format!(
            "Snapshot schema v{snapshot_schema_version} is newer than this build (v{live_schema_version}); \
             upgrade Vestige before restoring it"
        )));
    }

    let live_columns = table_columns(conn, "main", "knowledge_nodes")?;
    let snapshot_columns = table_columns(conn, "snapshot", "knowledge_nodes")?;
    let shared: Vec<String> = live_columns
        .into_iter()
        .filter(|c| snapshot_columns.contains(c))
        .collect();

    if shared.is_empty() {
        return Err(StorageError::Init(
            "Snapshot has no knowledge_nodes columns in common with this build".into(),
        ));
    }

    let nodes_in_snapshot: usize = conn
        .query_row("SELECT COUNT(*) FROM snapshot.knowledge_nodes", [], |r| {
            r.get::<_, i64>(0)
        })
        .map(|c| c as usize)?;

    // Explicit column list on both sides: robust to columns added or removed between the
    // snapshot's schema and this build, which `SELECT *` would silently mis-align.
    //
    // `recorded_at` needs more than a shared-column copy. A snapshot written before V17
    // has no such column at all, and a snapshot written by V17 itself can hold NULL in it
    // for rows that predated the column. Copying the intersection would import those rows
    // with no record time stored — the read path papers over it by falling back to
    // `created_at`, but the stored invariant ("no row lacks a record time") would be
    // false, an audit filtering on the column would silently skip those memories, and any
    // point-in-time query would be wrong for exactly the memories whose record time
    // matters most: old ones arriving in a new store. So the value is derived here instead.
    // Column names stay unqualified on purpose: in SQLite a column of an attached database
    // is `schema.table.column`, so `snapshot.<column>` is read as a *table alias* and fails
    // with "no such column". The FROM clause names the one table, so bare names are exact.
    let mut insert_columns = shared.clone();
    let mut select_expressions: Vec<String> = shared.clone();
    if let Some(position) = insert_columns.iter().position(|c| c == "recorded_at") {
        select_expressions[position] = "COALESCE(recorded_at, created_at)".to_string();
    } else if snapshot_columns.iter().any(|c| c == "created_at") {
        // The snapshot predates the column: its `created_at` is the only honest claim
        // available about when the fact was recorded.
        insert_columns.push("recorded_at".to_string());
        select_expressions.push("created_at".to_string());
    }

    let column_list = insert_columns.join(", ");
    let value_list = select_expressions.join(", ");
    let nodes_imported = conn.execute(
        &format!(
            "INSERT OR REPLACE INTO main.knowledge_nodes ({column_list}) \
             SELECT {value_list} FROM snapshot.knowledge_nodes"
        ),
        [],
    )?;

    // Belt and braces for the same invariant: whatever the snapshot looked like, no row
    // may be left without a record time after a merge. A no-op when the copy above did its
    // job, which is the point — the guarantee should not depend on that branch being right.
    conn.execute(
        "UPDATE main.knowledge_nodes SET recorded_at = created_at WHERE recorded_at IS NULL",
        [],
    )?;

    let embeddings_imported = import_embeddings(conn)?;

    // Anything without a usable vector must go back through the embedder rather than sit
    // in the store claiming to have one.
    let embeddings_reset = conn.execute(
        "UPDATE main.knowledge_nodes SET has_embedding = CASE \
             WHEN EXISTS (SELECT 1 FROM main.node_embeddings e WHERE e.node_id = main.knowledge_nodes.id) \
             THEN 1 ELSE 0 END",
        [],
    )?;

    // REPLACE does not fire the delete trigger for the conflicting row, so the external
    // content index has to be rebuilt from the table rather than trusted.
    // The FTS5 command column cannot be schema-qualified, and this connection's default
    // schema is `main`.
    conn.execute_batch("INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild');")?;

    Ok(SnapshotRestoreReport {
        nodes_imported,
        nodes_in_snapshot,
        embeddings_imported,
        embeddings_reset,
        snapshot_schema_version,
        live_schema_version,
    })
}

/// Copy vectors from the snapshot when their dimension matches what this build produces.
///
/// A snapshot taken with a different embedding profile (or an older, wider model) keeps
/// its vectors in the file but they are not imported: mixing dimensions would corrupt
/// cosine similarity, and the honest alternative — re-embedding — is already available.
fn import_embeddings(conn: &Connection) -> Result<usize> {
    let live_columns = table_columns(conn, "main", "node_embeddings")?;
    if live_columns.is_empty() {
        return Ok(0);
    }

    let snapshot_columns = match table_columns(conn, "snapshot", "node_embeddings") {
        Ok(columns) => columns,
        Err(_) => return Ok(0), // snapshot predates the embeddings table
    };
    let shared: Vec<String> = live_columns
        .into_iter()
        .filter(|c| snapshot_columns.contains(c))
        .collect();
    if shared.is_empty() {
        return Ok(0);
    }

    #[cfg(feature = "embeddings")]
    let expected_dimensions = crate::embeddings::EMBEDDING_DIMENSIONS as i64;
    #[cfg(not(feature = "embeddings"))]
    let expected_dimensions = -1i64;

    let column_list = shared.join(", ");
    let imported = conn.execute(
        &format!(
            "INSERT OR REPLACE INTO main.node_embeddings ({column_list}) \
             SELECT {column_list} FROM snapshot.node_embeddings \
             WHERE dimensions = ?1"
        ),
        params![expected_dimensions],
    )?;

    Ok(imported)
}

fn schema_version(conn: &Connection, schema: &str) -> Result<u32> {
    let version: i64 = conn.query_row(
        &format!("SELECT COALESCE(MAX(version), 0) FROM {schema}.schema_version"),
        [],
        |row| row.get(0),
    )?;
    Ok(version.max(0) as u32)
}

/// Column names of `schema.table`, in declaration order.
fn table_columns(conn: &Connection, schema: &str, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA {schema}.table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

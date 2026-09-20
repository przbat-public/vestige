//! Import a `VACUUM INTO` snapshot — the artefact the `backup` tool writes — back into a
//! live store.
//!
//! `backup` produces a consistent `.db` snapshot, but until now nothing could read it
//! back: `restore` (the MCP tool, the CLI and the `vestige-restore` binary) only
//! understands the JSON export, so the snapshot was a dead end. A backup that cannot be
//! restored is a promise rather than a safety net — and the automation trigger happily
//! reported the store as protected.
//!
//! The import is a **merge**, not a replacement: a memory the live store does not have
//! arrives, and everything else in the live store is left alone. Embeddings travel with
//! the snapshot only when their dimension matches what this build produces; otherwise the
//! imported rows are marked `has_embedding = 0` so the existing `regenerate_embeddings`
//! path re-embeds them instead of leaving a vector from a different model in place.
//!
//! A memory is more than its `knowledge_nodes` row, so the merge covers the four tables
//! that describe it: `memory_revisions` (V17 — how its wording changed), `code_refs`
//! (V19 — what it points at in code), `memory_connections` (the association graph it
//! takes part in) and `memory_states` (its lifecycle row). All four are imported
//! **additively**, and every statement is scoped so a row can only arrive for a memory
//! the merged store holds — the snapshot's nodes, which the node merge has just inserted
//! — so an older snapshot can add to the live store but never take away from it. Each
//! import states its own identity and conflict rule and why the alternative loses data.
//! The node row itself is still replaced by the snapshot's version — pre-existing
//! behaviour, pinned by the tests.
//!
//! The whole merge runs inside one `BEGIN IMMEDIATE` transaction. Without it a constraint
//! the snapshot's rows do not satisfy left a half-merged store behind — some tables
//! imported, the rest not — and returned an error the caller could only read as "the
//! restore failed" while the store no longer matched that description. `ATTACH` cannot
//! run inside a transaction, so the snapshot is attached before the transaction opens and
//! detached after it has committed or rolled back.
//!
//! Two further tables hold node ids as JSON arrays — `insights.source_memories` and
//! `intentions.related_memories` — and this import neither brings them across nor repairs
//! them afterwards, because a merge cannot leave them dangling and no reader dereferences
//! them. The merge only inserts or replaces `knowledge_nodes` rows under ids that already
//! exist, so an id that resolved before it resolves after it too; the direction that could
//! strand a reference, dropping a node, is not one this import has. Nor is a dead id a
//! failure for anything that reads them: `resources::memory` echoes `source_memories` as
//! ids without looking them up, `tools::cross_reference` uses only the insight text and
//! its confidence, `tools::intention` never reads `related_memories` at all, and the
//! dashboard's insight surface reads `knowledge_nodes.extra_json` rather than the
//! `insights` table. The one statement that looks inside an array is GDPR's
//! `json_each(insights.source_memories)` membership delete, where an id with no node
//! simply never matches. A cleanup pass would have to delete an insight because a node
//! went missing, which destroys the record instead of repairing it.

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
    /// Content-history rows **added** to the live store's history. A revision the
    /// live store already had is not counted and not touched.
    pub revisions_imported: usize,
    /// Code anchors **added** to the live store. An anchor the live store already
    /// had keeps its own (possibly fresher) verdict and is not counted.
    pub code_refs_imported: usize,
    /// Association edges **added from the snapshot**. An edge the live store
    /// already held keeps its own strength and activation count, and is not
    /// counted.
    pub connections_imported: usize,
    /// Lifecycle rows **added from the snapshot**. A memory the live store
    /// already had a lifecycle row for keeps that row, and is not counted.
    pub states_imported: usize,
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
    ///
    /// The merge is all-or-nothing: it commits as one transaction, so a failure part-way
    /// through leaves the store exactly as it was found.
    pub fn restore_from_snapshot(&self, path: &Path) -> Result<SnapshotRestoreReport> {
        let report = {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;

            // ATTACH cannot run inside a transaction, and we are not in one here.
            writer.execute(
                "ATTACH DATABASE ?1 AS snapshot",
                params![path.to_string_lossy()],
            )?;

            let outcome = import_in_transaction(&mut writer);

            // Best effort: a failed DETACH must not mask the import result, but the
            // connection must not keep the snapshot attached either. Runs after the
            // transaction has ended either way — DETACH is not allowed inside one.
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

/// Run the whole merge as one transaction, so a failure leaves nothing behind.
///
/// Every statement in [`import_snapshot`] used to run bare on the writer connection.
/// SQLite makes a *statement* atomic, not a sequence of them, so a row the snapshot
/// carries that this build's schema rejects — a `CHECK` the snapshot's writer never
/// enforced, a `NOT NULL` column the intersection left to its default, a full disk —
/// aborted in the middle of the merge and left the store with the node rows imported,
/// the history and anchors missing, and the full-text index rebuilt over a mixture of
/// the two. The caller sees only an error, cannot tell which tables landed, and has no
/// supported way to undo them; a retry then merges on top of that partial state. One
/// transaction turns "the restore failed" back into a statement about the store.
///
/// `BEGIN IMMEDIATE` rather than `DEFERRED` for the reason every writer in this crate
/// uses the same helper: a deferred transaction that reads before it writes can lose
/// the upgrade to a competing writer without consulting `busy_timeout`.
///
/// The returned `Transaction` rolls back when it is dropped, which is what the `?`
/// below relies on: an error from [`import_snapshot`] returns before `commit`, and the
/// rollback happens as the value goes out of scope, on the same connection.
fn import_in_transaction(writer: &mut Connection) -> Result<SnapshotRestoreReport> {
    let tx = super::helpers::begin_write_transaction(writer)?;
    let report = import_snapshot(&tx)?;
    tx.commit()?;
    Ok(report)
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

    // Before the node rows are touched: the node merge destroys this store's own
    // child rows for every memory the snapshot carries (see the function), and the
    // imports below must still be able to see them to know which rows the live
    // store already has.
    capture_live_child_rows(conn)?;

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

    // Put this store's own edges and lifecycle rows back before importing the
    // snapshot's, so the imports below see them and leave them alone.
    reinstate_live_child_rows(conn)?;

    let revisions_imported = import_revisions(conn)?;
    let code_refs_imported = import_code_refs(conn)?;
    let connections_imported = import_connections(conn)?;
    let states_imported = import_states(conn)?;

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
        revisions_imported,
        code_refs_imported,
        connections_imported,
        states_imported,
        embeddings_imported,
        embeddings_reset,
        snapshot_schema_version,
        live_schema_version,
    })
}

/// Import a memory's content history (`memory_revisions`) from the snapshot.
///
/// **The rule is add, never replace.** A snapshot is a claim about one moment;
/// a revision the live store recorded after the backup is a change the backup
/// cannot testify against, and the running store is the only place that change
/// exists. Clearing a live node's history before copying the snapshot's would
/// therefore lose the most recent part of the timeline for every memory the two
/// stores share — the same silent loss this import exists to end, reached from
/// the other direction. So nothing in `main` is ever updated or deleted here.
///
/// `id` is not copied. It is a rowid local to the store that minted it, so the
/// same number in another store belongs to an unrelated revision; carrying it
/// (with `INSERT OR REPLACE`) would delete a live row, and `INSERT OR IGNORE`
/// would silently drop the snapshot's row instead. SQLite mints fresh ids, and
/// idempotency comes from the dedup below: a row is the same revision when every
/// shared column matches, compared with `IS` so a NULL `reason` or `actor`
/// matches its own kind rather than falling out of the comparison.
///
/// Scoped to the nodes the snapshot itself carries. A revision whose node the
/// snapshot does not hold belongs to no memory the merge is bringing back, and
/// importing it would put text the live store no longer has into the one table
/// that exists to keep text — erasure undone by a file the user was told was a
/// backup. What this scope cannot do is withhold history from a memory the
/// snapshot *does* carry: the node merge already resurrects that row (pre-existing
/// behaviour, unchanged here), its revisions describe no text the node does not
/// itself hold, and the live store keeps no tombstone — so "erased here" and
/// "never seen here" are the same absence, and no finer rule is available to this
/// function.
fn import_revisions(conn: &Connection) -> Result<usize> {
    let Some(columns) = importable_columns(
        conn,
        "memory_revisions",
        // NOT NULL in every version of the table; without them there is no row
        // to insert and nothing to match one against.
        &["node_id", "recorded_at", "kind"],
        &["id"],
    )?
    else {
        return Ok(0);
    };

    // Aliases (`s` for the snapshot, `m` for the live table) are required here,
    // not cosmetic: the dedup subquery names the same table the outer query reads
    // from, so a bare column name would be ambiguous between the two.
    let select_list = columns
        .iter()
        .map(|c| format!("s.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let same_row = columns
        .iter()
        .map(|c| format!("m.{c} IS s.{c}"))
        .collect::<Vec<_>>()
        .join(" AND ");

    let sql = format!(
        "INSERT INTO main.memory_revisions ({}) \
         SELECT {select_list} FROM snapshot.memory_revisions s \
         WHERE s.node_id IN (SELECT id FROM snapshot.knowledge_nodes) \
           AND NOT EXISTS (SELECT 1 FROM main.memory_revisions m WHERE {same_row})",
        columns.join(", "),
    );

    Ok(conn.execute(&sql, [])?)
}

/// Import a memory's code anchors (`code_refs`) from the snapshot.
///
/// **The rule is add, never replace**, for the reasons [`import_revisions`]
/// gives, plus one this table adds: a live row's `verdict` and `resolved_at` are
/// the product of this store's own audit and are at least as recent as the
/// snapshot's frozen copy, so overwriting them would trade a checked answer for
/// an older one. An anchor this store already holds is left exactly as it is.
///
/// Identity is the reference itself — every shared column except `verdict` and
/// `resolved_at`, which say when the check last ran rather than what the memory
/// points at. The same anchor re-checked is still the same anchor, so importing
/// it twice would list the same file twice in every search result that cites the
/// memory. `id` is excluded for the reason given above: it is a rowid, not an
/// identity.
///
/// Scoped to the snapshot's nodes, also as above: an anchor with no memory
/// behind it is a record of which files a memory the live store let go of used
/// to cite, which is precisely what erasure removes.
fn import_code_refs(conn: &Connection) -> Result<usize> {
    let Some(columns) = importable_columns(conn, "code_refs", &["node_id", "path"], &["id"])?
    else {
        return Ok(0);
    };

    // `node_id` and `path` are required above, so the identity is never empty and
    // the `NOT EXISTS` clause below is always a well-formed predicate.
    let identity = columns
        .iter()
        .filter(|c| !matches!(c.as_str(), "verdict" | "resolved_at"))
        .map(|c| format!("m.{c} IS s.{c}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let select_list = columns
        .iter()
        .map(|c| format!("s.{c}"))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "INSERT INTO main.code_refs ({}) \
         SELECT {select_list} FROM snapshot.code_refs s \
         WHERE s.node_id IN (SELECT id FROM snapshot.knowledge_nodes) \
           AND NOT EXISTS (SELECT 1 FROM main.code_refs m WHERE {identity})",
        columns.join(", "),
    );

    Ok(conn.execute(&sql, [])?)
}

/// Import the association graph (`memory_connections`) from the snapshot.
///
/// **The rule is add, never replace**, for the reasons [`import_revisions`]
/// gives, with the identity this table adds: the schema declares
/// `(source_id, target_id)` the primary key, so an edge *is* that ordered pair
/// and nothing else identifies it. Direction is part of the identity — `A -> B`
/// and `B -> A` are two edges here, and collapsing them would drop half of a
/// graph the snapshot attests to.
///
/// A duplicate on that key is possible, not theoretical: the capture below puts
/// this store's own edges back into `main` before the snapshot's are read, the
/// graph is minted from content by consolidation as well as by an explicit
/// `save_connection`, and a snapshot restored twice — the weekly case — names
/// the same pairs again. A plain `INSERT` would abort on the primary key, and
/// with the merge inside one transaction that abort now costs the whole restore.
/// So a pair the live store already holds is skipped, and only the snapshot's
/// new edges are added. The live row also wins on the numbers: `strength`,
/// `last_activated` and `activation_count` are the product of this store's own
/// decay, pruning and spreading activation, all of which ran after the snapshot
/// was taken, so importing the frozen copy over them would rewind every edge to
/// an older moment and could resurrect strength the store's decay had already
/// removed — the same trade [`import_code_refs`] refuses to make with a verdict.
///
/// Both endpoints must exist in the merged store. That is what makes an edge an
/// edge: a row naming a node this store does not have is a dangling reference
/// no traversal can use, and would also violate the foreign key the schema
/// declares. For a well-formed snapshot it is the same scope the other imports
/// use — the node merge has just inserted every node the snapshot carries — and
/// it keeps, rather than drops, an edge whose far endpoint the snapshot happens
/// not to hold while the live store does. An edge carries no text of its own,
/// so unlike a revision or an anchor it cannot smuggle back content the live
/// store let go of; what it can do is re-associate two memories that both exist.
fn import_connections(conn: &Connection) -> Result<usize> {
    let Some(columns) = importable_columns(
        conn,
        "memory_connections",
        // The primary key, plus every NOT NULL column this build's table declares
        // without a default: a row missing one of those cannot be inserted, and
        // skipping the table with a warning is the honest answer to a shape this
        // build does not know.
        &[
            "source_id",
            "target_id",
            "strength",
            "link_type",
            "created_at",
            "last_activated",
        ],
        &[],
    )?
    else {
        return Ok(0);
    };

    let select_list = columns
        .iter()
        .map(|c| format!("s.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO main.memory_connections ({}) \
         SELECT {select_list} FROM snapshot.memory_connections s \
         WHERE EXISTS (SELECT 1 FROM main.knowledge_nodes n WHERE n.id = s.source_id) \
           AND EXISTS (SELECT 1 FROM main.knowledge_nodes n WHERE n.id = s.target_id) \
           AND NOT EXISTS (SELECT 1 FROM main.memory_connections m \
                           WHERE m.source_id IS s.source_id AND m.target_id IS s.target_id)",
        columns.join(", "),
    );

    Ok(conn.execute(&sql, [])?)
}

/// Import the lifecycle rows (`memory_states`) from the snapshot.
///
/// **The live row wins, and a memory that has none gets the snapshot's.** The
/// primary key is `memory_id`, so a memory has exactly one lifecycle row, and
/// the question is not whether a duplicate is possible but which of the two
/// claims about that memory to keep. It is the live one, for the reason
/// [`import_revisions`] gives: the row is bookkeeping about *this* store —
/// `access_count` and `last_access` count retrievals it served, `state` is
/// re-derived from the reconciled retention on every access and consolidation,
/// and `suppression_until`/`suppressed_by` are explicit overrides a caller set,
/// possibly after the backup. The snapshot's copy was frozen at backup time, so
/// writing it over a live row would roll the lifecycle back to an older moment:
/// the store would forget accesses it served, and a memory it has since
/// suppressed or let decay could come back active. That is the same failure the
/// additive rules exist to prevent, and "the snapshot is a claim about one
/// moment" applies to the lifecycle even more than to the text.
///
/// A memory the live store does not have does need the row: `state` is what the
/// dashboard and the retention bands read, and until something touches the
/// memory the create-if-missing path in `record_memory_access` will not run.
/// Importing it is not a claim the snapshot never made — it is the claim it did
/// make, about a memory that had no local history to contradict it.
///
/// Scoped to the snapshot's nodes, as above: a lifecycle row for a memory the
/// snapshot does not carry describes a memory the merge is not bringing back,
/// and its access history is the live store's to keep or to have erased.
fn import_states(conn: &Connection) -> Result<usize> {
    let Some(columns) = importable_columns(
        conn,
        "memory_states",
        // `memory_id` is the identity; the rest are NOT NULL in this build's
        // table without a default, so a snapshot lacking one is a shape this
        // build cannot import.
        &["memory_id", "state", "last_access", "state_entered_at"],
        &[],
    )?
    else {
        return Ok(0);
    };

    let select_list = columns
        .iter()
        .map(|c| format!("s.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO main.memory_states ({}) \
         SELECT {select_list} FROM snapshot.memory_states s \
         WHERE s.memory_id IN (SELECT id FROM snapshot.knowledge_nodes) \
           AND NOT EXISTS (SELECT 1 FROM main.memory_states m WHERE m.memory_id IS s.memory_id)",
        columns.join(", "),
    );

    Ok(conn.execute(&sql, [])?)
}

/// Park the live child rows of the snapshot's nodes in `temp` across the node merge.
///
/// The two tables above are the two the merge would otherwise destroy: both
/// declare `FOREIGN KEY (…) REFERENCES knowledge_nodes(id) ON DELETE CASCADE`,
/// and the node merge is `INSERT OR REPLACE`, which SQLite implements by
/// deleting the conflicting row and inserting the snapshot's version — with
/// foreign keys enabled, as they are on this connection, the delete cascades.
/// So every live edge and every live lifecycle row belonging to a memory the
/// snapshot carries disappears during that one statement, before either import
/// runs. Without parking them, "add, never replace" would be a rule about rows
/// that no longer exist: restoring the same snapshot weekly would reset the
/// whole graph to the backup's frozen strengths and every shared memory's
/// access history to the backup's count, which is the loss the additive rules
/// exist to prevent, arriving through the foreign key instead of an `UPDATE`.
///
/// Only rows that touch a node the snapshot carries can be destroyed this way,
/// so only those are copied. `temp` is per-connection, invisible to readers and
/// inside the import's transaction: a failed merge rolls the copy back with
/// everything else, and [`reinstate_live_child_rows`] drops it once the rows
/// are back in `main`.
fn capture_live_child_rows(conn: &Connection) -> Result<()> {
    for (table, parked, scope) in [
        (
            "memory_connections",
            "restore_live_connections",
            "source_id IN (SELECT id FROM snapshot.knowledge_nodes) \
             OR target_id IN (SELECT id FROM snapshot.knowledge_nodes)",
        ),
        (
            "memory_states",
            "restore_live_states",
            "memory_id IN (SELECT id FROM snapshot.knowledge_nodes)",
        ),
    ] {
        // The base schema defines both tables, so this only skips for a store that
        // never had one — in which case there is nothing to lose and nothing to
        // put back.
        if table_columns(conn, "main", table)?.is_empty() {
            continue;
        }
        conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS temp.{parked};
             CREATE TEMP TABLE {parked} AS SELECT * FROM main.{table} WHERE {scope};"
        ))?;
    }
    Ok(())
}

/// Put the rows [`capture_live_child_rows`] parked back into `main`.
///
/// Runs after the node merge and before the two imports, which is what makes
/// their `NOT EXISTS` guards mean "the live store already has this row": the
/// row is back in `main` by the time they look. `INSERT OR REPLACE` rather than
/// `INSERT` so the live values win even on a connection where the cascade did
/// not run — the rule belongs to this function, not to a `PRAGMA`.
fn reinstate_live_child_rows(conn: &Connection) -> Result<()> {
    for (table, parked) in [
        ("memory_connections", "restore_live_connections"),
        ("memory_states", "restore_live_states"),
    ] {
        let columns = table_columns(conn, "main", table)?;
        if columns.is_empty() {
            continue;
        }
        let column_list = columns.join(", ");
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO main.{table} ({column_list}) \
                 SELECT {column_list} FROM temp.{parked}"
            ),
            [],
        )?;
        conn.execute_batch(&format!("DROP TABLE IF EXISTS temp.{parked}"))?;
    }
    Ok(())
}

/// The columns of `table` a snapshot can contribute to `main`: the intersection
/// of both schemas, minus `excluded`.
///
/// `Ok(None)` means "nothing to import" and is not an error. A snapshot written
/// before V17 has no `memory_revisions` and one before V19 has no `code_refs`;
/// refusing the whole restore over a table the snapshot never had would trade a
/// recoverable store for a lost one. A table that exists but lacks a `required`
/// column is a shape this build does not know, so it is skipped with a warning
/// rather than imported as a partial row that would either fail the NOT NULL
/// constraint or invent the missing value.
fn importable_columns(
    conn: &Connection,
    table: &str,
    required: &[&str],
    excluded: &[&str],
) -> Result<Option<Vec<String>>> {
    let live_columns = table_columns(conn, "main", table)?;
    if live_columns.is_empty() {
        return Ok(None);
    }

    let snapshot_columns = match table_columns(conn, "snapshot", table) {
        Ok(columns) => columns,
        Err(_) => return Ok(None), // the attached file has no such schema/table
    };
    if snapshot_columns.is_empty() {
        return Ok(None);
    }

    let shared: Vec<String> = live_columns
        .into_iter()
        .filter(|c| snapshot_columns.contains(c) && !excluded.contains(&c.as_str()))
        .collect();

    if required.iter().any(|r| !shared.iter().any(|c| c == r)) {
        tracing::warn!(
            table,
            "snapshot table is missing columns this build needs; skipping its import"
        );
        return Ok(None);
    }

    Ok(Some(shared))
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

//! Tests for the migration pipeline.

use super::migration::Migration;
use super::registry::MIGRATIONS;
use super::runner::{apply_migrations, apply_pending, get_current_version};

#[test]
fn test_migrations_apply_cleanly_on_fresh_db() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let applied = apply_migrations(&conn).unwrap();
    assert_eq!(applied, MIGRATIONS.len() as u32);
    let version = get_current_version(&conn).unwrap();
    assert_eq!(version, MIGRATIONS.last().unwrap().version);
}

#[test]
fn test_migrations_are_idempotent() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();
    let applied_again = apply_migrations(&conn).unwrap();
    assert_eq!(applied_again, 0);
}

#[test]
fn test_partial_migration_rolls_back() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();
    let version_before = get_current_version(&conn).unwrap();

    // Attempt to run a batch with valid + invalid SQL inside a transaction
    conn.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let result = conn.execute_batch("CREATE TABLE _test_rollback (id INTEGER); INVALID SQL HERE;");
    assert!(result.is_err());
    let _ = conn.execute_batch("ROLLBACK;");

    // The temp table should NOT exist after rollback
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE name = '_test_rollback'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!exists);

    let version_after = get_current_version(&conn).unwrap();
    assert_eq!(version_before, version_after);
}

/// After all migrations apply, the database must run in
/// `PRAGMA auto_vacuum = INCREMENTAL` (mode 2) so deleted pages are
/// reclaimable via `PRAGMA incremental_vacuum` without a full table
/// rewrite. Long-lived stores accumulate dead pages otherwise (the v3.5
/// audit caught this).
///
/// The pragma can only be *changed* from NONE→INCREMENTAL by running
/// `VACUUM` after toggling it, so this is necessarily a one-shot
/// migration that uses the same "outside the txn" escape hatch as V7.
#[test]
fn test_auto_vacuum_is_incremental_after_migrations() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();

    let mode: i32 = conn
        .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        mode, 2,
        "auto_vacuum must be INCREMENTAL (2), got {} — long-lived databases will bloat without page reclamation",
        mode
    );
}

// ============================================================================
// V16: narrowed FTS update trigger + waking-tag partial index
// ============================================================================

/// `knowledge_au` used to fire on every UPDATE, so each search hit and each
/// whole-table `apply_decay` pass re-tokenized the FTS5 document even though
/// `content`/`tags` did not move. Pin the narrowed trigger: if a later
/// migration recreates it without `OF content, tags`, the write amplification
/// silently comes back.
#[test]
fn v16_update_trigger_is_narrowed_to_content_and_tags() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();

    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'knowledge_au'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        sql.contains("UPDATE OF content, tags"),
        "knowledge_au must only fire for content/tags writes, got: {sql}"
    );
}

/// `get_waking_tagged_memories` runs twice per dream cycle. V8 added the
/// columns but no index, so each call scanned `knowledge_nodes` and parsed the
/// JSON of every row. The partial index only contains tagged rows.
#[test]
fn waking_tag_lookup_uses_partial_index() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();

    // A few rows so the planner has something to reason about.
    for i in 0..50 {
        conn.execute(
            "INSERT INTO knowledge_nodes (id, content, node_type, created_at, updated_at, last_accessed, tags)
             VALUES (?1, ?2, 'fact', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '[]')",
            rusqlite::params![format!("node-{i}"), format!("memory {i}")],
        )
        .unwrap();
    }

    let mut stmt = conn
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT * FROM knowledge_nodes WHERE waking_tag = TRUE
             ORDER BY waking_tag_at DESC LIMIT 10",
        )
        .unwrap();
    let plan: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert!(
        plan.iter()
            .any(|line| line.contains("idx_nodes_waking_tag")),
        "planner ignored the waking-tag partial index: {plan:?}"
    );
}

// ============================================================================
// Migration runner hardening
// ============================================================================

/// Two processes can boot against the same fresh database (CLI beside the MCP
/// server): the loser reads version 0 before the winner commits, then waits on
/// the write lock. Replaying V1..Vn at that point dies with `duplicate column
/// name` even though the database is perfectly healthy. `apply_pending` takes
/// the stale hint and must re-read the version under the lock, applying
/// nothing.
#[test]
fn stale_version_hint_does_not_replay_migrations() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();

    let applied = apply_pending(&conn, MIGRATIONS, 0)
        .expect("a stale version hint must not replay migrations against a migrated database");
    assert_eq!(applied, 0, "nothing was pending");
    assert_eq!(
        get_current_version(&conn).unwrap(),
        MIGRATIONS.last().unwrap().version
    );
}

/// A database written by a newer build must be refused, not opened: columns
/// this binary does not know about would be left unmanaged.
#[test]
fn future_schema_version_is_refused() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();
    conn.execute_batch("UPDATE schema_version SET version = 99;")
        .unwrap();

    let err = apply_migrations(&conn).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("newer than this build"),
        "expected a clear refusal, got: {message}"
    );
}

/// `get_current_version` used to end in `.or(Ok(0))`, so a corrupted or
/// unreadable version row made an existing database look brand new.
#[test]
fn corrupted_version_row_is_not_read_as_zero() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();
    // `version` is an INTEGER PRIMARY KEY (rowid alias), so the corruption has
    // to come from a schema mismatch rather than a direct text write.
    conn.execute_batch(
        "DROP TABLE schema_version;
         CREATE VIEW schema_version AS SELECT 'not-a-number' AS version;",
    )
    .unwrap();

    assert!(
        get_current_version(&conn).is_err(),
        "a corrupted schema_version must surface as an error, not as version 0"
    );
    assert!(apply_migrations(&conn).is_err());
}

/// The old `test_partial_migration_rolls_back` exercised raw `execute_batch`
/// semantics, never the runner. This one drives the real code path: a
/// migration whose SQL fails half-way must roll back and leave nothing behind.
#[test]
fn runner_rolls_back_a_failed_migration() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let migrations = [Migration {
        version: 1,
        description: "test: valid statement followed by invalid SQL",
        up: "CREATE TABLE _rollback_probe (id INTEGER); INVALID SQL HERE;",
    }];

    let err = apply_pending(&conn, &migrations, 0).unwrap_err();
    assert!(
        err.to_string().contains("syntax error"),
        "expected the SQL error to surface, got: {err}"
    );

    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE name = '_rollback_probe'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        !exists,
        "the runner must roll back the partial migration — the probe table survived"
    );
}

// ============================================================================
// V15 — FTS5 tokenizer upgrade
// ============================================================================

/// V15 replaced `porter ascii` (V7) with `porter unicode61 remove_diacritics 2`
/// and rebuilt the index. The distinction that matters to a user is whether
/// non-ASCII text is indexed at all: `ascii` treats every byte above 0x7F as a
/// separator, so `Gdańsk` produced a token fragment that no query reaches,
/// while `unicode61 remove_diacritics 2` folds the accent and indexes `gdansk`.
///
/// The migration is only correct if the REBUILD covers rows that already
/// existed, not just rows written afterwards — otherwise the upgrade silently
/// leaves every pre-existing memory unsearchable, which is exactly the kind of
/// data-shaped bug a schema change can hide. The test therefore writes a row
/// under the old tokenizer, asserts it is unreachable by its folded form before
/// the migration, and asserts it is reachable after.
#[test]
fn v15_rebuilds_the_index_for_rows_written_before_the_upgrade() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    // Database as it stood at V14, with one memory written under `porter ascii`.
    apply_pending(&conn, &MIGRATIONS[..14], 0).unwrap();
    conn.execute(
        "INSERT INTO knowledge_nodes (id, content, node_type, created_at, updated_at, last_accessed, tags)
         VALUES ('pre-upgrade', 'Gdańsk wspomnienia z wyjazdu', 'fact',
                 '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '[]')",
        [],
    )
    .unwrap();

    let fts_hits = |term: &str| -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM knowledge_fts WHERE knowledge_fts MATCH ?1",
            rusqlite::params![term],
            |row| row.get(0),
        )
        .unwrap()
    };

    assert_eq!(
        fts_hits("wspomnienia"),
        1,
        "the ASCII part of the memory must be indexed before the upgrade — \
         without this the test would pass for the wrong reason"
    );
    assert_eq!(
        fts_hits("gdansk"),
        0,
        "`porter ascii` cannot index `Gdańsk`; if this is 1 the pre-upgrade \
         state under test no longer exists"
    );

    // The upgrade itself. `apply_pending` reports how many migrations it ran,
    // so the resulting version is read back separately.
    let applied = apply_pending(&conn, &MIGRATIONS[..15], 14).unwrap();
    assert_eq!(applied, 1, "only V15 is pending after V14");
    assert_eq!(get_current_version(&conn).unwrap(), 15);

    let create_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'knowledge_fts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        create_sql.contains("remove_diacritics 2"),
        "V15 must install the folding tokenizer, found: {create_sql}"
    );

    assert_eq!(
        fts_hits("gdansk"),
        1,
        "the rebuild must re-tokenize existing rows, not only the ones written afterwards"
    );
    assert_eq!(
        fts_hits("wspomnienia"),
        1,
        "the rebuild must keep the rows it had"
    );

    // V15 drops and re-creates the sync triggers; if that step regressed, new
    // memories would stop reaching the index while every old row still answers.
    conn.execute(
        "INSERT INTO knowledge_nodes (id, content, node_type, created_at, updated_at, last_accessed, tags)
         VALUES ('post-upgrade', 'Kraków after the upgrade', 'fact',
                 '2026-01-02T00:00:00+00:00', '2026-01-02T00:00:00+00:00', '2026-01-02T00:00:00+00:00', '[]')",
        [],
    )
    .unwrap();
    assert_eq!(
        fts_hits("krakow"),
        1,
        "rows written after V15 must be indexed by the re-created triggers"
    );
    assert_eq!(
        fts_hits("gdansk"),
        1,
        "the re-created triggers must not disturb the rows the rebuild indexed"
    );

    let nodes: i64 = conn
        .query_row("SELECT COUNT(*) FROM knowledge_nodes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(nodes, 2, "the migration must not add or drop memories");
}

// ============================================================================
// V17 — record time + content history
// ============================================================================

/// The registry must stay contiguous and apply through its last entry on a
/// fresh database. A gap (say v16 → v18) would be applied by the runner anyway,
/// since it only compares numbers — the missing version's schema would simply
/// never exist.
///
/// Written against `MIGRATIONS.last()` rather than a literal: the invariant is
/// contiguity, and pinning the number here as well as in the per-migration
/// tests turned every new migration into a two-place edit whose failure said
/// nothing about the registry.
#[test]
fn fresh_database_reaches_the_last_registered_version_with_a_contiguous_registry() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();

    let last = MIGRATIONS.last().unwrap().version;
    assert_eq!(get_current_version(&conn).unwrap(), last);

    let versions: Vec<u32> = MIGRATIONS.iter().map(|m| m.version).collect();
    let expected: Vec<u32> = (1..=last).collect();
    assert_eq!(
        versions, expected,
        "migration versions must stay contiguous through {last}"
    );
}

/// An existing V16 database must be migrated *and* backfilled: a pre-existing
/// memory's record time can only honestly be its `created_at`, and a NULL
/// `recorded_at` would make every historical memory look like it was learned at
/// the epoch the moment a reader forgot the column is nullable.
#[test]
fn v17_migrates_a_v16_database_with_recorded_at_equal_to_created_at() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_pending(&conn, &MIGRATIONS[..16], 0).unwrap();
    assert_eq!(get_current_version(&conn).unwrap(), 16);

    conn.execute(
        "INSERT INTO knowledge_nodes (id, content, node_type, created_at, updated_at, last_accessed, tags)
         VALUES ('legacy-1', 'learned before record time existed', 'fact',
                 '2026-03-04T05:06:07+00:00', '2026-03-04T05:06:07+00:00',
                 '2026-04-01T00:00:00+00:00', '[]')",
        [],
    )
    .unwrap();

    let applied = apply_pending(&conn, &MIGRATIONS[..17], 16).unwrap();
    assert_eq!(applied, 1, "only V17 is pending after V16");
    assert_eq!(get_current_version(&conn).unwrap(), 17);

    let recorded_at: String = conn
        .query_row(
            "SELECT recorded_at FROM knowledge_nodes WHERE id = 'legacy-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        recorded_at, "2026-03-04T05:06:07+00:00",
        "a row that predates the column must be backfilled from created_at"
    );

    // The column is deliberately nullable in SQLite (it cannot be added with a
    // non-constant NOT NULL default), so the enforced invariant is "no row
    // lacks a record time". Pin it here: a later migration or writer that
    // leaves NULL behind fails this.
    let missing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_nodes WHERE recorded_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(missing, 0, "every row must carry a record time after V17");

    // The history table exists and rejects an unknown kind — the audit trail is
    // useless if a typo can write a category nobody queries.
    conn.execute(
        "INSERT INTO memory_revisions (node_id, recorded_at, kind, new_content)
         VALUES ('legacy-1', '2026-03-04T05:06:07+00:00', 'create', 'learned before record time existed')",
        [],
    )
    .unwrap();
    assert!(
        conn.execute(
            "INSERT INTO memory_revisions (node_id, recorded_at, kind) VALUES ('legacy-1', '2026-03-04T05:06:07+00:00', 'rewritten')",
            [],
        )
        .is_err(),
        "memory_revisions.kind must be constrained to the five known kinds"
    );
}

/// V17 must roll back like any other migration. It touches two schemas (a
/// column on `knowledge_nodes` and a new table), so a failure half-way used to
/// leave the store with a `recorded_at` column and no history table — a shape
/// no later `INSERT` would understand.
#[test]
fn v17_rolls_back_when_a_statement_fails() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_pending(&conn, &MIGRATIONS[..16], 0).unwrap();

    let broken = [Migration {
        version: 17,
        description: "test: V17 shape with a failing tail",
        up: "ALTER TABLE knowledge_nodes ADD COLUMN recorded_at TEXT;
             CREATE TABLE IF NOT EXISTS memory_revisions (id INTEGER PRIMARY KEY);
             INVALID SQL HERE;",
    }];
    let err = apply_pending(&conn, &broken, 16).unwrap_err();
    assert!(
        err.to_string().contains("syntax error"),
        "expected the SQL error to surface, got: {err}"
    );

    assert_eq!(
        get_current_version(&conn).unwrap(),
        16,
        "a failed V17 must not bump the schema version"
    );

    let has_column: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('knowledge_nodes') WHERE name = 'recorded_at'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        !has_column,
        "the added column survived the rollback — the migration was not atomic"
    );

    let has_table: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE name = 'memory_revisions'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!has_table, "the new table survived the rollback");
}

// ============================================================================
// V18 — the self-containedness marker
// ============================================================================

/// V18 adds the marker columns and deliberately does **not** backfill them.
///
/// A memory written before the gate existed has not passed it, and a backfill
/// to `1` would make the one query this column exists for — "which memories
/// need their conversation?" — report a clean store it never checked.
#[test]
fn v18_leaves_rows_written_before_it_unmarked() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_pending(&conn, &MIGRATIONS[..17], 0).unwrap();
    assert_eq!(get_current_version(&conn).unwrap(), 17);

    conn.execute(
        "INSERT INTO knowledge_nodes (id, content, node_type, created_at, updated_at, last_accessed, tags)
         VALUES ('legacy-1', 'written before the gate existed', 'fact',
                 '2026-03-04T05:06:07+00:00', '2026-03-04T05:06:07+00:00',
                 '2026-04-01T00:00:00+00:00', '[]')",
        [],
    )
    .unwrap();

    apply_pending(&conn, &MIGRATIONS[..18], 17).unwrap();
    assert_eq!(get_current_version(&conn).unwrap(), 18);

    let (marker, findings): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT self_contained, self_contained_findings FROM knowledge_nodes WHERE id = 'legacy-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        marker, None,
        "an unchecked row must stay NULL, not be backfilled to 'clean'"
    );
    assert_eq!(findings, None);

    let has_index: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_nodes_flagged_self_contained'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        has_index,
        "the partial index is what keeps 'find the flagged memories' off a full scan"
    );
}

// ============================================================================
// V19 — code anchors
// ============================================================================

/// V19 exists, applies on top of V18 without touching `knowledge_nodes`, and
/// constrains `verdict` to the four states the resolver can produce. The
/// `CHECK` matters: every query in the feature groups by verdict, so a typo
/// would create a category nobody reads instead of failing.
#[test]
fn v19_adds_the_code_anchor_table_with_its_indexes_and_verdict_constraint() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_pending(&conn, &MIGRATIONS[..18], 0).unwrap();
    assert_eq!(get_current_version(&conn).unwrap(), 18);

    let applied = apply_pending(&conn, &MIGRATIONS[..19], 18).unwrap();
    assert_eq!(applied, 1, "only V19 is pending after V18");
    assert_eq!(get_current_version(&conn).unwrap(), 19);

    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('code_refs')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    for expected in [
        "id",
        "node_id",
        "repo_remote",
        "commit_sha",
        "path",
        "symbol",
        "hint_line",
        "content_hash",
        "resolved_at",
        "verdict",
    ] {
        assert!(
            columns.iter().any(|c| c == expected),
            "code_refs is missing {expected}: {columns:?}"
        );
    }

    // Both indexes the read path and the audit depend on.
    for index in ["idx_code_refs_node", "idx_code_refs_verdict"] {
        let present: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'index' AND name = ?1",
                rusqlite::params![index],
                |row| row.get(0),
            )
            .unwrap();
        assert!(present, "{index} is what keeps its query off a full scan");
    }

    // The table carries no foreign key, for the reason V17 gives: erasure names
    // every child table explicitly so the deletion stays auditable.
    let create_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'code_refs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        !create_sql.to_uppercase().contains("REFERENCES"),
        "a cascading foreign key would put the delete behind PRAGMA foreign_keys: {create_sql}"
    );

    conn.execute(
        "INSERT INTO code_refs (node_id, path, verdict) VALUES ('n1', 'src/lib.rs', 'fresh')",
        [],
    )
    .unwrap();
    assert!(
        conn.execute(
            "INSERT INTO code_refs (node_id, path, verdict) VALUES ('n1', 'src/lib.rs', 'repaired')",
            [],
        )
        .is_err(),
        "code_refs.verdict must be constrained to the four states"
    );
}

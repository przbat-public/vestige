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

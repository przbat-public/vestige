//! Migration runner — apply pending migrations to a connection.

use super::migration::Migration;
use super::registry::MIGRATIONS;

/// Get current schema version from database.
///
/// A missing `schema_version` table means a fresh database, so that one error
/// maps to 0. Every *other* read failure (locked database, wrong column type,
/// corrupted storage) is propagated: the previous `.or(Ok(0))` turned all of
/// them into "fresh", which made the runner replay V1..Vn against an existing
/// database and die on the first `duplicate column name` instead of reporting
/// the real problem.
pub fn get_current_version(conn: &rusqlite::Connection) -> rusqlite::Result<u32> {
    match conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    ) {
        Ok(version) => Ok(version),
        Err(err) if is_missing_schema_version_table(&err) => Ok(0),
        Err(err) => Err(err),
    }
}

fn is_missing_schema_version_table(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(_, Some(message))
            if message.contains("no such table: schema_version")
    )
}

/// Apply pending migrations
///
/// Each migration (except V7/V14 which require `VACUUM` outside a transaction)
/// is wrapped in an explicit transaction so a partial failure rolls back
/// cleanly instead of leaving the schema in an inconsistent state.
pub fn apply_migrations(conn: &rusqlite::Connection) -> rusqlite::Result<u32> {
    let current_version = get_current_version(conn)?;
    apply_pending(conn, MIGRATIONS, current_version)
}

/// Apply the migrations in `migrations` that are newer than `known_version`.
///
/// `known_version` is only a *hint* read before the write lock was taken: two
/// processes can boot against the same fresh database (CLI beside the MCP
/// server), and the loser reads 0 while the winner is still inside its first
/// migration transaction. Every ordinary migration therefore re-reads
/// `schema_version` **after** `BEGIN IMMEDIATE` and skips itself when the
/// version has already moved past it — without that, the loser replays
/// V1..Vn on an already-migrated database and start-up fails with
/// `duplicate column name`.
///
/// A database written by a *newer* build is refused instead of being opened:
/// columns this binary does not know about would be silently left unmanaged.
pub(crate) fn apply_pending(
    conn: &rusqlite::Connection,
    migrations: &[Migration],
    known_version: u32,
) -> rusqlite::Result<u32> {
    if let Some(latest) = migrations.last().map(|m| m.version)
        && known_version > latest
    {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!(
                "database schema version {} is newer than this build ({}); \
                 upgrade Vestige before opening it",
                known_version, latest
            )),
        ));
    }

    let mut current_version = known_version;
    let mut applied = 0;

    for migration in migrations {
        if migration.version <= current_version {
            continue;
        }

        if migration.version == 7 || migration.version == 14 {
            // These two cannot run inside a transaction (`VACUUM`), so they
            // take their own fresh reading right before doing the work.
            let version_now = get_current_version(conn)?;
            if migration.version <= version_now {
                current_version = version_now;
                continue;
            }
        }

        tracing::info!(
            "Applying migration v{}: {}",
            migration.version,
            migration.description
        );

        if migration.version == 7 {
            // V7 includes a VACUUM which cannot run inside a transaction.
            // Apply the SQL statements first, then page_size + VACUUM,
            // then bump schema_version explicitly — only when every step
            // succeeded. The earlier version of this branch bumped the
            // version *inside* `migration.up`, which meant a VACUUM
            // failure left the DB stuck at version=7 with no page_size
            // upgrade and no way to retry on next start. (Audit 2026-05-19.)
            conn.execute_batch(migration.up)?;
            conn.pragma_update(None, "page_size", 8192)?;
            conn.execute_batch("VACUUM;")?;
            conn.execute_batch(
                "UPDATE schema_version SET version = 7, applied_at = datetime('now');",
            )?;
            tracing::info!("Database page_size upgraded to 8192 via VACUUM");
        } else if migration.version == 14 {
            // V14: switch to auto_vacuum=INCREMENTAL. Same dance as V7 —
            // `PRAGMA auto_vacuum` only takes effect *after* a VACUUM,
            // and VACUUM cannot run inside a transaction. We toggle the
            // pragma, run VACUUM, *then* bump the schema version, so a
            // VACUUM failure leaves us at v13 and the migration retries
            // on next start.
            conn.pragma_update(None, "auto_vacuum", 2)?;
            conn.execute_batch("VACUUM;")?;
            conn.execute_batch(
                "UPDATE schema_version SET version = 14, applied_at = datetime('now');",
            )?;
            tracing::info!(
                "Database switched to auto_vacuum=INCREMENTAL — incremental_vacuum is now usable"
            );
        } else {
            // Wrap in a transaction so partial failures roll back atomically.
            conn.execute_batch("BEGIN IMMEDIATE;")?;

            // Re-read under the write lock: a competing process may have
            // applied this migration while we were waiting for the lock.
            let version_under_lock = get_current_version(conn)?;
            if migration.version <= version_under_lock {
                conn.execute_batch("COMMIT;")?;
                current_version = version_under_lock;
                continue;
            }

            match conn.execute_batch(migration.up) {
                Ok(()) => {
                    conn.execute_batch("COMMIT;")?;
                }
                Err(e) => {
                    tracing::error!(
                        "Migration v{} failed: {} — rolling back",
                        migration.version,
                        e
                    );
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(e);
                }
            }
        }

        current_version = get_current_version(conn)?;
        applied += 1;
    }

    Ok(applied)
}

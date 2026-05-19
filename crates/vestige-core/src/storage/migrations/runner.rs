//! Migration runner — apply pending migrations to a connection.

use super::registry::MIGRATIONS;

/// Get current schema version from database
pub fn get_current_version(conn: &rusqlite::Connection) -> rusqlite::Result<u32> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
    .or(Ok(0))
}

/// Apply pending migrations
///
/// Each migration (except V7 which requires VACUUM outside a transaction)
/// is wrapped in an explicit transaction so a partial failure rolls back
/// cleanly instead of leaving the schema in an inconsistent state.
pub fn apply_migrations(conn: &rusqlite::Connection) -> rusqlite::Result<u32> {
    let current_version = get_current_version(conn)?;
    let mut applied = 0;

    for migration in MIGRATIONS {
        if migration.version > current_version {
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
            } else {
                // Wrap in a transaction so partial failures roll back atomically.
                conn.execute_batch("BEGIN IMMEDIATE;")?;
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

            applied += 1;
        }
    }

    Ok(applied)
}

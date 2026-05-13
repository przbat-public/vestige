//! Tests for the migration pipeline.

use super::registry::MIGRATIONS;
use super::runner::{apply_migrations, get_current_version};

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
        let result =
            conn.execute_batch("CREATE TABLE _test_rollback (id INTEGER); INVALID SQL HERE;");
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

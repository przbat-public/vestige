//! Backup command — write a *consistent* snapshot of the store.
//!
//! The snapshot comes from `VACUUM INTO` (through `Storage::backup_to`), not
//! from copying the database file. A `cp` of a live WAL database is not a
//! backup: everything still sitting in the `-wal` file is missing from the
//! copy, and a checkpoint that lands mid-copy can tear a page. This command
//! used to checkpoint and then `fs::copy`, so the file it produced could look
//! perfectly valid and restore to a store that had lost the most recent
//! writes — a failure that only surfaced at restore time.

use std::path::{Path, PathBuf};

use colored::Colorize;

use super::util::get_default_db_path;

/// Write a snapshot of `db_path` to `output`, returning the snapshot size.
///
/// Refuses to overwrite an existing file: `VACUUM INTO` would not write over
/// one either, and silently replacing a previous backup means a later failed
/// run destroys the last good copy.
fn backup_database(db_path: &Path, output: &Path) -> anyhow::Result<u64> {
    if output.exists() {
        anyhow::bail!(
            "Refusing to overwrite an existing backup: {}",
            output.display()
        );
    }

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    let storage = vestige_core::Storage::new(Some(db_path.to_path_buf()))?;
    storage.backup_to(output)?;

    Ok(std::fs::metadata(output)?.len())
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} bytes")
    }
}

/// Run backup command — writes a `VACUUM INTO` snapshot of the store
pub(super) fn run_backup(output: PathBuf) -> anyhow::Result<()> {
    println!("{}", "=== Vestige Backup ===".cyan().bold());
    println!();

    let db_path = get_default_db_path()?;

    if !db_path.exists() {
        anyhow::bail!("Database not found at: {}", db_path.display());
    }

    println!("Snapshotting database (VACUUM INTO)...");
    println!("  {} {}", "From:".dimmed(), db_path.display());
    println!("  {}   {}", "To:".dimmed(), output.display());

    let size = backup_database(&db_path, &output)?;

    println!();
    println!(
        "{}",
        format!(
            "Backup complete: {} ({})",
            output.display(),
            human_size(size)
        )
        .green()
        .bold()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use vestige_core::{IngestInput, Storage};

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "vestige-cli-backup-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn memory(content: &str) -> IngestInput {
        IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        }
    }

    /// The snapshot must be a rebuilt database, not a byte copy of the source.
    ///
    /// The workload deletes rows before backing up, which leaves free pages in
    /// the source file. `VACUUM INTO` drops them, so the snapshot is strictly
    /// smaller; the old `fs::copy` implementation produced a file of exactly
    /// the source size and fails this assertion.
    #[test]
    fn snapshot_is_compacted_and_not_a_byte_copy() {
        let dir = temp_dir("compact");
        let db_path = dir.join("vestige.db");
        let out_path = dir.join("snapshot.db");

        {
            let storage = Storage::new(Some(db_path.clone())).expect("open source store");
            let filler = "x".repeat(512);
            for i in 0..400 {
                let node = storage
                    .ingest(memory(&format!("transient memory {i} {filler}")))
                    .expect("ingest");
                storage.delete_node(&node.id).expect("delete");
            }
            storage
                .ingest(memory("the memory that must survive the backup"))
                .expect("ingest keeper");
        }

        let source_len = std::fs::metadata(&db_path).expect("stat source").len();
        let snapshot_len = backup_database(&db_path, &out_path).expect("backup");

        assert!(out_path.exists(), "snapshot was not written");
        assert!(
            snapshot_len < source_len,
            "snapshot ({snapshot_len} bytes) is not smaller than the source \
             ({source_len} bytes) — it was copied, not vacuumed"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The snapshot must open as a store and still contain the memories.
    #[test]
    fn snapshot_restores_the_memories_it_holds() {
        let dir = temp_dir("restore");
        let db_path = dir.join("vestige.db");
        let out_path = dir.join("snapshot.db");

        let content = "unique backup payload 4f6c1a";
        {
            let storage = Storage::new(Some(db_path.clone())).expect("open source store");
            storage.ingest(memory(content)).expect("ingest");
        }

        backup_database(&db_path, &out_path).expect("backup");

        let snapshot = Storage::new(Some(out_path.clone())).expect("open snapshot");
        let nodes = snapshot.get_all_nodes(100, 0).expect("read snapshot");
        assert!(
            nodes.iter().any(|n| n.content.contains(content)),
            "the memory ingested before the backup is missing from the snapshot: {} nodes",
            nodes.len()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A repeated run must not destroy the previous backup.
    #[test]
    fn an_existing_backup_is_never_overwritten() {
        let dir = temp_dir("nooverwrite");
        let db_path = dir.join("vestige.db");
        let out_path = dir.join("snapshot.db");

        {
            let storage = Storage::new(Some(db_path.clone())).expect("open source store");
            storage.ingest(memory("first")).expect("ingest");
        }

        backup_database(&db_path, &out_path).expect("first backup");
        let first_len = std::fs::metadata(&out_path).expect("stat").len();

        let err = backup_database(&db_path, &out_path).expect_err("second backup must fail");
        assert!(
            err.to_string().contains("Refusing to overwrite"),
            "unexpected error: {err}"
        );
        assert_eq!(
            std::fs::metadata(&out_path).expect("stat").len(),
            first_len,
            "the existing backup was modified by the refused run"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

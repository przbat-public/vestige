//! Backup command — create a full SQLite database backup.

use std::path::PathBuf;

use colored::Colorize;
use vestige_core::Storage;

use super::util::get_default_db_path;

/// Run backup command - copies the SQLite database file
pub(super) fn run_backup(output: PathBuf) -> anyhow::Result<()> {
    println!("{}", "=== Vestige Backup ===".cyan().bold());
    println!();

    let db_path = get_default_db_path()?;

    if !db_path.exists() {
        anyhow::bail!("Database not found at: {}", db_path.display());
    }

    // Open storage to flush WAL before copying
    println!("Flushing WAL checkpoint...");
    {
        let storage = Storage::new(None)?;
        // get_stats triggers a read so the connection is active, then drop flushes
        let _ = storage.get_stats()?;
    }

    // Also flush WAL directly via a separate connection for safety
    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }

    // Create parent directories if needed
    if let Some(parent) = output.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    // Copy the database file
    println!("Copying database...");
    println!("  {} {}", "From:".dimmed(), db_path.display());
    println!("  {}   {}", "To:".dimmed(), output.display());

    std::fs::copy(&db_path, &output)?;

    let file_size = std::fs::metadata(&output)?.len();
    let size_display = if file_size >= 1024 * 1024 {
        format!("{:.2} MB", file_size as f64 / (1024.0 * 1024.0))
    } else if file_size >= 1024 {
        format!("{:.1} KB", file_size as f64 / 1024.0)
    } else {
        format!("{} bytes", file_size)
    };

    println!();
    println!(
        "{}",
        format!("Backup complete: {} ({})", output.display(), size_display)
            .green()
            .bold()
    );

    Ok(())
}

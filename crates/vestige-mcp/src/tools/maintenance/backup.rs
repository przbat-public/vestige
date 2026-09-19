//! `backup` tool — creates a timestamped JSON snapshot of the database.

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use vestige_core::Storage;

pub fn backup_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

pub async fn execute_backup(storage: &Arc<Storage>, _args: Option<Value>) -> Result<Value, String> {
    // Backup path — inside the Vestige data directory (`…/com.vestige.core/backups`),
    // created 0700. It used to be the data directory's *parent*: a directory shared
    // with every other application on the machine, outside the 0600/0700 hardening
    // that protects the live database.
    let backup_dir = super::artifact_dir("backups")?;

    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let backup_path = backup_dir.join(format!("vestige-{}.db", timestamp));

    // Use VACUUM INTO for a consistent backup (handles WAL properly).
    // backup_to is a long-running blocking operation (single-digit seconds
    // on multi-GB DBs), so push it to the blocking pool — otherwise we
    // freeze every async task on the runtime worker that picked us up.
    {
        let storage_clone = storage.clone();
        let backup_path_clone = backup_path.clone();
        tokio::task::spawn_blocking(move || storage_clone.backup_to(&backup_path_clone))
            .await
            .map_err(|e| format!("Backup task panicked: {}", e))?
            .map_err(|e| format!("Failed to create backup: {}", e))?;
    }

    // `VACUUM INTO` creates the file itself, so the mode cannot be passed at
    // creation time; tighten it now that it exists. The 0700 parent directory
    // already bounds the exposure window.
    super::restrict_to_owner(&backup_path);

    let file_size = std::fs::metadata(&backup_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(serde_json::json!({
        "tool": "backup",
        "path": backup_path.display().to_string(),
        "sizeBytes": file_size,
        "timestamp": Utc::now().to_rfc3339(),
    }))
}

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
    // Determine backup path
    let vestige_dir = directories::ProjectDirs::from("com", "vestige", "core")
        .ok_or("Could not determine data directory")?;
    let backup_dir = vestige_dir
        .data_dir()
        .parent()
        .unwrap_or(vestige_dir.data_dir())
        .join("backups");

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

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

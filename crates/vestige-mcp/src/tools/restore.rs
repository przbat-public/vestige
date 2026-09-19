//! Restore Tool
//!
//! Restores memories from a JSON backup file.
//! Previously CLI-only (vestige-restore binary), now available as an MCP tool
//! so Claude Code can trigger restores directly.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use vestige_core::{IngestInput, Storage};

/// Input schema for restore tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Path to the backup JSON file to restore from"
            },
            "confirmed": {
                "type": "boolean",
                "description": "Required to be `true`. Restore reingests every memory in the backup and can shadow or overwrite live state — there is no dry-run mode, so the gate is unconditional. Mirrors the MCP `elicitation/create` flow for transports that cannot prompt mid-call.",
                "default": false
            }
        },
        "required": ["path", "confirmed"]
    })
}

#[derive(Debug, Deserialize)]
struct RestoreArgs {
    path: String,
}

#[derive(Deserialize)]
struct BackupWrapper {
    #[serde(rename = "type")]
    _type: String,
    text: String,
}

#[derive(Deserialize)]
struct RecallResult {
    results: Vec<MemoryBackup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryBackup {
    content: String,
    #[serde(alias = "node_type")]
    node_type: Option<String>,
    tags: Option<Vec<String>>,
    source: Option<String>,
}

pub async fn execute(storage: &Arc<Storage>, args: Option<Value>) -> Result<Value, String> {
    let raw = args.clone().unwrap_or_else(|| serde_json::json!({}));

    // Destructive-op gate. Restore has no dry-run; the only safe call is
    // an explicitly confirmed one. See `tools::common`.
    if !crate::tools::common::is_confirmed(&raw) {
        return Err(crate::tools::common::missing_confirmation_error(
            "restore",
            "Restore reingests every memory in the backup and may shadow or overwrite live state.",
        ));
    }

    let args: RestoreArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    let path = std::path::Path::new(&args.path);
    if !path.exists() {
        return Err(format!("Backup file not found: {}", args.path));
    }

    // Read and parse backup
    let backup_content =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read backup: {}", e))?;

    // Try parsing as wrapped format first (MCP response wrapper),
    // then fall back to direct RecallResult
    let memories: Vec<MemoryBackup> =
        if let Ok(wrapper) = serde_json::from_str::<Vec<BackupWrapper>>(&backup_content) {
            if let Some(first) = wrapper.first() {
                let recall: RecallResult = serde_json::from_str(&first.text)
                    .map_err(|e| format!("Failed to parse backup contents: {}", e))?;
                recall.results
            } else {
                return Err("Empty backup file".to_string());
            }
        } else if let Ok(recall) = serde_json::from_str::<RecallResult>(&backup_content) {
            recall.results
        } else if let Ok(nodes) = serde_json::from_str::<Vec<MemoryBackup>>(&backup_content) {
            nodes
        } else {
            return Err(
            "Unrecognized backup format. Expected MCP wrapper, RecallResult, or array of memories."
                .to_string(),
        );
        };

    let total = memories.len();
    if total == 0 {
        return Ok(serde_json::json!({
            "tool": "restore",
            "success": true,
            "restored": 0,
            "total": 0,
            "message": "No memories found in backup file.",
        }));
    }

    // Restore is essentially an iterated INSERT — run the whole loop on
    // the blocking pool so the async runtime stays responsive even when
    // the backup contains tens of thousands of memories.
    let storage_clone = storage.clone();
    let memories_for_task = memories;
    let (success_count, error_count) = tokio::task::spawn_blocking(move || {
        let mut ok = 0_usize;
        let mut err = 0_usize;
        for memory in &memories_for_task {
            let input = IngestInput {
                content: memory.content.clone(),
                node_type: memory
                    .node_type
                    .clone()
                    .unwrap_or_else(|| "fact".to_string()),
                source: memory.source.clone(),
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: memory.tags.clone().unwrap_or_default(),
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            };
            match storage_clone.ingest(input) {
                Ok(_) => ok += 1,
                Err(_) => err += 1,
            }
        }
        (ok, err)
    })
    .await
    .map_err(|e| format!("restore task panicked: {}", e))?;

    Ok(serde_json::json!({
        "tool": "restore",
        "success": true,
        "restored": success_count,
        "errors": error_count,
        "total": total,
        "message": format!("Restored {}/{} memories from backup.", success_count, total),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    fn write_temp_file(dir: &TempDir, name: &str, content: &str) -> String {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path.to_string_lossy().to_string()
    }

    /// Every legitimate restore call must opt in via `confirmed: true`.
    /// Tests below funnel through this helper so we don't duplicate the
    /// pattern thirteen times.
    fn restore_args(path: &str) -> serde_json::Value {
        serde_json::json!({ "path": path, "confirmed": true })
    }

    #[test]
    fn test_schema_has_required_fields() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["path"].is_object());
        assert!(s["properties"]["confirmed"].is_object());
        let required = s["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("path")));
        assert!(
            required.contains(&serde_json::json!("confirmed")),
            "restore must require `confirmed`; without it the destructive-op gate has no schema-level signal"
        );
    }

    #[tokio::test]
    async fn test_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        assert!(result.is_err());
        // The confirmation gate fires before deserialization, so the
        // error mentions the operation, not "Missing arguments". This is
        // intentional — the destructive guard is the FIRST thing a
        // caller should fix.
        let err = result.unwrap_err();
        assert!(err.contains("`restore`"), "got: {err}");
    }

    #[tokio::test]
    async fn test_missing_path_field_fails_after_gate() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, Some(serde_json::json!({ "confirmed": true }))).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid arguments"));
    }

    // ---- Destructive-op confirmation gate ---------------------------

    #[tokio::test]
    async fn test_restore_without_confirmation_is_blocked() {
        let (storage, dir) = test_storage().await;
        let backup = serde_json::json!([{ "content": "should not be restored" }]);
        let path = write_temp_file(&dir, "blocked.json", &backup.to_string());

        // No `confirmed` flag — even with a valid backup we must refuse.
        let args = serde_json::json!({ "path": path });
        let result = execute(&storage, Some(args)).await;
        let err = result.expect_err("restore without confirmation must error");
        assert!(err.contains("`restore`"), "must name the op: {err}");
        assert!(
            err.contains("confirmed: true"),
            "must tell the agent how to retry: {err}"
        );
        assert!(
            err.contains("requestedSchema"),
            "must include the elicitation schema clients translate into a prompt: {err}"
        );

        // Make sure NOTHING was ingested.
        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.total_nodes, 0, "blocked restore must not ingest");
    }

    #[tokio::test]
    async fn test_restore_with_confirmation_false_is_blocked() {
        let (storage, dir) = test_storage().await;
        let backup = serde_json::json!([{ "content": "still no" }]);
        let path = write_temp_file(&dir, "no.json", &backup.to_string());
        let args = serde_json::json!({ "path": path, "confirmed": false });
        let result = execute(&storage, Some(args)).await;
        assert!(
            result.is_err(),
            "explicit confirmed=false must remain blocked"
        );
    }

    // ---- Original happy-path tests (now with `confirmed: true`) -----

    #[tokio::test]
    async fn test_nonexistent_file_fails() {
        let (storage, _dir) = test_storage().await;
        let args = restore_args("/tmp/does_not_exist_vestige_test.json");
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_malformed_json_fails() {
        let (storage, dir) = test_storage().await;
        let path = write_temp_file(&dir, "bad.json", "this is not json {{{");
        let result = execute(&storage, Some(restore_args(&path))).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unrecognized backup format"));
    }

    #[tokio::test]
    async fn test_restore_direct_array_format() {
        let (storage, dir) = test_storage().await;
        let backup = serde_json::json!([
            { "content": "Memory one", "nodeType": "fact", "tags": ["test"] },
            { "content": "Memory two", "nodeType": "concept" }
        ]);
        let path = write_temp_file(&dir, "backup.json", &backup.to_string());
        let result = execute(&storage, Some(restore_args(&path))).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["tool"], "restore");
        assert_eq!(value["success"], true);
        assert_eq!(value["restored"], 2);
        assert_eq!(value["errors"], 0);
        assert_eq!(value["total"], 2);
    }

    #[tokio::test]
    async fn test_restore_recall_result_format() {
        let (storage, dir) = test_storage().await;
        let backup = serde_json::json!({
            "results": [
                { "content": "Recall memory one" },
                { "content": "Recall memory two" },
                { "content": "Recall memory three" }
            ]
        });
        let path = write_temp_file(&dir, "recall.json", &backup.to_string());
        let result = execute(&storage, Some(restore_args(&path))).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["restored"], 3);
        assert_eq!(value["total"], 3);
    }

    #[tokio::test]
    async fn test_restore_empty_results_array() {
        let (storage, dir) = test_storage().await;
        let backup = serde_json::json!({ "results": [] });
        let path = write_temp_file(&dir, "empty.json", &backup.to_string());
        let result = execute(&storage, Some(restore_args(&path))).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["restored"], 0);
        assert_eq!(value["total"], 0);
    }

    #[tokio::test]
    async fn test_restore_empty_array_returns_error() {
        let (storage, dir) = test_storage().await;
        let path = write_temp_file(&dir, "empty_arr.json", "[]");
        let result = execute(&storage, Some(restore_args(&path))).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Empty backup file"));
    }

    #[tokio::test]
    async fn test_restore_defaults_node_type_to_fact() {
        let (storage, dir) = test_storage().await;
        let backup = serde_json::json!([{ "content": "No type specified" }]);
        let path = write_temp_file(&dir, "notype.json", &backup.to_string());
        let result = execute(&storage, Some(restore_args(&path))).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["restored"], 1);
    }
}

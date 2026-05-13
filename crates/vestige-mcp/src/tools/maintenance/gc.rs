//! `gc` tool — soft-delete weak/expired memories (with dry-run).

use std::sync::Arc;

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;

use vestige_core::Storage;

pub fn gc_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "min_retention": {
                "type": "number",
                "description": "Delete memories with retention below this threshold (default: 0.1)",
                "default": 0.1,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "max_age_days": {
                "type": "integer",
                "description": "Only delete memories older than this many days (optional additional filter)",
                "minimum": 1
            },
            "dry_run": {
                "type": "boolean",
                "description": "If true (default), only report what would be deleted without actually deleting",
                "default": true
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GcArgs {
    #[serde(alias = "min_retention")]
    min_retention: Option<f64>,
    #[serde(alias = "max_age_days")]
    max_age_days: Option<u64>,
    #[serde(alias = "dry_run")]
    dry_run: Option<bool>,
}

/// Garbage collection tool
pub async fn execute_gc(storage: &Arc<Storage>, args: Option<Value>) -> Result<Value, String> {
    let args: GcArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => GcArgs {
            min_retention: None,
            max_age_days: None,
            dry_run: None,
        },
    };

    let min_retention = args.min_retention.unwrap_or(0.1).clamp(0.0, 1.0);
    let max_age_days = args.max_age_days;
    let dry_run = args.dry_run.unwrap_or(true); // Default to dry_run for safety

    let now = Utc::now();

    // Fetch all nodes (capped at 100K to prevent OOM). Paginated reads on
    // a multi-GB DB can easily take seconds — never on the reactor.
    let storage_fetch = storage.clone();
    let all_nodes = tokio::task::spawn_blocking(
        move || -> Result<Vec<vestige_core::KnowledgeNode>, vestige_core::StorageError> {
            let mut all = Vec::new();
            let page_size = 500;
            let max_nodes = 100_000;
            let mut offset = 0;
            loop {
                let batch = storage_fetch.get_all_nodes(page_size, offset)?;
                let batch_len = batch.len();
                all.extend(batch);
                if batch_len < page_size as usize || all.len() >= max_nodes {
                    break;
                }
                offset += page_size;
            }
            Ok(all)
        },
    )
    .await
    .map_err(|e| format!("GC fetch task panicked: {}", e))?
    .map_err(|e| e.to_string())?;

    // Find candidates
    let candidates: Vec<&vestige_core::KnowledgeNode> = all_nodes
        .iter()
        .filter(|node| {
            if node.retention_strength >= min_retention {
                return false;
            }
            if let Some(max_days) = max_age_days {
                let age_days = (now - node.created_at).num_days();
                if age_days < 0 || (age_days as u64) < max_days {
                    return false;
                }
            }
            true
        })
        .collect();

    let candidate_count = candidates.len();

    // Build sample for display
    let sample: Vec<Value> = candidates
        .iter()
        .take(10)
        .map(|node| {
            let age_days = (now - node.created_at).num_days();
            let content_preview: String = {
                let preview: String = node.content.chars().take(60).collect();
                if preview.len() < node.content.len() {
                    format!("{}...", preview)
                } else {
                    preview
                }
            };
            serde_json::json!({
                "id": &node.id[..8.min(node.id.len())],
                "retention": node.retention_strength,
                "ageDays": age_days,
                "contentPreview": content_preview,
            })
        })
        .collect();

    if dry_run {
        return Ok(serde_json::json!({
            "tool": "gc",
            "dryRun": true,
            "minRetention": min_retention,
            "maxAgeDays": max_age_days,
            "candidateCount": candidate_count,
            "totalMemories": all_nodes.len(),
            "sample": sample,
            "message": format!("{} memories would be deleted. Set dry_run=false to delete.", candidate_count),
        }));
    }

    // Perform actual deletion on the blocking pool. Each DELETE is fast,
    // but doing tens of thousands of them on the reactor would starve every
    // other request on this worker thread.
    let ids: Vec<String> = candidates.iter().map(|n| n.id.clone()).collect();
    let storage_delete = storage.clone();
    let (deleted, errors) = tokio::task::spawn_blocking(move || {
        let mut deleted = 0usize;
        let mut errors = 0usize;
        for id in &ids {
            match storage_delete.delete_node(id) {
                Ok(true) => deleted += 1,
                Ok(false) => errors += 1,
                Err(_) => errors += 1,
            }
        }
        (deleted, errors)
    })
    .await
    .map_err(|e| format!("GC delete task panicked: {}", e))?;

    Ok(serde_json::json!({
        "tool": "gc",
        "dryRun": false,
        "minRetention": min_retention,
        "maxAgeDays": max_age_days,
        "deleted": deleted,
        "errors": errors,
        "totalBefore": all_nodes.len(),
        "totalAfter": all_nodes.len() - deleted,
    }))
}

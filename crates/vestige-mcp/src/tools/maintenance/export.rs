//! `export` tool — dumps memories to JSON/Markdown filtered by tag/date.

use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use serde_json::Value;

use vestige_core::Storage;

pub fn export_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "format": {
                "type": "string",
                "description": "Export format: 'json' (default) or 'jsonl'",
                "enum": ["json", "jsonl"],
                "default": "json"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Filter by tags (ALL must match)"
            },
            "since": {
                "type": "string",
                "description": "Only export memories created after this date (YYYY-MM-DD)"
            },
            "path": {
                "type": "string",
                "description": "Custom filename (not path). File is saved in ~/.vestige/exports/. Default: memories-{timestamp}.{format}"
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportArgs {
    format: Option<String>,
    tags: Option<Vec<String>>,
    since: Option<String>,
    path: Option<String>,
}

pub async fn execute_export(storage: &Arc<Storage>, args: Option<Value>) -> Result<Value, String> {
    let args: ExportArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => ExportArgs {
            format: None,
            tags: None,
            since: None,
            path: None,
        },
    };

    let format = args.format.unwrap_or_else(|| "json".to_string());
    if format != "json" && format != "jsonl" {
        return Err(format!(
            "Invalid format '{}'. Must be 'json' or 'jsonl'.",
            format
        ));
    }

    // Parse since date
    let since_date = match &args.since {
        Some(date_str) => {
            let naive = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{}': {}. Use YYYY-MM-DD.", date_str, e))?;
            Some(
                naive
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight is always valid")
                    .and_utc(),
            )
        }
        None => None,
    };

    let tag_filter: Vec<String> = args.tags.unwrap_or_default();

    // Fetch all nodes (capped at 100K to prevent OOM). Same offload pattern
    // as `execute_gc` — paginated reads can run for seconds.
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
    .map_err(|e| format!("Export fetch task panicked: {}", e))?
    .map_err(|e| e.to_string())?;

    // Apply filters
    let filtered: Vec<&vestige_core::KnowledgeNode> = all_nodes
        .iter()
        .filter(|node| {
            if since_date
                .as_ref()
                .is_some_and(|since_dt| node.created_at < *since_dt)
            {
                return false;
            }
            if !tag_filter.is_empty() {
                for tag in &tag_filter {
                    if !node.tags.iter().any(|t| t == tag) {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    // Determine export path — always constrained to vestige exports directory
    let vestige_dir = directories::ProjectDirs::from("com", "vestige", "core")
        .ok_or("Could not determine data directory")?;
    let export_dir = vestige_dir
        .data_dir()
        .parent()
        .unwrap_or(vestige_dir.data_dir())
        .join("exports");
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("Failed to create export directory: {}", e))?;

    let export_path = match args.path {
        Some(ref p) => {
            // Only allow a filename, not a path — prevent path traversal
            let filename = std::path::Path::new(p)
                .file_name()
                .ok_or("Invalid export filename: must be a simple filename, not a path")?;
            let name_str = filename.to_str().ok_or("Invalid filename encoding")?;
            if name_str.contains("..") {
                return Err("Invalid export filename: '..' not allowed".to_string());
            }
            export_dir.join(filename)
        }
        None => {
            let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
            export_dir.join(format!("memories-{}.{}", timestamp, format))
        }
    };

    // Write export
    let file = std::fs::File::create(&export_path)
        .map_err(|e| format!("Failed to create export file: {}", e))?;
    let mut writer = std::io::BufWriter::new(file);

    use std::io::Write;
    match format.as_str() {
        "json" => {
            serde_json::to_writer_pretty(&mut writer, &filtered)
                .map_err(|e| format!("Failed to write JSON: {}", e))?;
            writer.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        "jsonl" => {
            for node in &filtered {
                serde_json::to_writer(&mut writer, node)
                    .map_err(|e| format!("Failed to write JSONL: {}", e))?;
                writer.write_all(b"\n").map_err(|e| e.to_string())?;
            }
        }
        _ => unreachable!(),
    }
    writer.flush().map_err(|e| e.to_string())?;

    let file_size = std::fs::metadata(&export_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(serde_json::json!({
        "tool": "export",
        "path": export_path.display().to_string(),
        "format": format,
        "memoriesExported": filtered.len(),
        "totalMemories": all_nodes.len(),
        "sizeBytes": file_size,
    }))
}

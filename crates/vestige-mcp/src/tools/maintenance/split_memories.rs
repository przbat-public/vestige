//! `split_memories` tool — find compound memories for agent-assisted splitting.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use vestige_core::Storage;

pub fn split_memories_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "min_length": {
                "type": "integer",
                "description": "Minimum content length to consider (default: 300). Shorter memories are always atomic.",
                "default": 300,
                "minimum": 100
            },
            "limit": {
                "type": "integer",
                "description": "Max number of compound memories to return (default: 20)",
                "default": 20,
                "minimum": 1,
                "maximum": 100
            },
            "dry_run": {
                "type": "boolean",
                "description": "If true (default), only report compound memories without deleting. If false, delete compound memories after reporting (you must re-ingest as atomic items) — and `confirmed: true` is then required.",
                "default": true
            },
            "confirmed": {
                "type": "boolean",
                "description": "Required to be `true` when `dry_run` is `false`. Deletion is irreversible: the compound memory is dropped and its atomic replacements do not exist yet, so every destructive call has to be explicit. Mirrors the gate on `gc` and `restore`.",
                "default": false
            }
        }
    })
}

#[derive(Deserialize)]
struct SplitMemoriesArgs {
    min_length: Option<usize>,
    limit: Option<usize>,
    dry_run: Option<bool>,
}

pub async fn execute_split_memories(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    let raw_args = args.clone().unwrap_or_else(|| serde_json::json!({}));
    let args: SplitMemoriesArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => SplitMemoriesArgs {
            min_length: None,
            limit: None,
            dry_run: None,
        },
    };

    let min_length = args.min_length.unwrap_or(300);
    let limit = args.limit.unwrap_or(20);
    let dry_run = args.dry_run.unwrap_or(true);

    // Destructive-op gate, same contract as `gc` and `restore`: a dry run needs nothing,
    // a real deletion needs an explicit confirmation. `dry_run: false` used to delete
    // every compound memory it found without any acknowledgement, which contradicted the
    // project's own rule that nothing removes memories unless the user says so.
    if !dry_run && !crate::tools::common::is_confirmed(&raw_args) {
        return Err(crate::tools::common::missing_confirmation_error(
            "split_memories",
            "split_memories with dry_run:false deletes every compound memory it reports,              and the atomic replacements have to be re-ingested afterwards.",
        ));
    }

    let storage_fetch = storage.clone();
    let all_nodes = tokio::task::spawn_blocking(move || storage_fetch.get_all_nodes(500, 0))
        .await
        .map_err(|e| format!("get_all_nodes task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    let mut compound_memories: Vec<Value> = Vec::new();

    for node in &all_nodes {
        if node.content.len() < min_length {
            continue;
        }
        if let Some(reason) = detect_compound_content_reason(&node.content) {
            compound_memories.push(serde_json::json!({
                "id": node.id,
                "content": if node.content.len() > 500 {
                    format!("{}...", &node.content[..node.content.char_indices().take_while(|(i, _)| *i < 500).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(500)])
                } else {
                    node.content.clone()
                },
                "content_length": node.content.len(),
                "node_type": node.node_type,
                "created_at": node.created_at.to_rfc3339(),
                "reason": reason,
                "action": "Split this memory into separate atomic memories using smart_ingest batch mode, then delete the original."
            }));

            if compound_memories.len() >= limit {
                break;
            }
        }
    }

    let mut deleted_ids: Vec<String> = Vec::new();
    if !dry_run {
        let ids: Vec<String> = compound_memories
            .iter()
            .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
            .collect();
        let storage_delete = storage.clone();
        deleted_ids = tokio::task::spawn_blocking(move || {
            let mut ok = Vec::new();
            for id in &ids {
                if storage_delete.delete_node(id).is_ok() {
                    ok.push(id.clone());
                }
            }
            ok
        })
        .await
        .map_err(|e| format!("Split delete task panicked: {}", e))?;
    }

    Ok(serde_json::json!({
        "total_scanned": all_nodes.len(),
        "compound_found": compound_memories.len(),
        "dry_run": dry_run,
        "deleted": deleted_ids.len(),
        "deleted_ids": deleted_ids,
        "memories": compound_memories,
        "instruction": if compound_memories.is_empty() {
            "All memories are atomic. No splitting needed.".to_string()
        } else if dry_run {
            format!(
                "Found {} compound memories. To fix: for each memory above, read the full content with memory(action='get'), \
                 split into atomic facts, then smart_ingest as batch items, and finally memory(action='delete') the original. \
                 Or re-run with dry_run=false to auto-delete originals.",
                compound_memories.len()
            )
        } else {
            format!(
                "Deleted {} compound memories. Now re-ingest each as atomic items using smart_ingest batch mode. \
                 Each original memory's content is shown above — split each into separate facts/decisions/events.",
                deleted_ids.len()
            )
        }
    }))
}

pub(super) fn detect_compound_content_reason(content: &str) -> Option<String> {
    let len = content.len();
    if len < 300 {
        return None;
    }

    let mut signals: Vec<&str> = Vec::new();

    let lines: Vec<&str> = content.lines().collect();
    let paragraph_count = content
        .split("\n\n")
        .filter(|p| p.trim().len() > 30)
        .count();
    if paragraph_count >= 3 {
        signals.push("multiple paragraphs");
    }

    let speaker_count = lines
        .iter()
        .filter(|l| {
            let trimmed = l.trim();
            if let Some(colon_pos) = trimmed.find(':') {
                let before = &trimmed[..colon_pos];
                colon_pos < 40
                    && !before.is_empty()
                    && before
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
                    && trimmed.len() > colon_pos + 5
            } else {
                false
            }
        })
        .count();
    if speaker_count >= 3 {
        signals.push("conversation transcript");
    }

    let bullet_count = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("- ")
                || t.starts_with("* ")
                || t.starts_with("• ")
                || (t.len() > 3
                    && t.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && (t.contains(". ") || t.contains(") ")))
        })
        .count();
    if bullet_count >= 4 {
        signals.push("multi-item list");
    }

    let topic_indicators = [
        "also,",
        "additionally,",
        "on another note",
        "separately,",
        "moving on",
        "another thing",
        "by the way",
        "btw,",
        "oh and",
        "also worth noting",
        "furthermore,",
    ];
    let topic_shifts = lines
        .iter()
        .filter(|l| {
            let lower = l.to_lowercase();
            topic_indicators.iter().any(|ind| lower.contains(ind))
        })
        .count();
    if topic_shifts >= 2 {
        signals.push("topic-shift phrases");
    }

    if signals.is_empty() {
        return None;
    }

    Some(signals.join(", "))
}

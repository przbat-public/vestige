//! Temporal fact versioning tool.
//!
//! Queries and manages time-sensitive knowledge: which facts are currently
//! valid, what's expired, and how knowledge evolved over time.
//!
//! Based on: Graphiti temporal knowledge graphs (Zep 2024),
//! bi-temporal database theory (Snodgrass 1999).

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use vestige_core::Storage;

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["current", "expired", "history", "invalidate"],
                "description": "current: valid-now facts. expired: no-longer-valid. history: evolution of a topic. invalidate: mark a fact as no-longer-valid."
            },
            "topic": {
                "type": "string",
                "description": "Topic to query (required for history/current/expired, optional for invalidate)"
            },
            "memory_id": {
                "type": "string",
                "description": "Memory ID to invalidate (required for 'invalidate' action)"
            },
            "limit": {
                "type": "integer",
                "description": "Max results (default: 20)",
                "default": 20
            }
        },
        "required": ["action"]
    })
}

pub async fn execute(storage: &Arc<Storage>, args: Option<Value>) -> Result<Value, String> {
    let args = args.ok_or("Arguments required")?;
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or("action is required")?;
    let topic = args.get("topic").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20) as i32;

    match action {
        "current" => {
            let now = Utc::now();
            let storage_clone = storage.clone();
            let topic_owned = topic.map(|s| s.to_string());
            let memories: Vec<vestige_core::KnowledgeNode> =
                tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
                    if let Some(query) = topic_owned {
                        let results = storage_clone
                            .hybrid_search(&query, limit, 0.3, 0.7)
                            .map_err(|e| e.to_string())?;
                        Ok(results
                            .into_iter()
                            .filter_map(|r| storage_clone.get_node(&r.node.id).ok().flatten())
                            .filter(|n| {
                                // is_none_or: true when the bound is open OR the bound holds.
                                // Replaces the older `is_none() || .unwrap() <op> bound` idiom
                                // (stable in Rust 1.82) and removes the latent panic surface
                                // even though the surrounding `is_none()` short-circuits in
                                // practice.
                                let from_ok = n.valid_from.is_none_or(|t| t <= now);
                                let until_ok = n.valid_until.is_none_or(|t| t > now);
                                from_ok && until_ok
                            })
                            .collect())
                    } else {
                        Ok(storage_clone
                            .query_time_range(None, Some(now), limit)
                            .map_err(|e| e.to_string())?
                            .into_iter()
                            .filter(|n| n.valid_until.is_none_or(|t| t > now))
                            .collect())
                    }
                })
                .await
                .map_err(|e| format!("temporal current task panicked: {}", e))??;

            Ok(serde_json::json!({
                "action": "current",
                "topic": topic,
                "count": memories.len(),
                "memories": memories.iter().map(|m| serde_json::json!({
                    "id": m.id,
                    "content": m.content,
                    "valid_from": m.valid_from.map(|d| d.to_rfc3339()),
                    "valid_until": m.valid_until.map(|d| d.to_rfc3339()),
                    "retention": format!("{:.2}", m.retention_strength),
                    "tags": m.tags,
                })).collect::<Vec<_>>()
            }))
        }

        "expired" => {
            let now = Utc::now();
            let storage_clone = storage.clone();
            let topic_owned = topic.map(|s| s.to_string());
            let all: Vec<vestige_core::KnowledgeNode> =
                tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
                    if let Some(query) = topic_owned {
                        let results = storage_clone
                            .hybrid_search(&query, limit * 3, 0.3, 0.7)
                            .map_err(|e| e.to_string())?;
                        Ok(results
                            .into_iter()
                            .filter_map(|r| storage_clone.get_node(&r.node.id).ok().flatten())
                            .collect())
                    } else {
                        storage_clone
                            .get_all_nodes(limit * 3, 0)
                            .map_err(|e| e.to_string())
                    }
                })
                .await
                .map_err(|e| format!("temporal expired task panicked: {}", e))??;

            let expired: Vec<_> = all
                .into_iter()
                .filter(|n| n.valid_until.is_some_and(|t| t <= now))
                .take(limit as usize)
                .collect();

            Ok(serde_json::json!({
                "action": "expired",
                "topic": topic,
                "count": expired.len(),
                "memories": expired.iter().map(|m| serde_json::json!({
                    "id": m.id,
                    "content": truncate(&m.content, 200),
                    "valid_from": m.valid_from.map(|dt| dt.to_rfc3339()),
                    "expired_at": m.valid_until.map(|dt| dt.to_rfc3339()),
                    "days_expired": m.valid_until.map(|dt| (now - dt).num_days()),
                    "tags": m.tags,
                })).collect::<Vec<_>>()
            }))
        }

        "history" => {
            let query = topic.ok_or("topic is required for 'history' action")?;
            let storage_clone = storage.clone();
            let query_owned = query.to_string();
            let mut memories: Vec<vestige_core::KnowledgeNode> =
                tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
                    let results = storage_clone
                        .hybrid_search(&query_owned, limit, 0.2, 0.8)
                        .map_err(|e| e.to_string())?;
                    Ok(results
                        .into_iter()
                        .filter_map(|r| storage_clone.get_node(&r.node.id).ok().flatten())
                        .collect())
                })
                .await
                .map_err(|e| format!("temporal history task panicked: {}", e))??;

            memories.sort_by_key(|m| m.created_at);

            let now = Utc::now();
            Ok(serde_json::json!({
                "action": "history",
                "topic": query,
                "count": memories.len(),
                "timeline": memories.iter().map(|m| {
                    let is_current = m.valid_until.is_none_or(|t| t > now);
                    serde_json::json!({
                        "id": m.id,
                        "content": truncate(&m.content, 200),
                        "created_at": m.created_at.to_rfc3339(),
                        "valid_from": m.valid_from.map(|d| d.to_rfc3339()),
                        "valid_until": m.valid_until.map(|d| d.to_rfc3339()),
                        "status": if is_current { "current" } else { "superseded" },
                        "tags": m.tags,
                    })
                }).collect::<Vec<_>>()
            }))
        }

        "invalidate" => {
            let memory_id = args
                .get("memory_id")
                .and_then(|v| v.as_str())
                .ok_or("memory_id is required for 'invalidate' action")?;

            let storage_clone = storage.clone();
            let memory_id_owned = memory_id.to_string();
            let node = tokio::task::spawn_blocking(move || {
                storage_clone
                    .get_node(&memory_id_owned)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("temporal invalidate fetch task panicked: {}", e))??
            .ok_or_else(|| format!("Memory not found: {memory_id}"))?;

            let now = Utc::now();

            if node.valid_until.is_some_and(|t| t <= now) {
                return Ok(serde_json::json!({
                    "action": "invalidate",
                    "status": "already_expired",
                    "memory_id": memory_id,
                    "expired_at": node.valid_until.map(|d| d.to_rfc3339()),
                }));
            }

            // Two-step invalidation:
            //   1. write `valid_until = now` on the row so `temporal expired`
            //      / `temporal current` / bitemporal queries treat it as gone.
            //   2. demote ranking so it falls behind better-calibrated peers
            //      in retrieval (matches the verbal contract the dashboard
            //      copy makes — "no longer valid" is both *visible-as-expired*
            //      and *ranked-lower*).
            //
            // Before v3.4.1 only step 2 ran, which left invalidated memories
            // silently in "current" while telling the user they were "expired".
            let storage_update = storage.clone();
            let memory_id_for_until = memory_id.to_string();
            tokio::task::spawn_blocking(move || {
                storage_update
                    .set_valid_until(&memory_id_for_until, now)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("set_valid_until task panicked: {}", e))??;

            let storage_demote = storage.clone();
            let memory_id_for_demote = memory_id.to_string();
            tokio::task::spawn_blocking(move || {
                storage_demote
                    .demote_memory(&memory_id_for_demote)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("demote_memory task panicked: {}", e))??;

            Ok(serde_json::json!({
                "action": "invalidate",
                "status": "invalidated",
                "memory_id": memory_id,
                "invalidated_at": now.to_rfc3339(),
                "valid_until": now.to_rfc3339(),
                "content_preview": truncate(&node.content, 150),
            }))
        }

        _ => Err(format!(
            "Unknown action: {action}. Use: current, expired, history, invalidate"
        )),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max)])
    }
}

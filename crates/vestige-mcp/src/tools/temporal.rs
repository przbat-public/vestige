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
                "description": "current: valid-now facts. expired: no-longer-valid. history: evolution of a topic — each memory also carries `lastChange`, the newest content revision (kind, record time, previous and new text, reason), so the entry shows how it changed and not only that it did; the full content chain is memory_changelog(memory_id). invalidate: mark a fact as no-longer-valid."
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
                        let (kw, sem) = vestige_core::default_hybrid_weights();
                        let results =
                            crate::retrieval::hybrid_search(&storage_clone, &query, limit, kw, sem)
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
                        let (kw, sem) = vestige_core::default_hybrid_weights();
                        let results = crate::retrieval::hybrid_search(
                            &storage_clone,
                            &query,
                            limit * 3,
                            kw,
                            sem,
                        )
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
                    let results = crate::retrieval::hybrid_search(
                        &storage_clone,
                        &query_owned,
                        limit,
                        0.2,
                        0.8,
                    )
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
            // The life-cycle status says *that* a memory changed; the newest
            // revision says *how* and *why*. One hop per memory, not the whole
            // chain: a topic-level read must not balloon, and the full content
            // history already has a home in `memory_changelog`.
            let ids: Vec<String> = memories.iter().map(|m| m.id.clone()).collect();
            let storage_revisions = storage.clone();
            let last_changes: Vec<Option<vestige_core::MemoryRevision>> =
                tokio::task::spawn_blocking(move || {
                    ids.iter()
                        .map(|id| storage_revisions.get_latest_revision(id).ok().flatten())
                        .collect()
                })
                .await
                .map_err(|e| format!("temporal history revision task panicked: {}", e))?;

            // The dashboard wire DTO (`TemporalResultDto`) consumes one
            // shape across all four actions: `memories: [TemporalEntryDto]`
            // with `id`, `content`, `retention`, `tags` (+ optional
            // validFrom / validUntil). The history branch used to emit
            // `timeline` with `status`/`createdAt` instead, which silently
            // produced `count = N` with an empty list on the dashboard
            // (the array name didn't match) and would also have 502-ed
            // because `retention` was missing. Keep `createdAt` and the
            // `status` chip embedded under names the DTO ignores — the
            // wire layer is forwards-compatible on extra fields — and the
            // same goes for `lastChange`.
            Ok(serde_json::json!({
                "action": "history",
                "topic": query,
                "count": memories.len(),
                "contentLimitChars": vestige_core::REVISION_CONTENT_CHAR_LIMIT,
                "memories": memories.iter().enumerate().map(|(index, m)| {
                    let is_current = m.valid_until.is_none_or(|t| t > now);
                    let mut entry = serde_json::json!({
                        "id": m.id,
                        "content": truncate(&m.content, 200),
                        "created_at": m.created_at.to_rfc3339(),
                        "valid_from": m.valid_from.map(|d| d.to_rfc3339()),
                        "valid_until": m.valid_until.map(|d| d.to_rfc3339()),
                        "retention": format!("{:.2}", m.retention_strength),
                        "status": if is_current { "current" } else { "superseded" },
                        "tags": m.tags,
                    });
                    if let Some(revision) = last_changes.get(index).and_then(|r| r.as_ref()) {
                        entry["lastChange"] = crate::tools::search_unified::revision_json(
                            revision,
                            vestige_core::REVISION_CONTENT_CHAR_LIMIT,
                        );
                    }
                    entry
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vestige_core::{IngestInput, Storage};

    /// Regression test for the temporal `history` action wire-shape bug.
    ///
    /// The dashboard wire contract (`TemporalResultDto`) consumes
    /// `memories: Vec<TemporalEntryDto>` for all four actions. The
    /// `current` and `expired` branches emit `"memories"`, but the
    /// `history` branch used to emit `"timeline"`, so the dashboard saw
    /// `count = N` with an empty `memories` array — a silent split
    /// between the headline number and the list rendered beneath it.
    ///
    /// We pin the contract here: history MUST emit `memories` (same
    /// field name as the other actions), and that array MUST have
    /// `count` entries.
    #[tokio::test]
    async fn history_action_emits_memories_field_matching_count() {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(Some(dir.path().join("test.db"))).unwrap());

        // A handful of memories the hybrid search will pick up under
        // the topic "redis caching".
        for content in [
            "Redis caching gives sub-ms reads when the working set fits in RAM",
            "Redis caching can silently mask stale data when TTLs are wrong",
            "Switching from Redis caching to in-memory LRU dropped tail latency",
        ] {
            storage
                .ingest(IngestInput {
                    content: content.to_string(),
                    node_type: "fact".to_string(),
                    tags: vec!["redis".to_string(), "caching".to_string()],
                    ..Default::default()
                })
                .unwrap();
        }

        let args = Some(serde_json::json!({
            "action": "history",
            "topic": "redis caching",
            "limit": 10,
        }));

        let result = execute(&storage, args).await.unwrap();

        // The wire-shape invariant the bug violated.
        let memories = result
            .get("memories")
            .and_then(|v| v.as_array())
            .expect("history response must expose a `memories` array (not `timeline`)");
        let count = result["count"].as_u64().unwrap() as usize;

        assert_eq!(
            memories.len(),
            count,
            "history `count` must equal `memories.len()` (was {} vs {})",
            count,
            memories.len(),
        );
        assert!(
            count > 0,
            "history search for an indexed topic should return at least one memory"
        );
    }

    /// Each entry in the `memories` array must carry every field
    /// `TemporalEntryDto` declares as non-optional or the dashboard
    /// renderer 502s on parse. We keep this guard tight on `id`,
    /// `content`, `retention`, and `tags`.
    #[tokio::test]
    async fn history_entries_match_temporal_entry_dto_shape() {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(Some(dir.path().join("test.db"))).unwrap());

        storage
            .ingest(IngestInput {
                content: "Redis caching tip about TTLs".to_string(),
                node_type: "fact".to_string(),
                tags: vec!["redis".to_string()],
                ..Default::default()
            })
            .unwrap();

        let args = Some(serde_json::json!({
            "action": "history",
            "topic": "redis",
            "limit": 5,
        }));
        let result = execute(&storage, args).await.unwrap();
        let memories = result["memories"].as_array().expect("memories array");
        let entry = memories
            .first()
            .expect("at least one history entry for this topic");

        assert!(entry.get("id").and_then(|v| v.as_str()).is_some());
        assert!(entry.get("content").and_then(|v| v.as_str()).is_some());
        assert!(entry.get("tags").and_then(|v| v.as_array()).is_some());
        // `retention` is required by the DTO; missing it makes the
        // dashboard parse layer return 502 for history.
        assert!(
            entry.get("retention").is_some(),
            "history entries must include `retention` (TemporalEntryDto requires it)"
        );
    }

    /// `temporal history` answers "how did knowledge about this topic evolve",
    /// and the life-cycle status alone does not: an entry has to say what the
    /// memory last said, what it says now, and why it changed. The full chain
    /// stays in `memory_changelog` so a topic-level read cannot balloon.
    #[tokio::test]
    async fn history_entries_carry_the_last_content_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(Some(dir.path().join("test.db"))).unwrap());

        let first = "Redis caching gives sub-ms reads when the working set fits in RAM";
        let id = storage
            .ingest(IngestInput {
                content: first.to_string(),
                node_type: "fact".to_string(),
                tags: vec!["redis".to_string(), "caching".to_string()],
                ..Default::default()
            })
            .unwrap()
            .id;
        let second = "Redis caching gives sub-ms reads when the working set fits in RAM, measured 2026-05-12";
        storage
            .update_node_content_with_revision(&id, second, Some("added the measurement date"))
            .unwrap();

        let args = Some(serde_json::json!({
            "action": "history",
            "topic": "redis caching",
            "limit": 10,
        }));
        let result = execute(&storage, args).await.unwrap();

        let entry = result["memories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == serde_json::json!(id))
            .expect("the updated memory must appear in its topic history")
            .clone();
        let change = entry
            .get("lastChange")
            .cloned()
            .expect("a history entry must carry the last content change");

        assert_eq!(change["kind"], "edit");
        assert_eq!(change["reason"], "added the measurement date");
        assert_eq!(change["oldContent"], first);
        assert!(
            change["newContent"]
                .as_str()
                .is_some_and(|text| text.contains("measured 2026-05-12")),
            "the new wording has to be there, not just the fact that it changed: {change}"
        );
    }
}

//! Unified Memory Tool
//!
//! Merges get_knowledge, delete_knowledge, and get_memory_state into a single
//! `memory` tool with action-based dispatch.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{MemoryState, Modification, OutcomeType, Storage};

pub const ACCESSIBILITY_ACTIVE: f64 = 0.7;
pub const ACCESSIBILITY_DORMANT: f64 = 0.4;
pub const ACCESSIBILITY_SILENT: f64 = 0.1;

/// Weighted accessibility: retention 50%, retrieval 30%, storage 20%.
pub fn compute_accessibility(retention: f64, retrieval: f64, storage: f64) -> f64 {
    retention * 0.5 + retrieval * 0.3 + storage * 0.2
}

pub fn state_from_accessibility(accessibility: f64) -> MemoryState {
    if accessibility >= ACCESSIBILITY_ACTIVE {
        MemoryState::Active
    } else if accessibility >= ACCESSIBILITY_DORMANT {
        MemoryState::Dormant
    } else if accessibility >= ACCESSIBILITY_SILENT {
        MemoryState::Silent
    } else {
        MemoryState::Unavailable
    }
}

/// Input schema for the unified memory tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["get", "get_batch", "delete", "state", "promote", "demote", "edit"],
                "description": "Action to perform: 'get' retrieves full memory node, 'get_batch' retrieves multiple memories by IDs (use 'ids' array), 'delete' removes memory, 'state' returns accessibility state, 'promote' increases retrieval strength (thumbs up), 'demote' decreases retrieval strength (thumbs down), 'edit' updates content in-place (preserves FSRS state)"
            },
            "id": {
                "type": "string",
                "description": "The ID of the memory node (for single-memory actions)"
            },
            "ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Array of memory IDs (for get_batch action). Max 20 IDs per call."
            },
            "reason": {
                "type": "string",
                "description": "Why this memory is being promoted/demoted (optional, for logging). Only used with promote/demote actions."
            },
            "content": {
                "type": "string",
                "description": "New content for edit action. Replaces existing content, regenerates embedding, preserves FSRS state."
            }
        },
        "required": ["action"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryArgs {
    action: String,
    id: Option<String>,
    ids: Option<Vec<String>>,
    reason: Option<String>,
    content: Option<String>,
}

/// Execute the unified memory tool
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: MemoryArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    // get_batch uses 'ids' array, all other actions use 'id'
    if args.action == "get_batch" {
        let ids = args.ids.ok_or("get_batch requires 'ids' array")?;
        if ids.is_empty() {
            return Err("ids array cannot be empty".to_string());
        }
        if ids.len() > 20 {
            return Err("get_batch supports max 20 IDs per call".to_string());
        }
        for id in &ids {
            uuid::Uuid::parse_str(id).map_err(|_| format!("Invalid memory ID format: {}", id))?;
        }
        return execute_get_batch(storage, &ids).await;
    }

    let id = args.id.ok_or("This action requires 'id' parameter")?;
    uuid::Uuid::parse_str(&id).map_err(|_| "Invalid memory ID format".to_string())?;

    match args.action.as_str() {
        "get" => execute_get(storage, &id).await,
        "delete" => execute_delete(storage, &id).await,
        "state" => execute_state(storage, &id).await,
        "promote" => execute_promote(storage, cognitive, &id, args.reason).await,
        "demote" => execute_demote(storage, cognitive, &id, args.reason).await,
        "edit" => execute_edit(storage, &id, args.content).await,
        _ => Err(format!(
            "Invalid action '{}'. Must be one of: get, get_batch, delete, state, promote, demote, edit",
            args.action
        )),
    }
}

/// Get full memory node with all metadata
async fn execute_get(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
    // record_memory_access + get_node are both blocking SQLite operations.
    // Group them into a single task so we hand the runtime back exactly once.
    let storage_clone = storage.clone();
    let id_owned = id.to_string();
    let node = tokio::task::spawn_blocking(move || {
        if let Err(e) = storage_clone.record_memory_access(&id_owned) {
            tracing::debug!(error = %e, memory_id = %id_owned, "Failed to record memory access");
        }
        storage_clone.get_node(&id_owned)
    })
    .await
    .map_err(|e| format!("execute_get task panicked: {}", e))?
    .map_err(|e| e.to_string())?;

    match node {
        Some(n) => Ok(serde_json::json!({
            "action": "get",
            "found": true,
            "node": {
                "id": n.id,
                "content": n.content,
                "nodeType": n.node_type,
                "createdAt": n.created_at.to_rfc3339(),
                "updatedAt": n.updated_at.to_rfc3339(),
                "lastAccessed": n.last_accessed.to_rfc3339(),
                "stability": n.stability,
                "difficulty": n.difficulty,
                "reps": n.reps,
                "lapses": n.lapses,
                "storageStrength": n.storage_strength,
                "retrievalStrength": n.retrieval_strength,
                "retentionStrength": n.retention_strength,
                "sentimentScore": n.sentiment_score,
                "sentimentMagnitude": n.sentiment_magnitude,
                "nextReview": n.next_review.map(|d| d.to_rfc3339()),
                "source": n.source,
                "tags": n.tags,
                "hasEmbedding": n.has_embedding,
                "embeddingModel": n.embedding_model,
            }
        })),
        None => Ok(serde_json::json!({
            "action": "get",
            "found": false,
            "nodeId": id,
            "message": "Memory not found",
        })),
    }
}

/// Batch-retrieve multiple memory nodes by IDs
async fn execute_get_batch(storage: &Arc<Storage>, ids: &[String]) -> Result<Value, String> {
    // Many tiny SELECTs in a row — never on the reactor. One spawn_blocking
    // hop runs the whole loop on the SQLite-blocking thread pool.
    let storage_clone = storage.clone();
    let ids_owned: Vec<String> = ids.to_vec();
    let (nodes, not_found): (Vec<Value>, Vec<String>) = tokio::task::spawn_blocking(move || {
        let mut nodes = Vec::new();
        let mut not_found = Vec::new();
        for id in &ids_owned {
            if let Err(e) = storage_clone.record_memory_access(id) {
                tracing::debug!(error = %e, memory_id = %id, "Failed to record memory access");
            }
            match storage_clone.get_node(id) {
                Ok(Some(n)) => {
                    nodes.push(serde_json::json!({
                        "id": n.id,
                        "content": n.content,
                        "nodeType": n.node_type,
                        "createdAt": n.created_at.to_rfc3339(),
                        "updatedAt": n.updated_at.to_rfc3339(),
                        "lastAccessed": n.last_accessed.to_rfc3339(),
                        "stability": n.stability,
                        "difficulty": n.difficulty,
                        "reps": n.reps,
                        "lapses": n.lapses,
                        "storageStrength": n.storage_strength,
                        "retrievalStrength": n.retrieval_strength,
                        "retentionStrength": n.retention_strength,
                        "sentimentScore": n.sentiment_score,
                        "sentimentMagnitude": n.sentiment_magnitude,
                        "nextReview": n.next_review.map(|d| d.to_rfc3339()),
                        "source": n.source,
                        "tags": n.tags,
                        "hasEmbedding": n.has_embedding,
                        "embeddingModel": n.embedding_model,
                    }));
                }
                Ok(None) => not_found.push(id.clone()),
                Err(e) => {
                    tracing::warn!(memory_id = %id, "get_batch error: {}", e);
                    not_found.push(id.clone());
                }
            }
        }
        (nodes, not_found)
    })
    .await
    .map_err(|e| format!("execute_get_batch task panicked: {}", e))?;

    Ok(serde_json::json!({
        "action": "get_batch",
        "nodes": nodes,
        "found": nodes.len(),
        "notFound": not_found,
    }))
}

/// Delete a memory and return success status
async fn execute_delete(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
    let storage_clone = storage.clone();
    let id_owned = id.to_string();
    let deleted = tokio::task::spawn_blocking(move || storage_clone.delete_node(&id_owned))
        .await
        .map_err(|e| format!("delete_node task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "action": "delete",
        "success": deleted,
        "nodeId": id,
        "message": if deleted { "Memory deleted successfully" } else { "Memory not found" },
    }))
}

/// Get accessibility state of a memory (Active/Dormant/Silent/Unavailable)
async fn execute_state(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
    // Get the memory
    let storage_clone = storage.clone();
    let id_owned = id.to_string();
    let memory = tokio::task::spawn_blocking(move || storage_clone.get_node(&id_owned))
        .await
        .map_err(|e| format!("execute_state task panicked: {}", e))?
        .map_err(|e| format!("Error: {}", e))?
        .ok_or("Memory not found")?;

    // Calculate accessibility score
    let accessibility = compute_accessibility(
        memory.retention_strength,
        memory.retrieval_strength,
        memory.storage_strength,
    );

    // Determine state
    let state = state_from_accessibility(accessibility);

    let state_description = match state {
        MemoryState::Active => "Easily retrievable - this memory is fresh and accessible",
        MemoryState::Dormant => "Retrievable with effort - may need cues to recall",
        MemoryState::Silent => "Difficult to retrieve - exists but hard to access",
        MemoryState::Unavailable => "Cannot be retrieved - needs significant reinforcement",
    };

    Ok(serde_json::json!({
        "action": "state",
        "memoryId": id,
        "content": memory.content,
        "state": format!("{:?}", state),
        "accessibility": accessibility,
        "description": state_description,
        "components": {
            "retentionStrength": memory.retention_strength,
            "retrievalStrength": memory.retrieval_strength,
            "storageStrength": memory.storage_strength
        },
        "thresholds": {
            "active": ACCESSIBILITY_ACTIVE,
            "dormant": ACCESSIBILITY_DORMANT,
            "silent": ACCESSIBILITY_SILENT
        }
    }))
}

/// Promote a memory (thumbs up) — increases retrieval strength with cognitive feedback pipeline
async fn execute_promote(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    id: &str,
    reason: Option<String>,
) -> Result<Value, String> {
    // get_node + promote_memory both write to SQLite. Bundle into one
    // blocking task and propagate "not found" through the inner Option.
    let storage_clone = storage.clone();
    let id_owned = id.to_string();
    let pair = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let Some(before) = storage_clone
            .get_node(&id_owned)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let node = storage_clone
            .promote_memory(&id_owned)
            .map_err(|e| e.to_string())?;
        Ok(Some((before, node)))
    })
    .await
    .map_err(|e| format!("execute_promote task panicked: {}", e))??;
    let (before, node) = pair.ok_or_else(|| format!("Node not found: {}", id))?;

    // Cognitive feedback pipeline
    if let Ok(mut cog) = cognitive.try_lock() {
        cog.reward_signal.record_outcome(id, OutcomeType::Helpful);
        cog.importance_tracker.on_retrieved(id, true);
        if cog.reconsolidation.is_labile(id) {
            cog.reconsolidation.apply_modification(
                id,
                Modification::StrengthenConnection {
                    target_memory_id: id.to_string(),
                    boost: 0.2,
                },
            );
        }
    }

    Ok(serde_json::json!({
        "success": true,
        "action": "promoted",
        "nodeId": node.id,
        "reason": reason,
        "changes": {
            "retrievalStrength": {
                "before": before.retrieval_strength,
                "after": node.retrieval_strength,
                "delta": "+0.20"
            },
            "retentionStrength": {
                "before": before.retention_strength,
                "after": node.retention_strength,
                "delta": "+0.10"
            },
            "stability": {
                "before": before.stability,
                "after": node.stability,
                "multiplier": "1.5x"
            }
        },
        "message": format!("Memory promoted. It will now surface more often in searches. Retrieval: {:.2} -> {:.2}",
            before.retrieval_strength, node.retrieval_strength),
    }))
}

/// Demote a memory (thumbs down) — decreases retrieval strength with cognitive feedback pipeline
async fn execute_demote(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    id: &str,
    reason: Option<String>,
) -> Result<Value, String> {
    // Same pattern as `execute_promote` — group the SELECT + UPDATE into a
    // single blocking task so we yield the runtime only once.
    let storage_clone = storage.clone();
    let id_owned = id.to_string();
    let pair = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let Some(before) = storage_clone
            .get_node(&id_owned)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let node = storage_clone
            .demote_memory(&id_owned)
            .map_err(|e| e.to_string())?;
        Ok(Some((before, node)))
    })
    .await
    .map_err(|e| format!("execute_demote task panicked: {}", e))??;
    let (before, node) = pair.ok_or_else(|| format!("Node not found: {}", id))?;

    // Cognitive feedback pipeline
    if let Ok(mut cog) = cognitive.try_lock() {
        cog.reward_signal
            .record_outcome(id, OutcomeType::NotHelpful);
        cog.importance_tracker.on_retrieved(id, false);
        if cog.reconsolidation.is_labile(id) {
            cog.reconsolidation.apply_modification(
                id,
                Modification::AddContext {
                    context: "User reported this memory was wrong/unhelpful".to_string(),
                },
            );
        }
    }

    Ok(serde_json::json!({
        "success": true,
        "action": "demoted",
        "nodeId": node.id,
        "reason": reason,
        "changes": {
            "retrievalStrength": {
                "before": before.retrieval_strength,
                "after": node.retrieval_strength,
                "delta": "-0.30"
            },
            "retentionStrength": {
                "before": before.retention_strength,
                "after": node.retention_strength,
                "delta": "-0.15"
            },
            "stability": {
                "before": before.stability,
                "after": node.stability,
                "multiplier": "0.5x"
            }
        },
        "message": format!("Memory demoted. Better alternatives will now surface instead. Retrieval: {:.2} -> {:.2}",
            before.retrieval_strength, node.retrieval_strength),
        "note": "Memory is NOT deleted - it remains searchable but ranks lower."
    }))
}

/// Edit a memory's content in-place — preserves FSRS state, regenerates embedding
async fn execute_edit(
    storage: &Arc<Storage>,
    id: &str,
    content: Option<String>,
) -> Result<Value, String> {
    let new_content = content.ok_or("Missing 'content' field. Required for edit action.")?;

    if new_content.trim().is_empty() {
        return Err("Content cannot be empty".to_string());
    }

    // Get existing node to capture old content
    let old_node = storage
        .get_node(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Memory not found: {}", id))?;

    // Update content (regenerates embedding, syncs FTS5)
    storage
        .update_node_content(id, &new_content)
        .map_err(|e| e.to_string())?;

    // Truncate previews for response (char-safe to avoid UTF-8 panics)
    let old_preview = if old_node.content.chars().count() > 200 {
        let truncated: String = old_node.content.chars().take(197).collect();
        format!("{}...", truncated)
    } else {
        old_node.content.clone()
    };
    let new_preview = if new_content.chars().count() > 200 {
        let truncated: String = new_content.chars().take(197).collect();
        format!("{}...", truncated)
    } else {
        new_content.clone()
    };

    Ok(serde_json::json!({
        "success": true,
        "action": "edit",
        "nodeId": id,
        "oldContentPreview": old_preview,
        "newContentPreview": new_preview,
        "note": "FSRS state preserved (stability, difficulty, reps, lapses unchanged). Embedding regenerated for new content."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_thresholds() {
        // Test Active state
        let accessibility = compute_accessibility(0.9, 0.8, 0.7);
        assert!(accessibility >= ACCESSIBILITY_ACTIVE);
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Active
        ));

        // Test Dormant state
        let accessibility = compute_accessibility(0.5, 0.5, 0.5);
        assert!((ACCESSIBILITY_DORMANT..ACCESSIBILITY_ACTIVE).contains(&accessibility));
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Dormant
        ));

        // Test Silent state
        let accessibility = compute_accessibility(0.2, 0.2, 0.2);
        assert!((ACCESSIBILITY_SILENT..ACCESSIBILITY_DORMANT).contains(&accessibility));
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Silent
        ));

        // Test Unavailable state
        let accessibility = compute_accessibility(0.05, 0.05, 0.05);
        assert!(accessibility < ACCESSIBILITY_SILENT);
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Unavailable
        ));
    }

    #[test]
    fn test_schema_structure() {
        let schema = schema();
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["id"].is_object());
        assert!(schema["properties"]["reason"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["action"]));
        // Verify all 7 actions are in enum
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert_eq!(actions.len(), 7);
        assert!(actions.contains(&serde_json::json!("get_batch")));
        assert!(actions.contains(&serde_json::json!("edit")));
        assert!(actions.contains(&serde_json::json!("promote")));
        assert!(actions.contains(&serde_json::json!("demote")));
    }

    // === INTEGRATION TESTS ===

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    async fn test_storage() -> (Arc<Storage>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    async fn ingest_memory(storage: &Arc<Storage>) -> String {
        let node = storage
            .ingest(vestige_core::IngestInput {
                content: "Memory unified test content".to_string(),
                node_type: "fact".to_string(),
                source: Some("test".to_string()),
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["test-tag".to_string()],
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            })
            .unwrap();
        node.id
    }

    #[tokio::test]
    async fn test_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_invalid_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "invalid", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid action"));
    }

    #[tokio::test]
    async fn test_invalid_uuid_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "get", "id": "not-a-uuid" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory ID format"));
    }

    #[tokio::test]
    async fn test_get_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "get", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "get");
        assert_eq!(value["found"], true);
        assert_eq!(value["node"]["id"], id);
        assert_eq!(value["node"]["content"], "Memory unified test content");
        assert_eq!(value["node"]["nodeType"], "fact");
        assert!(value["node"]["createdAt"].is_string());
        assert!(value["node"]["tags"].is_array());
    }

    #[tokio::test]
    async fn test_get_nonexistent_memory() {
        let (storage, _dir) = test_storage().await;
        let args =
            serde_json::json!({ "action": "get", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["found"], false);
        assert_eq!(value["message"], "Memory not found");
    }

    #[tokio::test]
    async fn test_delete_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "delete", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "delete");
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_memory() {
        let (storage, _dir) = test_storage().await;
        // Ingest+delete a throwaway memory to warm writer after WAL migration
        let warmup_id = storage
            .ingest(vestige_core::IngestInput {
                content: "warmup".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap()
            .id;
        let _ = storage.delete_node(&warmup_id);
        let args =
            serde_json::json!({ "action": "delete", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], false);
        assert!(value["message"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_then_get_returns_not_found() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let del_args = serde_json::json!({ "action": "delete", "id": id });
        execute(&storage, &test_cognitive(), Some(del_args))
            .await
            .unwrap();
        let get_args = serde_json::json!({ "action": "get", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(get_args)).await;
        let value = result.unwrap();
        assert_eq!(value["found"], false);
    }

    #[tokio::test]
    async fn test_state_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "state", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "state");
        assert_eq!(value["memoryId"], id);
        assert!(value["accessibility"].is_number());
        assert!(value["state"].is_string());
        assert!(value["description"].is_string());
        assert!(value["components"]["retentionStrength"].is_number());
        assert!(value["components"]["retrievalStrength"].is_number());
        assert!(value["components"]["storageStrength"].is_number());
        assert_eq!(value["thresholds"]["active"], 0.7);
        assert_eq!(value["thresholds"]["dormant"], 0.4);
        assert_eq!(value["thresholds"]["silent"], 0.1);
    }

    #[tokio::test]
    async fn test_state_nonexistent_memory_fails() {
        let (storage, _dir) = test_storage().await;
        let args =
            serde_json::json!({ "action": "state", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_accessibility_boundary_active() {
        let a = compute_accessibility(1.0, 0.7, 0.5);
        assert!(a >= ACCESSIBILITY_ACTIVE);
        assert!(matches!(state_from_accessibility(a), MemoryState::Active));
    }

    #[test]
    fn test_accessibility_boundary_zero() {
        let a = compute_accessibility(0.0, 0.0, 0.0);
        assert_eq!(a, 0.0);
        assert!(matches!(
            state_from_accessibility(a),
            MemoryState::Unavailable
        ));
    }

    // ========================================================================
    // PROMOTE/DEMOTE TESTS (ported from feedback.rs, v1.7.0 merge)
    // ========================================================================

    #[tokio::test]
    async fn test_promote_missing_id_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "promote", "id": "not-a-uuid" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory ID format"));
    }

    #[tokio::test]
    async fn test_promote_nonexistent_node_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "promote", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Node not found"));
    }

    #[tokio::test]
    async fn test_promote_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "promote", "id": id, "reason": "It was helpful" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "promoted");
        assert_eq!(value["nodeId"], id);
        assert_eq!(value["reason"], "It was helpful");
        assert!(value["changes"]["retrievalStrength"].is_object());
    }

    #[tokio::test]
    async fn test_promote_without_reason_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "promote", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert!(value["reason"].is_null());
    }

    #[tokio::test]
    async fn test_promote_changes_contain_expected_fields() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "promote", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        let value = result.unwrap();
        assert!(value["changes"]["retrievalStrength"]["before"].is_number());
        assert!(value["changes"]["retrievalStrength"]["after"].is_number());
        assert_eq!(value["changes"]["retrievalStrength"]["delta"], "+0.20");
        assert!(value["changes"]["retentionStrength"]["before"].is_number());
        assert_eq!(value["changes"]["retentionStrength"]["delta"], "+0.10");
        assert_eq!(value["changes"]["stability"]["multiplier"], "1.5x");
    }

    #[tokio::test]
    async fn test_demote_invalid_uuid_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "demote", "id": "bad-id" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory ID format"));
    }

    #[tokio::test]
    async fn test_demote_nonexistent_node_fails() {
        let (storage, _dir) = test_storage().await;
        let args =
            serde_json::json!({ "action": "demote", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Node not found"));
    }

    #[tokio::test]
    async fn test_demote_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "demote", "id": id, "reason": "It was wrong" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "demoted");
        assert_eq!(value["nodeId"], id);
        assert_eq!(value["reason"], "It was wrong");
        assert!(value["note"].as_str().unwrap().contains("NOT deleted"));
    }

    #[tokio::test]
    async fn test_demote_changes_contain_expected_fields() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "demote", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        let value = result.unwrap();
        assert!(value["changes"]["retrievalStrength"]["before"].is_number());
        assert_eq!(value["changes"]["retrievalStrength"]["delta"], "-0.30");
        assert_eq!(value["changes"]["retentionStrength"]["delta"], "-0.15");
        assert_eq!(value["changes"]["stability"]["multiplier"], "0.5x");
    }

    // ========================================================================
    // EDIT TESTS (v1.9.2)
    // ========================================================================

    #[tokio::test]
    async fn test_edit_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "content": "Updated memory content"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "edit");
        assert_eq!(value["nodeId"], id);
        assert!(
            value["oldContentPreview"]
                .as_str()
                .unwrap()
                .contains("Memory unified test content")
        );
        assert!(
            value["newContentPreview"]
                .as_str()
                .unwrap()
                .contains("Updated memory content")
        );
        assert!(
            value["note"]
                .as_str()
                .unwrap()
                .contains("FSRS state preserved")
        );
    }

    #[tokio::test]
    async fn test_edit_preserves_fsrs_state() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;

        // Get FSRS state before edit
        let before = storage.get_node(&id).unwrap().unwrap();

        // Edit content
        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "content": "Completely new content after edit"
        });
        execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        // Verify FSRS state preserved
        let after = storage.get_node(&id).unwrap().unwrap();
        assert_eq!(after.stability, before.stability);
        assert_eq!(after.difficulty, before.difficulty);
        assert_eq!(after.reps, before.reps);
        assert_eq!(after.lapses, before.lapses);
        assert_eq!(after.retention_strength, before.retention_strength);
        // Content should be updated
        assert_eq!(after.content, "Completely new content after edit");
        assert_ne!(after.content, before.content);
    }

    #[tokio::test]
    async fn test_edit_missing_content_fails() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "edit", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("content"));
    }

    #[tokio::test]
    async fn test_edit_empty_content_fails() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "edit", "id": id, "content": "  " });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_edit_nonexistent_memory_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "edit",
            "id": "00000000-0000-0000-0000-000000000000",
            "content": "New content"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_with_multibyte_utf8_content() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        // Content with emoji and CJK characters (multi-byte UTF-8)
        let long_content = "🧠".repeat(100); // 100 brain emoji = 400 bytes but only 100 chars
        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "content": long_content
        });
        // This must NOT panic (previous code would panic on byte-level truncation)
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }
}

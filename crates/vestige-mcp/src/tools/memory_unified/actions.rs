//! Per-action implementations (get, get_batch, delete, state, promote, demote, edit).

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{MemoryState, Modification, OutcomeType, Storage};

use crate::cognitive::CognitiveEngine;

use super::helpers::{
    ACCESSIBILITY_ACTIVE, ACCESSIBILITY_DORMANT, ACCESSIBILITY_SILENT, compute_accessibility,
    state_from_accessibility,
};

/// Get full memory node with all metadata
pub(super) async fn execute_get(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
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
pub(super) async fn execute_get_batch(
    storage: &Arc<Storage>,
    ids: &[String],
) -> Result<Value, String> {
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
pub(super) async fn execute_delete(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
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
pub(super) async fn execute_state(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
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
pub(super) async fn execute_promote(
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
pub(super) async fn execute_demote(
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
pub(super) async fn execute_edit(
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

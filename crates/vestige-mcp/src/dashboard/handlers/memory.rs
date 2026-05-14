//! Dashboard handlers — memory CRUD
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;

use super::super::events::VestigeEvent;
use super::super::state::AppState;
use super::{log_err, log_join_err};

#[derive(Debug, Deserialize)]
pub struct MemoryListParams {
    pub q: Option<String>,
    // `alias = "type"` keeps older dashboard builds (and any external caller
    // that read the original endpoint) working after the v3.3.x filter rename.
    // The canonical name stays `node_type` to match the storage column.
    #[serde(alias = "type")]
    pub node_type: Option<String>,
    pub tag: Option<String>,
    pub min_retention: Option<f64>,
    pub sort: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

/// List memories with optional search
pub async fn list_memories(
    State(state): State<AppState>,
    Query(params): Query<MemoryListParams>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    if let Some(query) = params.q.as_ref().filter(|q| !q.trim().is_empty()) {
        // Hybrid search re-embeds the query (~150–500ms on cold cache) and
        // touches BM25 + HNSW indices. Run it on the blocking pool so other
        // dashboard requests don't queue up on the reactor thread.
        let storage = state.storage.clone();
        let q = query.clone();
        let results =
            tokio::task::spawn_blocking(move || storage.hybrid_search(&q, limit, 0.3, 0.7))
                .await
                .map_err(log_join_err("hybrid_search task panicked"))?
                .map_err(log_err("storage operation"))?;

        let formatted: Vec<Value> = results
            .into_iter()
            .filter(|r| {
                if let Some(min_ret) = params.min_retention {
                    r.node.retention_strength >= min_ret
                } else {
                    true
                }
            })
            .map(|r| {
                serde_json::json!({
                    "id": r.node.id,
                    "content": r.node.content,
                    "nodeType": r.node.node_type,
                    "tags": r.node.tags,
                    "retentionStrength": r.node.retention_strength,
                    "storageStrength": r.node.storage_strength,
                    "retrievalStrength": r.node.retrieval_strength,
                    "createdAt": r.node.created_at.to_rfc3339(),
                    "updatedAt": r.node.updated_at.to_rfc3339(),
                    "combinedScore": r.combined_score,
                    "source": r.node.source,
                    "reviewCount": r.node.reps,
                })
            })
            .collect();

        return Ok(Json(serde_json::json!({
            "total": formatted.len(),
            "memories": formatted,
        })));
    }

    // No search query — list all memories. `get_all_nodes` scans the full
    // node table up to `limit`; per-row JSON deserialization makes it block
    // longer than the 10ms reactor budget on large bases.
    let storage = state.storage.clone();
    let mut nodes = tokio::task::spawn_blocking(move || storage.get_all_nodes(limit, offset))
        .await
        .map_err(log_join_err("get_all_nodes task panicked"))?
        .map_err(log_err("storage operation"))?;

    // Apply filters
    if let Some(ref node_type) = params.node_type {
        nodes.retain(|n| n.node_type == *node_type);
    }
    if let Some(ref tag) = params.tag {
        nodes.retain(|n| n.tags.iter().any(|t| t == tag));
    }
    if let Some(min_ret) = params.min_retention {
        nodes.retain(|n| n.retention_strength >= min_ret);
    }

    let formatted: Vec<Value> = nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content,
                "nodeType": n.node_type,
                "tags": n.tags,
                "retentionStrength": n.retention_strength,
                "storageStrength": n.storage_strength,
                "retrievalStrength": n.retrieval_strength,
                "createdAt": n.created_at.to_rfc3339(),
                "updatedAt": n.updated_at.to_rfc3339(),
                "source": n.source,
                "reviewCount": n.reps,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "total": formatted.len(),
        "memories": formatted,
    })))
}

/// Get a single memory by ID
pub async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let node = state
        .storage
        .get_node(&id)
        .map_err(log_err("get memory"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "id": node.id,
        "content": node.content,
        "nodeType": node.node_type,
        "tags": node.tags,
        "retentionStrength": node.retention_strength,
        "storageStrength": node.storage_strength,
        "retrievalStrength": node.retrieval_strength,
        "sentimentScore": node.sentiment_score,
        "sentimentMagnitude": node.sentiment_magnitude,
        "source": node.source,
        "createdAt": node.created_at.to_rfc3339(),
        "updatedAt": node.updated_at.to_rfc3339(),
        "lastAccessedAt": node.last_accessed.to_rfc3339(),
        "nextReviewAt": node.next_review.map(|dt| dt.to_rfc3339()),
        "reviewCount": node.reps,
        "validFrom": node.valid_from.map(|dt| dt.to_rfc3339()),
        "validUntil": node.valid_until.map(|dt| dt.to_rfc3339()),
    })))
}

/// Delete a memory by ID
pub async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let deleted = state
        .storage
        .delete_node(&id)
        .map_err(log_err("storage operation"))?;

    if deleted {
        state.emit(VestigeEvent::MemoryDeleted {
            id: id.clone(),
            timestamp: chrono::Utc::now(),
        });
        Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Promote a memory
pub async fn promote_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let node = state
        .storage
        .promote_memory(&id)
        .map_err(log_err("storage operation"))?;

    state.emit(VestigeEvent::MemoryPromoted {
        id: node.id.clone(),
        new_retention: node.retention_strength,
        timestamp: chrono::Utc::now(),
    });

    Ok(Json(serde_json::json!({
        "promoted": true,
        "id": node.id,
        "retentionStrength": node.retention_strength,
    })))
}

/// Demote a memory
pub async fn demote_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let node = state
        .storage
        .demote_memory(&id)
        .map_err(log_err("storage operation"))?;

    state.emit(VestigeEvent::MemoryDemoted {
        id: node.id.clone(),
        new_retention: node.retention_strength,
        timestamp: chrono::Utc::now(),
    });

    Ok(Json(serde_json::json!({
        "demoted": true,
        "id": node.id,
        "retentionStrength": node.retention_strength,
    })))
}

/// Body for `PATCH /api/memories/{id}` — both fields optional.
/// Either or both can be supplied. Empty body returns 400.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemoryBody {
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Body for `POST /api/smart_ingest`.
///
/// Mirrors the MCP `smart_ingest` tool's single-mode arguments. We accept
/// camelCase here to match the rest of the dashboard wire format and rename
/// to snake_case before handing the value to `tools::smart_ingest::execute`,
/// which expects the MCP-tool-native casing.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartIngestBody {
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    /// When true, bypass the prediction-error gate (similarity check) and
    /// always create a new node. Maps to `smart_ingest`'s `force_create`.
    #[serde(default)]
    pub force_create: bool,
}

/// `POST /api/smart_ingest` — create a memory from the dashboard.
///
/// Thin REST wrapper over `tools::smart_ingest::execute`, which handles the
/// full ingest pipeline (importance scoring, intent detection, preprocessing,
/// prediction-error gating, near-duplicate detection). The dashboard surfaces
/// the same affordances (`compound_content_warning`, `near_duplicate_warning`,
/// `decision`) the MCP tool returns, so users see exactly what the engine
/// decided to do with their input.
///
/// Emits `MemoryCreated` so other dashboard tabs and connected clients see
/// the new node without polling.
pub async fn smart_ingest_memory(
    State(state): State<AppState>,
    Json(body): Json<SmartIngestBody>,
) -> Result<Json<Value>, StatusCode> {
    let cognitive = state
        .cognitive
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    if body.content.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // The tool expects snake_case `force_create` and `node_type` — we
    // rebuild the args here rather than re-deriving Serialize on
    // `SmartIngestBody` so the dashboard contract stays decoupled from the
    // MCP tool's argument schema.
    let args = serde_json::json!({
        "content": body.content,
        "tags": body.tags,
        "node_type": body.node_type,
        "source": body.source.clone().or(Some("dashboard".to_string())),
        "force_create": body.force_create,
        "agent": "dashboard",
    });

    let result = crate::tools::smart_ingest::execute(&state.storage, cognitive, Some(args))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "smart_ingest failed");
            // Tool returns user-friendly errors for validation issues
            // (empty content, content too large, missing fields). Map them
            // to 400 so the dashboard surfaces them inline without forcing
            // the user to open a console.
            if e.contains("empty")
                || e.contains("too large")
                || e.contains("Missing")
                || e.contains("Invalid")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    // Emit MemoryCreated only on a real new-node decision. `update`,
    // `reinforce`, `merge`, etc. mutate an existing node — we want
    // MemoryUpdated semantics for those, but the tool doesn't expose
    // enough to differentiate cleanly. Conservative: emit Created for
    // create/supersede, Updated for the rest, when we have a node id.
    let decision = result["decision"].as_str().unwrap_or("");
    let node_id = result["nodeId"].as_str().unwrap_or("").to_string();
    if !node_id.is_empty() {
        let preview: String = body.content.chars().take(80).collect();
        let now = chrono::Utc::now();
        let node_type = body.node_type.clone().unwrap_or_else(|| "fact".to_string());
        if matches!(decision, "create" | "supersede") {
            state.emit(VestigeEvent::MemoryCreated {
                id: node_id,
                content_preview: preview,
                node_type,
                tags: body.tags.clone(),
                timestamp: now,
            });
        } else {
            state.emit(VestigeEvent::MemoryUpdated {
                id: node_id,
                content_preview: preview,
                field: "smart_ingest".to_string(),
                timestamp: now,
            });
        }
    }

    Ok(Json(result))
}

/// Edit a memory's content and/or tags.
///
/// Mirrors the MCP `memory(action="edit")` flow but is exposed via REST so the
/// dashboard can give users an inline editor. Updating content also schedules
/// embedding regeneration server-side (see `Storage::update_node_content`).
pub async fn update_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMemoryBody>,
) -> Result<Json<Value>, StatusCode> {
    if body.content.is_none() && body.tags.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Some(ref new_content) = body.content {
        let trimmed = new_content.trim();
        if trimmed.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        // update_node_content regenerates the embedding (~150–500ms), so
        // dispatch it to the blocking pool to keep the reactor free.
        let storage = state.storage.clone();
        let id_owned = id.clone();
        let trimmed_owned = trimmed.to_string();
        tokio::task::spawn_blocking(move || storage.update_node_content(&id_owned, &trimmed_owned))
            .await
            .map_err(log_join_err("update_node_content task panicked"))?
            .map_err(log_err("update_node_content"))?;
    }

    if let Some(ref new_tags) = body.tags {
        state
            .storage
            .update_node_tags(&id, new_tags)
            .map_err(log_err("update_node_tags"))?;
    }

    let node = state
        .storage
        .get_node(&id)
        .map_err(log_err("get_node after update"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    let preview: String = node.content.chars().take(80).collect();
    let field = match (body.content.is_some(), body.tags.is_some()) {
        (true, true) => "content+tags",
        (true, false) => "content",
        (false, true) => "tags",
        _ => "unknown",
    };
    state.emit(VestigeEvent::MemoryUpdated {
        id: node.id.clone(),
        content_preview: preview,
        field: field.to_string(),
        timestamp: chrono::Utc::now(),
    });

    Ok(Json(serde_json::json!({
        "id": node.id,
        "content": node.content,
        "nodeType": node.node_type,
        "tags": node.tags,
        "retentionStrength": node.retention_strength,
        "storageStrength": node.storage_strength,
        "retrievalStrength": node.retrieval_strength,
        "createdAt": node.created_at.to_rfc3339(),
        "updatedAt": node.updated_at.to_rfc3339(),
        "lastAccessedAt": node.last_accessed.to_rfc3339(),
        "reviewCount": node.reps,
    })))
}

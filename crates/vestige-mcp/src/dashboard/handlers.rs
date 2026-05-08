//! Dashboard API endpoint handlers
//!
//! v2.0: Adds cognitive operation endpoints (dream, explore, predict, importance, consolidation)

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Json, Redirect};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::events::VestigeEvent;
use super::state::AppState;

fn log_err(context: &str) -> impl Fn(vestige_core::StorageError) -> StatusCode + '_ {
    move |e| {
        tracing::error!(error = %e, context = context, "Dashboard handler error");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// Redirect root to the React dashboard
pub async fn serve_dashboard() -> Redirect {
    Redirect::permanent("/dashboard")
}

#[derive(Debug, Deserialize)]
pub struct MemoryListParams {
    pub q: Option<String>,
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
        // Use hybrid search
        let results = state.storage
            .hybrid_search(query, limit, 0.3, 0.7)
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

    // No search query — list all memories
    let mut nodes = state.storage
        .get_all_nodes(limit, offset)
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
    let node = state.storage
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
    let deleted = state.storage
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
    let node = state.storage
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
    let node = state.storage
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
        state.storage
            .update_node_content(&id, trimmed)
            .map_err(log_err("update_node_content"))?;
    }

    if let Some(ref new_tags) = body.tags {
        state.storage
            .update_node_tags(&id, new_tags)
            .map_err(log_err("update_node_tags"))?;
    }

    let node = state.storage
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

// =============================================================================
// MAINTENANCE WRAPPERS
//
// These thin REST shims forward to the existing MCP tool functions so the
// dashboard does not duplicate logic. Each accepts the same JSON args as the
// MCP tool and returns its JSON response verbatim.
// =============================================================================

fn maintenance_err(tool: &'static str) -> impl Fn(String) -> StatusCode {
    move |e| {
        tracing::error!(tool = tool, error = %e, "maintenance handler failed");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// `POST /api/maintenance/regenerate-embeddings` — backfill or rebuild embeddings.
pub async fn maintenance_regenerate_embeddings(
    State(state): State<AppState>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, StatusCode> {
    let args = body.map(|Json(v)| v);
    crate::tools::maintenance::execute_regenerate_embeddings(&state.storage, args)
        .await
        .map(Json)
        .map_err(maintenance_err("regenerate_embeddings"))
}

/// `POST /api/maintenance/find-duplicates` — return clusters of near-duplicate memories.
pub async fn maintenance_find_duplicates(
    State(state): State<AppState>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, StatusCode> {
    let args = body.map(|Json(v)| v);
    crate::tools::dedup::execute(&state.storage, args)
        .await
        .map(Json)
        .map_err(maintenance_err("find_duplicates"))
}

/// `POST /api/maintenance/gc` — list (or delete, with dry_run=false) low-retention memories.
pub async fn maintenance_gc(
    State(state): State<AppState>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, StatusCode> {
    let args = body.map(|Json(v)| v);
    crate::tools::maintenance::execute_gc(&state.storage, args)
        .await
        .map(Json)
        .map_err(maintenance_err("gc"))
}

/// `POST /api/maintenance/backup` — write a SQLite snapshot to ~/.vestige/backups.
pub async fn maintenance_backup(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    crate::tools::maintenance::execute_backup(&state.storage, None)
        .await
        .map(Json)
        .map_err(maintenance_err("backup"))
}

/// Body for `POST /api/memories/{id}/review` — FSRS-6 rating.
/// 1=Again, 2=Hard, 3=Good, 4=Easy. Defaults to 3 if omitted.
#[derive(Debug, Deserialize)]
pub struct ReviewBody {
    pub rating: Option<i32>,
}

/// Apply an FSRS-6 review rating to a memory.
///
/// This is the missing piece that lets the dashboard expose Anki/Logseq-style
/// spaced repetition directly. Backend updates `stability`, `difficulty`,
/// `next_review`, increments `reps`/`lapses`, and bumps `retrieval_strength`.
pub async fn review_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, StatusCode> {
    let rating_value = body.rating.unwrap_or(3);
    let rating = vestige_core::Rating::from_i32(rating_value)
        .ok_or(StatusCode::BAD_REQUEST)?;

    let before = state.storage
        .get_node(&id)
        .map_err(log_err("get_node before review"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    let node = state.storage
        .mark_reviewed(&id, rating)
        .map_err(log_err("mark_reviewed"))?;

    let rating_name = match rating {
        vestige_core::Rating::Again => "again",
        vestige_core::Rating::Hard => "hard",
        vestige_core::Rating::Good => "good",
        vestige_core::Rating::Easy => "easy",
    };

    Ok(Json(serde_json::json!({
        "id": node.id,
        "rating": rating_name,
        "previousRetention": before.retention_strength,
        "newRetention": node.retention_strength,
        "previousStability": before.stability,
        "newStability": node.stability,
        "difficulty": node.difficulty,
        "reps": node.reps,
        "lapses": node.lapses,
        "nextReviewAt": node.next_review.map(|d| d.to_rfc3339()),
    })))
}

#[derive(Debug, Deserialize)]
pub struct ReviewQueueParams {
    pub limit: Option<i32>,
}

/// Get the queue of memories due for review (FSRS-6 `next_review <= now`).
///
/// Sorted ascending by `next_review` so the most overdue memories come first.
/// Limit is clamped to `[1, 200]` to keep the response bounded; the dashboard
/// pages through if there are more.
pub async fn get_review_queue(
    State(state): State<AppState>,
    Query(params): Query<ReviewQueueParams>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let nodes = state.storage
        .get_review_queue(limit)
        .map_err(log_err("get_review_queue"))?;

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
                "lastAccessedAt": n.last_accessed.to_rfc3339(),
                "nextReviewAt": n.next_review.map(|d| d.to_rfc3339()),
                "reviewCount": n.reps,
                "difficulty": n.difficulty,
                "stability": n.stability,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "total": formatted.len(),
        "memories": formatted,
    })))
}

/// Get the per-memory changelog (state transitions audit trail).
///
/// Mirrors the MCP `memory_changelog` tool with `memory_id` set, but trimmed
/// to the data the dashboard actually renders (transitions list).
pub async fn get_memory_changelog(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let node = state.storage
        .get_node(&id)
        .map_err(log_err("get_node for changelog"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    let transitions = state.storage
        .get_state_transitions(&id, 100)
        .map_err(log_err("get_state_transitions"))?;

    let formatted: Vec<Value> = transitions
        .iter()
        .map(|t| {
            serde_json::json!({
                "fromState": t.from_state,
                "toState": t.to_state,
                "reasonType": t.reason_type,
                "reasonData": t.reason_data,
                "timestamp": t.timestamp.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "memoryId": id,
        "memoryContent": node.content,
        "totalTransitions": formatted.len(),
        "transitions": formatted,
    })))
}

/// Get system stats
pub async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let stats = state.storage
        .get_stats()
        .map_err(log_err("storage operation"))?;

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "totalMemories": stats.total_nodes,
        "dueForReview": stats.nodes_due_for_review,
        "averageRetention": stats.average_retention,
        "averageStorageStrength": stats.average_storage_strength,
        "averageRetrievalStrength": stats.average_retrieval_strength,
        "withEmbeddings": stats.nodes_with_embeddings,
        "embeddingCoverage": embedding_coverage,
        "embeddingModel": stats.embedding_model,
        "oldestMemory": stats.oldest_memory.map(|dt| dt.to_rfc3339()),
        "newestMemory": stats.newest_memory.map(|dt| dt.to_rfc3339()),
    })))
}

#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    pub days: Option<i64>,
    pub limit: Option<i32>,
}

/// Get timeline data
pub async fn get_timeline(
    State(state): State<AppState>,
    Query(params): Query<TimelineParams>,
) -> Result<Json<Value>, StatusCode> {
    let days = params.days.unwrap_or(7).clamp(1, 90);
    let limit = params.limit.unwrap_or(200).clamp(1, 500);

    let start = Utc::now() - Duration::days(days);
    let nodes = state.storage
        .query_time_range(Some(start), Some(Utc::now()), limit)
        .map_err(log_err("storage operation"))?;

    // Group by day
    let mut by_day: std::collections::BTreeMap<String, Vec<Value>> = std::collections::BTreeMap::new();
    for node in &nodes {
        let date = node.created_at.format("%Y-%m-%d").to_string();
        let content_preview: String = {
            let preview: String = node.content.chars().take(100).collect();
            if preview.len() < node.content.len() {
                format!("{}...", preview)
            } else {
                preview
            }
        };
        by_day.entry(date).or_default().push(serde_json::json!({
            "id": node.id,
            "content": content_preview,
            "nodeType": node.node_type,
            "retentionStrength": node.retention_strength,
            "createdAt": node.created_at.to_rfc3339(),
        }));
    }

    let timeline: Vec<Value> = by_day
        .into_iter()
        .rev()
        .map(|(date, memories)| {
            serde_json::json!({
                "date": date,
                "count": memories.len(),
                "memories": memories,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "days": days,
        "totalMemories": nodes.len(),
        "timeline": timeline,
    })))
}

/// Health check
pub async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let stats = state.storage
        .get_stats()
        .map_err(log_err("storage operation"))?;

    let status = if stats.total_nodes == 0 {
        "empty"
    } else if stats.average_retention < 0.3 {
        "critical"
    } else if stats.average_retention < 0.5 {
        "degraded"
    } else {
        "healthy"
    };

    Ok(Json(serde_json::json!({
        "status": status,
        "totalMemories": stats.total_nodes,
        "averageRetention": stats.average_retention,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

// ============================================================================
// MEMORY GRAPH
// ============================================================================

/// Redirect legacy graph to dashboard graph page
pub async fn serve_graph() -> Redirect {
    Redirect::permanent("/dashboard/graph")
}

#[derive(Debug, Deserialize)]
pub struct GraphParams {
    pub query: Option<String>,
    pub center_id: Option<String>,
    pub depth: Option<u32>,
    pub max_nodes: Option<usize>,
}

/// Get memory graph data (nodes + edges with layout positions)
pub async fn get_graph(
    State(state): State<AppState>,
    Query(params): Query<GraphParams>,
) -> Result<Json<Value>, StatusCode> {
    let depth = params.depth.unwrap_or(2).clamp(1, 3);
    let max_nodes = params.max_nodes.unwrap_or(50).clamp(1, 200);

    // Determine center node
    let center_id = if let Some(ref id) = params.center_id {
        id.clone()
    } else if let Some(ref query) = params.query {
        let results = state.storage
            .search(query, 1)
            .map_err(log_err("storage operation"))?;
        results.first()
            .map(|n| n.id.clone())
            .ok_or(StatusCode::NOT_FOUND)?
    } else {
        // Default: most connected memory (for a rich initial graph)
        let most_connected = state.storage
            .get_most_connected_memory()
            .map_err(log_err("storage operation"))?;
        if let Some(id) = most_connected {
            id
        } else {
            // Fallback: most recent memory
            let recent = state.storage
                .get_all_nodes(1, 0)
                .map_err(log_err("storage operation"))?;
            recent.first()
                .map(|n| n.id.clone())
                .ok_or(StatusCode::NOT_FOUND)?
        }
    };

    // Get subgraph
    let (nodes, edges) = state.storage
        .get_memory_subgraph(&center_id, depth, max_nodes)
        .map_err(log_err("storage operation"))?;

    if nodes.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Build nodes JSON with timestamps for recency calculation
    let nodes_json: Vec<Value> = nodes.iter()
        .map(|n| {
            let label = if n.content.chars().count() > 80 {
                format!("{}...", n.content.chars().take(77).collect::<String>())
            } else {
                n.content.clone()
            };
            serde_json::json!({
                "id": n.id,
                "label": label,
                "type": n.node_type,
                "retention": n.retention_strength,
                "tags": n.tags,
                "createdAt": n.created_at.to_rfc3339(),
                "updatedAt": n.updated_at.to_rfc3339(),
                "isCenter": n.id == center_id,
            })
        })
        .collect();

    let edges_json: Vec<Value> = edges.iter()
        .map(|e| {
            serde_json::json!({
                "source": e.source_id,
                "target": e.target_id,
                "weight": e.strength,
                "type": e.link_type,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "nodes": nodes_json,
        "edges": edges_json,
        "centerId": center_id,
        "depth": depth,
        "nodeCount": nodes.len(),
        "edgeCount": edges.len(),
    })))
}

// ============================================================================
// SEARCH (dedicated endpoint)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<i32>,
    pub min_retention: Option<f64>,
}

/// Search memories with hybrid search
pub async fn search_memories(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let start = std::time::Instant::now();

    let results = state
        .storage
        .hybrid_search(&params.q, limit, 0.3, 0.7)
        .map_err(log_err("storage operation"))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let result_ids: Vec<String> = results.iter().map(|r| r.node.id.clone()).collect();

    // Emit search event
    state.emit(VestigeEvent::SearchPerformed {
        query: params.q.clone(),
        result_count: results.len(),
        result_ids: result_ids.clone(),
        duration_ms,
        timestamp: Utc::now(),
    });

    let formatted: Vec<Value> = results
        .into_iter()
        .filter(|r| {
            params
                .min_retention
                .is_none_or(|min| r.node.retention_strength >= min)
        })
        .map(|r| {
            serde_json::json!({
                "id": r.node.id,
                "content": r.node.content,
                "nodeType": r.node.node_type,
                "tags": r.node.tags,
                "retentionStrength": r.node.retention_strength,
                "combinedScore": r.combined_score,
                "createdAt": r.node.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "query": params.q,
        "total": formatted.len(),
        "durationMs": duration_ms,
        "results": formatted,
    })))
}

// ============================================================================
// COGNITIVE OPERATIONS (v2.0)
// ============================================================================

/// Trigger a dream cycle — delegates to the DreamEngine in `tools::dream`
pub async fn trigger_dream(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let cognitive = state.cognitive.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    state.emit(VestigeEvent::DreamStarted {
        memory_count: 50,
        timestamp: Utc::now(),
    });

    let args = Some(serde_json::json!({ "memory_count": 50 }));
    let result = crate::tools::dream::execute(&state.storage, cognitive, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Dream cycle failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let memories_replayed = result["memoriesReplayed"].as_u64().unwrap_or(0) as usize;
    let connections = result["connectionsPersisted"].as_u64().unwrap_or(0) as usize;
    let insights_count = result["insights"].as_array().map_or(0, |a| a.len());
    let duration_ms = result["stats"]["total_duration_ms"].as_u64().unwrap_or(0);

    state.emit(VestigeEvent::DreamCompleted {
        memories_replayed,
        connections_found: connections,
        insights_generated: insights_count,
        duration_ms,
        timestamp: Utc::now(),
    });

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct ExploreRequest {
    pub from_id: String,
    pub to_id: Option<String>,
    pub action: Option<String>, // "associations", "chains", "bridges"
    pub limit: Option<usize>,
}

type ExploreError = (StatusCode, Json<Value>);

fn explore_error(status: StatusCode, code: &str, message: &str) -> ExploreError {
    (
        status,
        Json(serde_json::json!({ "error": { "code": code, "message": message } })),
    )
}

fn explore_storage_err(context: &'static str) -> impl Fn(vestige_core::StorageError) -> ExploreError {
    move |e| {
        tracing::error!(error = %e, context = context, "Dashboard handler error");
        explore_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            "Internal storage error",
        )
    }
}

/// Explore connections between memories
pub async fn explore_connections(
    State(state): State<AppState>,
    Json(req): Json<ExploreRequest>,
) -> Result<Json<Value>, ExploreError> {
    let action = req.action.as_deref().unwrap_or("associations");
    let limit = req.limit.unwrap_or(10).clamp(1, 50);

    match action {
        "associations" => {
            // Get the source memory content for similarity search
            let source_node = state
                .storage
                .get_node(&req.from_id)
                .map_err(explore_storage_err("explore associations"))?
                .ok_or_else(|| {
                    explore_error(
                        StatusCode::NOT_FOUND,
                        "from_not_found",
                        "Source memory does not exist",
                    )
                })?;

            // Use hybrid search with source content to find associated memories
            let results = state
                .storage
                .hybrid_search(&source_node.content, limit as i32, 0.3, 0.7)
                .map_err(explore_storage_err("storage operation"))?;

            let formatted: Vec<Value> = results
                .iter()
                .filter(|r| r.node.id != req.from_id) // Exclude self
                .map(|r| {
                    serde_json::json!({
                        "id": r.node.id,
                        "content": r.node.content,
                        "nodeType": r.node.node_type,
                        "score": r.combined_score,
                        "retention": r.node.retention_strength,
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({
                "action": "associations",
                "fromId": req.from_id,
                "results": formatted,
            })))
        }
        "chains" | "bridges" => {
            let to_id = req.to_id.as_deref().ok_or_else(|| {
                explore_error(
                    StatusCode::BAD_REQUEST,
                    "to_id_required",
                    "Field `to_id` is required for chains and bridges modes",
                )
            })?;

            let (nodes, edges) = state
                .storage
                .get_memory_subgraph(&req.from_id, 2, limit)
                .map_err(explore_storage_err("storage operation"))?;

            let nodes_json: Vec<Value> = nodes
                .iter()
                .map(|n| {
                    serde_json::json!({
                        "id": n.id,
                        "content": n.content.chars().take(100).collect::<String>(),
                        "nodeType": n.node_type,
                        "retention": n.retention_strength,
                    })
                })
                .collect();

            let edges_json: Vec<Value> = edges
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "source": e.source_id,
                        "target": e.target_id,
                        "weight": e.strength,
                        "type": e.link_type,
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({
                "action": action,
                "fromId": req.from_id,
                "toId": to_id,
                "nodes": nodes_json,
                "edges": edges_json,
            })))
        }
        _ => Err(explore_error(
            StatusCode::BAD_REQUEST,
            "unknown_action",
            "Unknown action — expected one of: associations, chains, bridges",
        )),
    }
}

/// Predict which memories will be needed
pub async fn predict_memories(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    // Get recent memories as predictions based on activity
    let recent = state
        .storage
        .get_all_nodes(10, 0)
        .map_err(log_err("storage operation"))?;

    let predictions: Vec<Value> = recent
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content.chars().take(100).collect::<String>(),
                "nodeType": n.node_type,
                "retention": n.retention_strength,
                "predictedNeed": "high",
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "predictions": predictions,
        "basedOn": "recent_activity",
    })))
}

#[derive(Debug, Deserialize)]
pub struct ImportanceRequest {
    pub content: String,
}

/// Score content importance using 4-channel model
pub async fn score_importance(
    State(state): State<AppState>,
    Json(req): Json<ImportanceRequest>,
) -> Result<Json<Value>, StatusCode> {
    if let Some(ref cognitive) = state.cognitive {
        let context = vestige_core::ImportanceContext::current();
        let cog = cognitive.lock().await;
        let score = cog.importance_signals.compute_importance(&req.content, &context);
        drop(cog);

        let composite = score.composite;
        let novelty = score.novelty;
        let arousal = score.arousal;
        let reward = score.reward;
        let attention = score.attention;

        state.emit(VestigeEvent::ImportanceScored {
            content_preview: req.content.chars().take(80).collect(),
            composite_score: composite,
            novelty,
            arousal,
            reward,
            attention,
            timestamp: Utc::now(),
        });

        Ok(Json(serde_json::json!({
            "composite": composite,
            "channels": {
                "novelty": novelty,
                "arousal": arousal,
                "reward": reward,
                "attention": attention,
            },
            "recommendation": if composite > 0.6 { "save" } else { "skip" },
        })))
    } else {
        // Fallback: basic heuristic scoring
        let word_count = req.content.split_whitespace().count();
        let has_code = req.content.contains("```") || req.content.contains("fn ");
        let composite = if has_code { 0.7 } else { (word_count as f64 / 100.0).min(0.8) };

        Ok(Json(serde_json::json!({
            "composite": composite,
            "channels": {
                "novelty": composite,
                "arousal": 0.5,
                "reward": 0.5,
                "attention": composite,
            },
            "recommendation": if composite > 0.6 { "save" } else { "skip" },
        })))
    }
}

/// Trigger consolidation
pub async fn trigger_consolidation(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    state.emit(VestigeEvent::ConsolidationStarted {
        timestamp: Utc::now(),
    });

    let start = std::time::Instant::now();

    let storage = state.storage.clone();
    let result = tokio::task::spawn_blocking(move || storage.run_consolidation())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Consolidation task panicked");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(log_err("storage operation"))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    state.emit(VestigeEvent::ConsolidationCompleted {
        nodes_processed: result.nodes_processed as usize,
        decay_applied: result.decay_applied as usize,
        embeddings_generated: result.embeddings_generated as usize,
        duration_ms,
        timestamp: Utc::now(),
    });

    Ok(Json(serde_json::json!({
        "nodesProcessed": result.nodes_processed,
        "decayApplied": result.decay_applied,
        "embeddingsGenerated": result.embeddings_generated,
        "duplicatesMerged": result.duplicates_merged,
        "activationsComputed": result.activations_computed,
        "durationMs": duration_ms,
    })))
}

/// Get retention distribution (for histogram visualization)
pub async fn retention_distribution(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    // Cap at 1000 to prevent excessive memory usage on large databases
    let nodes = state
        .storage
        .get_all_nodes(1000, 0)
        .map_err(log_err("storage operation"))?;

    // Build distribution buckets
    let mut buckets = [0u32; 10]; // 0-10%, 10-20%, ..., 90-100%
    let mut by_type: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut endangered = Vec::new();

    for node in &nodes {
        let bucket = ((node.retention_strength * 10.0).floor() as usize).min(9);
        buckets[bucket] += 1;
        *by_type.entry(node.node_type.clone()).or_default() += 1;

        // Endangered: retention below 30%
        if node.retention_strength < 0.3 {
            endangered.push(serde_json::json!({
                "id": node.id,
                "content": node.content.chars().take(60).collect::<String>(),
                "retention": node.retention_strength,
                "nodeType": node.node_type,
            }));
        }
    }

    let distribution: Vec<Value> = buckets
        .iter()
        .enumerate()
        .map(|(i, &count)| {
            serde_json::json!({
                "range": format!("{}-{}%", i * 10, (i + 1) * 10),
                "count": count,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "distribution": distribution,
        "byType": by_type,
        "endangered": endangered,
        "total": nodes.len(),
    })))
}

// ============================================================================
// INTENTIONS (v2.0)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct IntentionListParams {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateIntentionRequest {
    pub content: String,
    pub trigger_type: String,
    pub trigger_value: String,
    pub priority: Option<String>,
    pub deadline: Option<String>,
}

/// Create a new intention via the dashboard
pub async fn create_intention(
    State(state): State<AppState>,
    Json(req): Json<CreateIntentionRequest>,
) -> Result<Json<Value>, StatusCode> {
    let id = uuid::Uuid::new_v4().to_string();
    let priority = match req.priority.as_deref().unwrap_or("medium") {
        "high" => 3,
        "low" => 1,
        _ => 2, // medium
    };
    let deadline = req.deadline.as_ref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .map(|nd| nd.and_hms_opt(23, 59, 59).unwrap())
            .map(|ndt| chrono::DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
    });

    let trigger_data = serde_json::json!({
        "type": req.trigger_type,
        "value": req.trigger_value,
    }).to_string();

    let record = vestige_core::IntentionRecord {
        id: id.clone(),
        content: req.content.clone(),
        trigger_type: req.trigger_type.clone(),
        trigger_data,
        priority,
        status: "active".to_string(),
        created_at: Utc::now(),
        deadline,
        fulfilled_at: None,
        reminder_count: 0,
        last_reminded_at: None,
        notes: None,
        tags: vec![],
        related_memories: vec![],
        snoozed_until: None,
        source_type: "dashboard".to_string(),
        source_data: None,
    };

    state.storage.save_intention(&record)
        .map_err(log_err("storage operation"))?;

    let priority_label = match priority {
        3 => "high",
        1 => "low",
        _ => "medium",
    };

    Ok(Json(serde_json::json!({
        "id": id,
        "intention": {
            "id": id,
            "content": req.content,
            "triggerType": req.trigger_type,
            "triggerValue": req.trigger_value,
            "status": "active",
            "priority": priority_label,
            "createdAt": record.created_at.to_rfc3339(),
            "deadline": deadline.map(|d| d.to_rfc3339()),
        }
    })))
}

/// List intentions
pub async fn list_intentions(
    State(state): State<AppState>,
    Query(params): Query<IntentionListParams>,
) -> Result<Json<Value>, StatusCode> {
    let status_filter = params.status.unwrap_or_else(|| "active".to_string());

    let intentions = if status_filter == "all" {
        let mut all = state.storage.get_active_intentions()
            .map_err(log_err("list intentions (active)"))?;
        all.extend(state.storage.get_intentions_by_status("fulfilled")
            .map_err(log_err("list intentions (fulfilled)"))?);
        all.extend(state.storage.get_intentions_by_status("cancelled")
            .map_err(log_err("list intentions (cancelled)"))?);
        all.extend(state.storage.get_intentions_by_status("snoozed")
            .map_err(log_err("list intentions (snoozed)"))?);
        all
    } else if status_filter == "active" {
        state.storage.get_active_intentions()
            .map_err(log_err("list intentions"))?
    } else {
        state.storage.get_intentions_by_status(&status_filter)
            .map_err(log_err("list intentions"))?
    };

    let count = intentions.len();
    let intentions_json: Vec<Value> = intentions.iter().map(|r| {
        let trigger_value = match serde_json::from_str::<Value>(&r.trigger_data) {
            Ok(v) => v.get("value")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            Err(e) => {
                tracing::debug!(id = %r.id, raw = %r.trigger_data, error = %e, "Malformed trigger_data");
                String::new()
            }
        };
        let priority_label = match r.priority {
            3 => "high",
            1 => "low",
            _ => "medium",
        };
        serde_json::json!({
            "id": r.id,
            "content": r.content,
            "triggerType": r.trigger_type,
            "triggerValue": trigger_value,
            "status": r.status,
            "priority": priority_label,
            "createdAt": r.created_at.to_rfc3339(),
            "deadline": r.deadline.map(|d| d.to_rfc3339()),
            "snoozedUntil": r.snoozed_until.map(|d| d.to_rfc3339()),
        })
    }).collect();
    Ok(Json(serde_json::json!({
        "intentions": intentions_json,
        "total": count,
        "filter": status_filter,
    })))
}

// ============================================================================
// METACOGNITIVE TOOLS (v2.1)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ReflectRequest {
    pub focus: Option<String>,
    pub depth: Option<String>,
}

pub async fn trigger_reflect(
    State(state): State<AppState>,
    Json(req): Json<ReflectRequest>,
) -> Result<Json<Value>, StatusCode> {
    let cognitive = state.cognitive.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let args = Some(serde_json::json!({
        "focus": req.focus,
        "depth": req.depth.unwrap_or_else(|| "standard".to_string()),
    }));

    let result = crate::tools::reflect::execute(&state.storage, cognitive, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Reflect failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct TemporalRequest {
    pub action: String,
    pub topic: Option<String>,
    pub memory_id: Option<String>,
    pub limit: Option<i64>,
}

pub async fn query_temporal(
    State(state): State<AppState>,
    Json(req): Json<TemporalRequest>,
) -> Result<Json<Value>, StatusCode> {
    let args = Some(serde_json::json!({
        "action": req.action,
        "topic": req.topic,
        "memory_id": req.memory_id,
        "limit": req.limit.unwrap_or(20),
    }));

    let result = crate::tools::temporal::execute(&state.storage, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Temporal query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct ConfidenceRequest {
    pub action: String,
    pub memory_id: Option<String>,
    pub limit: Option<i64>,
}

pub async fn query_confidence(
    State(state): State<AppState>,
    Json(req): Json<ConfidenceRequest>,
) -> Result<Json<Value>, StatusCode> {
    let args = Some(serde_json::json!({
        "action": req.action,
        "memory_id": req.memory_id,
        "limit": req.limit.unwrap_or(20),
    }));

    let result = crate::tools::confidence::execute(&state.storage, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Confidence query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

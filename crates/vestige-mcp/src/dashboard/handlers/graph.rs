//! Dashboard handlers — graph view and explore
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;

use super::super::state::AppState;
use super::{log_err, log_join_err};

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
    // Cap raised from 200 to 500 to match the dashboard's `maxNodes`
    // dropdown options. Previously the dashboard offered 300 and 500
    // entries that silently capped server-side at 200 — a misleading UI
    // contract. 500 + depth=3 is still bounded by `get_memory_subgraph`'s
    // BFS with cycle detection and runs on `spawn_blocking`, so the
    // larger ceiling doesn't risk reactor stalls. Beyond 500, the 3D
    // rendering frame budget becomes the bottleneck rather than the SQL
    // query — semantic clustering / LOD work is the right next step
    // there.
    let max_nodes = params.max_nodes.unwrap_or(50).clamp(1, 500);

    // Determine center node
    let center_id = if let Some(ref id) = params.center_id {
        id.clone()
    } else if let Some(ref query) = params.query {
        let results = state
            .storage
            .search(query, 1)
            .map_err(log_err("storage operation"))?;
        results
            .first()
            .map(|n| n.id.clone())
            .ok_or(StatusCode::NOT_FOUND)?
    } else {
        // Default: most connected memory (for a rich initial graph)
        let most_connected = state
            .storage
            .get_most_connected_memory()
            .map_err(log_err("storage operation"))?;
        if let Some(id) = most_connected {
            id
        } else {
            // Fallback: most recent memory
            let recent = state
                .storage
                .get_all_nodes(1, 0)
                .map_err(log_err("storage operation"))?;
            recent
                .first()
                .map(|n| n.id.clone())
                .ok_or(StatusCode::NOT_FOUND)?
        }
    };

    // get_memory_subgraph runs a depth-bounded BFS over the connections
    // table and joins back to knowledge_nodes — easily hundreds of rows
    // for depth=3, max_nodes=200 on dense graphs. Move it off the reactor.
    let storage = state.storage.clone();
    let center_id_owned = center_id.clone();
    let (nodes, edges) = tokio::task::spawn_blocking(move || {
        storage.get_memory_subgraph(&center_id_owned, depth, max_nodes)
    })
    .await
    .map_err(log_join_err("get_memory_subgraph task panicked"))?
    .map_err(log_err("storage operation"))?;

    if nodes.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Build nodes JSON with timestamps for recency calculation
    let nodes_json: Vec<Value> = nodes
        .iter()
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
        "nodes": nodes_json,
        "edges": edges_json,
        "centerId": center_id,
        "depth": depth,
        "nodeCount": nodes.len(),
        "edgeCount": edges.len(),
    })))
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

fn explore_storage_err(
    context: &'static str,
) -> impl Fn(vestige_core::StorageError) -> ExploreError {
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

            // Use hybrid search with source content to find associated memories.
            // hybrid_search re-embeds the query (~150–500ms cold) — keep it
            // off the async reactor.
            let storage = state.storage.clone();
            let source_content = source_node.content.clone();
            let limit_i32 = limit as i32;
            let results = tokio::task::spawn_blocking(move || {
                storage.hybrid_search(&source_content, limit_i32, 0.3, 0.7)
            })
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "hybrid_search task panicked");
                explore_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_panic",
                    "Internal task panic",
                )
            })?
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

            let storage = state.storage.clone();
            let from_id_owned = req.from_id.clone();
            let (nodes, edges) = tokio::task::spawn_blocking(move || {
                storage.get_memory_subgraph(&from_id_owned, 2, limit)
            })
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "get_memory_subgraph task panicked");
                explore_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_panic",
                    "Internal task panic",
                )
            })?
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

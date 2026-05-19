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
use super::super::wire::{
    ExploreResponseDto, ExploreResultDto, GraphEdgeDto, GraphNodeDto, GraphResponseDto,
};
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
) -> Result<Json<GraphResponseDto>, StatusCode> {
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

    let node_count = nodes.len();
    let edge_count = edges.len();

    let nodes_dto: Vec<GraphNodeDto> = nodes
        .iter()
        .map(|n| {
            // 80-char cap matches the legacy serializer; keeps the
            // labels readable in the 3D renderer without overflowing
            // sprite atlases.
            let label = if n.content.chars().count() > 80 {
                format!("{}...", n.content.chars().take(77).collect::<String>())
            } else {
                n.content.clone()
            };
            GraphNodeDto {
                id: n.id.clone(),
                label,
                node_type: n.node_type.clone(),
                retention: n.retention_strength,
                tags: n.tags.clone(),
                created_at: n.created_at.to_rfc3339(),
                updated_at: n.updated_at.to_rfc3339(),
                is_center: n.id == center_id,
            }
        })
        .collect();

    let edges_dto: Vec<GraphEdgeDto> = edges
        .iter()
        .map(|e| GraphEdgeDto {
            source: e.source_id.clone(),
            target: e.target_id.clone(),
            weight: e.strength,
            link_type: e.link_type.clone(),
        })
        .collect();

    Ok(Json(GraphResponseDto {
        nodes: nodes_dto,
        edges: edges_dto,
        center_id,
        depth,
        node_count,
        edge_count,
    }))
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

/// Explore connections between memories. Three modes:
/// - `associations`: hybrid search seeded with the source memory's
///   content; returns ranked similar memories.
/// - `chains` / `bridges`: BFS subgraph between `from_id` and `to_id`.
pub async fn explore_connections(
    State(state): State<AppState>,
    Json(req): Json<ExploreRequest>,
) -> Result<Json<ExploreResponseDto>, ExploreError> {
    let action = req.action.as_deref().unwrap_or("associations");
    let limit = req.limit.unwrap_or(10).clamp(1, 50);

    match action {
        "associations" => {
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

            let results_dto: Vec<ExploreResultDto> = results
                .iter()
                .filter(|r| r.node.id != req.from_id) // Exclude self
                .map(|r| ExploreResultDto {
                    id: r.node.id.clone(),
                    content: r.node.content.clone(),
                    node_type: Some(r.node.node_type.clone()),
                    score: Some(f64::from(r.combined_score)),
                    retention: Some(r.node.retention_strength),
                    ..ExploreResultDto::default()
                })
                .collect();

            Ok(Json(ExploreResponseDto {
                action: "associations".to_string(),
                from_id: req.from_id,
                to_id: None,
                results: results_dto,
                nodes: None,
                edges: None,
            }))
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

            // The chains/bridges UI shows two surfaces: a flat result
            // list (each node abbreviated) plus the subgraph the 3D
            // overlay highlights. We emit both.
            let results_dto: Vec<ExploreResultDto> = nodes
                .iter()
                .map(|n| ExploreResultDto {
                    id: n.id.clone(),
                    content: n.content.chars().take(100).collect::<String>(),
                    node_type: Some(n.node_type.clone()),
                    retention: Some(n.retention_strength),
                    ..ExploreResultDto::default()
                })
                .collect();

            let nodes_dto: Vec<GraphNodeDto> = nodes
                .iter()
                .map(|n| GraphNodeDto {
                    id: n.id.clone(),
                    label: n.content.chars().take(80).collect::<String>(),
                    node_type: n.node_type.clone(),
                    retention: n.retention_strength,
                    tags: n.tags.clone(),
                    created_at: n.created_at.to_rfc3339(),
                    updated_at: n.updated_at.to_rfc3339(),
                    is_center: n.id == req.from_id,
                })
                .collect();

            let edges_dto: Vec<GraphEdgeDto> = edges
                .iter()
                .map(|e| GraphEdgeDto {
                    source: e.source_id.clone(),
                    target: e.target_id.clone(),
                    weight: e.strength,
                    link_type: e.link_type.clone(),
                })
                .collect();

            Ok(Json(ExploreResponseDto {
                action: action.to_string(),
                from_id: req.from_id,
                to_id: Some(to_id.to_string()),
                results: results_dto,
                nodes: Some(nodes_dto),
                edges: Some(edges_dto),
            }))
        }
        _ => Err(explore_error(
            StatusCode::BAD_REQUEST,
            "unknown_action",
            "Unknown action — expected one of: associations, chains, bridges",
        )),
    }
}

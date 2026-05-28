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
    // Cap raised from 500 to 1000 as a deliberate UX call so users on
    // faster hardware can opt in to denser graphs. The earlier bump
    // (200 → 500) aligned the server clamp with the dropdown so the UI
    // wouldn't lie. The latest bump (500 → 1000) keeps that property —
    // the dropdown now offers `1000` and the server honors it. The SQL
    // side (`get_memory_subgraph` BFS with cycle detection, dispatched
    // via `spawn_blocking`) still scales comfortably; the bottleneck at
    // this point is the 3D frame budget in `Graph3D`. The proper fix is
    // semantic clustering / LOD, tracked separately — until that lands,
    // 1000 is the responsible ceiling. The shared source of truth lives
    // in `dashboard::wire::limits::DashboardLimitsDto::DEFAULT`; the
    // limits-parity test pins these together.
    let max_nodes = params.max_nodes.unwrap_or(50).clamp(1, 1000);

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
            let (kw, sem) = vestige_core::default_hybrid_weights();
            let results = tokio::task::spawn_blocking(move || {
                storage.hybrid_search(&source_content, limit_i32, kw, sem)
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

            // Depth cap matches the `explore` MCP tool's default — wide
            // enough to surface real multi-hop connections, narrow enough
            // that BFS stays bounded on dense graphs.
            const MAX_PATH_DEPTH: u32 = 6;

            // Pathfinding + connection lookups go through SQLite; keep
            // them off the async reactor.
            let storage = state.storage.clone();
            let from_id_owned = req.from_id.clone();
            let to_id_owned = to_id.to_string();
            let path_result = tokio::task::spawn_blocking(move || {
                storage.find_path_between(&from_id_owned, &to_id_owned, MAX_PATH_DEPTH)
            })
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "find_path_between task panicked");
                explore_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_panic",
                    "Internal task panic",
                )
            })?
            .map_err(explore_storage_err("storage operation"))?;

            let Some(path) = path_result else {
                // No chain between the two memories. Honest empty
                // response instead of a misleading "subgraph around A"
                // fallback that the UI used to render as if it answered
                // the question.
                return Ok(Json(ExploreResponseDto {
                    action: action.to_string(),
                    from_id: req.from_id,
                    to_id: Some(to_id.to_string()),
                    results: Vec::new(),
                    nodes: Some(Vec::new()),
                    edges: Some(Vec::new()),
                }));
            };

            // Hydrate node payloads for the path. `get_node` is a
            // single-row read each, but BFS only returns up to
            // MAX_PATH_DEPTH+1 IDs so the cost is bounded.
            let storage_for_nodes = state.storage.clone();
            let path_for_nodes = path.clone();
            let nodes_on_path = tokio::task::spawn_blocking(move || {
                let mut out = Vec::with_capacity(path_for_nodes.len());
                for id in &path_for_nodes {
                    if let Some(node) = storage_for_nodes.get_node(id)? {
                        out.push(node);
                    }
                }
                Ok::<_, vestige_core::StorageError>(out)
            })
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "hydrate path-nodes task panicked");
                explore_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_panic",
                    "Internal task panic",
                )
            })?
            .map_err(explore_storage_err("storage operation"))?;

            // Edges between consecutive nodes on the path. We pull each
            // node's connections and filter to the canonical pair so the
            // 3D overlay can draw exactly the highlighted route.
            let storage_for_edges = state.storage.clone();
            let path_for_edges = path.clone();
            let edges_on_path = tokio::task::spawn_blocking(move || {
                let mut edges = Vec::new();
                for window in path_for_edges.windows(2) {
                    let (a, b) = (&window[0], &window[1]);
                    let conns = storage_for_edges.get_connections_for_memory(a)?;
                    if let Some(conn) = conns.iter().find(|c| {
                        (c.source_id == *a && c.target_id == *b)
                            || (c.source_id == *b && c.target_id == *a)
                    }) {
                        edges.push(conn.clone());
                    }
                }
                Ok::<_, vestige_core::StorageError>(edges)
            })
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "hydrate path-edges task panicked");
                explore_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_panic",
                    "Internal task panic",
                )
            })?
            .map_err(explore_storage_err("storage operation"))?;

            // `chains` shows the whole route (endpoints + intermediates).
            // `bridges` answers "what sits between A and B", so it
            // strips the endpoints — empty list if the chain is direct
            // (one-hop), which is the truthful answer.
            let result_slice: &[_] = if action == "bridges" {
                if nodes_on_path.len() <= 2 {
                    &[]
                } else {
                    &nodes_on_path[1..nodes_on_path.len() - 1]
                }
            } else {
                &nodes_on_path[..]
            };

            let results_dto: Vec<ExploreResultDto> = result_slice
                .iter()
                .take(limit)
                .map(|n| ExploreResultDto {
                    id: n.id.clone(),
                    content: n.content.chars().take(100).collect::<String>(),
                    node_type: Some(n.node_type.clone()),
                    retention: Some(n.retention_strength),
                    ..ExploreResultDto::default()
                })
                .collect();

            // The 3D overlay always renders the full route so users can
            // see both endpoints + bridges, regardless of which mode
            // they picked. Centering on `from_id` keeps the camera
            // anchored where they started.
            let nodes_dto: Vec<GraphNodeDto> = nodes_on_path
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

            let edges_dto: Vec<GraphEdgeDto> = edges_on_path
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use std::sync::Arc;
    use tempfile::tempdir;
    use vestige_core::Storage;
    use vestige_core::memory::IngestInput;
    use vestige_core::storage::ConnectionRecord;

    fn make_state() -> (AppState, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let storage = Arc::new(Storage::new(Some(db_path)).unwrap());
        let cognitive = Some(Arc::new(tokio::sync::Mutex::new(CognitiveEngine::new())));
        (AppState::new(storage, cognitive), dir)
    }

    fn ingest(state: &AppState, content: &str) -> String {
        state
            .storage
            .ingest(IngestInput {
                content: content.to_string(),
                ..Default::default()
            })
            .unwrap()
            .id
    }

    fn link(state: &AppState, source: &str, target: &str) {
        let now = chrono::Utc::now();
        state
            .storage
            .save_connection(&ConnectionRecord {
                source_id: source.to_string(),
                target_id: target.to_string(),
                strength: 0.8,
                link_type: "related".to_string(),
                created_at: now,
                last_activated: now,
                activation_count: 0,
            })
            .unwrap();
    }

    /// Regression for the BFS-ignores-`to_id` bug. The old handler
    /// returned the subgraph around `from_id` regardless of the target,
    /// so `chains` from A→D came back as "everything reachable from A
    /// within 2 hops" — silently dropping D when the chain was longer
    /// than 2, and never highlighting the actual path. The new contract:
    /// when a chain exists, the response includes every memory on the
    /// path from `from_id` to `to_id`, in order.
    #[tokio::test]
    async fn chains_returns_path_from_source_to_target() {
        let (state, _dir) = make_state();
        let a = ingest(&state, "A — origin");
        let b = ingest(&state, "B — relay");
        let c = ingest(&state, "C — relay");
        let d = ingest(&state, "D — destination");
        link(&state, &a, &b);
        link(&state, &b, &c);
        link(&state, &c, &d);

        let resp = explore_connections(
            State(state),
            Json(ExploreRequest {
                from_id: a.clone(),
                to_id: Some(d.clone()),
                action: Some("chains".to_string()),
                limit: Some(10),
            }),
        )
        .await
        .expect("chains should succeed for a connected graph");

        let ids: Vec<&str> = resp.0.results.iter().map(|r| r.id.as_str()).collect();
        assert!(
            ids.contains(&a.as_str()) && ids.contains(&d.as_str()),
            "results must contain both endpoints, got {ids:?}"
        );
        assert!(
            ids.contains(&b.as_str()) || ids.contains(&c.as_str()),
            "results must walk through at least one intermediate, got {ids:?}"
        );
        let nodes = resp.0.nodes.as_ref().expect("nodes payload");
        let node_ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(
            node_ids.contains(&d.as_str()),
            "subgraph for the 3D overlay must include the destination, got {node_ids:?}"
        );
    }

    /// `bridges` is a different question from `chains`: it answers
    /// "which memories sit between A and D" — explicitly excluding the
    /// endpoints. The old handler couldn't answer this at all because
    /// it ignored `to_id`; this pins the new contract.
    #[tokio::test]
    async fn bridges_returns_intermediates_only() {
        let (state, _dir) = make_state();
        let a = ingest(&state, "A");
        let b = ingest(&state, "B");
        let c = ingest(&state, "C");
        let d = ingest(&state, "D");
        link(&state, &a, &b);
        link(&state, &b, &c);
        link(&state, &c, &d);

        let resp = explore_connections(
            State(state),
            Json(ExploreRequest {
                from_id: a.clone(),
                to_id: Some(d.clone()),
                action: Some("bridges".to_string()),
                limit: Some(10),
            }),
        )
        .await
        .expect("bridges should succeed when a path exists");

        let result_ids: Vec<&str> = resp.0.results.iter().map(|r| r.id.as_str()).collect();
        assert!(
            !result_ids.contains(&a.as_str()),
            "bridges must not include the source, got {result_ids:?}"
        );
        assert!(
            !result_ids.contains(&d.as_str()),
            "bridges must not include the destination, got {result_ids:?}"
        );
        assert!(
            result_ids.contains(&b.as_str()) || result_ids.contains(&c.as_str()),
            "bridges should surface at least one intermediate, got {result_ids:?}"
        );
    }

    /// When no chain exists between the two memories, the handler must
    /// not lie — it returns an empty `results`/`nodes`/`edges` payload
    /// so the dashboard can show its "no connection found" empty state.
    /// The old code returned the subgraph around `from_id` and the UI
    /// happily rendered it as if the two memories were related.
    #[tokio::test]
    async fn chains_returns_empty_when_no_path_exists() {
        let (state, _dir) = make_state();
        let a = ingest(&state, "A — island 1");
        let b = ingest(&state, "B — island 1");
        let c = ingest(&state, "C — island 2");
        let d = ingest(&state, "D — island 2");
        link(&state, &a, &b);
        link(&state, &c, &d);

        let resp = explore_connections(
            State(state),
            Json(ExploreRequest {
                from_id: a.clone(),
                to_id: Some(d.clone()),
                action: Some("chains".to_string()),
                limit: Some(10),
            }),
        )
        .await
        .expect("chains should succeed even when no path is found");

        assert!(
            resp.0.results.is_empty(),
            "no path → no results, got {:?}",
            resp.0.results
        );
        assert!(
            resp.0
                .nodes
                .as_ref()
                .map(|n| n.is_empty())
                .unwrap_or(true),
            "no path → no subgraph nodes"
        );
    }
}

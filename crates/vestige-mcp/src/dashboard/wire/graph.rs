//! Wire DTOs for the graph view (`GET /api/graph`) and explore endpoint
//! (`POST /api/explore`).
//!
//! Frontend counterpart: `GraphResponse`, `GraphNode`, `GraphEdge`,
//! `ExploreResponse`, `ExploreResult` in `apps/dashboard/src/types/index.ts`.

use serde::Serialize;
use ts_rs::TS;

/// One node in a memory subgraph snapshot.
///
/// `label` is the truncated content (≤80 chars) — full content lives in
/// the memory detail endpoint. `is_center` flags the BFS root for the 3D
/// renderer's pinning logic.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "GraphNodeDto.ts", rename_all = "camelCase")]
pub struct GraphNodeDto {
    pub id: String,
    pub label: String,
    /// Renamed from the storage `node_type` to plain `type` because the
    /// dashboard's color-mapper keys on this exact field name.
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub node_type: String,
    pub retention: f64,
    pub tags: Vec<String>,
    /// ISO-8601 — used by the recency tinting on the 3D nodes.
    pub created_at: String,
    pub updated_at: String,
    pub is_center: bool,
}

/// One edge — undirected weight + a string discriminator
/// ("similarity", "co-recall", "explicit", …) the renderer maps to a
/// color and label.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "GraphEdgeDto.ts")]
pub struct GraphEdgeDto {
    pub source: String,
    pub target: String,
    pub weight: f64,
    /// Storage column is `link_type`; we expose plain `type` to the
    /// dashboard so the renderer doesn't need to translate.
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub link_type: String,
}

/// Response for `GET /api/graph` — a node-and-edge snapshot of one
/// memory's neighborhood.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "GraphResponseDto.ts", rename_all = "camelCase")]
pub struct GraphResponseDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
    pub center_id: String,
    pub depth: u32,
    pub node_count: usize,
    pub edge_count: usize,
}

/// One result row from `POST /api/explore`. Optional fields cover the
/// three explore modes — `associations` populates `score` and
/// `retention`, `chains`/`bridges` populate `connection_type`.
#[derive(Debug, Clone, Serialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ExploreResultDto.ts", rename_all = "camelCase")]
pub struct ExploreResultDto {
    pub id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub similarity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub retention: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub connection_type: Option<String>,
}

/// Wrapper for `POST /api/explore` — discriminator (`action`) plus
/// per-mode result list. `chains` and `bridges` modes also return the
/// underlying subgraph for the 3D renderer.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ExploreResponseDto.ts", rename_all = "camelCase")]
pub struct ExploreResponseDto {
    /// "associations" | "chains" | "bridges".
    pub action: String,
    pub from_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_id: Option<String>,
    pub results: Vec<ExploreResultDto>,
    /// Only populated for `chains`/`bridges`. The 3D explore overlay
    /// uses these to highlight the path in the existing graph view.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub nodes: Option<Vec<GraphNodeDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub edges: Option<Vec<GraphEdgeDto>>,
}

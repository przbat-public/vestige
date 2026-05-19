//! Dashboard handlers — Insight Tier surface (Proposal B).
//!
//! `GET /api/insights` returns every node of type `"insight"` whose
//! `extra_json` carries a valid `InsightMetadata`. Filtering and
//! pagination happen server-side; legacy insights that only live in
//! the `InsightRecord` table (no host KnowledgeNode) are out of scope
//! here — they remain reachable through the existing dream history
//! surface.
//!
//! Single endpoint for now; the detail view piggybacks on
//! `GET /api/memories/:id` like decisions do.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;

use super::super::state::AppState;
use super::super::wire::{InsightDto, InsightListResponseDto};
use super::{log_err, log_join_err};

/// Query string for `GET /api/insights`.
#[derive(Debug, Deserialize)]
pub struct InsightListParams {
    /// `true` to only return insights flipped to `validatedByAgent=true`.
    /// Defaults to `false` (return everything, with `validated` flag set).
    #[serde(default)]
    pub only_validated: bool,
    /// `true` to only return *unvalidated* insights — the inverse of
    /// `only_validated`. Set both to false to get the full list.
    #[serde(default)]
    pub only_unvalidated: bool,
    /// Filter to a single origin (`"dream"`, `"synthesized"`,
    /// `"reflect"`, `"manual"`). Optional.
    pub origin: Option<String>,
    /// Hard cap on result count. Defaults to 100, clamped to [1, 500].
    pub limit: Option<i32>,
}

/// `GET /api/insights` — list synthesised insights backed by structured
/// metadata.
///
/// Implementation notes:
///
/// * Walks `get_nodes_by_type_and_tag("insight", None, limit)` so we
///   reuse the existing type index instead of scanning the table.
/// * Drops legacy InsightRecord-only rows silently — they don't have a
///   host KnowledgeNode. The dream history dashboard panel surfaces
///   those.
/// * `only_validated` + `only_unvalidated` combined returns the empty
///   set (intentional — agents that pass both contradictory filters
///   meant to test the endpoint, not to query data).
pub async fn list_insights(
    State(state): State<AppState>,
    Query(params): Query<InsightListParams>,
) -> Result<Json<InsightListResponseDto>, StatusCode> {
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    let only_validated = params.only_validated;
    let only_unvalidated = params.only_unvalidated;
    let origin_filter = params.origin.clone();

    let storage = state.storage.clone();
    let nodes = tokio::task::spawn_blocking(move || {
        storage.get_nodes_by_type_and_tag("insight", None, limit)
    })
    .await
    .map_err(log_join_err("list_insights task panicked"))?
    .map_err(log_err("list_insights storage"))?;

    let insights: Vec<InsightDto> = nodes
        .iter()
        .filter_map(|n| {
            vestige_core::memory::extract_insight(n.extra_json.as_ref())
                .map(|metadata| (n, metadata))
        })
        .filter(|(_, m)| {
            if let Some(want) = &origin_filter {
                m.origin.as_str() == want.as_str()
            } else {
                true
            }
        })
        .filter(|(_, m)| !only_validated || m.validated_by_agent)
        .filter(|(_, m)| !only_unvalidated || !m.validated_by_agent)
        .map(|(n, m)| InsightDto::from_metadata(n, &m))
        .collect();

    let validated_count = insights.iter().filter(|i| i.validated).count();
    Ok(Json(InsightListResponseDto {
        total: insights.len(),
        validated_count,
        insights,
    }))
}

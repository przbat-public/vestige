//! Dashboard handlers — Topic Hubs surface (Proposal A).
//!
//! `GET /api/hubs` returns every node of type `"hub"` whose
//! `extra_json.hub` parses into valid `HubMetadata`. Sorted by
//! `last_regenerated_at` descending so the most active clusters surface
//! first.
//!
//! Detail/expand views piggyback on the existing `GET /api/memories/:id`
//! endpoint — `child_ids` is on the wire payload, the dashboard fetches
//! members through the normal memory API.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;

use super::super::state::AppState;
use super::super::wire::{HubDto, HubListResponseDto};
use super::{log_err, log_join_err};

/// Query string for `GET /api/hubs`.
#[derive(Debug, Deserialize)]
pub struct HubListParams {
    /// Hard cap on result count. Defaults to 100, clamped to [1, 500].
    pub limit: Option<i32>,
}

/// `GET /api/hubs` — list every Topic Hub the dream cycle has
/// synthesised, ordered by most-recently regenerated.
pub async fn list_hubs(
    State(state): State<AppState>,
    Query(params): Query<HubListParams>,
) -> Result<Json<HubListResponseDto>, StatusCode> {
    let limit = params.limit.unwrap_or(100).clamp(1, 500);

    let storage = state.storage.clone();
    let nodes = tokio::task::spawn_blocking(move || {
        storage.get_nodes_by_type_and_tag("hub", None, limit)
    })
    .await
    .map_err(log_join_err("list_hubs task panicked"))?
    .map_err(log_err("list_hubs storage"))?;

    let mut hubs: Vec<HubDto> = nodes
        .iter()
        .filter_map(|n| {
            vestige_core::memory::extract_hub(n.extra_json.as_ref())
                .map(|metadata| HubDto::from_metadata(n, &metadata))
        })
        .collect();

    // Most recently regenerated first — the dream cycle bumps
    // `last_regenerated_at` on every refresh, so this surfaces the
    // hottest clusters.
    hubs.sort_by(|a, b| b.last_regenerated_at.cmp(&a.last_regenerated_at));

    Ok(Json(HubListResponseDto {
        total: hubs.len(),
        hubs,
    }))
}

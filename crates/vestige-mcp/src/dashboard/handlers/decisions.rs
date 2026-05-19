//! Dashboard handlers — Decision Matrix surface (Proposal C).
//!
//! `GET /api/decisions` returns every decision node whose `extra_json`
//! carries a valid `DecisionPayload`. Filtering and pagination happen
//! server-side; legacy Markdown decisions (no payload) are silently
//! dropped.
//!
//! Single endpoint for now — once we have a stable list view the detail
//! view will piggyback on the existing `GET /api/memories/:id` route.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;

use super::super::state::AppState;
use super::super::wire::{DecisionDto, DecisionListResponseDto};
use super::{log_err, log_join_err};

/// Query string for `GET /api/decisions`.
#[derive(Debug, Deserialize)]
pub struct DecisionListParams {
    /// Filter to a single codebase tag (`codebase:<name>`). Optional —
    /// when omitted the endpoint returns decisions across all codebases.
    pub codebase: Option<String>,
    /// `true` to only return decisions whose `validUntil` has elapsed.
    /// Defaults to `false` (return everything, with `expired` flag set).
    #[serde(default)]
    pub only_expired: bool,
    /// Hard cap on result count. Defaults to 100, clamped to [1, 500].
    pub limit: Option<i32>,
}

/// `GET /api/decisions` — list decisions with structured matrix payload.
///
/// Implementation notes:
///
/// * Walks `get_nodes_by_type_and_tag("decision", …)` from the storage
///   layer so we reuse the existing tag index instead of doing a full
///   table scan.
/// * Drops legacy Markdown decisions silently — they lack
///   `extra_json.decision`. The dashboard's "Legacy decisions" panel
///   reads from `GET /api/memories?type=decision` if it needs them.
/// * Pre-computes the `expired` flag with a single `Utc::now()` so every
///   row in a given response uses the same reference time.
pub async fn list_decisions(
    State(state): State<AppState>,
    Query(params): Query<DecisionListParams>,
) -> Result<Json<DecisionListResponseDto>, StatusCode> {
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    let codebase = params.codebase.clone();
    let only_expired = params.only_expired;

    let storage = state.storage.clone();
    let nodes = tokio::task::spawn_blocking(move || {
        let tag = codebase.as_ref().map(|c| format!("codebase:{}", c));
        storage.get_nodes_by_type_and_tag("decision", tag.as_deref(), limit)
    })
    .await
    .map_err(log_join_err("list_decisions task panicked"))?
    .map_err(log_err("list_decisions storage"))?;

    let now = Utc::now();
    let decisions: Vec<DecisionDto> = nodes
        .iter()
        .filter_map(|n| {
            vestige_core::memory::extract_decision(n.extra_json.as_ref())
                .map(|payload| DecisionDto::from_payload(n, &payload, now))
        })
        .filter(|d| !only_expired || d.expired)
        .collect();

    Ok(Json(DecisionListResponseDto {
        total: decisions.len(),
        decisions,
    }))
}

//! Dashboard handlers — memory search
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;

use super::super::events::VestigeEvent;
use super::super::state::AppState;
use super::log_err;

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

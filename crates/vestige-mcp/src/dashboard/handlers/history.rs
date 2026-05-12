//! Dashboard handlers — changelog and timeline
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::super::state::AppState;
use super::log_err;

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

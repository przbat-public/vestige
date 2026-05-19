//! Dashboard handlers — changelog and timeline
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::{Duration, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;

use super::super::state::AppState;
use super::super::wire::{
    MemoryChangelogDto, MemoryChangelogEntryDto, TimelineDayDto, TimelineMemoryDto,
    TimelineResponseDto,
};
use super::{log_err, log_join_err};

/// Get the per-memory changelog (state transitions audit trail).
///
/// Mirrors the MCP `memory_changelog` tool with `memory_id` set, but
/// trimmed to the data the dashboard actually renders (transitions list).
pub async fn get_memory_changelog(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryChangelogDto>, StatusCode> {
    // get_node + get_state_transitions are two short SELECTs but still
    // belong on the blocking pool; the inner Option preserves "not
    // found" so the handler can map it back to 404 cleanly.
    let storage = state.storage.clone();
    let id_owned = id.clone();
    let pair = tokio::task::spawn_blocking(move || -> vestige_core::Result<_> {
        let Some(node) = storage.get_node(&id_owned)? else {
            return Ok(None);
        };
        let transitions = storage.get_state_transitions(&id_owned, 100)?;
        Ok(Some((node, transitions)))
    })
    .await
    .map_err(log_join_err("changelog task panicked"))?
    .map_err(log_err("get_state_transitions"))?;
    let (node, transitions) = pair.ok_or(StatusCode::NOT_FOUND)?;

    let formatted: Vec<MemoryChangelogEntryDto> = transitions
        .iter()
        .map(|t| MemoryChangelogEntryDto {
            from_state: t.from_state.clone(),
            to_state: t.to_state.clone(),
            reason_type: t.reason_type.clone(),
            reason_data: t.reason_data.clone(),
            timestamp: t.timestamp.to_rfc3339(),
        })
        .collect();

    Ok(Json(MemoryChangelogDto {
        memory_id: id,
        memory_content: node.content,
        total_transitions: formatted.len(),
        transitions: formatted,
    }))
}

#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    pub days: Option<i64>,
    pub limit: Option<i32>,
}

/// Get timeline data — memories grouped by `created_at` day, newest
/// first.
pub async fn get_timeline(
    State(state): State<AppState>,
    Query(params): Query<TimelineParams>,
) -> Result<Json<TimelineResponseDto>, StatusCode> {
    let days = params.days.unwrap_or(7).clamp(1, 90);
    let limit = params.limit.unwrap_or(200).clamp(1, 500);

    let start = Utc::now() - Duration::days(days);
    let now = Utc::now();
    let storage = state.storage.clone();
    let nodes = tokio::task::spawn_blocking(move || {
        storage.query_time_range(Some(start), Some(now), limit)
    })
    .await
    .map_err(log_join_err("query_time_range task panicked"))?
    .map_err(log_err("storage operation"))?;

    let mut by_day: BTreeMap<String, Vec<TimelineMemoryDto>> = BTreeMap::new();
    for node in &nodes {
        let date = node.created_at.format("%Y-%m-%d").to_string();
        let preview: String = node.content.chars().take(100).collect();
        let content = if preview.len() < node.content.len() {
            format!("{}...", preview)
        } else {
            preview
        };
        by_day.entry(date).or_default().push(TimelineMemoryDto {
            id: node.id.clone(),
            content,
            node_type: node.node_type.clone(),
            retention_strength: node.retention_strength,
            created_at: node.created_at.to_rfc3339(),
        });
    }

    let timeline: Vec<TimelineDayDto> = by_day
        .into_iter()
        .rev()
        .map(|(date, memories)| TimelineDayDto {
            date,
            count: memories.len(),
            memories,
        })
        .collect();

    Ok(Json(TimelineResponseDto {
        days,
        total_memories: nodes.len(),
        timeline,
    }))
}

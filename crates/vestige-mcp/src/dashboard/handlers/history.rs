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
    DashboardLimitsDto, MemoryChangelogDto, MemoryChangelogEntryDto, MemoryRevisionDto,
    MemoryRevisionsDto, TimelineDayDto, TimelineMemoryDto, TimelineResponseDto,
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

/// Query parameters for `GET /api/memories/{id}/revisions`.
#[derive(Debug, Deserialize)]
pub struct RevisionParams {
    pub limit: Option<i64>,
}

/// Get a memory's *content* history (`memory_revisions`, V17) — newest first.
///
/// This is deliberately a separate endpoint from `…/changelog`: the changelog
/// answers "what state was this memory in", and this answers "what did it say
/// before". Both panels are labelled for what they actually show, which is why
/// neither is called "edit history" any more.
///
/// `limit` comes from the caller but is clamped against
/// [`DashboardLimitsDto::DEFAULT`] — the documented limits pattern, so the
/// dashboard reads its page size from `GET /api/_meta/limits` instead of a
/// component-local constant.
pub async fn get_memory_revisions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<RevisionParams>,
) -> Result<Json<MemoryRevisionsDto>, StatusCode> {
    let limits = DashboardLimitsDto::DEFAULT;
    let limit = params
        .limit
        .unwrap_or(i64::from(limits.revision_history_limit_default))
        .clamp(1, i64::from(limits.revision_history_limit_max));

    // get_node + get_memory_revisions are two short SELECTs, but history rows
    // carry two copies of the memory's text each; keep them off the reactor.
    // The inner Option preserves "not found" so the handler can answer 404
    // rather than an empty history for a memory that does not exist.
    let storage = state.storage.clone();
    let id_owned = id.clone();
    let revisions = tokio::task::spawn_blocking(move || -> vestige_core::Result<_> {
        if storage.get_node(&id_owned)?.is_none() {
            return Ok(None);
        }
        let revisions = storage.get_memory_revisions(&id_owned, limit)?;
        Ok(Some(revisions))
    })
    .await
    .map_err(log_join_err("revisions task panicked"))?
    .map_err(log_err("get_memory_revisions"))?
    .ok_or(StatusCode::NOT_FOUND)?;

    let formatted: Vec<MemoryRevisionDto> = revisions.iter().map(MemoryRevisionDto::from).collect();

    Ok(Json(MemoryRevisionsDto {
        memory_id: id,
        total: formatted.len(),
        revisions: formatted,
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, header};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tower::ServiceExt;
    use vestige_core::{IngestInput, Storage};

    /// Same port the router is normally built with, so the OriginGuard layer
    /// sees the same-origin Host it expects.
    const PORT: u16 = 3927;

    fn storage_at(path: &std::path::Path) -> Arc<Storage> {
        Arc::new(Storage::new(Some(path.to_path_buf())).unwrap())
    }

    fn ingest(storage: &Storage, content: &str) -> String {
        storage
            .ingest(IngestInput {
                content: content.to_string(),
                node_type: "fact".to_string(),
                tags: vec!["seed".to_string()],
                ..Default::default()
            })
            .unwrap()
            .id
    }

    async fn get_revisions(storage: Arc<Storage>, uri: &str) -> axum::response::Response {
        let (app, _state): (Router, AppState) = crate::dashboard::build_router(storage, None, PORT);
        app.oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::HOST, format!("127.0.0.1:{PORT}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
    }

    /// Every write that changes what a memory says appends a revision; the
    /// dashboard could not read a single one of them before this route existed,
    /// so its "edit history" panel showed state transitions instead.
    #[tokio::test]
    async fn revisions_route_returns_the_content_history_newest_first() {
        let dir = TempDir::new().unwrap();
        let storage = storage_at(&dir.path().join("revisions.db"));
        let id = ingest(&storage, "first wording");
        storage
            .update_node_content(&id, "second wording")
            .expect("edit must append a revision");

        let response =
            get_revisions(storage.clone(), &format!("/api/memories/{id}/revisions")).await;
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(body["memoryId"], id);
        assert_eq!(body["total"], 2);
        let revisions = body["revisions"].as_array().unwrap();
        assert_eq!(revisions[0]["kind"], "edit", "newest first: {body}");
        assert_eq!(revisions[0]["oldContent"], "first wording");
        assert_eq!(revisions[0]["newContent"], "second wording");
        assert_eq!(revisions[1]["kind"], "create");
        assert_eq!(revisions[1]["newContent"], "first wording");
        assert!(
            revisions[0]["recordedAt"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "a revision without its storage clock cannot date the change: {body}"
        );
    }

    /// A memory that does not exist must not answer with an empty history —
    /// "no revisions" and "no such memory" are different claims.
    #[tokio::test]
    async fn revisions_route_answers_404_for_an_unknown_memory() {
        let dir = TempDir::new().unwrap();
        let storage = storage_at(&dir.path().join("missing.db"));

        let response = get_revisions(storage, "/api/memories/does-not-exist/revisions").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// The page size is clamped against `DashboardLimitsDto`, not a constant
    /// buried in the handler: `?limit=0` asks for *some* history, not none.
    #[tokio::test]
    async fn revisions_route_clamps_the_page_size() {
        let dir = TempDir::new().unwrap();
        let storage = storage_at(&dir.path().join("clamp.db"));
        let id = ingest(&storage, "only wording");

        let response =
            get_revisions(storage, &format!("/api/memories/{id}/revisions?limit=0")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(body["total"], 1);
    }
}

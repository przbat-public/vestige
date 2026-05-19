//! Dashboard handlers — FSRS-6 review
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;

use super::super::state::AppState;
use super::super::wire::{MemoryDto, ReviewItemDto, ReviewQueueResponseDto, ReviewResultDto};
use super::{log_err, log_join_err};

/// Body for `POST /api/memories/{id}/review` — FSRS-6 rating.
/// 1=Again, 2=Hard, 3=Good, 4=Easy. Defaults to 3 if omitted.
#[derive(Debug, Deserialize)]
pub struct ReviewBody {
    pub rating: Option<i32>,
}

/// Apply an FSRS-6 review rating to a memory.
///
/// This is the missing piece that lets the dashboard expose Anki/Logseq-style
/// spaced repetition directly. Backend updates `stability`, `difficulty`,
/// `next_review`, increments `reps`/`lapses`, and bumps
/// `retrieval_strength`.
pub async fn review_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<ReviewResultDto>, StatusCode> {
    let rating_value = body.rating.unwrap_or(3);
    let rating = vestige_core::Rating::from_i32(rating_value).ok_or(StatusCode::BAD_REQUEST)?;

    // mark_reviewed does a SELECT + UPDATE plus FSRS-6 math; group both
    // reads into one blocking task. The inner `Option` preserves "not
    // found" so the outer handler can still map it to 404.
    let storage = state.storage.clone();
    let id_owned = id.clone();
    let pair = tokio::task::spawn_blocking(move || -> vestige_core::Result<_> {
        let Some(before) = storage.get_node(&id_owned)? else {
            return Ok(None);
        };
        let node = storage.mark_reviewed(&id_owned, rating)?;
        Ok(Some((before, node)))
    })
    .await
    .map_err(log_join_err("review_memory task panicked"))?
    .map_err(log_err("mark_reviewed"))?;
    let (before, node) = pair.ok_or(StatusCode::NOT_FOUND)?;

    let rating_name = match rating {
        vestige_core::Rating::Again => "again",
        vestige_core::Rating::Hard => "hard",
        vestige_core::Rating::Good => "good",
        vestige_core::Rating::Easy => "easy",
    };

    Ok(Json(ReviewResultDto {
        id: node.id,
        rating: rating_name.to_string(),
        previous_retention: before.retention_strength,
        new_retention: node.retention_strength,
        previous_stability: before.stability,
        new_stability: node.stability,
        difficulty: node.difficulty,
        reps: node.reps,
        lapses: node.lapses,
        next_review_at: node.next_review.map(|d| d.to_rfc3339()),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ReviewQueueParams {
    pub limit: Option<i32>,
}

/// Get the queue of memories due for review (FSRS-6 `next_review <= now`).
///
/// Sorted ascending by `next_review` so the most overdue memories come
/// first. Limit is clamped to `[1, 200]` to keep the response bounded;
/// the dashboard pages through if there are more.
pub async fn get_review_queue(
    State(state): State<AppState>,
    Query(params): Query<ReviewQueueParams>,
) -> Result<Json<ReviewQueueResponseDto>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let storage = state.storage.clone();
    let nodes = tokio::task::spawn_blocking(move || storage.get_review_queue(limit))
        .await
        .map_err(log_join_err("get_review_queue task panicked"))?
        .map_err(log_err("get_review_queue"))?;

    let memories: Vec<ReviewItemDto> = nodes
        .iter()
        .map(|n| ReviewItemDto {
            memory: MemoryDto::from(n),
            difficulty: n.difficulty,
            stability: n.stability,
        })
        .collect();

    Ok(Json(ReviewQueueResponseDto {
        total: memories.len(),
        memories,
    }))
}

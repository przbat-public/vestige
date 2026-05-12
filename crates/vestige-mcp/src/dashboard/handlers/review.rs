//! Dashboard handlers — FSRS-6 review
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;

use super::super::state::AppState;
use super::log_err;

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
/// `next_review`, increments `reps`/`lapses`, and bumps `retrieval_strength`.
pub async fn review_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, StatusCode> {
    let rating_value = body.rating.unwrap_or(3);
    let rating = vestige_core::Rating::from_i32(rating_value)
        .ok_or(StatusCode::BAD_REQUEST)?;

    let before = state.storage
        .get_node(&id)
        .map_err(log_err("get_node before review"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    let node = state.storage
        .mark_reviewed(&id, rating)
        .map_err(log_err("mark_reviewed"))?;

    let rating_name = match rating {
        vestige_core::Rating::Again => "again",
        vestige_core::Rating::Hard => "hard",
        vestige_core::Rating::Good => "good",
        vestige_core::Rating::Easy => "easy",
    };

    Ok(Json(serde_json::json!({
        "id": node.id,
        "rating": rating_name,
        "previousRetention": before.retention_strength,
        "newRetention": node.retention_strength,
        "previousStability": before.stability,
        "newStability": node.stability,
        "difficulty": node.difficulty,
        "reps": node.reps,
        "lapses": node.lapses,
        "nextReviewAt": node.next_review.map(|d| d.to_rfc3339()),
    })))
}

#[derive(Debug, Deserialize)]
pub struct ReviewQueueParams {
    pub limit: Option<i32>,
}

/// Get the queue of memories due for review (FSRS-6 `next_review <= now`).
///
/// Sorted ascending by `next_review` so the most overdue memories come first.
/// Limit is clamped to `[1, 200]` to keep the response bounded; the dashboard
/// pages through if there are more.
pub async fn get_review_queue(
    State(state): State<AppState>,
    Query(params): Query<ReviewQueueParams>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let nodes = state.storage
        .get_review_queue(limit)
        .map_err(log_err("get_review_queue"))?;

    let formatted: Vec<Value> = nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content,
                "nodeType": n.node_type,
                "tags": n.tags,
                "retentionStrength": n.retention_strength,
                "storageStrength": n.storage_strength,
                "retrievalStrength": n.retrieval_strength,
                "createdAt": n.created_at.to_rfc3339(),
                "updatedAt": n.updated_at.to_rfc3339(),
                "lastAccessedAt": n.last_accessed.to_rfc3339(),
                "nextReviewAt": n.next_review.map(|d| d.to_rfc3339()),
                "reviewCount": n.reps,
                "difficulty": n.difficulty,
                "stability": n.stability,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "total": formatted.len(),
        "memories": formatted,
    })))
}

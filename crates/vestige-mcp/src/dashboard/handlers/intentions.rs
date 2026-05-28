//! Dashboard handlers — prospective memory (intentions)
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;

use super::super::state::AppState;
use super::super::wire::{
    CreateIntentionResponseDto, IntentionItemDto, IntentionListResponseDto,
    UpdateIntentionRequestDto, UpdateIntentionResponseDto,
};
use super::{log_err, log_join_err};

/// Statuses the dashboard is allowed to set on an intention.
///
/// The storage layer accepts any string, but the dashboard contract is
/// narrower: only the four lifecycle states the UI knows how to render.
/// Rejecting unknowns at the boundary keeps junk out of the table and
/// matches the MCP `intention(action="update")` semantics.
const ALLOWED_UPDATE_STATUSES: &[&str] = &["fulfilled", "cancelled", "snoozed", "active"];

#[derive(Debug, Deserialize)]
pub struct IntentionListParams {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateIntentionRequest {
    pub content: String,
    pub trigger_type: String,
    pub trigger_value: String,
    pub priority: Option<String>,
    pub deadline: Option<String>,
}

/// Map an `IntentionRecord` (storage shape) to the wire DTO. Pulled out
/// of the list handler so the create response uses the same projection.
fn record_to_dto(r: &vestige_core::IntentionRecord) -> IntentionItemDto {
    let trigger_value = match serde_json::from_str::<Value>(&r.trigger_data) {
        Ok(v) => v
            .get("value")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default(),
        Err(e) => {
            tracing::debug!(
                id = %r.id,
                raw = %r.trigger_data,
                error = %e,
                "Malformed trigger_data — exposing empty triggerValue"
            );
            String::new()
        }
    };
    let priority_label = match r.priority {
        3 => "high",
        1 => "low",
        _ => "medium",
    };
    IntentionItemDto {
        id: r.id.clone(),
        content: r.content.clone(),
        trigger_type: r.trigger_type.clone(),
        trigger_value,
        status: r.status.clone(),
        priority: priority_label.to_string(),
        created_at: r.created_at.to_rfc3339(),
        deadline: r.deadline.map(|d| d.to_rfc3339()),
        snoozed_until: r.snoozed_until.map(|d| d.to_rfc3339()),
    }
}

/// Create a new intention via the dashboard.
///
/// Encodes the trigger_value into the storage envelope and persists
/// with `priority` mapped from the string label to the storage int
/// (1=low, 2=medium, 3=high).
pub async fn create_intention(
    State(state): State<AppState>,
    Json(req): Json<CreateIntentionRequest>,
) -> Result<Json<CreateIntentionResponseDto>, StatusCode> {
    let id = uuid::Uuid::new_v4().to_string();
    let priority = match req.priority.as_deref().unwrap_or("medium") {
        "high" => 3,
        "low" => 1,
        _ => 2,
    };
    let deadline = req.deadline.as_ref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .map(|nd| nd.and_hms_opt(23, 59, 59).unwrap())
            .map(|ndt| chrono::DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
    });

    let trigger_data = serde_json::json!({
        "type": req.trigger_type,
        "value": req.trigger_value,
    })
    .to_string();

    let record = vestige_core::IntentionRecord {
        id: id.clone(),
        content: req.content.clone(),
        trigger_type: req.trigger_type.clone(),
        trigger_data,
        priority,
        status: "active".to_string(),
        created_at: Utc::now(),
        deadline,
        fulfilled_at: None,
        reminder_count: 0,
        last_reminded_at: None,
        notes: None,
        tags: vec![],
        related_memories: vec![],
        snoozed_until: None,
        source_type: "dashboard".to_string(),
        source_data: None,
    };

    let storage = state.storage.clone();
    let record_clone = record.clone();
    tokio::task::spawn_blocking(move || storage.save_intention(&record_clone))
        .await
        .map_err(log_join_err("save_intention task panicked"))?
        .map_err(log_err("storage operation"))?;

    let intention = record_to_dto(&record);
    Ok(Json(CreateIntentionResponseDto { id, intention }))
}

/// List intentions, optionally filtered by status.
///
/// `?status=all` fans out into four separate queries (active, fulfilled,
/// cancelled, snoozed). `?status=active` (default) is the cheapest path.
/// Anything else passes through to `get_intentions_by_status`.
pub async fn list_intentions(
    State(state): State<AppState>,
    Query(params): Query<IntentionListParams>,
) -> Result<Json<IntentionListResponseDto>, StatusCode> {
    let status_filter = params.status.unwrap_or_else(|| "active".to_string());

    let storage = state.storage.clone();
    let filter = status_filter.clone();
    let intentions = tokio::task::spawn_blocking(move || -> vestige_core::Result<_> {
        if filter == "all" {
            let mut all = storage.get_active_intentions()?;
            all.extend(storage.get_intentions_by_status("fulfilled")?);
            all.extend(storage.get_intentions_by_status("cancelled")?);
            all.extend(storage.get_intentions_by_status("snoozed")?);
            Ok(all)
        } else if filter == "active" {
            storage.get_active_intentions()
        } else {
            storage.get_intentions_by_status(&filter)
        }
    })
    .await
    .map_err(log_join_err("list_intentions task panicked"))?
    .map_err(log_err("list intentions"))?;

    let dtos: Vec<IntentionItemDto> = intentions.iter().map(record_to_dto).collect();
    let total = dtos.len();
    Ok(Json(IntentionListResponseDto {
        intentions: dtos,
        total,
        filter: status_filter,
    }))
}

/// Update an intention's lifecycle status.
///
/// `PATCH /api/intentions/{id}` — flips an active intention to
/// `fulfilled`, `cancelled`, `snoozed`, or back to `active`. Returns
/// 400 for any other status, 404 when the id doesn't exist, 500 on
/// storage errors.
pub async fn update_intention(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<UpdateIntentionRequestDto>,
) -> Result<Json<UpdateIntentionResponseDto>, StatusCode> {
    if !ALLOWED_UPDATE_STATUSES.contains(&req.status.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let storage = state.storage.clone();
    let id_for_task = id.clone();
    let status_for_task = req.status.clone();
    let updated = tokio::task::spawn_blocking(move || {
        storage.update_intention_status(&id_for_task, &status_for_task)
    })
    .await
    .map_err(log_join_err("update_intention task panicked"))?
    .map_err(log_err("update intention status"))?;

    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(UpdateIntentionResponseDto {
        id,
        status: req.status,
        updated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use axum::extract::Path;
    use std::sync::Arc;
    use tempfile::tempdir;
    use vestige_core::Storage;

    fn make_state() -> (AppState, tempfile::TempDir) {
        // Keep TempDir alive for the duration of the test so the SQLite
        // file is not removed under our feet.
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let storage = Arc::new(Storage::new(Some(db_path)).unwrap());
        let cognitive = Some(Arc::new(tokio::sync::Mutex::new(CognitiveEngine::new())));
        (AppState::new(storage, cognitive), dir)
    }

    async fn seed_active_intention(state: &AppState) -> String {
        let req = CreateIntentionRequest {
            content: "Test intention".to_string(),
            trigger_type: "context".to_string(),
            trigger_value: "testing".to_string(),
            priority: Some("medium".to_string()),
            deadline: None,
        };
        let resp = create_intention(State(state.clone()), Json(req))
            .await
            .expect("create_intention should succeed");
        resp.0.id
    }

    #[tokio::test]
    async fn update_intention_marks_active_intention_as_fulfilled() {
        let (state, _dir) = make_state();
        let id = seed_active_intention(&state).await;

        let resp = update_intention(
            State(state.clone()),
            Path(id.clone()),
            Json(UpdateIntentionRequestDto {
                status: "fulfilled".to_string(),
            }),
        )
        .await
        .expect("update should succeed for existing intention");

        assert_eq!(resp.0.id, id);
        assert_eq!(resp.0.status, "fulfilled");
        assert!(resp.0.updated);

        let list = list_intentions(
            State(state),
            Query(IntentionListParams {
                status: Some("fulfilled".to_string()),
            }),
        )
        .await
        .expect("list should succeed");

        assert_eq!(list.0.total, 1);
        assert_eq!(list.0.intentions[0].id, id);
        assert_eq!(list.0.intentions[0].status, "fulfilled");
    }

    #[tokio::test]
    async fn update_intention_rejects_unknown_status_with_400() {
        let (state, _dir) = make_state();
        let id = seed_active_intention(&state).await;

        let err = update_intention(
            State(state),
            Path(id),
            Json(UpdateIntentionRequestDto {
                status: "garbage".to_string(),
            }),
        )
        .await
        .expect_err("unknown status must be rejected");

        assert_eq!(err, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_intention_returns_404_for_unknown_id() {
        let (state, _dir) = make_state();

        let err = update_intention(
            State(state),
            Path("00000000-0000-0000-0000-000000000000".to_string()),
            Json(UpdateIntentionRequestDto {
                status: "fulfilled".to_string(),
            }),
        )
        .await
        .expect_err("missing intention must 404");

        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_intention_accepts_cancel_and_snooze_and_reactivate() {
        // One round-trip per allowed status confirms the allow-list isn't
        // accidentally narrowed to just `fulfilled`.
        for status in ["cancelled", "snoozed", "active"] {
            let (state, _dir) = make_state();
            let id = seed_active_intention(&state).await;

            let resp = update_intention(
                State(state),
                Path(id.clone()),
                Json(UpdateIntentionRequestDto {
                    status: status.to_string(),
                }),
            )
            .await
            .unwrap_or_else(|e| {
                panic!("status `{status}` should be accepted, got {e:?}");
            });

            assert_eq!(resp.0.status, status);
            assert!(resp.0.updated);
        }
    }
}

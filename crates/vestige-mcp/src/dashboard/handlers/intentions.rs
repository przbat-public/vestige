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
};
use super::{log_err, log_join_err};

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

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
use super::log_err;

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

/// Create a new intention via the dashboard
pub async fn create_intention(
    State(state): State<AppState>,
    Json(req): Json<CreateIntentionRequest>,
) -> Result<Json<Value>, StatusCode> {
    let id = uuid::Uuid::new_v4().to_string();
    let priority = match req.priority.as_deref().unwrap_or("medium") {
        "high" => 3,
        "low" => 1,
        _ => 2, // medium
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
    }).to_string();

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

    state.storage.save_intention(&record)
        .map_err(log_err("storage operation"))?;

    let priority_label = match priority {
        3 => "high",
        1 => "low",
        _ => "medium",
    };

    Ok(Json(serde_json::json!({
        "id": id,
        "intention": {
            "id": id,
            "content": req.content,
            "triggerType": req.trigger_type,
            "triggerValue": req.trigger_value,
            "status": "active",
            "priority": priority_label,
            "createdAt": record.created_at.to_rfc3339(),
            "deadline": deadline.map(|d| d.to_rfc3339()),
        }
    })))
}

/// List intentions
pub async fn list_intentions(
    State(state): State<AppState>,
    Query(params): Query<IntentionListParams>,
) -> Result<Json<Value>, StatusCode> {
    let status_filter = params.status.unwrap_or_else(|| "active".to_string());

    let intentions = if status_filter == "all" {
        let mut all = state.storage.get_active_intentions()
            .map_err(log_err("list intentions (active)"))?;
        all.extend(state.storage.get_intentions_by_status("fulfilled")
            .map_err(log_err("list intentions (fulfilled)"))?);
        all.extend(state.storage.get_intentions_by_status("cancelled")
            .map_err(log_err("list intentions (cancelled)"))?);
        all.extend(state.storage.get_intentions_by_status("snoozed")
            .map_err(log_err("list intentions (snoozed)"))?);
        all
    } else if status_filter == "active" {
        state.storage.get_active_intentions()
            .map_err(log_err("list intentions"))?
    } else {
        state.storage.get_intentions_by_status(&status_filter)
            .map_err(log_err("list intentions"))?
    };

    let count = intentions.len();
    let intentions_json: Vec<Value> = intentions.iter().map(|r| {
        let trigger_value = match serde_json::from_str::<Value>(&r.trigger_data) {
            Ok(v) => v.get("value")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            Err(e) => {
                tracing::debug!(id = %r.id, raw = %r.trigger_data, error = %e, "Malformed trigger_data");
                String::new()
            }
        };
        let priority_label = match r.priority {
            3 => "high",
            1 => "low",
            _ => "medium",
        };
        serde_json::json!({
            "id": r.id,
            "content": r.content,
            "triggerType": r.trigger_type,
            "triggerValue": trigger_value,
            "status": r.status,
            "priority": priority_label,
            "createdAt": r.created_at.to_rfc3339(),
            "deadline": r.deadline.map(|d| d.to_rfc3339()),
            "snoozedUntil": r.snoozed_until.map(|d| d.to_rfc3339()),
        })
    }).collect();
    Ok(Json(serde_json::json!({
        "intentions": intentions_json,
        "total": count,
        "filter": status_filter,
    })))
}

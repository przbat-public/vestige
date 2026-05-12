//! Dashboard handlers — reflect, temporal, confidence
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;

use super::super::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ReflectRequest {
    pub focus: Option<String>,
    pub depth: Option<String>,
}

pub async fn trigger_reflect(
    State(state): State<AppState>,
    Json(req): Json<ReflectRequest>,
) -> Result<Json<Value>, StatusCode> {
    let cognitive = state.cognitive.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let args = Some(serde_json::json!({
        "focus": req.focus,
        "depth": req.depth.unwrap_or_else(|| "standard".to_string()),
    }));

    let result = crate::tools::reflect::execute(&state.storage, cognitive, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Reflect failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct TemporalRequest {
    pub action: String,
    pub topic: Option<String>,
    pub memory_id: Option<String>,
    pub limit: Option<i64>,
}

pub async fn query_temporal(
    State(state): State<AppState>,
    Json(req): Json<TemporalRequest>,
) -> Result<Json<Value>, StatusCode> {
    let args = Some(serde_json::json!({
        "action": req.action,
        "topic": req.topic,
        "memory_id": req.memory_id,
        "limit": req.limit.unwrap_or(20),
    }));

    let result = crate::tools::temporal::execute(&state.storage, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Temporal query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct ConfidenceRequest {
    pub action: String,
    pub memory_id: Option<String>,
    pub limit: Option<i64>,
}

pub async fn query_confidence(
    State(state): State<AppState>,
    Json(req): Json<ConfidenceRequest>,
) -> Result<Json<Value>, StatusCode> {
    let args = Some(serde_json::json!({
        "action": req.action,
        "memory_id": req.memory_id,
        "limit": req.limit.unwrap_or(20),
    }));

    let result = crate::tools::confidence::execute(&state.storage, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Confidence query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

//! Dashboard handlers — reflect, temporal, confidence
//!
//! These three handlers delegate to MCP tools that return
//! `serde_json::Value`. The wire DTOs in `dashboard::wire::metacognitive`
//! are the typed contract the dashboard relies on; we deserialize the
//! tool's `Value` through them on the way out so any drift surfaces as
//! a 502-shaped error here rather than as a silent
//! `undefined`/`NaN`/missing-field on the dashboard.
//!
//! The parse cost is negligible (these payloads are kilobytes at most)
//! and the safety win is the whole point of the wire layer.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;

use super::super::state::AppState;
use super::super::wire::{
    ConfidenceResultDto, DeepReferenceResultDto, ReflectResultDto, TemporalResultDto,
};

#[derive(Debug, Deserialize)]
pub struct ReflectRequest {
    pub focus: Option<String>,
    pub depth: Option<String>,
}

/// Trigger a reflection cycle. Delegates to `tools::reflect::execute`
/// and parses the result through `ReflectResultDto` so any drift in the
/// upstream tool's wire shape surfaces as a 502 here.
pub async fn trigger_reflect(
    State(state): State<AppState>,
    Json(req): Json<ReflectRequest>,
) -> Result<Json<ReflectResultDto>, StatusCode> {
    let cognitive = state
        .cognitive
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let args = Some(serde_json::json!({
        "focus": req.focus,
        "depth": req.depth.unwrap_or_else(|| "standard".to_string()),
    }));

    let raw = crate::tools::reflect::execute(&state.storage, cognitive, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Reflect failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    parse_or_502::<ReflectResultDto>(raw, "reflect")
}

#[derive(Debug, Deserialize)]
pub struct TemporalRequest {
    pub action: String,
    pub topic: Option<String>,
    pub memory_id: Option<String>,
    pub limit: Option<i64>,
}

/// Query temporal memory versions. Same DTO-parse pattern as reflect.
pub async fn query_temporal(
    State(state): State<AppState>,
    Json(req): Json<TemporalRequest>,
) -> Result<Json<TemporalResultDto>, StatusCode> {
    let args = Some(serde_json::json!({
        "action": req.action,
        "topic": req.topic,
        "memory_id": req.memory_id,
        "limit": req.limit.unwrap_or(20),
    }));

    let raw = crate::tools::temporal::execute(&state.storage, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Temporal query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    parse_or_502::<TemporalResultDto>(raw, "temporal")
}

#[derive(Debug, Deserialize)]
pub struct ConfidenceRequest {
    pub action: String,
    pub memory_id: Option<String>,
    pub limit: Option<i64>,
}

/// Query confidence scores. Same DTO-parse pattern as reflect.
pub async fn query_confidence(
    State(state): State<AppState>,
    Json(req): Json<ConfidenceRequest>,
) -> Result<Json<ConfidenceResultDto>, StatusCode> {
    let args = Some(serde_json::json!({
        "action": req.action,
        "memory_id": req.memory_id,
        "limit": req.limit.unwrap_or(20),
    }));

    let raw = crate::tools::confidence::execute(&state.storage, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Confidence query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    parse_or_502::<ConfidenceResultDto>(raw, "confidence")
}

#[derive(Debug, Deserialize)]
pub struct DeepReferenceRequest {
    pub query: String,
    pub depth: Option<i64>,
}

/// Run a deep_reference reasoning cycle across all memories. Same
/// DTO-parse pattern as the other metacognitive handlers — the
/// upstream tool returns a `Value`, we re-validate through the wire
/// contract so dashboard shape drift surfaces as a 502.
pub async fn query_deep_reference(
    State(state): State<AppState>,
    Json(req): Json<DeepReferenceRequest>,
) -> Result<Json<DeepReferenceResultDto>, StatusCode> {
    let cognitive = state
        .cognitive
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    if req.query.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let args = Some(serde_json::json!({
        "query": req.query,
        "depth": req.depth.unwrap_or(20),
    }));

    let raw = crate::tools::cross_reference::execute(&state.storage, cognitive, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "deep_reference failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    parse_or_502::<DeepReferenceResultDto>(raw, "deep_reference")
}

/// Try to deserialize the upstream tool's `Value` into a typed DTO. On
/// failure we log the field that broke (so contributors can spot the
/// drift in stderr) and surface 502 BAD GATEWAY — semantically: "the
/// upstream service responded with something we don't understand."
fn parse_or_502<T>(raw: Value, source: &'static str) -> Result<Json<T>, StatusCode>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value::<T>(raw.clone()).map(Json).map_err(|e| {
        tracing::error!(
            source = source,
            error = %e,
            payload_preview = %raw.to_string().chars().take(300).collect::<String>(),
            "Wire DTO drift — tool returned a shape the dashboard contract doesn't accept"
        );
        StatusCode::BAD_GATEWAY
    })
}

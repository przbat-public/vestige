//! Dashboard handlers — cognitive operations (dream, predict, importance, consolidation)
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;

use super::super::events::VestigeEvent;
use super::super::state::AppState;
use super::log_err;

/// Trigger a dream cycle — delegates to the DreamEngine in `tools::dream`
pub async fn trigger_dream(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let cognitive = state.cognitive.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    state.emit(VestigeEvent::DreamStarted {
        memory_count: 50,
        timestamp: Utc::now(),
    });

    let args = Some(serde_json::json!({ "memory_count": 50 }));
    let result = crate::tools::dream::execute(&state.storage, cognitive, args)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Dream cycle failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let memories_replayed = result["memoriesReplayed"].as_u64().unwrap_or(0) as usize;
    let connections = result["connectionsPersisted"].as_u64().unwrap_or(0) as usize;
    let insights_count = result["insights"].as_array().map_or(0, |a| a.len());
    let duration_ms = result["stats"]["total_duration_ms"].as_u64().unwrap_or(0);

    state.emit(VestigeEvent::DreamCompleted {
        memories_replayed,
        connections_found: connections,
        insights_generated: insights_count,
        duration_ms,
        timestamp: Utc::now(),
    });

    Ok(Json(result))
}

/// Predict which memories will be needed
pub async fn predict_memories(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    // Get recent memories as predictions based on activity
    let recent = state
        .storage
        .get_all_nodes(10, 0)
        .map_err(log_err("storage operation"))?;

    let predictions: Vec<Value> = recent
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content.chars().take(100).collect::<String>(),
                "nodeType": n.node_type,
                "retention": n.retention_strength,
                "predictedNeed": "high",
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "predictions": predictions,
        "basedOn": "recent_activity",
    })))
}

#[derive(Debug, Deserialize)]
pub struct ImportanceRequest {
    pub content: String,
}

/// Score content importance using 4-channel model
pub async fn score_importance(
    State(state): State<AppState>,
    Json(req): Json<ImportanceRequest>,
) -> Result<Json<Value>, StatusCode> {
    if let Some(ref cognitive) = state.cognitive {
        let context = vestige_core::ImportanceContext::current();
        let cog = cognitive.lock().await;
        let score = cog.importance_signals.compute_importance(&req.content, &context);
        drop(cog);

        let composite = score.composite;
        let novelty = score.novelty;
        let arousal = score.arousal;
        let reward = score.reward;
        let attention = score.attention;

        state.emit(VestigeEvent::ImportanceScored {
            content_preview: req.content.chars().take(80).collect(),
            composite_score: composite,
            novelty,
            arousal,
            reward,
            attention,
            timestamp: Utc::now(),
        });

        Ok(Json(serde_json::json!({
            "composite": composite,
            "channels": {
                "novelty": novelty,
                "arousal": arousal,
                "reward": reward,
                "attention": attention,
            },
            "recommendation": if composite > 0.6 { "save" } else { "skip" },
        })))
    } else {
        // Fallback: basic heuristic scoring
        let word_count = req.content.split_whitespace().count();
        let has_code = req.content.contains("```") || req.content.contains("fn ");
        let composite = if has_code { 0.7 } else { (word_count as f64 / 100.0).min(0.8) };

        Ok(Json(serde_json::json!({
            "composite": composite,
            "channels": {
                "novelty": composite,
                "arousal": 0.5,
                "reward": 0.5,
                "attention": composite,
            },
            "recommendation": if composite > 0.6 { "save" } else { "skip" },
        })))
    }
}

/// Trigger consolidation
pub async fn trigger_consolidation(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    state.emit(VestigeEvent::ConsolidationStarted {
        timestamp: Utc::now(),
    });

    let start = std::time::Instant::now();

    let storage = state.storage.clone();
    let result = tokio::task::spawn_blocking(move || storage.run_consolidation())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Consolidation task panicked");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(log_err("storage operation"))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    state.emit(VestigeEvent::ConsolidationCompleted {
        nodes_processed: result.nodes_processed as usize,
        decay_applied: result.decay_applied as usize,
        embeddings_generated: result.embeddings_generated as usize,
        duration_ms,
        timestamp: Utc::now(),
    });

    Ok(Json(serde_json::json!({
        "nodesProcessed": result.nodes_processed,
        "decayApplied": result.decay_applied,
        "embeddingsGenerated": result.embeddings_generated,
        "duplicatesMerged": result.duplicates_merged,
        "activationsComputed": result.activations_computed,
        "durationMs": duration_ms,
    })))
}

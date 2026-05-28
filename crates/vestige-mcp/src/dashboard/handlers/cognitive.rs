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
use super::super::wire::{
    ConsolidationResultDto, ImportanceChannelsDto, ImportanceScoreDto, PredictResponseDto,
    PredictedMemoryDto,
};
use super::{log_err, log_join_err};

/// Trigger a dream cycle — delegates to the DreamEngine in `tools::dream`
pub async fn trigger_dream(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let cognitive = state
        .cognitive
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

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
    // `stats.durationMs` (camelCase) is the canonical wire shape — see
    // `tools/dream.rs` and the dashboard `DreamStats` type. The previous
    // `total_duration_ms` lookup silently returned 0 after the v3.4 schema
    // tightening, which made `DreamCompleted` events report `durationMs: 0`.
    let duration_ms = result["stats"]["durationMs"].as_u64().unwrap_or(0);

    state.emit(VestigeEvent::DreamCompleted {
        memories_replayed,
        connections_found: connections,
        insights_generated: insights_count,
        duration_ms,
        timestamp: Utc::now(),
    });

    Ok(Json(result))
}

/// Predict which memories will be needed.
///
/// Stub heuristic: most-recent 10 memories. Returns each entry tagged
/// with `predicted_need = "high"` because the heuristic doesn't
/// distinguish — when this gets replaced with spreading-activation, the
/// per-entry bucket becomes meaningful.
pub async fn predict_memories(
    State(state): State<AppState>,
) -> Result<Json<PredictResponseDto>, StatusCode> {
    // Get recent memories as predictions based on activity. Bounded to
    // 10 rows, so the latency hit is small — but on a 100k-node DB the
    // ORDER BY scan still pushes us above 10ms, so spawn_blocking is
    // the safe default.
    let storage = state.storage.clone();
    let recent = tokio::task::spawn_blocking(move || storage.get_all_nodes(10, 0))
        .await
        .map_err(log_join_err("get_all_nodes task panicked"))?
        .map_err(log_err("storage operation"))?;

    let predictions: Vec<PredictedMemoryDto> = recent
        .iter()
        .map(|n| PredictedMemoryDto {
            id: n.id.clone(),
            content: n.content.chars().take(100).collect::<String>(),
            node_type: n.node_type.clone(),
            retention: n.retention_strength,
            predicted_need: "high".to_string(),
        })
        .collect();

    Ok(Json(PredictResponseDto {
        predictions,
        based_on: "recent_activity".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ImportanceRequest {
    pub content: String,
}

/// Score content importance using the 4-channel model
/// (novelty/arousal/reward/attention). Falls back to a simple word-count
/// and code-detection heuristic when the cognitive engine is unavailable
/// (e.g. dashboard launched without `--with-cognitive`).
pub async fn score_importance(
    State(state): State<AppState>,
    Json(req): Json<ImportanceRequest>,
) -> Result<Json<ImportanceScoreDto>, StatusCode> {
    if let Some(ref cognitive) = state.cognitive {
        let context = vestige_core::ImportanceContext::current();
        let cog = cognitive.lock().await;
        let score = cog
            .importance_signals
            .compute_importance(&req.content, &context);
        drop(cog);

        let channels = ImportanceChannelsDto {
            novelty: score.novelty,
            arousal: score.arousal,
            reward: score.reward,
            attention: score.attention,
        };

        state.emit(VestigeEvent::ImportanceScored {
            content_preview: req.content.chars().take(80).collect(),
            composite_score: score.composite,
            novelty: channels.novelty,
            arousal: channels.arousal,
            reward: channels.reward,
            attention: channels.attention,
            timestamp: Utc::now(),
        });

        Ok(Json(ImportanceScoreDto {
            composite: score.composite,
            channels,
            recommendation: recommendation_for(score.composite),
        }))
    } else {
        let word_count = req.content.split_whitespace().count();
        let has_code = req.content.contains("```") || req.content.contains("fn ");
        let composite = if has_code {
            0.7
        } else {
            (word_count as f64 / 100.0).min(0.8)
        };

        Ok(Json(ImportanceScoreDto {
            composite,
            channels: ImportanceChannelsDto {
                novelty: composite,
                arousal: 0.5,
                reward: 0.5,
                attention: composite,
            },
            recommendation: recommendation_for(composite),
        }))
    }
}

/// Single threshold used by both the cognitive-backed and fallback
/// paths so the recommendation never disagrees with the score.
fn recommendation_for(composite: f64) -> String {
    if composite > 0.6 { "save" } else { "skip" }.to_string()
}

/// Trigger consolidation — emits decay, embedding regen, dedup and
/// activation passes. Returns counts only (no per-memory data) so the
/// dashboard surfaces a "what just happened" toast.
pub async fn trigger_consolidation(
    State(state): State<AppState>,
) -> Result<Json<ConsolidationResultDto>, StatusCode> {
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

    Ok(Json(ConsolidationResultDto {
        nodes_processed: result.nodes_processed,
        decay_applied: result.decay_applied,
        embeddings_generated: result.embeddings_generated,
        duplicates_merged: result.duplicates_merged,
        activations_computed: result.activations_computed,
        duration_ms,
    }))
}

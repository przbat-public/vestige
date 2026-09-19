//! Wire DTOs for predict, importance, and consolidation endpoints.
//!
//! Note: `trigger_dream` still returns `serde_json::Value` because the
//! response is composed inside `tools::dream::execute` and shared with
//! the MCP tool surface. That tool's output structure is locked into
//! the MCP protocol contract, so converting it to a typed DTO here
//! would require duplicating ~10 nested types just for the dashboard.
//! Instead we rely on `dream.rs`'s own typed payload (`DreamResultJson`)
//! plus the dashboard's `DreamResult` interface to stay in sync; if
//! `dream` ever drifts, the test in `dashboard/handlers/cognitive.rs`
//! that decodes the camelCase fields on the way out will catch it.

use serde::Serialize;
use ts_rs::TS;

/// One predicted memory the engine guesses the user will want next.
/// Currently a deterministic "last 10 by recency" heuristic; the wire
/// shape is forwards-compatible with a real spreading-activation
/// predictor.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PredictedMemoryDto.ts", rename_all = "camelCase")]
pub struct PredictedMemoryDto {
    pub id: String,
    /// Truncated to 100 chars.
    pub content: String,
    pub node_type: String,
    pub retention: f64,
    /// Opaque ranking bucket: "low" | "medium" | "high".
    pub predicted_need: String,
}

/// `POST /api/predict` response. `based_on` is a free-form label so the
/// dashboard can show the strategy used ("recent_activity", later
/// "spreading_activation", etc.).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PredictResponseDto.ts", rename_all = "camelCase")]
pub struct PredictResponseDto {
    pub predictions: Vec<PredictedMemoryDto>,
    pub based_on: String,
}

/// 4-channel importance breakdown — used by the importance scoring
/// panel to draw a radar chart.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export, export_to = "ImportanceChannelsDto.ts")]
pub struct ImportanceChannelsDto {
    pub novelty: f64,
    pub arousal: f64,
    pub reward: f64,
    pub attention: f64,
}

/// `POST /api/importance` response.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "ImportanceScoreDto.ts")]
pub struct ImportanceScoreDto {
    pub composite: f64,
    pub channels: ImportanceChannelsDto,
    /// "save" when composite > 0.6, "skip" otherwise.
    pub recommendation: String,
}

/// `POST /api/consolidate` response — counts only, no per-memory data.
/// Dashboard surfaces these in a "what just happened" toast.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "ConsolidationResultDto.ts",
    rename_all = "camelCase"
)]
pub struct ConsolidationResultDto {
    pub nodes_processed: i64,
    pub decay_applied: i64,
    pub embeddings_generated: i64,
    pub duplicates_merged: i64,
    pub activations_computed: i64,
    pub duration_ms: u64,
}

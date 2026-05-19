//! Wire DTOs for stats, health, and retention distribution.

use serde::Serialize;
use std::collections::HashMap;
use ts_rs::TS;

use super::memory::MemoryDto;

/// `GET /api/stats` — broad system snapshot for the dashboard top bar.
///
/// Counts use `i64` because that's what storage returns (SQLite COUNT).
/// JS Number can hold them precisely up to 2^53 — well beyond any
/// realistic memory base — so the `TS_RS_LARGE_INT = "number"` setting
/// in `.cargo/config.toml` keeps the TypeScript side as plain `number`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SystemStatsDto.ts", rename_all = "camelCase")]
pub struct SystemStatsDto {
    pub total_memories: i64,
    pub due_for_review: i64,
    pub average_retention: f64,
    pub average_storage_strength: f64,
    pub average_retrieval_strength: f64,
    pub with_embeddings: i64,
    /// Percentage in `[0, 100]`.
    pub embedding_coverage: f64,
    /// Empty string when no embedding model is configured. We collapse
    /// `None` to `""` here (instead of leaving it `Option<String>`) so
    /// the dashboard's status pill always has a label to render.
    pub embedding_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub oldest_memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub newest_memory: Option<String>,
}

/// `GET /api/health` — high-level health summary used by the dashboard
/// to decide whether to badge the connection icon green / amber / red.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "HealthCheckDto.ts", rename_all = "camelCase")]
pub struct HealthCheckDto {
    pub status: HealthStatus,
    pub total_memories: i64,
    pub average_retention: f64,
    pub version: String,
}

/// Discriminator for `HealthCheckDto`. Keep these strings stable — the
/// dashboard pattern-matches on them to drive UI colors.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "HealthStatus.ts", rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Critical,
    Empty,
}

/// One bucket in the retention histogram.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "RetentionBucketDto.ts")]
pub struct RetentionBucketDto {
    /// Inclusive-exclusive label, e.g. "30-40%".
    pub range: String,
    pub count: u32,
}

/// One memory in the "endangered" list — surfaced when retention drops
/// below 30%. Kept slim because the panel only renders preview text.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "EndangeredMemoryDto.ts", rename_all = "camelCase")]
pub struct EndangeredMemoryDto {
    pub id: String,
    /// Truncated to 60 chars (matches the legacy serializer).
    pub content: String,
    pub retention: f64,
    pub node_type: String,
}

/// Wrapper shape that matches the legacy `RetentionDistribution` type
/// the dashboard reads. `endangered` is list of slim previews — the
/// dashboard maps them onto the full memory page on click.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "RetentionDistributionDto.ts",
    rename_all = "camelCase"
)]
pub struct RetentionDistributionDto {
    pub distribution: Vec<RetentionBucketDto>,
    /// Counts indexed by node_type. `HashMap<String, …>` survives the
    /// ts-rs round-trip as `{ [key in string]?: number }`.
    pub by_type: HashMap<String, usize>,
    pub endangered: Vec<EndangeredMemoryDto>,
    pub total: usize,
}

/// Search response — hit list with a duration so the dashboard can show
/// "found N in Mms" telemetry.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SearchResultDto.ts", rename_all = "camelCase")]
pub struct SearchResultDto {
    pub query: String,
    pub total: usize,
    pub duration_ms: u64,
    pub results: Vec<MemoryDto>,
}

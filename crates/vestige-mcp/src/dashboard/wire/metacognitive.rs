//! Wire DTOs for reflect, temporal, and confidence endpoints.
//!
//! These three handlers delegate to MCP tools (`tools::reflect`,
//! `tools::temporal`, `tools::confidence`) that return
//! `serde_json::Value`. We can't change those tools' signatures without
//! touching the MCP protocol contract, but we *can* describe their
//! dashboard-facing wire shape here as typed DTOs and run them through
//! `serde_json::from_value` in the handler. The conversion catches
//! drift at HTTP request time (loud 500 instead of silent
//! shape-mismatch on the dashboard) and ts-rs generates the matching
//! TypeScript declarations.
//!
//! Both `Serialize` and `Deserialize` are derived because:
//! - `Deserialize` parses the tool's `Value` → typed DTO
//! - `Serialize` re-encodes for axum's `Json(...)` response
//! - `TS` generates the dashboard's TypeScript type

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// =============================================================================
// REFLECT
// =============================================================================

/// One structured insight from `reflect`. The legacy `insights:
/// string[]` field is kept on the parent type for MCP compatibility;
/// the dashboard renders `structured_insights` instead because it
/// carries source memory ids it can link back to.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ReflectInsightDto.ts", rename_all = "camelCase")]
pub struct ReflectInsightDto {
    /// "contradiction" | "stale_decision" | "overconfident" | "knowledge_gap".
    /// Open-ended `String` because new categories may be added.
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub insight_type: String,
    pub description: String,
    /// Memory IDs that produced this insight. Empty for `knowledge_gap`,
    /// which is topic-scoped — those carry `tags` instead.
    pub source_memory_ids: Vec<String>,
    /// "high" | "medium" | "low".
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub suggestion: Option<String>,
    /// Topic tags for `knowledge_gap` insights; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
}

/// Status discriminator for `ReflectResultDto`. The reflect tool
/// returns "insufficient_memories" when fewer than 3 memories exist,
/// and "reflected" otherwise. Surfacing it on the wire lets the
/// dashboard render a friendly empty state without re-running the
/// length check.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "ReflectStatus.ts", rename_all = "snake_case")]
pub enum ReflectStatus {
    Reflected,
    InsufficientMemories,
}

/// `POST /api/reflect` response.
///
/// `insights: Vec<String>` is the legacy free-form representation,
/// preserved for MCP clients that already consume it. Dashboard reads
/// `structured_insights` exclusively.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ReflectResultDto.ts", rename_all = "camelCase")]
pub struct ReflectResultDto {
    pub status: ReflectStatus,
    /// Number of memories the engine actually inspected (after focus +
    /// depth filtering). `0` when status is `insufficient_memories`.
    #[serde(default)]
    pub memories_analyzed: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub focus: Option<String>,
    /// Default to "standard" so the wire shape stays well-typed even
    /// when the tool emits the value as `null`.
    #[serde(default = "default_depth")]
    pub depth: String,
    #[serde(default)]
    pub insights: Vec<String>,
    #[serde(default)]
    pub structured_insights: Vec<ReflectInsightDto>,
    /// Pre-rendered "tl;dr" sentence for the dashboard briefing.
    #[serde(default)]
    pub summary: String,
    /// Free-form message — populated when status is
    /// `insufficient_memories`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub message: Option<String>,
}

fn default_depth() -> String {
    "standard".to_string()
}

// =============================================================================
// TEMPORAL
// =============================================================================

/// One memory in a temporal-versioning result.
///
/// `valid_from` / `valid_until` are present only when the engine has
/// stored a validity window. `days_expired` is populated only for the
/// `expired` action.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TemporalEntryDto.ts", rename_all = "camelCase")]
pub struct TemporalEntryDto {
    pub id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub valid_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub days_expired: Option<i64>,
    /// Numeric in `[0, 1]`. Backend serialises with `format!("{:.2}",
    /// ...)` for `current` action; we accept both the formatted string
    /// and a raw number via a custom deserializer.
    #[serde(deserialize_with = "deserialize_retention", default)]
    pub retention: f64,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Accept both `0.85` (number) and `"0.85"` (formatted string) so the
/// dashboard sees a consistent `number` regardless of which `tools::
/// temporal` action shape the upstream emitted. Without this, the
/// `current` action's `format!("{:.2}", ...)` output deserialised as
/// `null` and the dashboard rendered "NaN%".
fn deserialize_retention<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Visitor};
    use std::fmt;

    struct RetentionVisitor;
    impl<'de> Visitor<'de> for RetentionVisitor {
        type Value = f64;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a number or a string parseable as f64")
        }
        fn visit_f64<E: Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_i64<E: Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E: Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: Error>(self, v: &str) -> Result<f64, E> {
            v.parse().map_err(E::custom)
        }
        fn visit_string<E: Error>(self, v: String) -> Result<f64, E> {
            v.parse().map_err(E::custom)
        }
        fn visit_unit<E: Error>(self) -> Result<f64, E> {
            Ok(0.0)
        }
    }
    deserializer.deserialize_any(RetentionVisitor)
}

/// `POST /api/temporal` response.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TemporalResultDto.ts", rename_all = "camelCase")]
pub struct TemporalResultDto {
    /// "current" | "expired" | "history" | "invalidate".
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub topic: Option<String>,
    pub count: usize,
    #[serde(default)]
    pub memories: Vec<TemporalEntryDto>,
}

// =============================================================================
// CONFIDENCE
// =============================================================================

/// 4-channel confidence breakdown.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "ConfidenceDimensionsDto.ts")]
pub struct ConfidenceDimensionsDto {
    pub encoding: f64,
    pub retrieval: f64,
    pub temporal: f64,
    pub evidence: f64,
}

/// One confidence-scored memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "ConfidenceEntryDto.ts")]
pub struct ConfidenceEntryDto {
    pub id: String,
    pub content: String,
    pub confidence: f64,
    pub dimensions: ConfidenceDimensionsDto,
    /// Free-form classification ("low_confidence", "high_confidence",
    /// "calibrated", …). `String` so the engine can introduce more
    /// buckets without breaking the dashboard.
    pub classification: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub concern: Option<String>,
}

/// `POST /api/confidence` response.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "ConfidenceResultDto.ts")]
pub struct ConfidenceResultDto {
    /// "score" | "audit" | "calibrate".
    pub action: String,
    #[serde(default)]
    pub results: Vec<ConfidenceEntryDto>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub summary: Option<String>,
}

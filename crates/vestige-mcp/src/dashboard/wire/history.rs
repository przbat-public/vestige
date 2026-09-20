//! Wire DTOs for changelog, content history and timeline endpoints.

use serde::Serialize;
use ts_rs::TS;

use vestige_core::MemoryRevision;

/// One state-machine transition in a memory's audit trail.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "MemoryChangelogEntryDto.ts",
    rename_all = "camelCase"
)]
pub struct MemoryChangelogEntryDto {
    pub from_state: String,
    pub to_state: String,
    pub reason_type: String,
    /// `reason_data` is free-form — the storage column is TEXT, often
    /// JSON-serialized but sometimes a plain string. We pass it through
    /// untouched.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason_data: Option<String>,
    /// ISO-8601 RFC 3339.
    pub timestamp: String,
}

/// Full changelog response for a single memory.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "MemoryChangelogDto.ts", rename_all = "camelCase")]
pub struct MemoryChangelogDto {
    pub memory_id: String,
    pub memory_content: String,
    pub total_transitions: usize,
    pub transitions: Vec<MemoryChangelogEntryDto>,
}

/// One row of a memory's *content* history (`memory_revisions`, V17).
///
/// Distinct from [`MemoryChangelogEntryDto`], which records life-cycle state
/// transitions: this is what the memory used to say. A panel labelled "edit
/// history" that shows scheduling transitions is the drift this DTO removes —
/// the two histories answer different questions and are surfaced separately.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "MemoryRevisionDto.ts", rename_all = "camelCase")]
pub struct MemoryRevisionDto {
    /// Monotonic row id — also the tie-breaker the storage read orders by, so
    /// two revisions written in the same millisecond still sort predictably.
    pub id: i64,
    /// `create` | `edit` | `supersede` | `invalidate` | `quarantine`.
    pub kind: String,
    /// ISO-8601 RFC 3339 — the *storage* clock, not a caller-supplied time.
    pub recorded_at: String,
    /// The previous wording. For `invalidate` this holds the previous
    /// `valid_until` value instead, because that is what the operation changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub old_content: Option<String>,
    /// The new wording (or new `valid_until` for `invalidate`). `None` when the
    /// revision records a deletion-shaped change (`quarantine`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub new_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// Who wrote the version, when the layer above storage knew. Absent rather
    /// than invented for writes that carried no actor.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub actor: Option<String>,
}

impl From<&MemoryRevision> for MemoryRevisionDto {
    fn from(r: &MemoryRevision) -> Self {
        Self {
            id: r.id,
            kind: r.kind.as_str().to_string(),
            recorded_at: r.recorded_at.to_rfc3339(),
            old_content: r.old_content.clone(),
            new_content: r.new_content.clone(),
            reason: r.reason.clone(),
            actor: r.actor.clone(),
        }
    }
}

/// `GET /api/memories/{id}/revisions` response.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "MemoryRevisionsDto.ts", rename_all = "camelCase")]
pub struct MemoryRevisionsDto {
    pub memory_id: String,
    /// How many revisions this response carries. The read is capped by the
    /// caller's `limit` (clamped to `DashboardLimitsDto`), so this is a page
    /// size — not the memory's total revision count, which no reader has.
    pub total: usize,
    pub revisions: Vec<MemoryRevisionDto>,
}

/// One memory entry in a timeline-by-day grouping. Slimmer than
/// `MemoryDto` — only what the timeline card surfaces.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TimelineMemoryDto.ts", rename_all = "camelCase")]
pub struct TimelineMemoryDto {
    pub id: String,
    /// Truncated to 100 chars + "..." suffix when needed.
    pub content: String,
    pub node_type: String,
    pub retention_strength: f64,
    pub created_at: String,
}

/// One day in a timeline response.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "TimelineDayDto.ts")]
pub struct TimelineDayDto {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub count: usize,
    pub memories: Vec<TimelineMemoryDto>,
}

/// `GET /api/timeline` response.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TimelineResponseDto.ts", rename_all = "camelCase")]
pub struct TimelineResponseDto {
    pub days: i64,
    pub total_memories: usize,
    pub timeline: Vec<TimelineDayDto>,
}

//! Wire DTOs for changelog and timeline endpoints.

use serde::Serialize;
use ts_rs::TS;

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

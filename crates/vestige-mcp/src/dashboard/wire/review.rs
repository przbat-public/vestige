//! Wire DTOs for FSRS-6 review endpoints.

use serde::Serialize;
use ts_rs::TS;

use super::memory::MemoryDto;

/// Result of a single FSRS-6 review.
///
/// Carries before/after values so the dashboard's review widget can
/// animate the change and show "+12% retention" feedback without a
/// follow-up GET.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ReviewResultDto.ts", rename_all = "camelCase")]
pub struct ReviewResultDto {
    pub id: String,
    /// "again" | "hard" | "good" | "easy" — string for legibility,
    /// matching the dashboard widget enum.
    pub rating: String,
    pub previous_retention: f64,
    pub new_retention: f64,
    pub previous_stability: f64,
    pub new_stability: f64,
    pub difficulty: f64,
    pub reps: i32,
    pub lapses: i32,
    /// `None` only when FSRS-6 marked the memory as suppressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_review_at: Option<String>,
}

/// One memory in the review queue. Adds `difficulty` + `stability`
/// (FSRS-6 internal state) on top of `MemoryDto` so the queue can
/// surface FSRS-6 telemetry inline.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ReviewItemDto.ts", rename_all = "camelCase")]
pub struct ReviewItemDto {
    #[serde(flatten)]
    #[ts(flatten)]
    pub memory: MemoryDto,
    pub difficulty: f64,
    pub stability: f64,
}

/// `GET /api/review/queue` response.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "ReviewQueueResponseDto.ts")]
pub struct ReviewQueueResponseDto {
    pub total: usize,
    pub memories: Vec<ReviewItemDto>,
}

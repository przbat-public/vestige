//! Wire DTOs for the prospective memory (intentions) endpoints.

use serde::Serialize;
use ts_rs::TS;

/// One intention as the dashboard sees it.
///
/// `priority` is exposed as a string ("low"/"medium"/"high") instead of
/// the storage `i32` because the dashboard already keys CSS classes on
/// the label. `triggerValue` is the parsed `value` field of the
/// internal trigger_data JSON envelope — we don't expose the envelope
/// itself.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "IntentionItemDto.ts", rename_all = "camelCase")]
pub struct IntentionItemDto {
    pub id: String,
    pub content: String,
    /// "context" | "time" | "event".
    pub trigger_type: String,
    pub trigger_value: String,
    /// "active" | "fulfilled" | "expired" | "snoozed" | "completed" | "cancelled".
    pub status: String,
    /// "low" | "medium" | "high".
    pub priority: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deadline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub snoozed_until: Option<String>,
}

/// `GET /api/intentions` response.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "IntentionListResponseDto.ts")]
pub struct IntentionListResponseDto {
    pub intentions: Vec<IntentionItemDto>,
    pub total: usize,
    /// The `?status=` filter applied — defaults to `"active"` when the
    /// caller didn't specify. Echoed back so the dashboard can update
    /// segmented controls without a separate state.
    pub filter: String,
}

/// `POST /api/intentions` response — returns the just-created item plus
/// its id so optimistic UIs can pick the right entry from the list.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "CreateIntentionResponseDto.ts")]
pub struct CreateIntentionResponseDto {
    pub id: String,
    pub intention: IntentionItemDto,
}

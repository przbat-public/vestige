//! Wire DTOs for the prospective memory (intentions) endpoints.

use serde::{Deserialize, Serialize};
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

/// `PATCH /api/intentions/{id}` request.
///
/// Mirrors the MCP `intention(action="update", status="...")` tool. The
/// dashboard uses this to close out the loop after the user finishes an
/// intention — previously the only way to mark it `fulfilled` was through
/// the MCP layer, so the list grew stale.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "UpdateIntentionRequestDto.ts", rename_all = "camelCase")]
pub struct UpdateIntentionRequestDto {
    /// New status — accepts `fulfilled` | `cancelled` | `snoozed` | `active`.
    /// Anything else is rejected with 400 so the storage layer never sees
    /// an unconstrained string.
    pub status: String,
}

/// `PATCH /api/intentions/{id}` response.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "UpdateIntentionResponseDto.ts", rename_all = "camelCase")]
pub struct UpdateIntentionResponseDto {
    pub id: String,
    pub status: String,
    /// True when the row existed and was updated. False is impossible from
    /// the public endpoint (we 404 first) but kept to mirror the storage
    /// contract for future consumers.
    pub updated: bool,
}

//! Wire DTOs for the GDPR Article 17 erasure endpoint
//! (`POST /api/maintenance/erase`).
//!
//! The dashboard deliberately gets its own typed contract instead of the raw
//! tool payload: erasure is irreversible, so the shape of "what was removed"
//! must be checked at compile time on the client and re-checked at runtime by
//! the handler before anyone renders it as a receipt.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::tools::erase::EraseOutcome;

/// Request body for `POST /api/maintenance/erase`.
///
/// `dry_run` defaults to `true` on the wire for the same reason it defaults to
/// `true` in the MCP tool: a client that forgets the flag must not delete
/// anything. The destructive pass additionally needs `confirmed: true`, which
/// the handler forwards to the tool's shared `run` entry point — the single
/// place the gate is implemented.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "EraseRequestDto.ts", rename_all = "camelCase")]
pub struct EraseRequestDto {
    /// `"memory"` (one memory by `id`) or `"tag"` (every memory carrying that
    /// exact tag).
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tag: Option<String>,
    /// Report what would be erased without deleting. Defaults to `true`.
    #[serde(default = "default_true")]
    pub dry_run: bool,
    /// Explicit acknowledgement for the destructive pass. Any value other
    /// than `true` is refused with 400.
    #[serde(default)]
    pub confirmed: bool,
}

fn default_true() -> bool {
    true
}

/// Response body for `POST /api/maintenance/erase`.
///
/// `matched` is exact even when `ids` is capped by the erasure tool's report
/// limit; `ids_truncated` tells the client to stop expecting the two numbers
/// to agree.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "EraseResponseDto.ts", rename_all = "camelCase")]
pub struct EraseResponseDto {
    /// `"memory"` or `"tag"` — echoes the request.
    pub action: String,
    pub dry_run: bool,
    /// Memories erased (or matching, on a dry run).
    pub matched: u64,
    /// The ids that were removed (or would be), capped and newest-first.
    pub ids: Vec<String>,
    pub ids_truncated: bool,
    /// Rows removed across every table the memory appeared in: node,
    /// connections, embeddings, access log, states, insights. Zero on a dry
    /// run.
    pub artifacts_erased: i64,
    /// One-line human summary, so the dashboard does not re-derive it.
    pub message: String,
}

impl From<&EraseOutcome> for EraseResponseDto {
    fn from(outcome: &EraseOutcome) -> Self {
        Self {
            action: outcome.action.to_string(),
            dry_run: outcome.dry_run,
            matched: outcome.matched,
            ids: outcome.ids.clone(),
            ids_truncated: outcome.ids_truncated,
            artifacts_erased: outcome.artifacts_erased,
            message: outcome.message(),
        }
    }
}

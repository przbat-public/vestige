//! Strongly-typed argument struct deserialized from the MCP request.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchArgs {
    pub query: String,
    pub limit: Option<i32>,
    #[serde(alias = "min_retention")]
    pub min_retention: Option<f64>,
    #[serde(alias = "min_similarity")]
    pub min_similarity: Option<f32>,
    #[serde(alias = "detail_level")]
    pub detail_level: Option<String>,
    #[serde(alias = "context_topics")]
    pub context_topics: Option<Vec<String>>,
    #[serde(alias = "token_budget")]
    pub token_budget: Option<i32>,
    #[serde(alias = "retrieval_mode")]
    pub retrieval_mode: Option<String>,
    /// Ask *what the store believed* at this instant, instead of what it
    /// believes now.
    ///
    /// This is a **record-time** question, and the only clock it reads is
    /// `recorded_at`: a memory counts as believed at `as_of` when it had been
    /// written down by then and nothing had retracted it by then (`invalidate` /
    /// `supersede` revision with a record time at or before the instant). The
    /// valid-time window is deliberately *not* used as a filter — it answers a
    /// different question, "when was this true", and intersecting the two would
    /// return a set that matches neither. Each result therefore carries
    /// `validityAtAsOf` (`true` / `not_yet_true` / `expired` / `unbounded`) so
    /// the caller can read the valid-time answer off the same rows.
    ///
    /// Format: RFC 3339 (`2026-09-01T12:00:00Z`) or a date (`2026-09-01`, read
    /// as midnight UTC).
    #[serde(alias = "as_of")]
    pub as_of: Option<String>,
}

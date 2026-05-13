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
}

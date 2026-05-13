//! Argument structs deserialized from MCP requests.

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub(super) struct SessionContextArgs {
    pub queries: Option<Vec<String>>,
    pub token_budget: Option<i32>,
    pub context: Option<ContextSpec>,
    pub include_status: Option<bool>,
    pub include_intentions: Option<bool>,
    pub include_predictions: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct ContextSpec {
    pub codebase: Option<String>,
    pub topics: Option<Vec<String>>,
    pub file: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TriggerData {
    #[serde(rename = "type")]
    pub trigger_type: Option<String>,
    pub at: Option<String>,
    #[serde(alias = "in_minutes")]
    pub in_minutes: Option<i64>,
    pub codebase: Option<String>,
    #[serde(alias = "file_pattern")]
    pub file_pattern: Option<String>,
    pub topic: Option<String>,
}

//! Argument structs for set/check/update/list actions.

use serde::Deserialize;

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TriggerSpec {
    #[serde(rename = "type")]
    pub trigger_type: Option<String>,
    pub at: Option<String>,
    #[serde(alias = "in_minutes")]
    pub in_minutes: Option<i64>,
    pub codebase: Option<String>,
    #[serde(alias = "file_pattern")]
    pub file_pattern: Option<String>,
    pub topic: Option<String>,
    pub condition: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContextSpec {
    #[allow(dead_code)]
    #[serde(alias = "current_time")]
    pub current_time: Option<String>,
    pub codebase: Option<String>,
    pub file: Option<String>,
    pub topics: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UnifiedIntentionArgs {
    pub action: String,
    // SET parameters
    pub description: Option<String>,
    pub trigger: Option<TriggerSpec>,
    pub priority: Option<String>,
    pub deadline: Option<String>,
    // UPDATE parameters
    pub id: Option<String>,
    pub status: Option<String>,
    #[serde(alias = "snoozeMinutes")]
    pub snooze_minutes: Option<i64>,
    // CHECK parameters
    pub context: Option<ContextSpec>,
    #[serde(alias = "includeSnoozed")]
    #[allow(dead_code)]
    pub include_snoozed: Option<bool>,
    // LIST parameters
    #[serde(alias = "filterStatus")]
    pub filter_status: Option<String>,
    pub limit: Option<i32>,
}

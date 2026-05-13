//! Argument structs deserialized from MCP requests (single + batch).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SmartIngestArgs {
    pub content: Option<String>,
    #[serde(alias = "node_type")]
    pub node_type: Option<String>,
    pub tags: Option<Vec<String>>,
    pub source: Option<String>,
    #[serde(alias = "force_create")]
    pub force_create: Option<bool>,
    pub items: Option<Vec<BatchItem>>,
    /// Session identifier for provenance tracking (e.g. conversation ID)
    #[serde(alias = "session_id")]
    pub session_id: Option<String>,
    /// Agent identifier for provenance tracking (e.g. "cursor", "claude")
    pub agent: Option<String>,
}

/// A single item in batch mode
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BatchItem {
    pub content: String,
    pub tags: Option<Vec<String>>,
    #[serde(alias = "node_type")]
    pub node_type: Option<String>,
    pub source: Option<String>,
    #[serde(alias = "force_create")]
    pub force_create: Option<bool>,
}

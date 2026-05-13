//! Argument struct deserialized from MCP requests.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MemoryArgs {
    pub action: String,
    pub id: Option<String>,
    pub ids: Option<Vec<String>>,
    pub reason: Option<String>,
    pub content: Option<String>,
}

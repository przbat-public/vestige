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

/// Who is writing, as `memory_revisions.actor` records it.
///
/// JSON rather than a joined string because the two identities are separate
/// facts and either can be absent: `"cursor session=abc"` cannot be split back
/// apart when the agent name itself contains a space or an `=`, so a reader
/// would have to guess. `None` when the caller named neither, which leaves the
/// column NULL — the honest value for "the tool was not told who is writing",
/// as opposed to a placeholder that looks like an identity.
pub(super) fn actor_of(agent: Option<&str>, session_id: Option<&str>) -> Option<String> {
    let mut fields = serde_json::Map::new();
    if let Some(agent) = agent.map(str::trim).filter(|a| !a.is_empty()) {
        fields.insert("agent".to_string(), serde_json::Value::from(agent));
    }
    if let Some(session_id) = session_id.map(str::trim).filter(|s| !s.is_empty()) {
        fields.insert(
            "session_id".to_string(),
            serde_json::Value::from(session_id),
        );
    }
    if fields.is_empty() {
        return None;
    }
    Some(serde_json::Value::Object(fields).to_string())
}

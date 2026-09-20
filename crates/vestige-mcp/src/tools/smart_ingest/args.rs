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
    /// Explicit code anchors for this memory: `path@commit#symbol`, or an object
    /// with `path` / `commit` / `symbol` / `line` / `repo`.
    ///
    /// Paths and line numbers belong here and not in `content`: a path inside
    /// the text keeps resolving to whatever it means today and nothing signals
    /// when that changes. An anchor carries the revision and the symbol, so the
    /// store can re-check it and a reader can see the verdict.
    #[serde(default, alias = "code_refs")]
    pub code_refs: Vec<AnchorArg>,
}

/// One explicit anchor, in either accepted shape.
///
/// Untagged on purpose: the short form is what a caller writes by hand, and the
/// object form is what a caller writes when it has the parts separately. Making
/// them two fields would mean two code paths that must agree about precedence.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AnchorArg {
    /// `path`, `path:line`, `path@sha`, or `path@sha#symbol`.
    Text(String),
    /// The parts, spelled out.
    Full {
        path: String,
        #[serde(default, alias = "commitSha", alias = "sha")]
        commit: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        line: Option<u32>,
        #[serde(default, alias = "repoRemote", alias = "remote")]
        repo: Option<String>,
    },
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
    /// Explicit anchors for this item, in the same two shapes as single mode.
    #[serde(default, alias = "code_refs")]
    pub code_refs: Vec<AnchorArg>,
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

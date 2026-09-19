//! MCP Protocol Messages
//!
//! Request and response types for MCP methods.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// INITIALIZE
// ============================================================================

/// Initialize request from client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

impl Default for InitializeRequest {
    fn default() -> Self {
        Self {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: "unknown".to_string(),
                version: "0.0.0".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<HashMap<String, Value>>,
    /// MCP `2025-06-18` elicitation capability. Clients advertise this
    /// when they can render `elicitation/create` prompts inline. The
    /// server still issues string-encoded confirmation errors today
    /// (stdio transport is one-way), but recording the capability lets
    /// us switch to genuine outbound prompts once bidirectional JSON-RPC
    /// lands without breaking the wire format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Initialize response to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub server_info: ServerInfo,
    pub capabilities: ServerCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<HashMap<String, Value>>,
}

// ============================================================================
// TOOLS
// ============================================================================

/// Tool description for `tools/list`.
///
/// Mirrors the `Tool` shape from the MCP `2025-11-25` schema. `title`
/// (display name) and `annotations` (behavior hints) are optional and were
/// added by the spec in `2025-03-26`. `outputSchema` is still omitted: our tools
/// return a JSON *object* in `structuredContent`, but none of them publishes a
/// schema for it yet, and a wrong schema is worse than none. See `b15` notes in
/// `server/catalog.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescription {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// Behavior hints that MCP clients use to decide whether a tool call needs
/// human approval before execution. All four flags are advisory per the spec
/// (`2025-03-26` onward) — clients that do not trust the server MUST ignore
/// them, but trusted-server flows like Cursor's "auto-run" rely on these.
///
/// Spec defaults (used when a flag is absent):
/// * `readOnlyHint` defaults to `false` (assume the tool mutates state).
/// * `destructiveHint` defaults to `true` (assume worst-case if mutating).
/// * `idempotentHint` defaults to `false`.
/// * `openWorldHint` defaults to `true` (assume external interaction).
///
/// We always emit an explicit value for the four flags we use rather than
/// relying on defaults, because the defaults are intentionally pessimistic
/// and would treat read-only tools the same as destructive ones.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// Optional human-readable display name, separate from `name` (which is
    /// the protocol identifier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// `true` if the tool only reads state (search, get, list, predict, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,

    /// `true` if the tool can perform destructive (irreversible) updates.
    /// Only meaningful when `read_only_hint` is `false` or absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,

    /// `true` if calling the tool twice with the same arguments has the same
    /// observable effect as calling it once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,

    /// `true` if the tool reaches outside the local Vestige store
    /// (network calls, filesystem outside the database, …). Vestige tools
    /// are currently fully local, so this is always `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// Result of tools/list
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    pub tools: Vec<ToolDescription>,
}

/// Request for tools/call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolRequest {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// Result of tools/call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    /// Human/model-readable rendering. Always present — clients that predate
    /// `structuredContent` (`2025-06-18`) still read the payload from here.
    pub content: Vec<ToolResultContent>,
    /// The same payload as a real JSON object rather than a JSON string.
    ///
    /// The spec's compatibility recipe is to emit both: `structuredContent` for
    /// clients that can consume objects, `content[0].text` for everything older.
    /// `None` when the tool's payload is not a JSON object, because
    /// `structuredContent` is defined as an object and a bare array/scalar there
    /// would be a schema violation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

// ============================================================================
// RESOURCES
// ============================================================================

/// Resource description for resources/list
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDescription {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Result of resources/list
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResult {
    pub resources: Vec<ResourceDescription>,
}

/// Request for resources/read
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceRequest {
    pub uri: String,
}

/// Result of resources/read
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

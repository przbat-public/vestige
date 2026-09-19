//! MCP Server Core
//!
//! Handles the main MCP server logic, routing requests to appropriate
//! tool and resource handlers.
//!
//! Submodules:
//! - [`catalog`](server/catalog.rs) — tool/resource catalog (b15 split)
//! - [`deprecated`](server/deprecated.rs) — alias rewriter for legacy tool names
//! - [`dispatch`](server/dispatch.rs) — `handle_tools_call` body
//! - [`events`](server/events.rs) — dashboard `VestigeEvent` emission
//! - [`tests`](server/tests.rs) — server-level integration tests

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::{Mutex, broadcast};
use tracing::{debug, info, warn};

use crate::cognitive::CognitiveEngine;
use crate::dashboard::events::VestigeEvent;
use crate::protocol::messages::{
    InitializeRequest, InitializeResult, ListResourcesResult, ListToolsResult, ReadResourceRequest,
    ReadResourceResult, ServerCapabilities, ServerInfo,
};
use crate::protocol::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, MCP_VERSION};
use crate::resources;
use vestige_core::Storage;

mod catalog;
mod deprecated;
mod dispatch;
mod events;

#[cfg(test)]
mod tests;

/// MCP Server implementation
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Fields are accessed by sibling submodules `dispatch` (tool dispatch) and `events` (event emission) of the cohesive `server` component (split-by-responsibility refactor); siblings have the same trust level as the parent module."
)]
pub struct McpServer {
    pub(super) storage: Arc<Storage>,
    pub(super) cognitive: Arc<Mutex<CognitiveEngine>>,
    pub(super) initialized: bool,
    /// Tool call counter for inline consolidation trigger (every 100 calls)
    pub(super) tool_call_count: AtomicU64,
    /// Optional event broadcast channel for dashboard real-time updates.
    pub(super) event_tx: Option<broadcast::Sender<VestigeEvent>>,
}

impl McpServer {
    #[allow(dead_code)]
    pub fn new(storage: Arc<Storage>, cognitive: Arc<Mutex<CognitiveEngine>>) -> Self {
        Self {
            storage,
            cognitive,
            initialized: false,
            tool_call_count: AtomicU64::new(0),
            event_tx: None,
        }
    }

    /// Create an MCP server that broadcasts events to the dashboard.
    pub fn new_with_events(
        storage: Arc<Storage>,
        cognitive: Arc<Mutex<CognitiveEngine>>,
        event_tx: broadcast::Sender<VestigeEvent>,
    ) -> Self {
        Self {
            storage,
            cognitive,
            initialized: false,
            tool_call_count: AtomicU64::new(0),
            event_tx: Some(event_tx),
        }
    }

    /// Emit an event to the dashboard (no-op if no event channel).
    pub(super) fn emit(&self, event: VestigeEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event);
        }
    }

    /// Handle an incoming JSON-RPC request
    ///
    /// Returns `None` when the frame was a **notification** — a request without an
    /// `id` member. JSON-RPC 2.0 §5 and every MCP revision are explicit that the
    /// receiver of a notification MUST NOT reply, not even with an error: there is
    /// no `id` to correlate the reply with, and emitting one desynchronises the
    /// stream for strict clients. This covers unknown notification methods and
    /// notifications that arrive before `initialize`; both are logged and dropped.
    pub async fn handle_request(&mut self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        debug!("Handling request: {}", request.method);

        let is_notification = request.id.is_none();

        // Check initialization for non-initialize requests
        if !self.initialized
            && request.method != "initialize"
            && request.method != "notifications/initialized"
        {
            warn!(
                "Rejecting request '{}': server not initialized",
                request.method
            );
            if is_notification {
                return None;
            }
            return Some(JsonRpcResponse::error(
                request.id,
                JsonRpcError::server_not_initialized(),
            ));
        }

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "notifications/initialized" => {
                // Notification, no response needed
                return None;
            }
            "tools/list" => self.handle_tools_list().await,
            "tools/call" => self.handle_tools_call(request.params).await,
            "resources/list" => self.handle_resources_list().await,
            "resources/read" => self.handle_resources_read(request.params).await,
            "ping" => Ok(serde_json::json!({})),
            method => {
                warn!("Unknown method: {}", method);
                Err(JsonRpcError::method_not_found())
            }
        };

        if is_notification {
            // Side effects already happened; the outcome is deliberately not sent.
            if let Err(error) = &result {
                debug!(
                    "Notification '{}' produced an error that is not reported: {}",
                    request.method, error
                );
            }
            return None;
        }

        Some(match result {
            Ok(result) => JsonRpcResponse::success(request.id, result),
            Err(error) => JsonRpcResponse::error(request.id, error),
        })
    }

    /// Handle initialize request
    async fn handle_initialize(
        &mut self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let request: InitializeRequest = match params {
            Some(p) => serde_json::from_value(p)
                .map_err(|e| JsonRpcError::invalid_params(&e.to_string()))?,
            None => InitializeRequest::default(),
        };

        // Version negotiation: use client's version if older than server's
        // Claude Desktop rejects servers with newer protocol versions
        let negotiated_version = if request.protocol_version.as_str() < MCP_VERSION {
            info!(
                "Client requested older protocol version {}, using it",
                request.protocol_version
            );
            request.protocol_version.clone()
        } else {
            MCP_VERSION.to_string()
        };

        self.initialized = true;
        info!(
            "MCP session initialized with protocol version {}",
            negotiated_version
        );

        let result = InitializeResult {
            protocol_version: negotiated_version,
            server_info: ServerInfo {
                name: "vestige".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ServerCapabilities {
                tools: Some({
                    let mut map = HashMap::new();
                    map.insert("listChanged".to_string(), serde_json::json!(false));
                    map
                }),
                resources: Some({
                    let mut map = HashMap::new();
                    map.insert("listChanged".to_string(), serde_json::json!(false));
                    map
                }),
                prompts: None,
            },
            instructions: Some(
                "Vestige is your long-term memory system. Use it to remember important information, \
                 recall past knowledge, and maintain context across sessions. The system uses \
                 FSRS-6 spaced repetition to naturally decay memories over time. \
                 \n\nFeedback Protocol: If the user explicitly confirms a memory was helpful, use \
                 memory(action='promote'). If they correct a hallucination or say a memory was wrong, use \
                 memory(action='demote'). Do not ask for permission - just act on their feedback.".to_string()
            ),
        };

        serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
    }

    /// Handle tools/list request.
    ///
    /// The full 27-tool catalog lives in [`catalog`] (b15 split). When adding
    /// a tool there also update `scripts/check-version-and-tools.sh` (CI guard)
    /// and the inventory comment at the top of `catalog.rs`.
    async fn handle_tools_list(&self) -> Result<serde_json::Value, JsonRpcError> {
        let result = ListToolsResult {
            tools: catalog::build_tools_list(),
        };
        serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
    }

    /// Handle resources/list request.
    ///
    /// The full 11-resource catalog lives in [`catalog`] (b15 split).
    async fn handle_resources_list(&self) -> Result<serde_json::Value, JsonRpcError> {
        let result = ListResourcesResult {
            resources: catalog::build_resources_list(),
        };
        serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
    }

    /// Handle resources/read request
    async fn handle_resources_read(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let request: ReadResourceRequest = match params {
            Some(p) => serde_json::from_value(p)
                .map_err(|e| JsonRpcError::invalid_params(&e.to_string()))?,
            None => return Err(JsonRpcError::invalid_params("Missing resource URI")),
        };

        let uri = &request.uri;
        // Normalize URI: strip provider prefix (e.g., "vestige/") for scheme matching
        // OpenCode and other MCP clients may send "vestige/memory://recent"
        // but we register resources as "memory://recent"
        let normalized_uri = uri.strip_prefix("vestige/").unwrap_or(uri);
        let content = if normalized_uri.starts_with("memory://") {
            resources::memory::read(&self.storage, normalized_uri).await
        } else if normalized_uri.starts_with("codebase://") {
            resources::codebase::read(&self.storage, normalized_uri).await
        } else {
            Err(resources::ResourceError::NotFound(uri.clone()))
        };

        match content {
            Ok(text) => {
                let result = ReadResourceResult {
                    contents: vec![crate::protocol::messages::ResourceContent {
                        uri: uri.clone(),
                        mime_type: Some("application/json".to_string()),
                        text: Some(text),
                        blob: None,
                    }],
                };
                serde_json::to_value(result)
                    .map_err(|e| JsonRpcError::internal_error(&e.to_string()))
            }
            // -32002 vs -32603 is part of the contract: a client must be able to tell
            // "this server does not serve that URI" from "the server broke".
            Err(resources::ResourceError::NotFound(uri)) => {
                Err(JsonRpcError::resource_not_found(&uri))
            }
            Err(resources::ResourceError::Internal(message)) => {
                Err(JsonRpcError::internal_error(&message))
            }
        }
    }
}

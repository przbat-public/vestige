//! Tool-call dispatcher — routes effective tool name to the correct `tools::*` handler.
//!
//! Lives in its own module so `server.rs` can stay focused on JSON-RPC plumbing.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use tracing;

use crate::dashboard::events::VestigeEvent;
use crate::protocol::messages::{CallToolRequest, CallToolResult};
use crate::protocol::types::JsonRpcError;
use crate::tools;

use super::McpServer;
use super::deprecated;

impl McpServer {
    /// Handle tools/call request
    pub(super) async fn handle_tools_call(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let request: CallToolRequest = match params {
            Some(p) => serde_json::from_value(p)
                .map_err(|e| JsonRpcError::invalid_params(&e.to_string()))?,
            None => return Err(JsonRpcError::invalid_params("Missing tool call parameters")),
        };

        // Record activity on every tool call (non-blocking)
        if let Ok(mut cog) = self.cognitive.try_lock() {
            cog.activity_tracker.record_activity();
            cog.consolidation_scheduler.record_activity();
        }

        // Save args for event emission (tool dispatch consumes request.arguments)
        let saved_args = if self.event_tx.is_some() {
            request.arguments.clone()
        } else {
            None
        };

        // Deprecated → unified rewrite. The whole compatibility layer (~14
        // tools) lives in `server::deprecated`; everything below dispatches on
        // CURRENT tool names only. Args are cloned because the rewriter
        // consumes them on a hit and we still need the originals on a miss.
        let original_args = request.arguments.clone();
        let (effective_name, effective_args) =
            match deprecated::rewrite_deprecated(&request.name, request.arguments) {
                Some((new_name, new_args)) => (new_name.to_string(), new_args),
                None => (request.name.clone(), original_args),
            };

        let result = match effective_name.as_str() {
            // ---- Unified tools (v1.1+) — current API -----------------------
            "search" => {
                tools::search_unified::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "memory" => {
                tools::memory_unified::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "codebase" => {
                tools::codebase_unified::execute(&self.storage, &self.cognitive, effective_args)
                    .await
            }
            "intention" => {
                tools::intention_unified::execute(&self.storage, &self.cognitive, effective_args)
                    .await
            }

            // ---- Core ingest (v1.7) ----------------------------------------
            "smart_ingest" => {
                tools::smart_ingest::execute(&self.storage, &self.cognitive, effective_args).await
            }

            // ---- System status (v1.7 unified replacement) ------------------
            "system_status" => {
                tools::maintenance::execute_system_status(
                    &self.storage,
                    &self.cognitive,
                    effective_args,
                )
                .await
            }

            // ---- Temporal browsing (v1.2+) ---------------------------------
            "memory_timeline" => tools::timeline::execute(&self.storage, effective_args).await,
            "memory_changelog" => tools::changelog::execute(&self.storage, effective_args).await,

            // ---- Maintenance (v1.2+, non-deprecated) -----------------------
            "consolidate" => {
                self.emit(VestigeEvent::ConsolidationStarted {
                    timestamp: chrono::Utc::now(),
                });
                tools::maintenance::execute_consolidate(&self.storage, effective_args).await
            }
            "backup" => tools::maintenance::execute_backup(&self.storage, effective_args).await,
            "export" => tools::maintenance::execute_export(&self.storage, effective_args).await,
            "gc" => tools::maintenance::execute_gc(&self.storage, effective_args).await,
            "split_memories" => {
                tools::maintenance::execute_split_memories(&self.storage, effective_args).await
            }
            "regenerate_embeddings" => {
                tools::maintenance::execute_regenerate_embeddings(&self.storage, effective_args)
                    .await
            }

            // ---- Auto-save & dedup (v1.3+) ---------------------------------
            "importance_score" => {
                tools::importance::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "find_duplicates" => tools::dedup::execute(&self.storage, effective_args).await,

            // ---- Cognitive tools (v1.5+) -----------------------------------
            "dream" => {
                let storage_for_event = self.storage.clone();
                let memory_count = tokio::task::spawn_blocking(move || {
                    storage_for_event
                        .get_stats()
                        .map(|s| s.total_nodes as usize)
                        .unwrap_or(0)
                })
                .await
                .unwrap_or(0);
                self.emit(VestigeEvent::DreamStarted {
                    memory_count,
                    timestamp: chrono::Utc::now(),
                });
                tools::dream::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "explore_connections" => {
                tools::explore::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "predict" => {
                tools::predict::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "precompute_for_context" => {
                tools::precompute::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "restore" => tools::restore::execute(&self.storage, effective_args).await,

            // ---- Context packets (v1.8+) -----------------------------------
            "session_context" => {
                tools::session_context::execute(&self.storage, &self.cognitive, effective_args)
                    .await
            }

            // ---- Autonomic (v1.9+) -----------------------------------------
            "memory_health" => tools::health::execute(&self.storage, effective_args).await,
            "memory_graph" => tools::graph::execute(&self.storage, effective_args).await,

            // ---- Metacognitive (v2.1+) -------------------------------------
            "reflect" => {
                tools::reflect::execute(&self.storage, &self.cognitive, effective_args).await
            }
            "temporal" => tools::temporal::execute(&self.storage, effective_args).await,
            "confidence" => tools::confidence::execute(&self.storage, effective_args).await,

            // ---- Cognitive reasoning (v3.2.1+) -----------------------------
            "deep_reference" | "cross_reference" => {
                tools::cross_reference::execute(&self.storage, &self.cognitive, effective_args)
                    .await
            }

            unknown => {
                // `-32602 Invalid params`, not `-32601 Method not found`: the method
                // (`tools/call`) was found and is supported — it is the tool *name* in
                // the params that the server does not know. MCP `server/tools.md`
                // (Error Handling) uses exactly this example, and the distinction is
                // what lets an agent self-correct ("fix the name") instead of
                // concluding the server is too old to expose tools at all.
                //
                // `-32601` stays reserved for an unknown JSON-RPC *method*
                // (see `McpServer::handle_request`); do not collapse the two.
                return Err(JsonRpcError::invalid_params(&format!(
                    "Unknown tool: {}",
                    unknown
                )));
            }
        };

        // ================================================================
        // DASHBOARD EVENT EMISSION (v2.0)
        // Emit real-time events to WebSocket clients after successful tool calls.
        // ================================================================
        if let Ok(ref content) = result {
            self.emit_tool_event(&request.name, &saved_args, content);
        }

        let response = match result {
            Ok(content) => {
                // Emit the payload twice on purpose: `structuredContent` is the object
                // form a client can validate/consume directly, `content[0].text` keeps
                // JSON-as-string for clients that predate `2025-06-18`. Both carry
                // *compact* JSON — the pretty form's indentation and newlines are
                // re-escaped inside the text block, inflating every response's token
                // count for no readability gain (the client re-renders anyway).
                let text = serde_json::to_string(&content).unwrap_or_else(|_| content.to_string());
                let call_result = CallToolResult {
                    content: vec![crate::protocol::messages::ToolResultContent {
                        content_type: "text".to_string(),
                        text,
                    }],
                    structured_content: content.is_object().then_some(content),
                    is_error: Some(false),
                };
                serde_json::to_value(call_result)
                    .map_err(|e| JsonRpcError::internal_error(&e.to_string()))
            }
            Err(e) => {
                let payload = serde_json::json!({ "error": e });
                let call_result = CallToolResult {
                    content: vec![crate::protocol::messages::ToolResultContent {
                        content_type: "text".to_string(),
                        text: payload.to_string(),
                    }],
                    structured_content: Some(payload),
                    is_error: Some(true),
                };
                serde_json::to_value(call_result)
                    .map_err(|e| JsonRpcError::internal_error(&e.to_string()))
            }
        };

        // Inline consolidation trigger: uses ConsolidationScheduler instead of fixed count
        let count = self.tool_call_count.fetch_add(1, Ordering::Relaxed) + 1;
        let should_consolidate = self
            .cognitive
            .try_lock()
            .ok()
            .map(|cog| cog.consolidation_scheduler.should_consolidate())
            .unwrap_or(count.is_multiple_of(100)); // Fallback to count-based if lock unavailable

        if should_consolidate {
            let storage_clone = Arc::clone(&self.storage);
            let cognitive_clone = Arc::clone(&self.cognitive);
            tokio::spawn(async move {
                if let Ok(mut cog) = cognitive_clone.try_lock() {
                    let _expired = cog.reconsolidation.reconsolidate_expired();
                }

                match tokio::task::spawn_blocking(move || storage_clone.run_consolidation()).await {
                    Ok(Ok(result)) => {
                        tracing::info!(
                            tool_calls = count,
                            decay_applied = result.decay_applied,
                            duplicates_merged = result.duplicates_merged,
                            activations_computed = result.activations_computed,
                            duration_ms = result.duration_ms,
                            "Inline consolidation triggered (scheduler)"
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Inline consolidation failed: {}", e);
                    }
                    Err(e) => {
                        tracing::warn!("Inline consolidation task panicked: {}", e);
                    }
                }
            });
        }

        response
    }
}

//! Dashboard event emission — turns tool-call results into `VestigeEvent`s
//! for the dashboard WebSocket. No-op when no `event_tx` is configured.

use chrono::Utc;

use crate::dashboard::events::VestigeEvent;

use super::McpServer;

impl McpServer {
    /// Extract event data from tool results and emit to dashboard.
    pub(super) fn emit_tool_event(
        &self,
        tool_name: &str,
        args: &Option<serde_json::Value>,
        result: &serde_json::Value,
    ) {
        if self.event_tx.is_none() {
            return;
        }
        let now = Utc::now();

        match tool_name {
            // -- smart_ingest: memory created/updated --
            "smart_ingest" | "ingest" | "session_checkpoint" => {
                // Single mode: result has "decision" (create/update/supersede/reinforce/merge/replace/add_context)
                if let Some(decision) = result.get("decision").and_then(|a| a.as_str()) {
                    let id = result
                        .get("nodeId")
                        .or(result.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let preview = result
                        .get("contentPreview")
                        .or(result.get("content"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    match decision {
                        "create" => {
                            let node_type = result
                                .get("nodeType")
                                .and_then(|v| v.as_str())
                                .unwrap_or("fact")
                                .to_string();
                            let tags = result
                                .get("tags")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|t| t.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default();
                            self.emit(VestigeEvent::MemoryCreated {
                                id,
                                content_preview: preview,
                                node_type,
                                tags,
                                timestamp: now,
                            });
                        }
                        "update" | "supersede" | "reinforce" | "merge" | "replace"
                        | "add_context" => {
                            self.emit(VestigeEvent::MemoryUpdated {
                                id,
                                content_preview: preview,
                                field: decision.to_string(),
                                timestamp: now,
                            });
                        }
                        _ => {}
                    }
                }
                // Batch mode: result has "results" array
                if let Some(results) = result.get("results").and_then(|r| r.as_array()) {
                    for item in results {
                        let decision = item.get("decision").and_then(|a| a.as_str()).unwrap_or("");
                        let id = item
                            .get("nodeId")
                            .or(item.get("id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let preview = item
                            .get("contentPreview")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if decision == "create" {
                            self.emit(VestigeEvent::MemoryCreated {
                                id,
                                content_preview: preview,
                                node_type: "fact".to_string(),
                                tags: vec![],
                                timestamp: now,
                            });
                        } else if !decision.is_empty() {
                            self.emit(VestigeEvent::MemoryUpdated {
                                id,
                                content_preview: preview,
                                field: decision.to_string(),
                                timestamp: now,
                            });
                        }
                    }
                }
            }

            // -- memory: get/delete/promote/demote --
            "memory" | "promote_memory" | "demote_memory" | "delete_knowledge"
            | "get_memory_state" => {
                let action = args
                    .as_ref()
                    .and_then(|a| a.get("action"))
                    .and_then(|a| a.as_str())
                    .unwrap_or(if tool_name == "promote_memory" {
                        "promote"
                    } else if tool_name == "demote_memory" {
                        "demote"
                    } else if tool_name == "delete_knowledge" {
                        "delete"
                    } else {
                        ""
                    });
                let id = args
                    .as_ref()
                    .and_then(|a| a.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                match action {
                    "delete" => {
                        self.emit(VestigeEvent::MemoryDeleted { id, timestamp: now });
                    }
                    "promote" => {
                        let retention = result
                            .get("newRetention")
                            .or(result.get("retrievalStrength"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        self.emit(VestigeEvent::MemoryPromoted {
                            id,
                            new_retention: retention,
                            timestamp: now,
                        });
                    }
                    "demote" => {
                        let retention = result
                            .get("newRetention")
                            .or(result.get("retrievalStrength"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        self.emit(VestigeEvent::MemoryDemoted {
                            id,
                            new_retention: retention,
                            timestamp: now,
                        });
                    }
                    _ => {}
                }
            }

            // -- search --
            "search" | "recall" | "semantic_search" | "hybrid_search" => {
                let query = args
                    .as_ref()
                    .and_then(|a| a.get("query"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let results = result.get("results").and_then(|r| r.as_array());
                let result_count = results.map(|r| r.len()).unwrap_or(0);
                let result_ids: Vec<String> = results
                    .map(|r| {
                        r.iter()
                            .filter_map(|item| {
                                item.get("id").and_then(|v| v.as_str()).map(String::from)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let duration_ms = result
                    .get("durationMs")
                    .or(result.get("duration_ms"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                self.emit(VestigeEvent::SearchPerformed {
                    query,
                    result_count,
                    result_ids,
                    duration_ms,
                    timestamp: now,
                });
            }

            // -- dream --
            "dream" => {
                let replayed = result
                    .get("memoriesReplayed")
                    .or(result.get("memories_replayed"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let connections = result
                    .get("connectionsFound")
                    .or(result.get("connections_found"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let insights = result
                    .get("insightsGenerated")
                    .or(result.get("insights"))
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let duration_ms = result
                    .get("durationMs")
                    .or(result.get("duration_ms"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                self.emit(VestigeEvent::DreamCompleted {
                    memories_replayed: replayed,
                    connections_found: connections,
                    insights_generated: insights,
                    duration_ms,
                    timestamp: now,
                });
            }

            // -- consolidate --
            "consolidate" => {
                let processed = result
                    .get("nodesProcessed")
                    .or(result.get("nodes_processed"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let decay = result
                    .get("decayApplied")
                    .or(result.get("decay_applied"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let embeddings = result
                    .get("embeddingsGenerated")
                    .or(result.get("embeddings_generated"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let duration_ms = result
                    .get("durationMs")
                    .or(result.get("duration_ms"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                self.emit(VestigeEvent::ConsolidationCompleted {
                    nodes_processed: processed,
                    decay_applied: decay,
                    embeddings_generated: embeddings,
                    duration_ms,
                    timestamp: now,
                });
            }

            // -- importance_score --
            "importance_score" => {
                let preview = args
                    .as_ref()
                    .and_then(|a| a.get("content"))
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        if s.len() > 100 {
                            format!("{}...", &s[..s.floor_char_boundary(100)])
                        } else {
                            s.to_string()
                        }
                    })
                    .unwrap_or_default();
                let composite = result
                    .get("compositeScore")
                    .or(result.get("composite_score"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let channels = result.get("channels").or(result.get("breakdown"));
                let novelty = channels
                    .and_then(|c| c.get("novelty"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let arousal = channels
                    .and_then(|c| c.get("arousal"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let reward = channels
                    .and_then(|c| c.get("reward"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let attention = channels
                    .and_then(|c| c.get("attention"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                self.emit(VestigeEvent::ImportanceScored {
                    content_preview: preview,
                    composite_score: composite,
                    novelty,
                    arousal,
                    reward,
                    attention,
                    timestamp: now,
                });
            }

            // Other tools don't emit events
            _ => {}
        }
    }
}

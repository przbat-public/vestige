//! Explore connections tool — Graph exploration, chain building, bridge discovery.
//! v1.5.0: Wires MemoryChainBuilder + ActivationNetwork + HippocampalIndex.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::Storage;

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["chain", "associations", "bridges"],
                "description": "Type of exploration: 'chain' builds reasoning path, 'associations' finds related memories, 'bridges' finds connecting memories"
            },
            "from": {
                "type": "string",
                "description": "Source memory ID"
            },
            "to": {
                "type": "string",
                "description": "Target memory ID (required for 'chain' and 'bridges')"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum results (default: 10)",
                "default": 10
            }
        },
        "required": ["action", "from"]
    })
}

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let args = args.ok_or("Missing arguments")?;
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'action'")?;
    let from = args
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'from'")?;
    let to = args.get("to").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let cog = cognitive.lock().await;

    match action {
        "chain" => {
            let to_id = to.ok_or("'to' is required for chain action")?;
            match cog.chain_builder.build_chain(from, to_id) {
                Some(chain) => Ok(serde_json::json!({
                    "action": "chain",
                    "from": from,
                    "to": to_id,
                    "steps": chain.steps.iter().map(|s| serde_json::json!({
                        "memory_id": s.memory_id,
                        "memory_preview": s.memory_preview,
                        "connection_type": format!("{:?}", s.connection_type),
                        "connection_strength": s.connection_strength,
                        "reasoning": s.reasoning,
                    })).collect::<Vec<_>>(),
                    "confidence": chain.confidence,
                    "total_hops": chain.total_hops,
                    "source": "cognitive_engine",
                })),
                None => {
                    drop(cog);
                    // Fallback: check persisted graph for direct connection
                    let mut steps = Vec::new();
                    let storage_chain = storage.clone();
                    let from_owned = from.to_string();
                    let connections = tokio::task::spawn_blocking(move || {
                        storage_chain.get_connections_for_memory(&from_owned).ok()
                    })
                    .await
                    .map_err(|e| format!("explore chain task panicked: {}", e))?;
                    if let Some(connections) = connections
                        && let Some(conn) = connections
                            .iter()
                            .find(|c| c.source_id == to_id || c.target_id == to_id)
                    {
                        steps.push(serde_json::json!({
                            "memory_id": to_id,
                            "connection_type": conn.link_type,
                            "connection_strength": conn.strength,
                            "reasoning": "Direct connection found in persistent graph",
                            "source": "persistent_graph",
                        }));
                    }
                    let msg = if steps.is_empty() {
                        "No chain found between these memories"
                    } else {
                        "Chain found via persistent graph fallback"
                    };
                    Ok(serde_json::json!({
                        "action": "chain",
                        "from": from,
                        "to": to_id,
                        "steps": steps,
                        "message": msg,
                        "source": if steps.is_empty() { "none" } else { "persistent_graph" },
                    }))
                }
            }
        }
        "associations" => {
            let activation_assocs = cog.activation_network.get_associations(from);
            let hippocampal_assocs = cog
                .hippocampal_index
                .get_associations(from, 2)
                .unwrap_or_default();

            let mut all_associations: Vec<serde_json::Value> = Vec::new();

            for assoc in activation_assocs.iter().take(limit) {
                all_associations.push(serde_json::json!({
                    "memory_id": assoc.memory_id,
                    "strength": assoc.association_strength,
                    "link_type": format!("{:?}", assoc.link_type),
                    "source": "spreading_activation",
                }));
            }
            for m in hippocampal_assocs.iter().take(limit) {
                all_associations.push(serde_json::json!({
                    "memory_id": m.index.memory_id,
                    "semantic_score": m.semantic_score,
                    "text_score": m.text_score,
                    "source": "hippocampal_index",
                }));
            }

            all_associations.truncate(limit);

            // Fallback: if in-memory modules are empty, query storage directly
            if all_associations.is_empty() {
                drop(cog); // release cognitive lock before storage call
                let storage_assoc = storage.clone();
                let from_owned = from.to_string();
                let connections = tokio::task::spawn_blocking(move || {
                    storage_assoc.get_connections_for_memory(&from_owned).ok()
                })
                .await
                .map_err(|e| format!("explore associations task panicked: {}", e))?;
                if let Some(connections) = connections {
                    for conn in connections.iter().take(limit) {
                        let other_id = if conn.source_id == from {
                            &conn.target_id
                        } else {
                            &conn.source_id
                        };
                        all_associations.push(serde_json::json!({
                            "memory_id": other_id,
                            "strength": conn.strength,
                            "link_type": conn.link_type,
                            "source": "persistent_graph",
                        }));
                    }
                }
            }

            Ok(serde_json::json!({
                "action": "associations",
                "from": from,
                "associations": all_associations,
                "count": all_associations.len(),
            }))
        }
        "bridges" => {
            let to_id = to.ok_or("'to' is required for bridges action")?;
            let bridges = cog.chain_builder.find_bridge_memories(from, to_id);
            let limited: Vec<_> = bridges.iter().take(limit).collect();
            if !limited.is_empty() {
                Ok(serde_json::json!({
                    "action": "bridges",
                    "from": from,
                    "to": to_id,
                    "bridges": limited,
                    "count": limited.len(),
                    "source": "cognitive_engine",
                }))
            } else {
                drop(cog);
                // Fallback: find memories connected to both endpoints in persistent graph.
                // Run both fetches in one blocking task to halve the spawn overhead.
                let mut bridge_ids = Vec::new();
                let storage_bridges = storage.clone();
                let from_owned = from.to_string();
                let to_owned = to_id.to_string();
                let conns_pair = tokio::task::spawn_blocking(move || {
                    let from_conns = storage_bridges.get_connections_for_memory(&from_owned).ok();
                    let to_conns = storage_bridges.get_connections_for_memory(&to_owned).ok();
                    (from_conns, to_conns)
                })
                .await
                .map_err(|e| format!("explore bridges task panicked: {}", e))?;
                if let (Some(from_conns), Some(to_conns)) = conns_pair {
                    let from_neighbors: std::collections::HashSet<&str> = from_conns
                        .iter()
                        .map(|c| {
                            if c.source_id == from {
                                c.target_id.as_str()
                            } else {
                                c.source_id.as_str()
                            }
                        })
                        .collect();
                    for conn in &to_conns {
                        let neighbor = if conn.source_id == to_id {
                            &conn.target_id
                        } else {
                            &conn.source_id
                        };
                        if from_neighbors.contains(neighbor.as_str())
                            && neighbor != from
                            && neighbor != to_id
                        {
                            bridge_ids.push(serde_json::json!({
                                "memory_id": neighbor,
                                "source": "persistent_graph",
                            }));
                            if bridge_ids.len() >= limit {
                                break;
                            }
                        }
                    }
                }
                Ok(serde_json::json!({
                    "action": "bridges",
                    "from": from,
                    "to": to_id,
                    "bridges": bridge_ids,
                    "count": bridge_ids.len(),
                    "source": if bridge_ids.is_empty() { "none" } else { "persistent_graph" },
                }))
            }
        }
        _ => Err(format!(
            "Unknown action: '{}'. Expected: chain, associations, bridges",
            action
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    #[test]
    fn test_schema_has_required_fields() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["action"].is_object());
        assert!(s["properties"]["from"].is_object());
        assert!(s["properties"]["to"].is_object());
        assert!(s["properties"]["limit"].is_object());
        let required = s["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")));
        assert!(required.contains(&serde_json::json!("from")));
    }

    #[test]
    fn test_schema_action_enum() {
        let s = schema();
        let action_enum = s["properties"]["action"]["enum"].as_array().unwrap();
        assert!(action_enum.contains(&serde_json::json!("chain")));
        assert!(action_enum.contains(&serde_json::json!("associations")));
        assert!(action_enum.contains(&serde_json::json!("bridges")));
    }

    #[tokio::test]
    async fn test_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_missing_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "from": "some-id" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'action'"));
    }

    #[tokio::test]
    async fn test_missing_from_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "associations" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'from'"));
    }

    #[tokio::test]
    async fn test_unknown_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "invalid", "from": "id1" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown action"));
    }

    #[tokio::test]
    async fn test_chain_missing_to_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "chain", "from": "id1" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'to' is required"));
    }

    #[tokio::test]
    async fn test_bridges_missing_to_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "bridges", "from": "id1" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'to' is required"));
    }

    #[tokio::test]
    async fn test_associations_succeeds_empty() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "associations",
            "from": "00000000-0000-0000-0000-000000000000"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "associations");
        assert!(value["associations"].is_array());
        assert_eq!(value["count"], 0);
    }

    #[tokio::test]
    async fn test_chain_no_path_found() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "chain",
            "from": "00000000-0000-0000-0000-000000000001",
            "to": "00000000-0000-0000-0000-000000000002"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "chain");
        assert_eq!(value["steps"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_bridges_no_results() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "bridges",
            "from": "00000000-0000-0000-0000-000000000001",
            "to": "00000000-0000-0000-0000-000000000002"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "bridges");
        assert_eq!(value["count"], 0);
    }

    #[tokio::test]
    async fn test_associations_with_limit() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "associations",
            "from": "00000000-0000-0000-0000-000000000000",
            "limit": 5
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_associations_storage_fallback() {
        let (storage, _dir) = test_storage().await;

        // Create two memories and a direct connection in storage
        let id1 = storage
            .ingest(vestige_core::IngestInput {
                content: "Memory about Rust".to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["test".to_string()],
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            })
            .unwrap()
            .id;

        let id2 = storage
            .ingest(vestige_core::IngestInput {
                content: "Memory about Cargo".to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["test".to_string()],
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            })
            .unwrap()
            .id;

        // Save connection directly to storage (bypassing cognitive engine)
        let now = chrono::Utc::now();
        storage
            .save_connection(&vestige_core::ConnectionRecord {
                source_id: id1.clone(),
                target_id: id2.clone(),
                strength: 0.9,
                link_type: "semantic".to_string(),
                created_at: now,
                last_activated: now,
                activation_count: 1,
            })
            .unwrap();

        // Execute with empty cognitive engine — should fall back to storage
        let cognitive = test_cognitive();
        let args = serde_json::json!({
            "action": "associations",
            "from": id1,
        });
        let result = execute(&storage, &cognitive, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        let associations = value["associations"].as_array().unwrap();
        assert!(
            !associations.is_empty(),
            "Should find associations via storage fallback"
        );
        assert_eq!(associations[0]["source"], "persistent_graph");
        assert_eq!(associations[0]["memory_id"], id2);
    }
}

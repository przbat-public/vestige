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
                "enum": ["chain", "associations", "bridges", "causal_chain"],
                "description": "Type of exploration: 'chain' builds a reasoning path, 'associations' finds related memories, 'bridges' finds connecting memories, 'causal_chain' walks ONLY causal edges (`X causes Y` style) from `from` outward — useful for `why?` and root-cause questions"
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
            },
            "depth": {
                "type": "integer",
                "description": "Max BFS depth for 'causal_chain' (default: 3, max: 6). Higher values may surface noisier links.",
                "default": 3
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
            // Activation network is its own lock — read it under a short
            // try_read so a concurrent ingest doesn't stall explore. Falls
            // back to an empty list on contention; hippocampal index +
            // storage fallback below still provide associations.
            let activation_assocs = cog
                .activation_network
                .try_read()
                .map(|n| n.get_associations(from))
                .unwrap_or_default();
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
        "causal_chain" => {
            // Drop the cognitive lock before the SQLite walk so other
            // tool calls can proceed concurrently. The walk is read-only
            // and never touches the cognitive engine.
            drop(cog);
            let depth_arg = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(3);
            // Schema documents max=6. We clamp instead of erroring so a
            // hostile or buggy caller can't trigger an unbounded BFS.
            let depth = depth_arg.min(MAX_CAUSAL_DEPTH);
            let storage_chain = storage.clone();
            let from_owned = from.to_string();
            let edges_per_node = tokio::task::spawn_blocking(move || {
                bfs_causal(&storage_chain, &from_owned, depth as usize, limit)
            })
            .await
            .map_err(|e| format!("explore causal_chain task panicked: {}", e))?;

            let nodes: Vec<serde_json::Value> = edges_per_node
                .iter()
                .map(|step| {
                    serde_json::json!({
                        "memory_id": step.target_id,
                        "via": step.parent_id,
                        "hop": step.hop,
                        "strength": step.strength,
                        "link_type": "causal",
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "action": "causal_chain",
                "from": from,
                "depth": depth,
                "nodes": nodes,
                "count": nodes.len(),
                "source": "persistent_graph",
            }))
        }
        _ => Err(format!(
            "Unknown action: '{}'. Expected: chain, associations, bridges, causal_chain",
            action
        )),
    }
}

/// Maximum BFS depth for `causal_chain`. Six hops is enough to walk
/// realistic root-cause chains without devolving into "every related
/// memory in the database" — keeps the response token-bounded and the
/// SQLite walk cheap. Bumping this requires re-checking the
/// `causal_chain_clamps_max_depth` test.
const MAX_CAUSAL_DEPTH: u64 = 6;

#[derive(Debug)]
struct CausalStep {
    target_id: String,
    parent_id: String,
    hop: usize,
    strength: f64,
}

/// Breadth-first traversal of the persistent `memory_connections` graph
/// that follows ONLY edges with `link_type == "causal"`.
///
/// Returns a flat list of `CausalStep` so the caller can render either a
/// node list or a parent-child tree. Stops at `depth` hops or `limit`
/// nodes, whichever comes first. Outgoing edges only — semantic and
/// part-of edges are deliberately skipped to keep the walk focused on
/// `X causes Y` chains.
fn bfs_causal(
    storage: &std::sync::Arc<vestige_core::Storage>,
    from: &str,
    depth: usize,
    limit: usize,
) -> Vec<CausalStep> {
    use std::collections::{HashSet, VecDeque};
    let mut out = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(from.to_string());
    let mut frontier: VecDeque<(String, usize)> = VecDeque::new();
    frontier.push_back((from.to_string(), 0));

    while let Some((current, hop)) = frontier.pop_front() {
        if hop >= depth || out.len() >= limit {
            continue;
        }
        let Ok(edges) = storage.get_connections_for_memory(&current) else {
            continue;
        };
        for edge in edges {
            if edge.link_type != "causal" {
                continue;
            }
            // Outgoing only — the causal direction matters. `B causes A`
            // is not the same as `A causes B`, even if both endpoints
            // are visible from either side of `get_connections_for_memory`.
            if edge.source_id != current {
                continue;
            }
            if !visited.insert(edge.target_id.clone()) {
                continue;
            }
            out.push(CausalStep {
                target_id: edge.target_id.clone(),
                parent_id: edge.source_id.clone(),
                hop: hop + 1,
                strength: edge.strength,
            });
            if out.len() >= limit {
                break;
            }
            frontier.push_back((edge.target_id, hop + 1));
        }
    }
    out
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
        assert!(
            action_enum.contains(&serde_json::json!("causal_chain")),
            "causal_chain must be in the action enum so MCP clients can call it; \
             without it, the only way to discover the action is to read the source"
        );
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

    // ------------------------------------------------------------------
    // Causal-chain action — walks ONLY persisted edges with
    // `link_type == "causal"`. Pre 2026-05-22 every ingest-time edge was
    // forced to Causal regardless of verb, which made any such walk
    // useless; this set of tests pins the new contract.
    // ------------------------------------------------------------------

    /// Builds a deterministic two-hop causal chain A → B → C in storage.
    async fn three_node_causal_chain(storage: &Arc<Storage>) -> (String, String, String) {
        let id_a = storage
            .ingest(vestige_core::IngestInput {
                content: "Root cause A".into(),
                node_type: "fact".into(),
                ..Default::default()
            })
            .unwrap()
            .id;
        let id_b = storage
            .ingest(vestige_core::IngestInput {
                content: "Intermediate B".into(),
                node_type: "fact".into(),
                ..Default::default()
            })
            .unwrap()
            .id;
        let id_c = storage
            .ingest(vestige_core::IngestInput {
                content: "Effect C".into(),
                node_type: "fact".into(),
                ..Default::default()
            })
            .unwrap()
            .id;

        let now = chrono::Utc::now();
        storage
            .save_connection(&vestige_core::ConnectionRecord {
                source_id: id_a.clone(),
                target_id: id_b.clone(),
                strength: 0.9,
                link_type: "causal".into(),
                created_at: now,
                last_activated: now,
                activation_count: 0,
            })
            .unwrap();
        storage
            .save_connection(&vestige_core::ConnectionRecord {
                source_id: id_b.clone(),
                target_id: id_c.clone(),
                strength: 0.85,
                link_type: "causal".into(),
                created_at: now,
                last_activated: now,
                activation_count: 0,
            })
            .unwrap();
        (id_a, id_b, id_c)
    }

    #[tokio::test]
    async fn causal_chain_walks_only_causal_edges() {
        let (storage, _dir) = test_storage().await;
        let (id_a, id_b, id_c) = three_node_causal_chain(&storage).await;

        // Inject a NON-causal edge from A → noise: we must not see it
        // in the result, otherwise the causal walk is just
        // `associations` with extra steps.
        let noise_id = storage
            .ingest(vestige_core::IngestInput {
                content: "Unrelated noise".into(),
                node_type: "fact".into(),
                ..Default::default()
            })
            .unwrap()
            .id;
        let now = chrono::Utc::now();
        storage
            .save_connection(&vestige_core::ConnectionRecord {
                source_id: id_a.clone(),
                target_id: noise_id.clone(),
                strength: 0.95,
                link_type: "semantic".into(),
                created_at: now,
                last_activated: now,
                activation_count: 0,
            })
            .unwrap();

        let args = serde_json::json!({
            "action": "causal_chain",
            "from": id_a,
            "depth": 3,
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .expect("causal_chain must not error");
        assert_eq!(result["action"], "causal_chain");
        let nodes = result["nodes"].as_array().expect("nodes array");
        let node_ids: Vec<&str> = nodes.iter().filter_map(|n| n["memory_id"].as_str()).collect();
        assert!(
            node_ids.contains(&id_b.as_str()),
            "expected B in causal chain from A; got {:?}",
            node_ids
        );
        assert!(
            node_ids.contains(&id_c.as_str()),
            "expected C (2 hops via causal edges) in chain from A; got {:?}",
            node_ids
        );
        assert!(
            !node_ids.contains(&noise_id.as_str()),
            "noise node attached via SEMANTIC edge must NOT appear in causal walk; got {:?}",
            node_ids
        );
    }

    #[tokio::test]
    async fn causal_chain_respects_depth_limit() {
        let (storage, _dir) = test_storage().await;
        let (id_a, id_b, id_c) = three_node_causal_chain(&storage).await;

        let args = serde_json::json!({
            "action": "causal_chain",
            "from": id_a,
            "depth": 1,
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .expect("causal_chain must not error");
        let nodes = result["nodes"].as_array().expect("nodes array");
        let node_ids: Vec<&str> = nodes.iter().filter_map(|n| n["memory_id"].as_str()).collect();
        assert!(
            node_ids.contains(&id_b.as_str()),
            "depth=1 must reach the immediate neighbour B"
        );
        assert!(
            !node_ids.contains(&id_c.as_str()),
            "depth=1 must NOT reach C (two hops away). got {:?}",
            node_ids
        );
    }

    #[tokio::test]
    async fn causal_chain_clamps_max_depth() {
        let (storage, _dir) = test_storage().await;
        let (id_a, _id_b, _id_c) = three_node_causal_chain(&storage).await;

        // Explicit hostile input — depth bigger than the documented cap.
        let args = serde_json::json!({
            "action": "causal_chain",
            "from": id_a,
            "depth": 9_999,
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .expect("causal_chain must accept large depth without panicking");
        // The cap is published in the schema and enforced in the
        // implementation. Surface it in the response so callers can
        // tell their requested depth was clamped.
        let effective = result["depth"]
            .as_u64()
            .expect("response must include applied `depth` after clamping");
        assert!(
            effective <= 6,
            "depth was not clamped — got {effective}, max allowed is 6"
        );
    }

    #[tokio::test]
    async fn causal_chain_empty_when_no_outgoing_edges() {
        let (storage, _dir) = test_storage().await;
        let isolated = storage
            .ingest(vestige_core::IngestInput {
                content: "Alone".into(),
                node_type: "fact".into(),
                ..Default::default()
            })
            .unwrap()
            .id;

        let args = serde_json::json!({
            "action": "causal_chain",
            "from": isolated,
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .expect("causal_chain on isolated node must not error");
        assert_eq!(result["count"], 0);
        let nodes = result["nodes"].as_array().expect("nodes array");
        assert!(nodes.is_empty());
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

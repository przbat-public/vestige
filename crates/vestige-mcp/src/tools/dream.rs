//! Dream tool — 4-phase biologically-accurate dream cycle.
//!
//! v2.1.0: Upgraded from MemoryDreamer heuristic pipeline to the
//! neuroscience-grounded DreamEngine (Diekelmann & Born 2010,
//! Stickgold & Walker 2013, Tononi & Cirelli 2006).
//!
//! Phases: NREM1 (triage) → NREM3 (consolidation) → REM (creative) → Integration

use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::Utc;
use crate::cognitive::CognitiveEngine;
use vestige_core::{CreativeConnectionType, DreamHistoryRecord, LinkType, Storage};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "memory_count": {
                "type": "integer",
                "description": "Number of recent memories to dream about (default: 50)",
                "default": 50
            }
        }
    })
}

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let memory_count = args
        .as_ref()
        .and_then(|a| a.get("memory_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;

    // v1.9.0: Waking SWR tagging — preferential replay of tagged memories (70/30 split)
    let tagged_nodes = storage.get_waking_tagged_memories(memory_count as i32)
        .unwrap_or_default();
    let tagged_count = tagged_nodes.len();

    // Calculate how many tagged vs random to include
    let tagged_target = (memory_count * 7 / 10).min(tagged_count); // 70% tagged
    let _random_target = memory_count.saturating_sub(tagged_target);  // 30% random (used for logging)

    // Build the dream memory set: tagged memories first, then fill with random
    let tagged_ids: std::collections::HashSet<String> = tagged_nodes.iter()
        .take(tagged_target)
        .map(|n| n.id.clone())
        .collect();

    let random_nodes = storage.get_all_nodes(memory_count as i32, 0)
        .map_err(|e| format!("Failed to load memories: {}", e))?;

    let mut all_nodes: Vec<_> = tagged_nodes.into_iter().take(tagged_target).collect();
    for node in random_nodes {
        if !tagged_ids.contains(&node.id) && all_nodes.len() < memory_count {
            all_nodes.push(node);
        }
    }
    // If still under capacity (e.g., all memories are tagged), fill from remaining tagged
    if all_nodes.len() < memory_count {
        let used_ids: std::collections::HashSet<String> = all_nodes.iter().map(|n| n.id.clone()).collect();
        let remaining_tagged = storage.get_waking_tagged_memories(memory_count as i32)
            .unwrap_or_default();
        for node in remaining_tagged {
            if !used_ids.contains(&node.id) && all_nodes.len() < memory_count {
                all_nodes.push(node);
            }
        }
    }

    if all_nodes.len() < 5 {
        return Ok(serde_json::json!({
            "status": "insufficient_memories",
            "message": format!("Need at least 5 memories to dream. Current count: {}", all_nodes.len()),
            "count": all_nodes.len()
        }));
    }

    // ====================================================================
    // Run DreamEngine 4-phase cycle (NREM1 → NREM3 → REM → Integration)
    // Uses split borrows on CognitiveEngine fields to satisfy borrow checker.
    // ====================================================================
    let dream_result = {
        let mut cog = cognitive.lock().await;
        let cog = &mut *cog;
        cog.dream_engine.run(
            &all_nodes,
            &mut cog.emotional_memory,
            &cog.importance_signals,
            &mut cog.synaptic_tagging,
        )
    };

    // Also run legacy MemoryDreamer for backward-compatible insight synthesis
    let dream_memories: Vec<vestige_core::DreamMemory> = all_nodes.iter().map(|n| {
        vestige_core::DreamMemory {
            id: n.id.clone(),
            content: n.content.clone(),
            embedding: storage.get_node_embedding(&n.id).ok().flatten(),
            tags: n.tags.clone(),
            created_at: n.created_at,
            access_count: n.reps as u32,
        }
    }).collect();

    let extra_insights = {
        let cog = cognitive.lock().await;
        cog.dreamer.synthesize_insights(&dream_memories)
    };

    // Persist creative connections from 4-phase dream cycle
    let mut connections_persisted = 0u64;
    {
        let now = Utc::now();
        for conn in &dream_result.creative_connections {
            let link_type = match conn.connection_type {
                CreativeConnectionType::CrossDomain => "semantic",
                CreativeConnectionType::Causal => "causal",
                CreativeConnectionType::Complementary => "complementary",
                CreativeConnectionType::Contradictory => "contradiction",
            };
            let record = vestige_core::ConnectionRecord {
                source_id: conn.memory_a_id.clone(),
                target_id: conn.memory_b_id.clone(),
                strength: conn.confidence,
                link_type: link_type.to_string(),
                created_at: now,
                last_activated: now,
                activation_count: 1,
            };
            match storage.save_connection(&record) {
                Ok(_) => connections_persisted += 1,
                Err(e) => {
                    tracing::warn!(
                        source = %conn.memory_a_id,
                        target = %conn.memory_b_id,
                        link_type = %link_type,
                        "Failed to persist dream connection: {}",
                        e
                    );
                }
            }
        }
        if connections_persisted > 0 {
            tracing::info!(
                connections_persisted = connections_persisted,
                "Dream: persisted {} connections to database",
                connections_persisted
            );
        }
    }

    // Hydrate live cognitive engine with newly persisted connections
    if connections_persisted > 0 {
        let mut cog = cognitive.lock().await;
        for conn in &dream_result.creative_connections {
            let link_type_enum = match conn.connection_type {
                CreativeConnectionType::CrossDomain => LinkType::Semantic,
                CreativeConnectionType::Causal => LinkType::Causal,
                CreativeConnectionType::Complementary => LinkType::Semantic,
                CreativeConnectionType::Contradictory => LinkType::Semantic,
            };
            cog.activation_network.add_edge(
                conn.memory_a_id.clone(),
                conn.memory_b_id.clone(),
                link_type_enum,
                conn.confidence,
            );
        }
    }

    // Persist dream history with per-phase timings
    {
        let phase_ms = |name: vestige_core::DreamPhase| -> Option<i64> {
            dream_result.phases.iter()
                .find(|p| p.phase == name)
                .map(|p| p.duration_ms as i64)
        };
        let record = DreamHistoryRecord {
            dreamed_at: Utc::now(),
            duration_ms: dream_result.total_duration_ms as i64,
            memories_replayed: dream_result.memories_replayed as i32,
            connections_found: dream_result.creative_connections.len() as i32,
            insights_generated: dream_result.insights.len() as i32,
            memories_strengthened: dream_result.memories_strengthened as i32,
            memories_compressed: dream_result.memories_downscaled as i32,
            phase_nrem1_ms: phase_ms(vestige_core::DreamPhase::Nrem1),
            phase_nrem3_ms: phase_ms(vestige_core::DreamPhase::Nrem3),
            phase_rem_ms: phase_ms(vestige_core::DreamPhase::Rem),
            phase_integration_ms: phase_ms(vestige_core::DreamPhase::Integration),
            summaries_generated: None,
            emotional_memories_processed: Some(dream_result.emotional_processed as i32),
            creative_connections_found: Some(dream_result.creative_connections.len() as i32),
        };
        if let Err(e) = storage.save_dream_history(&record) {
            tracing::warn!("Failed to persist dream history: {}", e);
        }
    }

    // Clear waking tags after dream processes them
    let tags_cleared = storage.clear_waking_tags().unwrap_or(0);

    // Merge insights from 4-phase engine + legacy synthesizer
    let all_insights: Vec<serde_json::Value> = dream_result.insights.iter()
        .map(|i| serde_json::json!({
            "insight_type": i.insight_type,
            "insight": i.insight,
            "source_memories": i.source_memory_ids,
            "confidence": i.confidence,
            "novelty_score": i.novelty,
        }))
        .chain(extra_insights.iter().map(|i| serde_json::json!({
            "insight_type": format!("{:?}", i.insight_type),
            "insight": i.insight,
            "source_memories": i.source_memories,
            "confidence": i.confidence,
            "novelty_score": i.novelty_score,
        })))
        .collect();

    // Contradiction detection from creative connections
    let contradictions: Vec<serde_json::Value> = dream_result.creative_connections.iter()
        .filter(|c| c.connection_type == CreativeConnectionType::Contradictory)
        .map(|c| serde_json::json!({
            "memoryA": c.memory_a_id,
            "memoryB": c.memory_b_id,
            "confidence": c.confidence,
            "insight": c.insight,
        }))
        .collect();

    Ok(serde_json::json!({
        "status": "dreamed",
        "memoriesReplayed": dream_result.memories_replayed,
        "wakingTagsProcessed": tagged_target,
        "wakingTagsCleared": tags_cleared,
        "insights": all_insights,
        "connectionsPersisted": connections_persisted,
        "contradictions": contradictions,
        "phases": dream_result.phases.iter().map(|p| serde_json::json!({
            "phase": p.phase.as_str(),
            "durationMs": p.duration_ms,
            "memoriesProcessed": p.memories_processed,
            "actions": p.actions,
        })).collect::<Vec<_>>(),
        "stats": {
            "creative_connections_found": dream_result.creative_connections.len(),
            "connections_persisted": connections_persisted,
            "memories_strengthened": dream_result.memories_strengthened,
            "memories_downscaled": dream_result.memories_downscaled,
            "emotional_processed": dream_result.emotional_processed,
            "contradictions_found": contradictions.len(),
            "insights_generated": all_insights.len(),
            "total_duration_ms": dream_result.total_duration_ms,
        }
    }))
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

    async fn ingest_n_memories(storage: &Arc<Storage>, n: usize) {
        for i in 0..n {
            storage.ingest(vestige_core::IngestInput {
                content: format!("Dream test memory number {}", i),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["dream-test".to_string()],
                valid_from: None,
                valid_until: None,
            })
            .unwrap();
        }
    }

    #[test]
    fn test_schema_has_properties() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["memory_count"].is_object());
        assert_eq!(s["properties"]["memory_count"]["default"], 50);
    }

    #[tokio::test]
    async fn test_dream_insufficient_memories() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 3).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "insufficient_memories");
        assert_eq!(value["count"], 3);
    }

    #[tokio::test]
    async fn test_dream_empty_database() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "insufficient_memories");
        assert_eq!(value["count"], 0);
    }

    #[tokio::test]
    async fn test_dream_with_enough_memories() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 10).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");
        assert!(value["memoriesReplayed"].as_u64().unwrap() >= 5);
        assert!(value["insights"].is_array());
        assert!(value["stats"].is_object());
    }

    #[tokio::test]
    async fn test_dream_custom_memory_count() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 10).await;
        let args = serde_json::json!({ "memory_count": 7 });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");
        assert!(value["memoriesReplayed"].as_u64().unwrap() <= 7);
    }

    #[tokio::test]
    async fn test_dream_with_exactly_5_memories() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 5).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");
    }

    #[tokio::test]
    async fn test_dream_stats_fields_present() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 6).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        let value = result.unwrap();
        assert!(value["stats"]["creative_connections_found"].is_number());
        assert!(value["stats"]["memories_strengthened"].is_number());
        assert!(value["stats"]["memories_downscaled"].is_number());
        assert!(value["stats"]["insights_generated"].is_number());
        assert!(value["stats"]["total_duration_ms"].is_number());
        assert!(value["stats"]["emotional_processed"].is_number());
        assert!(value["phases"].is_array());
    }

    #[tokio::test]
    async fn test_dream_persists_to_database() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 10).await;

        // Before dream: no dream history
        {
            assert!(storage.get_last_dream().unwrap().is_none());
        }

        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");

        // After dream: dream history should exist
        {
            let last = storage.get_last_dream().unwrap();
            assert!(last.is_some(), "Dream should have been persisted to database");
        }
    }

    #[tokio::test]
    async fn test_dream_connections_round_trip() {
        // Verify dream → persist → query round-trip
        let (storage, _dir) = test_storage().await;

        // Create enough diverse memories to trigger connection discovery
        for i in 0..15 {
            storage.ingest(vestige_core::IngestInput {
                content: format!(
                    "Memory {} about topic {}: detailed content for connection discovery",
                    i,
                    if i % 3 == 0 { "rust" } else if i % 3 == 1 { "cargo" } else { "testing" }
                ),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["dream-roundtrip".to_string()],
                valid_from: None,
                valid_until: None,
            }).unwrap();
        }

        let cognitive = test_cognitive();
        let result = execute(&storage, &cognitive, None).await.unwrap();
        assert_eq!(result["status"], "dreamed");

        let persisted = result["connectionsPersisted"].as_u64().unwrap_or(0);
        if persisted > 0 {
            // Verify connections are queryable from storage
            let all_conns = storage.get_all_connections().unwrap();
            assert!(!all_conns.is_empty(), "Persisted connections should be queryable");

            // Verify connection IDs reference valid memories
            let all_nodes = storage.get_all_nodes(100, 0).unwrap();
            let valid_ids: std::collections::HashSet<String> =
                all_nodes.iter().map(|n| n.id.clone()).collect();
            for conn in &all_conns {
                assert!(
                    valid_ids.contains(&conn.source_id),
                    "Connection source_id {} should reference a valid memory",
                    conn.source_id
                );
                assert!(
                    valid_ids.contains(&conn.target_id),
                    "Connection target_id {} should reference a valid memory",
                    conn.target_id
                );
            }

            // Verify live cognitive engine was hydrated
            let cog = cognitive.lock().await;
            let first_conn = &all_conns[0];
            let assocs = cog.activation_network.get_associations(&first_conn.source_id);
            assert!(
                !assocs.is_empty(),
                "Live cognitive engine should have been hydrated with dream connections"
            );
        }
    }

    /// Directly test save_connection with real memory IDs — isolates the persistence layer.
    #[tokio::test]
    async fn test_save_connection_with_dream_ids() {
        let (storage, _dir) = test_storage().await;

        // Ingest memories and collect their IDs
        let mut ids = Vec::new();
        for i in 0..5 {
            let result = storage.ingest(vestige_core::IngestInput {
                content: format!("Save connection test memory {}", i),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["save-conn-test".to_string()],
                valid_from: None,
                valid_until: None,
            }).unwrap();
            ids.push(result.id);
        }

        // Simulate what dream does: save connections between real memory IDs
        let now = chrono::Utc::now();
        let mut saved = 0u32;
        let mut errors = Vec::new();
        for i in 0..ids.len() {
            for j in (i+1)..ids.len() {
                let record = vestige_core::ConnectionRecord {
                    source_id: ids[i].clone(),
                    target_id: ids[j].clone(),
                    strength: 0.75,
                    link_type: "semantic".to_string(),
                    created_at: now,
                    last_activated: now,
                    activation_count: 1,
                };
                match storage.save_connection(&record) {
                    Ok(_) => saved += 1,
                    Err(e) => errors.push(format!(
                        "{} -> {}: {}",
                        ids[i], ids[j], e
                    )),
                }
            }
        }

        assert!(
            errors.is_empty(),
            "save_connection failed for {} of {} connections:\n{}",
            errors.len(),
            saved + errors.len() as u32,
            errors.join("\n")
        );
        assert!(saved > 0, "Should have saved at least one connection");

        // Verify they're queryable
        let all = storage.get_all_connections().unwrap();
        assert_eq!(all.len(), saved as usize);

        // Verify per-memory query
        let conns = storage.get_connections_for_memory(&ids[0]).unwrap();
        assert!(
            !conns.is_empty(),
            "get_connections_for_memory should return connections for {}",
            ids[0]
        );
    }

    /// Test that dream discovers cross-domain connections and persists them.
    /// DreamEngine's REM phase pairs memories across different primary tag
    /// groups — memories must have distinct first tags to trigger this.
    #[tokio::test]
    async fn test_dream_discovers_and_persists_connections() {
        let (storage, _dir) = test_storage().await;

        // Memories with DIFFERENT primary tags but overlapping content words
        // to enable cross-domain Jaccard similarity matching in REM phase
        let topics = [
            ("Error handling with Result type prevents crashes in production code", vec!["safety"]),
            ("Type safety prevents runtime crashes and data corruption", vec!["safety"]),
            ("Error handling with try-catch prevents crashes in production code", vec!["typescript"]),
            ("Type checking prevents runtime errors in compiled code", vec!["typescript"]),
            ("Error handling patterns prevent production failures in services", vec!["architecture"]),
            ("Production monitoring prevents cascading failures in code", vec!["architecture"]),
            ("Database error handling prevents data loss in production", vec!["database"]),
            ("Query optimization prevents timeout errors in production code", vec!["database"]),
        ];

        for (content, tags) in &topics {
            storage.ingest(vestige_core::IngestInput {
                content: content.to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: tags.iter().map(|t| t.to_string()).collect(),
                valid_from: None,
                valid_until: None,
            }).unwrap();
        }

        let cognitive = test_cognitive();
        let result = execute(&storage, &cognitive, None).await.unwrap();
        assert_eq!(result["status"], "dreamed");

        let found = result["stats"]["creative_connections_found"].as_u64().unwrap_or(0);
        let persisted = result["connectionsPersisted"].as_u64().unwrap_or(0);

        assert!(
            found > 0,
            "Dream should discover cross-domain connections between related memories (found: {})",
            found
        );

        assert_eq!(
            persisted, found,
            "All {} discovered connections should persist, but only {} did.",
            found, persisted
        );

        let stored = storage.get_all_connections().unwrap();
        assert_eq!(
            stored.len(),
            persisted as usize,
            "Storage should contain exactly {} connections",
            persisted
        );
    }
}

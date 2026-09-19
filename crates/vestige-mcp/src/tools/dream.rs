//! Dream tool — 4-phase biologically-accurate dream cycle.
//!
//! v2.1.0: Upgraded from MemoryDreamer heuristic pipeline to the
//! neuroscience-grounded DreamEngine (Diekelmann & Born 2010,
//! Stickgold & Walker 2013, Tononi & Cirelli 2006).
//!
//! Phases: NREM1 (triage) → NREM3 (consolidation) → REM (creative) → Integration

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use chrono::Utc;
use vestige_core::memory::{HubMetadata, InsightMetadata, InsightOrigin, merge_hub_into_extra};
use vestige_core::{
    CreativeConnectionType, DreamHistoryRecord, IngestInput, InsightRecord, LinkType, Storage,
};

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
        .unwrap_or(50)
        .min(500) as usize;

    // v1.9.0: Waking SWR tagging — preferential replay of tagged memories (70/30 split)
    // All four storage reads run on the blocking pool — they would otherwise
    // monopolize a Tokio worker on large bases.
    let storage_pre = storage.clone();
    let memory_count_i32 = memory_count as i32;
    let all_nodes = tokio::task::spawn_blocking(
        move || -> Result<Vec<vestige_core::KnowledgeNode>, String> {
            let tagged_nodes = storage_pre
                .get_waking_tagged_memories(memory_count_i32)
                .unwrap_or_default();
            let tagged_count = tagged_nodes.len();
            let tagged_target = (memory_count * 7 / 10).min(tagged_count); // 70% tagged

            let tagged_ids: std::collections::HashSet<String> = tagged_nodes
                .iter()
                .take(tagged_target)
                .map(|n| n.id.clone())
                .collect();

            let random_nodes = storage_pre
                .get_all_nodes(memory_count_i32, 0)
                .map_err(|e| format!("Failed to load memories: {}", e))?;

            let mut all_nodes: Vec<_> = tagged_nodes.into_iter().take(tagged_target).collect();
            for node in random_nodes {
                if !tagged_ids.contains(&node.id) && all_nodes.len() < memory_count {
                    all_nodes.push(node);
                }
            }
            // If still under capacity (e.g., all memories are tagged), top up from
            // the remaining tagged memories.
            if all_nodes.len() < memory_count {
                let used_ids: std::collections::HashSet<String> =
                    all_nodes.iter().map(|n| n.id.clone()).collect();
                let remaining_tagged = storage_pre
                    .get_waking_tagged_memories(memory_count_i32)
                    .unwrap_or_default();
                for node in remaining_tagged {
                    if !used_ids.contains(&node.id) && all_nodes.len() < memory_count {
                        all_nodes.push(node);
                    }
                }
            }
            Ok(all_nodes)
        },
    )
    .await
    .map_err(|e| format!("dream pre-fetch task panicked: {}", e))??;
    let tagged_target = (memory_count * 7 / 10).min(all_nodes.len());

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

    // Also run legacy MemoryDreamer for backward-compatible insight synthesis.
    // Fetch embeddings on the blocking pool — one SQL call per memory adds up
    // to dozens of milliseconds even at 50 memories.
    let storage_emb = storage.clone();
    type NodeMeta = (
        String,
        String,
        Vec<String>,
        chrono::DateTime<chrono::Utc>,
        i32,
    );
    let node_meta: Vec<NodeMeta> = all_nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                n.content.clone(),
                n.tags.clone(),
                n.created_at,
                n.reps,
            )
        })
        .collect();
    let dream_memories: Vec<vestige_core::DreamMemory> = tokio::task::spawn_blocking(move || {
        node_meta
            .into_iter()
            .map(|(id, content, tags, created_at, reps)| {
                // `DreamMemory.embedding` is optional, so a build without
                // embedding storage simply runs the dream cycle without the
                // vector-similarity signals instead of failing to compile.
                let embedding = crate::retrieval::node_embedding(&storage_emb, &id);
                vestige_core::DreamMemory {
                    id,
                    content,
                    embedding,
                    tags,
                    created_at,
                    access_count: reps as u32,
                }
            })
            .collect()
    })
    .await
    .map_err(|e| format!("dream embedding fetch task panicked: {}", e))?;

    let (extra_insights, hub_candidates) = {
        // Dreamer synthesis can take seconds on large memory sets. Clone
        // the dreamer Arc out of the engine and drop the engine lock
        // before doing the work — otherwise every other tool call stalls
        // for the duration of synthesis.
        let dreamer = {
            let cog = cognitive.lock().await;
            std::sync::Arc::clone(&cog.dreamer)
        };
        let dreamer = dreamer.read().await;
        let insights = dreamer.synthesize_insights(&dream_memories);
        // Topic Hubs (Proposal A) — feed the same memories through the
        // hub generator so a single dream cycle materialises both insights
        // and hubs. The dreamer enforces MIN_HUB_CLUSTER_SIZE so this is a
        // no-op when clustering is sparse.
        let hubs = dreamer.synthesize_hubs(&dream_memories);
        (insights, hubs)
    };

    // Apply NREM3 consolidation in SQL: strengthen replayed memories and
    // mark downscaled-low-importance memories for accelerated decay. Up to
    // here the boost was tag-only (in-memory `SynapticTaggingSystem`), which
    // made the dream look productive in the response but left the database
    // untouched. This block mirrors strengthened/downscaled IDs into the
    // durable retention/retrieval columns so the next search/FSRS pass sees
    // them. Failures are logged but never fail the dream call.
    {
        let storage_nrem3 = storage.clone();
        let strengthened: Vec<String> = dream_result.strengthened_ids.clone();
        let downscaled: Vec<String> = dream_result.downscaled_ids.clone();
        // Use the engine's own factor so MCP and core can't drift apart.
        // The engine reads `VESTIGE_NREM3_DOWNSCALE_FACTOR` at construction
        // time, so a fleet-wide change only needs the env var on the dream
        // worker, not a recompile of the MCP binary.
        let downscale_factor = dream_result.downscale_factor;
        let _ = tokio::task::spawn_blocking(move || {
            if !strengthened.is_empty() {
                let refs: Vec<&str> = strengthened.iter().map(|s| s.as_str()).collect();
                if let Err(e) = storage_nrem3.strengthen_batch_on_access(&refs) {
                    tracing::warn!(error = %e, count = strengthened.len(),
                        "NREM3 strengthen_batch_on_access failed");
                }
            }
            if !downscaled.is_empty() {
                let refs: Vec<&str> = downscaled.iter().map(|s| s.as_str()).collect();
                if let Err(e) = storage_nrem3.downscale_retention_batch(&refs, downscale_factor) {
                    tracing::warn!(error = %e, count = downscaled.len(),
                        "NREM3 downscale_retention_batch failed");
                }
            }
        })
        .await;
    }

    // Persist creative connections from 4-phase dream cycle.
    // Run all save_connection() inserts on the blocking pool — easily
    // hundreds of writes on dense graphs.
    let storage_conn = storage.clone();
    let creative_for_persist = dream_result.creative_connections.clone();
    let connections_persisted: u64 = tokio::task::spawn_blocking(move || {
        let now = Utc::now();
        let mut persisted = 0u64;
        for conn in &creative_for_persist {
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
            match storage_conn.save_connection(&record) {
                Ok(_) => persisted += 1,
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
        persisted
    })
    .await
    .map_err(|e| format!("save_connection task panicked: {}", e))?;
    if connections_persisted > 0 {
        tracing::info!(
            connections_persisted = connections_persisted,
            "Dream: persisted {} connections to database",
            connections_persisted
        );
    }

    // Hydrate live cognitive engine with newly persisted connections.
    // The activation network is its own lock now — clone the Arc and
    // drop the engine guard before the writes.
    if connections_persisted > 0 {
        let net = {
            let cog = cognitive.lock().await;
            std::sync::Arc::clone(&cog.activation_network)
        };
        let mut net = net.write().await;
        for conn in &dream_result.creative_connections {
            let link_type_enum = match conn.connection_type {
                CreativeConnectionType::CrossDomain => LinkType::Semantic,
                CreativeConnectionType::Causal => LinkType::Causal,
                CreativeConnectionType::Complementary => LinkType::Semantic,
                CreativeConnectionType::Contradictory => LinkType::Semantic,
            };
            net.add_edge(
                conn.memory_a_id.clone(),
                conn.memory_b_id.clone(),
                link_type_enum,
                conn.confidence,
            );
        }
    }

    // Persist dream history + insights + clear waking tags in one blocking
    // task — three sets of writes that together can run for hundreds of ms.
    let storage_persist = storage.clone();
    let dream_history_record = {
        let phase_ms = |name: vestige_core::DreamPhase| -> Option<i64> {
            dream_result
                .phases
                .iter()
                .find(|p| p.phase == name)
                .map(|p| p.duration_ms as i64)
        };
        DreamHistoryRecord {
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
        }
    };
    let primary_insights = dream_result.insights.clone();
    let extra_insights_owned = extra_insights.clone();
    let hub_candidates_owned = hub_candidates.clone();
    let tags_cleared: i64 = tokio::task::spawn_blocking(move || {
        if let Err(e) = storage_persist.save_dream_history(&dream_history_record) {
            tracing::warn!("Failed to persist dream history: {}", e);
        }
        let now = Utc::now();
        let mut insights_persisted = 0u64;
        let mut insight_nodes_created = 0u64;

        // For each insight: write the legacy InsightRecord row AND mirror
        // the same observation as a first-class KnowledgeNode of
        // node_type="insight" with structured metadata under
        // extra_json.insight (Proposal B — Insight Tier). The dual write
        // is intentional during the transition: existing dashboards keep
        // rendering off InsightRecord while the new Insight Tier
        // surfaces work through the regular memory pipeline (search,
        // promote/demote, FSRS-6 review).
        let mut persist_one = |insight_text: &str,
                               source_memories: Vec<String>,
                               confidence: f64,
                               novelty: f64,
                               insight_type: String,
                               origin: InsightOrigin,
                               tags: Vec<String>| {
            let record_id = uuid::Uuid::new_v4().to_string();
            let record = InsightRecord {
                id: record_id.clone(),
                insight: insight_text.to_string(),
                source_memories: source_memories.clone(),
                confidence,
                novelty_score: novelty,
                insight_type: insight_type.clone(),
                generated_at: now,
                tags: tags.clone(),
                feedback: None,
                applied_count: 0,
            };
            if storage_persist.save_insight(&record).is_ok() {
                insights_persisted += 1;
            }

            // Mirror as a KnowledgeNode of type "insight" so it shows up
            // in search and FSRS scheduling. Skip insights with no
            // source memories — InsightMetadata.validate() would reject
            // them anyway and an evidence-less insight is just noise.
            if source_memories.is_empty() {
                return;
            }
            // InsightMetadata uses f32 because the wire format already
            // does — convert from the storage layer's f64 once here.
            let metadata = InsightMetadata {
                insight_type: insight_type.clone(),
                origin,
                source_memory_ids: source_memories,
                confidence: confidence as f32,
                novelty: novelty as f32,
                validated_by_agent: false,
                validated_at: None,
                insight_record_id: Some(record_id),
            };
            if metadata.validate().is_err() {
                return;
            }
            let extra_json = serde_json::json!({ "insight": metadata });
            let mut node_tags = tags;
            // Make insight nodes filterable from search without scanning
            // node_type. "unvalidated" gets stripped on promote.
            node_tags.push("insight".to_string());
            node_tags.push("unvalidated".to_string());

            let input = IngestInput {
                content: insight_text.to_string(),
                node_type: "insight".to_string(),
                tags: node_tags,
                source: Some("dream".to_string()),
                extra_json: Some(extra_json),
                ..Default::default()
            };
            match storage_persist.ingest(input) {
                Ok(_) => insight_nodes_created += 1,
                Err(e) => tracing::warn!("Failed to ingest insight node: {}", e),
            }
        };

        for insight in &primary_insights {
            persist_one(
                &insight.insight,
                insight.source_memory_ids.clone(),
                insight.confidence,
                insight.novelty,
                insight.insight_type.clone(),
                InsightOrigin::Dream,
                vec!["dream".to_string()],
            );
        }
        for insight in &extra_insights_owned {
            persist_one(
                &insight.insight,
                insight.source_memories.clone(),
                insight.confidence,
                insight.novelty_score,
                format!("{:?}", insight.insight_type),
                InsightOrigin::Synthesized,
                vec!["dream".to_string(), "synthesized".to_string()],
            );
        }
        if insights_persisted > 0 {
            tracing::info!(
                insights_persisted,
                insight_nodes_created,
                "Dream: persisted {} insight records, {} insight nodes",
                insights_persisted,
                insight_nodes_created,
            );
        }

        // Topic Hub persistence (Proposal A). For each candidate:
        //   - look up an existing hub by cluster_signature (partial index
        //     idx_nodes_hub_signature, migration v13)
        //   - if found → bump regeneration count, rewrite content, refresh
        //     child_ids/dominant_tags; no new node created
        //   - if not   → ingest a fresh node_type="hub" node carrying the
        //     HubMetadata under extra_json.hub
        // The whole branch is best-effort — a failure to persist one hub
        // never breaks the rest of the dream cycle.
        let mut hubs_created = 0u64;
        let mut hubs_refreshed = 0u64;
        for candidate in &hub_candidates_owned {
            let mut metadata = HubMetadata {
                child_ids: candidate.child_ids.clone(),
                cluster_signature: candidate.cluster_signature.clone(),
                regeneration_count: 0,
                last_regenerated_at: now,
                generation_method: candidate.generation_method.clone(),
                dominant_tags: candidate.dominant_tags.clone(),
                date_range: candidate.date_range,
            };
            if metadata.validate().is_err() {
                continue;
            }

            match storage_persist.find_hub_by_signature(&candidate.cluster_signature) {
                Ok(Some(existing)) => {
                    // Carry forward the prior regeneration count so the
                    // dashboard can show "regenerated N times".
                    if let Some(prev) =
                        vestige_core::memory::extract_hub(existing.extra_json.as_ref())
                    {
                        metadata.regeneration_count = prev.regeneration_count;
                    }
                    metadata.touch_regeneration(now);

                    let new_extra = merge_hub_into_extra(existing.extra_json.as_ref(), &metadata);
                    if storage_persist
                        .update_node_extra_json(&existing.id, Some(&new_extra))
                        .is_ok()
                    {
                        // Rewrite the human-facing body so the new hub
                        // text replaces the stale one. Content change also
                        // refreshes the embedding (see update_node_content).
                        if let Err(e) =
                            storage_persist.update_node_content(&existing.id, &candidate.content)
                        {
                            tracing::warn!(
                                hub_id = %existing.id,
                                "Failed to refresh hub content: {}",
                                e
                            );
                        }
                        hubs_refreshed += 1;
                    }
                }
                Ok(None) => {
                    let extra_json = serde_json::json!({ "hub": metadata });
                    let input = IngestInput {
                        content: candidate.content.clone(),
                        node_type: "hub".to_string(),
                        tags: candidate.tags.clone(),
                        source: Some("dream".to_string()),
                        extra_json: Some(extra_json),
                        ..Default::default()
                    };
                    match storage_persist.ingest(input) {
                        Ok(_) => hubs_created += 1,
                        Err(e) => tracing::warn!("Failed to ingest hub node: {}", e),
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        signature = %candidate.cluster_signature,
                        "Failed to look up hub by signature: {}",
                        e
                    );
                }
            }
        }
        if hubs_created > 0 || hubs_refreshed > 0 {
            tracing::info!(
                hubs_created,
                hubs_refreshed,
                "Dream: persisted {} new hubs, refreshed {} existing hubs",
                hubs_created,
                hubs_refreshed,
            );
        }

        storage_persist.clear_waking_tags().unwrap_or(0)
    })
    .await
    .map_err(|e| format!("dream persist task panicked: {}", e))?;

    // Merge insights from 4-phase engine + legacy synthesizer.
    //
    // Wire shape is camelCase to match the dashboard `DreamInsight` type. The
    // pre-3.4 payload used snake_case here (`insight_type`, `source_memories`,
    // `novelty_score`) which silently rendered as undefined fields in
    // `DreamResultPanel`. See AGENTS.md note about wire-format stability.
    let all_insights: Vec<serde_json::Value> = dream_result
        .insights
        .iter()
        .map(|i| {
            serde_json::json!({
                "type": i.insight_type,
                "insight": i.insight,
                "sourceMemories": i.source_memory_ids,
                "confidence": i.confidence,
                "noveltyScore": i.novelty,
            })
        })
        .chain(extra_insights.iter().map(|i| {
            serde_json::json!({
                "type": format!("{:?}", i.insight_type),
                "insight": i.insight,
                "sourceMemories": i.source_memories,
                "confidence": i.confidence,
                "noveltyScore": i.novelty_score,
            })
        }))
        .collect();

    // Contradiction pairs from creative connections. The pair is symmetric —
    // neither side is the "winner" until the user (or a remediation flow)
    // resolves it — so the wire shape uses `memoryA`/`memoryB` rather than
    // `survivor`/`demoted`. UI is responsible for surfacing resolution
    // affordances on top of this shape.
    let contradictions: Vec<serde_json::Value> = dream_result
        .creative_connections
        .iter()
        .filter(|c| c.connection_type == CreativeConnectionType::Contradictory)
        .map(|c| {
            serde_json::json!({
                "memoryA": c.memory_a_id,
                "memoryB": c.memory_b_id,
                "confidence": c.confidence,
                "insight": c.insight,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "status": "dreamed",
        "memoriesReplayed": dream_result.memories_replayed,
        "wakingTagsProcessed": tagged_target,
        "wakingTagsCleared": tags_cleared,
        "insights": all_insights,
        "connectionsPersisted": connections_persisted,
        "contradictions": contradictions,
        // memoriesDemoted intentionally removed — the 4-phase engine never
        // tracked individual demotion IDs, only counts. If a future
        // remediation pass starts tracking them, re-add the field with a
        // real backing instead of the previous always-empty placeholder.
        "phases": dream_result.phases.iter().map(|p| serde_json::json!({
            "phase": p.phase.as_str(),
            "durationMs": p.duration_ms,
            "memoriesProcessed": p.memories_processed,
            "actions": p.actions,
        })).collect::<Vec<_>>(),
        // camelCase stats — matches the dashboard `DreamResult.stats` type.
        // The previous snake_case spelling broke `result.stats.duration_ms`
        // rendering in `DreamResultPanel` after the cognitive engine refactor.
        "stats": {
            "creativeConnectionsFound": dream_result.creative_connections.len(),
            "connectionsPersisted": connections_persisted,
            "memoriesStrengthened": dream_result.memories_strengthened,
            "memoriesDownscaled": dream_result.memories_downscaled,
            "emotionalProcessed": dream_result.emotional_processed,
            "contradictionsFound": contradictions.len(),
            "insightsGenerated": all_insights.len(),
            "durationMs": dream_result.total_duration_ms,
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
            storage
                .ingest(vestige_core::IngestInput {
                    content: format!("Dream test memory number {}", i),
                    node_type: "fact".to_string(),
                    source: None,
                    sentiment_score: 0.0,
                    sentiment_magnitude: 0.0,
                    tags: vec!["dream-test".to_string()],
                    valid_from: None,
                    valid_until: None,
                    provenance: None,
                    ..Default::default()
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
        // Wire shape switched to camelCase in v3.4 to match the dashboard
        // `DreamStats` type. Keep this test exhaustive — it's the main
        // contract test catching schema drift between dream engine and UI.
        assert!(value["stats"]["creativeConnectionsFound"].is_number());
        assert!(value["stats"]["memoriesStrengthened"].is_number());
        assert!(value["stats"]["memoriesDownscaled"].is_number());
        assert!(value["stats"]["insightsGenerated"].is_number());
        assert!(value["stats"]["durationMs"].is_number());
        assert!(value["stats"]["emotionalProcessed"].is_number());
        assert!(value["stats"]["contradictionsFound"].is_number());
        assert!(value["stats"]["connectionsPersisted"].is_number());
        assert!(value["phases"].is_array());
        // memoriesDemoted intentionally absent — see comment in execute().
        assert!(value.get("memoriesDemoted").is_none());
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
            assert!(
                last.is_some(),
                "Dream should have been persisted to database"
            );
        }
    }

    #[tokio::test]
    async fn test_dream_connections_round_trip() {
        // Verify dream → persist → query round-trip
        let (storage, _dir) = test_storage().await;

        // Create enough diverse memories to trigger connection discovery
        for i in 0..15 {
            storage
                .ingest(vestige_core::IngestInput {
                    content: format!(
                        "Memory {} about topic {}: detailed content for connection discovery",
                        i,
                        if i % 3 == 0 {
                            "rust"
                        } else if i % 3 == 1 {
                            "cargo"
                        } else {
                            "testing"
                        }
                    ),
                    node_type: "fact".to_string(),
                    source: None,
                    sentiment_score: 0.0,
                    sentiment_magnitude: 0.0,
                    tags: vec!["dream-roundtrip".to_string()],
                    valid_from: None,
                    valid_until: None,
                    provenance: None,
                    ..Default::default()
                })
                .unwrap();
        }

        let cognitive = test_cognitive();
        let result = execute(&storage, &cognitive, None).await.unwrap();
        assert_eq!(result["status"], "dreamed");

        let persisted = result["connectionsPersisted"].as_u64().unwrap_or(0);
        if persisted > 0 {
            // Verify connections are queryable from storage
            let all_conns = storage.get_all_connections().unwrap();
            assert!(
                !all_conns.is_empty(),
                "Persisted connections should be queryable"
            );

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
            let net = cog.activation_network.read().await;
            let assocs = net.get_associations(&first_conn.source_id);
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
            let result = storage
                .ingest(vestige_core::IngestInput {
                    content: format!("Save connection test memory {}", i),
                    node_type: "fact".to_string(),
                    source: None,
                    sentiment_score: 0.0,
                    sentiment_magnitude: 0.0,
                    tags: vec!["save-conn-test".to_string()],
                    valid_from: None,
                    valid_until: None,
                    provenance: None,
                    ..Default::default()
                })
                .unwrap();
            ids.push(result.id);
        }

        // Simulate what dream does: save connections between real memory IDs
        let now = chrono::Utc::now();
        let mut saved = 0u32;
        let mut errors = Vec::new();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
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
                    Err(e) => errors.push(format!("{} -> {}: {}", ids[i], ids[j], e)),
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
            (
                "Error handling with Result type prevents crashes in production code",
                vec!["safety"],
            ),
            (
                "Type safety prevents runtime crashes and data corruption",
                vec!["safety"],
            ),
            (
                "Error handling with try-catch prevents crashes in production code",
                vec!["typescript"],
            ),
            (
                "Type checking prevents runtime errors in compiled code",
                vec!["typescript"],
            ),
            (
                "Error handling patterns prevent production failures in services",
                vec!["architecture"],
            ),
            (
                "Production monitoring prevents cascading failures in code",
                vec!["architecture"],
            ),
            (
                "Database error handling prevents data loss in production",
                vec!["database"],
            ),
            (
                "Query optimization prevents timeout errors in production code",
                vec!["database"],
            ),
        ];

        for (content, tags) in &topics {
            storage
                .ingest(vestige_core::IngestInput {
                    content: content.to_string(),
                    node_type: "fact".to_string(),
                    source: None,
                    sentiment_score: 0.0,
                    sentiment_magnitude: 0.0,
                    tags: tags.iter().map(|t| t.to_string()).collect(),
                    valid_from: None,
                    valid_until: None,
                    provenance: None,
                    ..Default::default()
                })
                .unwrap();
        }

        let cognitive = test_cognitive();
        let result = execute(&storage, &cognitive, None).await.unwrap();
        assert_eq!(result["status"], "dreamed");

        let found = result["stats"]["creativeConnectionsFound"]
            .as_u64()
            .unwrap_or(0);
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

//! Unified Search Tool
//!
//! Merges recall, semantic_search, and hybrid_search into a single `search` tool.
//! Always uses hybrid search internally (keyword + semantic + RRF fusion).
//! Implements Testing Effect (Roediger & Karpicke 2006) by auto-strengthening memories on access.
//!
//! v2.1.0: Enhanced cognitive pipeline with retrieval quality improvements
//!   (Yuan et al. 2026: retrieval method = 20pp accuracy range vs 3-8pp for write strategy)
//!
//!   1. Reranker (over-fetch 3x, rerank down)
//!   2. Result deduplication (remove near-identical results — saves context tokens)
//!   3. Temporal boosting (recency + validity)
//!   4. Freshness-aware ranking (prefer newer when scores close + topics overlap)
//!   5. Memory state accessibility filtering
//!   6. Context matching (topic overlap)
//!   7. Spreading activation associations
//!   8. Score-adaptive pruning (drop results below dynamic threshold)
//!   9. Side effects: predictive memory recording + reconsolidation

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{
    CompetitionCandidate, EncodingContext, MemoryLifecycle, MemorySnapshot, MemoryState, Storage,
    TopicalContext,
};

/// Input schema for unified search tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of results (default: 10)",
                "default": 10,
                "minimum": 1,
                "maximum": 100
            },
            "min_retention": {
                "type": "number",
                "description": "Minimum retention strength (0.0-1.0, default: 0.0)",
                "default": 0.0,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "min_similarity": {
                "type": "number",
                "description": "Minimum similarity threshold (0.0-1.0, default: 0.5)",
                "default": 0.5,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "detail_level": {
                "type": "string",
                "description": "Level of detail in results. 'brief' = id/type/tags/score only (saves tokens). 'summary' = default 8-field response. 'full' = all fields including FSRS state, timestamps, and provenance metadata.",
                "enum": ["brief", "summary", "full"],
                "default": "summary"
            },
            "context_topics": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional topics for context-dependent retrieval boosting"
            },
            "token_budget": {
                "type": "integer",
                "description": "Max tokens for response. Server truncates content to fit budget. Use memory(action='get') for full content of specific IDs.",
                "minimum": 100,
                "maximum": 10000
            }
        },
        "required": ["query"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    query: String,
    limit: Option<i32>,
    #[serde(alias = "min_retention")]
    min_retention: Option<f64>,
    #[serde(alias = "min_similarity")]
    min_similarity: Option<f32>,
    #[serde(alias = "detail_level")]
    detail_level: Option<String>,
    #[serde(alias = "context_topics")]
    context_topics: Option<Vec<String>>,
    #[serde(alias = "token_budget")]
    token_budget: Option<i32>,
}

/// Execute unified search with 7-stage cognitive pipeline.
///
/// Pipeline:
///   1. Hybrid search (keyword + semantic + RRF) with 3x over-fetch
///   2. Reranker (BM25-like rescoring, trim to limit)
///   3. Temporal boosting (recency + validity windows)
///   4. Memory state accessibility filtering (Active/Dormant/Silent/Unavailable)
///   5. Context matching (topic overlap boosting)
///   6. Spreading activation (find associated memories)
///   7. Side effects: predictive memory recording + reconsolidation labile marking
///
/// Also applies Testing Effect (Roediger & Karpicke 2006) by auto-strengthening on access.
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: SearchArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    if args.query.trim().is_empty() {
        return Err("Query cannot be empty".to_string());
    }

    // ====================================================================
    // STAGE 0: Read path gating (Oblivion pattern)
    //
    // Skip the full retrieval pipeline for trivial queries that don't
    // benefit from memory lookup. Saves compute and prevents noise
    // from polluting the agent's context window.
    // ====================================================================
    if is_trivial_query(&args.query) {
        return Ok(serde_json::json!({
            "results": [],
            "totalFound": 0,
            "gated": true,
            "reason": "Query classified as trivial — retrieval skipped"
        }));
    }

    // Validate detail_level
    let detail_level = match args.detail_level.as_deref() {
        Some("brief") => "brief",
        Some("full") => "full",
        Some("summary") | None => "summary",
        Some(invalid) => {
            return Err(format!(
                "Invalid detail_level '{}'. Must be 'brief', 'summary', or 'full'.",
                invalid
            ));
        }
    };

    // Clamp all parameters to valid ranges
    let limit = args.limit.unwrap_or(10).clamp(1, 100);
    let min_retention = args.min_retention.unwrap_or(0.0).clamp(0.0, 1.0);
    let min_similarity = args.min_similarity.unwrap_or(0.5).clamp(0.0, 1.0);

    // Favor semantic search — research shows 0.3/0.7 outperforms equal weights
    let keyword_weight = 0.3_f32;
    let semantic_weight = 0.7_f32;

    // ====================================================================
    // STAGE 1: Hybrid search with 3x over-fetch for reranking pool
    //          + compound query decomposition for multi-part queries
    // ====================================================================
    let overfetch_limit = (limit * 3).min(100);

    // Check for compound queries and decompose if needed
    #[cfg(feature = "vector-search")]
    let decomposition = vestige_core::search::decompose::decompose_query(&args.query);

    #[cfg(feature = "vector-search")]
    let results = if decomposition.is_compound {
        // Run separate searches for each sub-query and merge results
        let mut all_results = Vec::new();
        for sub_query in &decomposition.sub_queries {
            let storage_clone = Arc::clone(storage);
            let sq = sub_query.clone();
            let sub_results = tokio::task::spawn_blocking(move || {
                storage_clone.hybrid_search(&sq, overfetch_limit, keyword_weight, semantic_weight)
            })
            .await
            .map_err(|e| format!("Search task panicked: {}", e))?
            .map_err(|e| e.to_string())?;
            all_results.push(sub_results);
        }

        // Merge: union, dedup by node_id, keep max combined_score
        let mut best: std::collections::HashMap<String, vestige_core::SearchResult> = std::collections::HashMap::new();
        for batch in all_results {
            for r in batch {
                let id = r.node.id.clone();
                let score = r.combined_score;
                match best.get(&id) {
                    Some(existing) if existing.combined_score >= score => {}
                    _ => { best.insert(id, r); }
                }
            }
        }
        let mut merged: Vec<_> = best.into_values().collect();
        merged.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged
    } else {
        // Single query — standard hybrid search
        let storage_clone = Arc::clone(storage);
        let query_clone = args.query.clone();
        tokio::task::spawn_blocking(move || {
            storage_clone.hybrid_search(&query_clone, overfetch_limit, keyword_weight, semantic_weight)
        })
        .await
        .map_err(|e| format!("Search task panicked: {}", e))?
        .map_err(|e| e.to_string())?
    };

    #[cfg(not(feature = "vector-search"))]
    let results = {
        let storage_clone = Arc::clone(storage);
        let query_clone = args.query.clone();
        tokio::task::spawn_blocking(move || {
            storage_clone.hybrid_search(&query_clone, overfetch_limit, keyword_weight, semantic_weight)
        })
        .await
        .map_err(|e| format!("Search task panicked: {}", e))?
        .map_err(|e| e.to_string())?
    };

    // Filter by min_retention, min_similarity, and temporal validity (cheap filters)
    let now = chrono::Utc::now();
    let mut filtered_results: Vec<_> = results
        .into_iter()
        .filter(|r| {
            // Exclude superseded memories (Graphiti temporal invalidation)
            if let Some(valid_until) = r.node.valid_until {
                if valid_until < now {
                    return false;
                }
            }
            if r.node.retention_strength < min_retention {
                return false;
            }
            if let Some(sem_score) = r.semantic_score
                && sem_score < min_similarity
            {
                return false;
            }
            true
        })
        .collect();

    // ====================================================================
    // STAGE 2: Reranker (BM25-like rescoring, trim to requested limit)
    // ====================================================================
    if let Ok(mut cog) = cognitive.try_lock() {
        let candidates: Vec<_> = filtered_results
            .iter()
            .map(|r| (r.clone(), r.node.content.clone()))
            .collect();

        if let Ok(reranked) = cog.reranker.rerank(&args.query, candidates, Some(limit as usize)) {
            // Replace filtered_results with reranked items (preserves original SearchResult)
            filtered_results = reranked.into_iter().map(|rr| rr.item).collect();
        } else {
            // Reranker failed — fall back to original order, just truncate
            filtered_results.truncate(limit as usize);
        }
    } else {
        tracing::debug!("Search stage 2: cognitive lock contention, skipping reranker");
        filtered_results.truncate(limit as usize);
    }

    // ====================================================================
    // STAGE 2B: Result deduplication (SmartSearch insight — Yuan et al. 2026)
    //
    // Near-identical memories waste context tokens without adding information.
    // When two results share >85% word overlap, keep only the higher-ranked one.
    // This runs AFTER reranking so we keep the best-scored version.
    // ====================================================================
    let pre_dedup_count = filtered_results.len();
    if filtered_results.len() > 1 {
        let mut keep = vec![true; filtered_results.len()];
        for i in 0..filtered_results.len() {
            if !keep[i] { continue; }
            for j in (i + 1)..filtered_results.len() {
                if !keep[j] { continue; }
                if content_overlap(&filtered_results[i].node.content, &filtered_results[j].node.content) > 0.85 {
                    keep[j] = false;
                }
            }
        }
        let mut deduped = Vec::with_capacity(filtered_results.len());
        for (idx, result) in filtered_results.into_iter().enumerate() {
            if keep[idx] {
                deduped.push(result);
            }
        }
        filtered_results = deduped;
    }
    let dedup_removed = pre_dedup_count - filtered_results.len();

    // ====================================================================
    // STAGE 3: Temporal boosting (recency + validity windows)
    // ====================================================================
    if let Ok(cog) = cognitive.try_lock() {
        for result in &mut filtered_results {
            let recency = cog.temporal_searcher.recency_boost(result.node.created_at);
            let validity = cog.temporal_searcher.validity_boost(
                result.node.valid_from,
                result.node.valid_until,
                None,
            );
            // Blend: 85% relevance + 15% temporal signal
            let temporal_factor = recency * validity;
            result.combined_score =
                result.combined_score * 0.85 + (result.combined_score * temporal_factor as f32) * 0.15;
        }
    }

    // ====================================================================
    // STAGE 3B: Freshness-aware ranking for overlapping topics
    //
    // When two memories cover the same topic and have similar scores,
    // prefer the newer one. Facts change — the latest version is usually
    // more accurate. This only fires when tag overlap is high AND scores
    // are within 15% of each other.
    // ====================================================================
    if filtered_results.len() > 1 {
        for i in 0..filtered_results.len() {
            for j in (i + 1)..filtered_results.len() {
                let score_i = filtered_results[i].combined_score;
                let score_j = filtered_results[j].combined_score;
                let max_score = score_i.max(score_j);
                if max_score <= 0.0 { continue; }

                let score_gap = (score_i - score_j).abs() / max_score;
                if score_gap > 0.15 { continue; }

                let tag_overlap = tag_jaccard(&filtered_results[i].node.tags, &filtered_results[j].node.tags);
                if tag_overlap < 0.5 { continue; }

                let newer_idx = if filtered_results[i].node.created_at > filtered_results[j].node.created_at { i } else { j };
                filtered_results[newer_idx].combined_score *= 1.0 + (tag_overlap as f32 * 0.10);
            }
        }
    }

    // ====================================================================
    // STAGE 4: Memory state accessibility filtering
    // ====================================================================
    if let Ok(cog) = cognitive.try_lock() {
        for result in &mut filtered_results {
            // Build a MemoryLifecycle from node data for the calculator
            let mut lifecycle = MemoryLifecycle::new();
            lifecycle.last_access = result.node.last_accessed;
            lifecycle.access_count = result.node.reps as u32;
            // Determine state from retention strength
            lifecycle.state = if result.node.retention_strength > 0.7 {
                MemoryState::Active
            } else if result.node.retention_strength > 0.3 {
                MemoryState::Dormant
            } else if result.node.retention_strength > 0.1 {
                MemoryState::Silent
            } else {
                MemoryState::Unavailable
            };

            let adjusted = cog
                .accessibility_calc
                .calculate(&lifecycle, result.combined_score as f64);
            result.combined_score = adjusted as f32;
        }
    }

    // ====================================================================
    // STAGE 5: Context matching (Tulving 1973 encoding specificity)
    // ====================================================================
    if let Some(ref topics) = args.context_topics
        && !topics.is_empty()
    {
        let retrieval_ctx = EncodingContext::new()
            .with_topical(TopicalContext::with_topics(topics.clone()));
        if let Ok(cog) = cognitive.try_lock() {
            for result in &mut filtered_results {
                // Build encoding context from memory's tags
                let encoding_ctx = EncodingContext::new()
                    .with_topical(TopicalContext::with_topics(result.node.tags.clone()));
                let context_score = cog.context_matcher.match_contexts(&encoding_ctx, &retrieval_ctx);
                // Blend: context match boosts relevance up to +30%
                result.combined_score *= 1.0 + (context_score as f32 * 0.3);
            }
        }
    }

    // Context reinstatement for top result (helps Claude understand WHY this memory matched)
    let reinstatement_info: Option<Value> = if let Ok(cog) = cognitive.try_lock() {
        if let Some(first) = filtered_results.first() {
            let current_ctx = if let Some(ref topics) = args.context_topics {
                EncodingContext::new().with_topical(TopicalContext::with_topics(topics.clone()))
            } else {
                EncodingContext::new()
            };
            let reinstatement = cog.context_matcher.reinstate_context(&first.node.id, &current_ctx);
            Some(serde_json::json!({
                "memoryId": reinstatement.memory_id,
                "temporalHint": reinstatement.temporal_hint,
                "topicalHint": reinstatement.topical_hint,
                "sessionHint": reinstatement.session_hint,
                "relatedMemories": reinstatement.related_memories,
            }))
        } else {
            None
        }
    } else {
        None
    };

    // ====================================================================
    // STAGE 5B: Retrieval competition (Anderson et al. 1994)
    // ====================================================================
    let mut suppressed_count = 0_usize;
    if filtered_results.len() > 1
        && let Ok(mut cog) = cognitive.try_lock()
    {
        let candidates: Vec<CompetitionCandidate> = filtered_results
            .iter()
            .map(|r| {
                #[cfg(all(feature = "embeddings", feature = "vector-search"))]
                let embedding = storage.get_node_embedding(&r.node.id).ok().flatten();
                #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
                let embedding = None;

                CompetitionCandidate {
                    memory_id: r.node.id.clone(),
                    relevance_score: r.combined_score as f64,
                    similarity_to_query: r.semantic_score.unwrap_or(0.0) as f64,
                    embedding,
                }
            })
            .collect();
        if let Some(result) = cog.competition_mgr.run_competition(&candidates, 0.7) {
            // Apply suppression: losers get penalized
            for suppressed_id in &result.suppressed_ids {
                if let Some(r) = filtered_results.iter_mut().find(|r| &r.node.id == suppressed_id) {
                    r.combined_score *= 0.85; // 15% suppression penalty
                    suppressed_count += 1;
                }
            }
        }
    }

    // ====================================================================
    // STAGE 5C: Utility-based ranking (MemRL-inspired)
    // Memories that proved useful in past sessions get a retrieval boost.
    // utility_score = times_useful / times_retrieved (0.0 to 1.0)
    // ====================================================================
    for result in &mut filtered_results {
        let utility = result.node.utility_score.unwrap_or(0.0) as f32;
        if utility > 0.0 {
            // Utility boost: up to +15% for memories with utility_score = 1.0
            result.combined_score *= 1.0 + (utility * 0.15);
        }
    }

    // ====================================================================
    // STAGE 5D: Emotional valence boost (McGaugh 2004, Bower 1981)
    //
    // Two effects from the neuroscience literature:
    // 1. Amygdala-mediated enhancement: high-arousal memories are more
    //    retrievable regardless of current mood (McGaugh 2004).
    // 2. Mood-congruent retrieval: memories whose emotional valence
    //    matches the agent's current mood get a retrieval boost (Bower 1981).
    //
    // The query itself updates the mood state so that searching for
    // "production crash" shifts mood toward negative/high-arousal,
    // naturally boosting retrieval of related incident memories.
    // ====================================================================
    if let Ok(mut cog) = cognitive.try_lock() {
        cog.emotional_memory.evaluate_content(&args.query);

        for result in &mut filtered_results {
            let arousal = result.node.sentiment_magnitude.abs() as f32;
            if arousal > 0.3 {
                result.combined_score *= 1.0 + (arousal * 0.10).min(0.10);
            }

            let valence = result.node.sentiment_score;
            let mood_boost = cog.emotional_memory.mood_congruence_boost(valence);
            if mood_boost > 0.0 {
                result.combined_score *= 1.0 + mood_boost as f32;
            }
        }
    }

    // ====================================================================
    // STAGE 5E-pre: Memory tier adjustment (Tulving 1972)
    //
    // Episodic memories get a recency boost (they're about "what happened").
    // Procedural memories get a stability boost (skills don't decay).
    // Semantic memories use default scoring.
    // ====================================================================
    for result in &mut filtered_results {
        match result.node.memory_system() {
            vestige_core::MemorySystem::Procedural => {
                result.combined_score *= 1.05;
            }
            vestige_core::MemorySystem::Episodic => {
                let age_days = (chrono::Utc::now() - result.node.created_at).num_hours() as f32 / 24.0;
                if age_days < 7.0 {
                    result.combined_score *= 1.0 + (1.0 - age_days / 7.0) * 0.10;
                }
            }
            vestige_core::MemorySystem::Semantic => {}
        }
    }

    // ====================================================================
    // STAGE 5E: Bayesian confidence boost (Gelman et al. 2013)
    //
    // Memories with a track record of being useful get a confidence-
    // weighted score boost. The posterior lower bound of the 95% CI
    // guards against overweighting memories with very few retrievals.
    // ====================================================================
    for result in &mut filtered_results {
        let conf = result.node.confidence();
        if conf.is_informed() {
            let boost = (conf.lower_95 as f32 - 0.5).max(0.0) * 0.15;
            result.combined_score *= 1.0 + boost;
        }
    }

    // ====================================================================
    // STAGE 5F: Proactive interference resolution (SleepGate pattern)
    //
    // When two memories in the result set have a "contradiction" connection,
    // downrank the older one. This prevents stale conflicting memories from
    // polluting the agent's context window.
    // ====================================================================
    {
        let result_ids: Vec<String> = filtered_results.iter().map(|r| r.node.id.clone()).collect();
        let mut penalties: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

        for id in &result_ids {
            if let Ok(connections) = storage.get_connections_for_memory(id) {
                for conn in &connections {
                    if conn.link_type == "contradiction" {
                        let other = if conn.source_id == *id { &conn.target_id } else { &conn.source_id };
                        if result_ids.contains(other) {
                            // Find which is older and penalize it
                            let id_time = filtered_results.iter().find(|r| r.node.id == *id).map(|r| r.node.created_at);
                            let other_time = filtered_results.iter().find(|r| r.node.id == *other).map(|r| r.node.created_at);
                            if let (Some(t1), Some(t2)) = (id_time, other_time) {
                                let older = if t1 < t2 { id } else { other };
                                penalties.entry(older.clone()).or_insert(0.20);
                            }
                        }
                    }
                }
            }
        }

        for result in &mut filtered_results {
            if let Some(penalty) = penalties.get(&result.node.id) {
                result.combined_score *= 1.0 - penalty;
            }
        }
    }

    // Re-sort by adjusted combined_score (descending) after all score modifications
    filtered_results.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // ====================================================================
    // STAGE 5G: Score-adaptive pruning (SmartSearch truncation insight)
    //
    // Instead of always returning a fixed top-K, drop results that fall
    // below a dynamic threshold relative to the top score. This prevents
    // low-confidence noise from polluting the agent's context window.
    //
    // Threshold: result must score at least 30% of the top result's score.
    // Only prunes when we have at least 3 results (don't over-prune small sets).
    // ====================================================================
    let pre_prune_count = filtered_results.len();
    if filtered_results.len() >= 3 {
        if let Some(top_score) = filtered_results.first().map(|r| r.combined_score) {
            if top_score > 0.0 {
                let threshold = top_score * 0.30;
                filtered_results.retain(|r| r.combined_score >= threshold);
            }
        }
    }
    let prune_removed = pre_prune_count - filtered_results.len();

    // ====================================================================
    // STAGE 6: Spreading activation — triple hybrid scoring
    //
    // Run spreading activation from the top result and use the activation
    // values to boost results that are reachable in the graph.
    // This creates a triple signal: keyword + semantic + graph.
    // ====================================================================
    let associations: Vec<Value> = if let Ok(mut cog) = cognitive.try_lock() {
        if let Some(first) = filtered_results.first() {
            let activated = cog.activation_network.activate(&first.node.id, 1.0);
            let activation_map: std::collections::HashMap<&str, f64> = activated
                .iter()
                .map(|a| (a.memory_id.as_str(), a.activation))
                .collect();

            // Boost remaining results by their activation proximity to the top result
            for result in filtered_results.iter_mut().skip(1) {
                if let Some(&act) = activation_map.get(result.node.id.as_str()) {
                    // Activation boost: up to +20% for strongly connected memories
                    result.combined_score *= 1.0 + (act as f32 * 0.20).min(0.20);
                }
            }

            activated
                .iter()
                .take(3)
                .map(|a| {
                    serde_json::json!({
                        "memoryId": a.memory_id,
                        "activation": a.activation,
                        "distance": a.distance,
                    })
                })
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // ====================================================================
    // Auto-strengthen on access (Testing Effect)
    // ====================================================================
    let storage_clone = Arc::clone(storage);
    let ids_owned: Vec<String> = filtered_results.iter().map(|r| r.node.id.clone()).collect();
    let _ = tokio::task::spawn_blocking(move || {
        let ids: Vec<&str> = ids_owned.iter().map(|s| s.as_str()).collect();
        storage_clone.strengthen_batch_on_access(&ids)
    })
    .await;

    // Drop storage lock before acquiring cognitive for side effects

    // ====================================================================
    // STAGE 7: Side effects — predictive memory + reconsolidation
    // ====================================================================
    if let Ok(mut cog) = cognitive.try_lock() {
        // 7A. Record query for predictive memory
        let _ = cog.predictive_memory.record_query(&args.query, &[]);

        // 7B. Record each accessed memory for predictive/speculative models + persistent state
        for result in &filtered_results {
            let _ = cog.predictive_memory.record_memory_access(
                &result.node.id,
                &result.node.content.chars().take(100).collect::<String>(),
                &result.node.tags,
            );

            if let Err(e) = storage.record_memory_access(&result.node.id) {
                tracing::debug!(error = %e, memory_id = %result.node.id, "Failed to record memory access");
            }

            cog.speculative_retriever.record_access(
                &result.node.id,
                None,                           // file_context
                Some(args.query.as_str()),       // query_context
                None,                            // was_helpful (unknown yet)
            );

            // 7C. Mark labile for reconsolidation window (5 min)
            let snapshot = MemorySnapshot {
                content: result.node.content.clone(),
                tags: result.node.tags.clone(),
                retention_strength: result.node.retention_strength,
                storage_strength: result.node.storage_strength,
                retrieval_strength: result.node.retrieval_strength,
                connection_ids: vec![],
                captured_at: Utc::now(),
            };
            cog.reconsolidation.mark_labile(&result.node.id, snapshot);
        }
    }

    // ====================================================================
    // Format and return
    // ====================================================================
    let mut formatted: Vec<Value> = filtered_results
        .iter()
        .map(|r| format_search_result(r, detail_level))
        .collect();

    // ====================================================================
    // Token budget enforcement with context compression (LightMem pattern)
    //
    // Three-tier approach:
    // 1. First pass: try to fit all results as-is
    // 2. If over budget: compress content to first N sentences per result
    // 3. If still over: drop lowest-scoring results into expandable list
    // ====================================================================
    let mut budget_expandable: Vec<String> = Vec::new();
    let mut budget_tokens_used: Option<usize> = None;
    if let Some(budget) = args.token_budget {
        let budget = budget.clamp(100, 10000) as usize;
        let budget_chars = budget * 4;

        let total_size: usize = formatted.iter()
            .map(|r| serde_json::to_string(r).unwrap_or_default().len())
            .sum();

        // Tier 2: compress content if over budget
        if total_size > budget_chars && detail_level != "brief" {
            let ratio = budget_chars as f64 / total_size as f64;
            for result in &mut formatted {
                if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
                    let compressed = compress_content(content, ratio);
                    result["content"] = serde_json::Value::String(compressed);
                }
            }
        }

        // Tier 3: drop results that still don't fit
        let mut used = 0;
        let mut budgeted = Vec::new();
        for result in &formatted {
            let size = serde_json::to_string(result).unwrap_or_default().len();
            if used + size > budget_chars {
                if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
                    budget_expandable.push(id.to_string());
                }
                continue;
            }
            used += size;
            budgeted.push(result.clone());
        }

        budget_tokens_used = Some(used / 4);
        formatted = budgeted;
    }

    // Check learning mode via attention signal
    let learning_mode = cognitive.try_lock().ok().map(|cog| cog.attention_signal.is_learning_mode()).unwrap_or(false);

    let mut response = serde_json::json!({
        "query": args.query,
        "method": "hybrid+cognitive",
        "detailLevel": detail_level,
        "total": formatted.len(),
        "results": formatted,
    });

    // Include associations if any were found
    if !associations.is_empty() {
        response["associations"] = serde_json::json!(associations);
    }
    // Include context reinstatement if computed
    if let Some(ri) = reinstatement_info {
        response["contextReinstatement"] = ri;
    }
    // Include competition stats
    if suppressed_count > 0 {
        response["competitionSuppressed"] = serde_json::json!(suppressed_count);
    }
    // Include retrieval quality stats
    if dedup_removed > 0 {
        response["deduplicated"] = serde_json::json!(dedup_removed);
    }
    if prune_removed > 0 {
        response["pruned"] = serde_json::json!(prune_removed);
    }
    // Include learning mode detection
    if learning_mode {
        response["learningModeDetected"] = serde_json::json!(true);
    }
    // Include token budget info (v1.8.0)
    if !budget_expandable.is_empty() {
        response["expandable"] = serde_json::json!(budget_expandable);
    }
    if let Some(budget) = args.token_budget {
        response["tokenBudget"] = serde_json::json!(budget);
    }
    if let Some(used) = budget_tokens_used {
        response["tokensUsed"] = serde_json::json!(used);
    }

    // ====================================================================
    // Metacognition: record search outcome and surface quality metrics
    // ====================================================================
    if let Ok(mut cog) = cognitive.try_lock() {
        let avg_conf = if filtered_results.is_empty() {
            0.0
        } else {
            filtered_results.iter()
                .map(|r| r.node.confidence().point)
                .sum::<f64>() / filtered_results.len() as f64
        };
        let topic = args.query.split_whitespace().next().unwrap_or("unknown");
        cog.metacognition.record_search(filtered_results.len(), avg_conf, topic);

        let report = cog.metacognition.report();
        if report.total_queries_tracked >= 5 {
            response["metacognition"] = serde_json::json!({
                "hitRate": format!("{:.0}%", report.hit_rate * 100.0),
                "avgConfidence": format!("{:.2}", report.avg_confidence),
                "queriesTracked": report.total_queries_tracked,
            });
            if !report.knowledge_gaps.is_empty() {
                response["knowledgeGaps"] = serde_json::json!(
                    report.knowledge_gaps.iter().map(|g| &g.topic).collect::<Vec<_>>()
                );
            }
        }
    }

    Ok(response)
}

/// Format a search result based on the requested detail level.
fn format_search_result(r: &vestige_core::SearchResult, detail_level: &str) -> Value {
    match detail_level {
        "brief" => serde_json::json!({
            "id": r.node.id,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "combinedScore": r.combined_score,
        }),
        "full" => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "storageStrength": r.node.storage_strength,
            "retrievalStrength": r.node.retrieval_strength,
            "source": r.node.source,
            "sentimentScore": r.node.sentiment_score,
            "sentimentMagnitude": r.node.sentiment_magnitude,
            "createdAt": r.node.created_at.to_rfc3339(),
            "updatedAt": r.node.updated_at.to_rfc3339(),
            "lastAccessed": r.node.last_accessed.to_rfc3339(),
            "nextReview": r.node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": r.node.stability,
            "difficulty": r.node.difficulty,
            "reps": r.node.reps,
            "lapses": r.node.lapses,
            "validFrom": r.node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": r.node.valid_until.map(|dt| dt.to_rfc3339()),
            "matchType": format!("{:?}", r.match_type),
            "epistemicStatus": r.node.epistemic_status().to_string(),
            "memorySystem": r.node.memory_system().to_string(),
            "provenance": r.node.provenance,
        }),
        // "summary" (default)
        _ => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "epistemicStatus": r.node.epistemic_status().to_string(),
            "memorySystem": r.node.memory_system().to_string(),
        }),
    }
}

/// Format a KnowledgeNode based on the requested detail level.
/// Reusable across search, timeline, and other tools.
pub fn format_node(node: &vestige_core::KnowledgeNode, detail_level: &str) -> Value {
    match detail_level {
        "brief" => serde_json::json!({
            "id": node.id,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
        }),
        "full" => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
            "storageStrength": node.storage_strength,
            "retrievalStrength": node.retrieval_strength,
            "source": node.source,
            "sentimentScore": node.sentiment_score,
            "sentimentMagnitude": node.sentiment_magnitude,
            "createdAt": node.created_at.to_rfc3339(),
            "updatedAt": node.updated_at.to_rfc3339(),
            "lastAccessed": node.last_accessed.to_rfc3339(),
            "nextReview": node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": node.stability,
            "difficulty": node.difficulty,
            "reps": node.reps,
            "lapses": node.lapses,
            "validFrom": node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": node.valid_until.map(|dt| dt.to_rfc3339()),
            "provenance": node.provenance,
        }),
        // "summary" (default)
        _ => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
        }),
    }
}

// ============================================================================
// READ PATH GATING (Oblivion pattern)
// ============================================================================

/// Greetings, acknowledgments, and social phrases that never need memory retrieval.
const TRIVIAL_PHRASES: &[&str] = &[
    "hi", "hello", "hey", "thanks", "thank you", "ok", "okay", "sure",
    "yes", "no", "bye", "goodbye", "got it", "right", "cool", "nice",
    "please", "welcome", "cheers", "np", "ty", "thx", "ack", "roger",
    "dzieki", "dzięki", "hej", "cześć", "tak", "nie", "dobra", "spoko",
    "siema", "nara", "ok ok", "oki", "jasne", "luzik",
];

/// Compress content to fit within a token budget ratio.
/// Keeps the most information-dense sentences, prioritizing the first and last
/// sentences (primacy/recency effect), plus any sentences containing key markers
/// like "BUG", "DECISION", "because", "root cause".
fn compress_content(content: &str, ratio: f64) -> String {
    let sentences: Vec<&str> = content
        .split(|c: char| c == '.' || c == '\n')
        .map(|s| s.trim())
        .filter(|s| s.len() > 5)
        .collect();

    if sentences.len() <= 2 || ratio >= 0.9 {
        return content.to_string();
    }

    let target_count = ((sentences.len() as f64 * ratio).ceil() as usize).max(2);
    let priority_markers = ["BUG", "DECISION", "because", "root cause", "solution", "fix",
                            "important", "note", "warning", "error", "pattern"];

    let mut scored: Vec<(usize, f64)> = sentences.iter().enumerate().map(|(i, s)| {
        let mut score = 0.0_f64;
        if i == 0 { score += 2.0; }
        if i == sentences.len() - 1 { score += 1.5; }
        let lower = s.to_lowercase();
        for marker in &priority_markers {
            if lower.contains(marker) { score += 1.0; }
        }
        score += s.len() as f64 / 200.0;
        (i, score)
    }).collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut selected: Vec<usize> = scored.iter().take(target_count).map(|s| s.0).collect();
    selected.sort();

    let compressed: Vec<&str> = selected.iter().map(|&i| sentences[i]).collect();
    let result = compressed.join(". ");
    if sentences.len() > target_count {
        format!("{}. [{} of {} segments]", result, target_count, sentences.len())
    } else {
        result
    }
}

/// Word-level Jaccard overlap between two content strings.
/// Returns 0.0 (no overlap) to 1.0 (identical word sets).
fn content_overlap(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();

    let union_size = words_a.union(&words_b).count();
    if union_size == 0 {
        return 0.0;
    }
    let intersection_size = words_a.intersection(&words_b).count();
    intersection_size as f64 / union_size as f64
}

/// Tag-level Jaccard similarity between two tag vectors.
fn tag_jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let set_a: std::collections::HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let union_size = set_a.union(&set_b).count();
    if union_size == 0 {
        return 0.0;
    }
    let intersection_size = set_a.intersection(&set_b).count();
    intersection_size as f64 / union_size as f64
}

fn is_trivial_query(query: &str) -> bool {
    let trimmed = query.trim();
    let lower = trimmed.to_lowercase();
    let word_count = trimmed.split_whitespace().count();

    if word_count == 0 {
        return true;
    }
    if word_count <= 3 {
        return TRIVIAL_PHRASES.iter().any(|p| lower == *p);
    }
    false
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;
    use vestige_core::IngestInput;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    /// Create a test storage instance with a temporary database
    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    /// Helper to ingest test content
    async fn ingest_test_content(storage: &Arc<Storage>, content: &str) -> String {
        let input = IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec![],
            valid_from: None,
            valid_until: None,
            provenance: None,
        };
        let node = storage.ingest(input).unwrap();
        node.id
    }

    // ========================================================================
    // QUERY VALIDATION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_empty_query_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "query": "" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_search_whitespace_only_query_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "query": "   \t\n  " });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_search_missing_arguments_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_search_missing_query_field_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "limit": 10 });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid arguments"));
    }

    // ========================================================================
    // LIMIT CLAMPING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_limit_clamped_to_minimum() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for limit clamping").await;

        // Try with limit 0 - should clamp to 1
        let args = serde_json::json!({
            "query": "test",
            "limit": 0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_limit_clamped_to_maximum() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for max limit").await;

        // Try with limit 1000 - should clamp to 100
        let args = serde_json::json!({
            "query": "test",
            "limit": 1000
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_negative_limit_clamped() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for negative limit").await;

        let args = serde_json::json!({
            "query": "test",
            "limit": -5
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // MIN_RETENTION CLAMPING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_min_retention_clamped_to_zero() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for retention clamping").await;

        let args = serde_json::json!({
            "query": "test",
            "min_retention": -0.5
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_min_retention_clamped_to_one() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for max retention").await;

        let args = serde_json::json!({
            "query": "test",
            "min_retention": 1.5
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        // Should succeed but may return no results (retention > 1.0 clamped to 1.0)
        assert!(result.is_ok());
    }

    // ========================================================================
    // MIN_SIMILARITY CLAMPING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_min_similarity_clamped_to_zero() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for similarity clamping").await;

        let args = serde_json::json!({
            "query": "test",
            "min_similarity": -0.5
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_min_similarity_clamped_to_one() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for max similarity").await;

        let args = serde_json::json!({
            "query": "test",
            "min_similarity": 1.5
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        // Should succeed but may return no results
        assert!(result.is_ok());
    }

    // ========================================================================
    // SUCCESSFUL SEARCH TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_basic_query_succeeds() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "The Rust programming language is memory safe.").await;

        let args = serde_json::json!({ "query": "rust" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["query"], "rust");
        assert_eq!(value["method"], "hybrid+cognitive");
        assert!(value["total"].is_number());
        assert!(value["results"].is_array());
    }

    #[tokio::test]
    async fn test_search_returns_matching_content() {
        let (storage, _dir) = test_storage().await;
        let node_id =
            ingest_test_content(&storage, "Python is a dynamic programming language.").await;

        let args = serde_json::json!({
            "query": "python",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0]["id"], node_id);
    }

    #[tokio::test]
    async fn test_search_with_limit() {
        let (storage, _dir) = test_storage().await;
        // Ingest multiple items
        ingest_test_content(&storage, "Testing content one").await;
        ingest_test_content(&storage, "Testing content two").await;
        ingest_test_content(&storage, "Testing content three").await;

        let args = serde_json::json!({
            "query": "testing",
            "limit": 2,
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert!(results.len() <= 2);
    }

    #[tokio::test]
    async fn test_search_empty_database_returns_empty_array() {
        let (storage, _dir) = test_storage().await;
        // Don't ingest anything - database is empty

        let args = serde_json::json!({ "query": "anything" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["total"], 0);
        assert!(value["results"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_search_result_contains_expected_fields() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Testing field presence in search results.").await;

        let args = serde_json::json!({
            "query": "testing",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            assert!(first["id"].is_string());
            assert!(first["content"].is_string());
            assert!(first["combinedScore"].is_number());
            // keywordScore and semanticScore may be null if not matched
            assert!(first["nodeType"].is_string());
            assert!(first["tags"].is_array());
            assert!(first["retentionStrength"].is_number());
        }
    }

    // ========================================================================
    // DEFAULT VALUES TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_default_limit_is_10() {
        let (storage, _dir) = test_storage().await;
        // Ingest more than 10 items
        for i in 0..15 {
            ingest_test_content(&storage, &format!("Item number {}", i)).await;
        }

        let args = serde_json::json!({
            "query": "item",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert!(results.len() <= 10);
    }

    // ========================================================================
    // SCHEMA TESTS
    // ========================================================================

    #[test]
    fn test_schema_has_required_fields() {
        let schema_value = schema();
        assert_eq!(schema_value["type"], "object");
        assert!(schema_value["properties"]["query"].is_object());
        assert!(schema_value["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("query")));
    }

    #[test]
    fn test_schema_has_optional_fields() {
        let schema_value = schema();
        assert!(schema_value["properties"]["limit"].is_object());
        assert!(schema_value["properties"]["min_retention"].is_object());
        assert!(schema_value["properties"]["min_similarity"].is_object());
    }

    #[test]
    fn test_schema_limit_has_bounds() {
        let schema_value = schema();
        let limit_schema = &schema_value["properties"]["limit"];
        assert_eq!(limit_schema["minimum"], 1);
        assert_eq!(limit_schema["maximum"], 100);
        assert_eq!(limit_schema["default"], 10);
    }

    #[test]
    fn test_schema_min_retention_has_bounds() {
        let schema_value = schema();
        let retention_schema = &schema_value["properties"]["min_retention"];
        assert_eq!(retention_schema["minimum"], 0.0);
        assert_eq!(retention_schema["maximum"], 1.0);
        assert_eq!(retention_schema["default"], 0.0);
    }

    #[test]
    fn test_schema_min_similarity_has_bounds() {
        let schema_value = schema();
        let similarity_schema = &schema_value["properties"]["min_similarity"];
        assert_eq!(similarity_schema["minimum"], 0.0);
        assert_eq!(similarity_schema["maximum"], 1.0);
        assert_eq!(similarity_schema["default"], 0.5);
    }

    // ========================================================================
    // DETAIL LEVEL TESTS
    // ========================================================================

    #[test]
    fn test_schema_has_detail_level() {
        let schema_value = schema();
        let dl = &schema_value["properties"]["detail_level"];
        assert!(dl.is_object());
        assert_eq!(dl["default"], "summary");
        let enum_values = dl["enum"].as_array().unwrap();
        assert!(enum_values.contains(&serde_json::json!("brief")));
        assert!(enum_values.contains(&serde_json::json!("summary")));
        assert!(enum_values.contains(&serde_json::json!("full")));
    }

    #[tokio::test]
    async fn test_search_detail_level_brief_excludes_content() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Brief mode test content for search.").await;

        let args = serde_json::json!({
            "query": "brief",
            "detail_level": "brief",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "brief");
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            // Brief should NOT have content
            assert!(first.get("content").is_none() || first["content"].is_null());
            // Brief should have these fields
            assert!(first["id"].is_string());
            assert!(first["nodeType"].is_string());
            assert!(first["tags"].is_array());
            assert!(first["retentionStrength"].is_number());
            assert!(first["combinedScore"].is_number());
        }
    }

    #[tokio::test]
    async fn test_search_detail_level_full_includes_timestamps() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Full mode test content for search.").await;

        let args = serde_json::json!({
            "query": "full",
            "detail_level": "full",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "full");
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            // Full should have timestamps
            assert!(first["createdAt"].is_string());
            assert!(first["updatedAt"].is_string());
            assert!(first["content"].is_string());
            assert!(first["storageStrength"].is_number());
            assert!(first["retrievalStrength"].is_number());
            assert!(first["matchType"].is_string());
        }
    }

    #[tokio::test]
    async fn test_search_detail_level_default_is_summary() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Default detail level test content.").await;

        let args = serde_json::json!({
            "query": "default",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "summary");
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            // Summary should have content but not timestamps
            assert!(first["content"].is_string());
            assert!(first["id"].is_string());
            assert!(first.get("createdAt").is_none() || first["createdAt"].is_null());
        }
    }

    #[tokio::test]
    async fn test_search_detail_level_invalid_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "query": "test",
            "detail_level": "invalid_level"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid detail_level"));
    }

    // ========================================================================
    // TOKEN BUDGET TESTS (v1.8.0)
    // ========================================================================

    #[tokio::test]
    async fn test_token_budget_limits_results() {
        let (storage, _dir) = test_storage().await;
        for i in 0..10 {
            ingest_test_content(
                &storage,
                &format!("Budget test content number {} with some extra text to increase size.", i),
            )
            .await;
        }

        // Small budget should reduce results
        let args = serde_json::json!({
            "query": "budget test",
            "token_budget": 200,
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value["tokenBudget"].as_i64().unwrap() == 200);
        assert!(value["tokensUsed"].is_number());
    }

    #[tokio::test]
    async fn test_token_budget_expandable() {
        let (storage, _dir) = test_storage().await;
        for i in 0..15 {
            ingest_test_content(
                &storage,
                &format!(
                    "Expandable budget test number {} with quite a bit of content to ensure we exceed the token budget allocation threshold.",
                    i
                ),
            )
            .await;
        }

        let args = serde_json::json!({
            "query": "expandable budget test",
            "token_budget": 150,
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        // expandable field should exist if results were dropped
        if let Some(expandable) = value.get("expandable") {
            assert!(expandable.is_array());
        }
    }

    #[tokio::test]
    async fn test_no_budget_unchanged() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "No budget test content.").await;

        let args = serde_json::json!({
            "query": "no budget",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        // No budget fields should be present
        assert!(value.get("tokenBudget").is_none());
        assert!(value.get("tokensUsed").is_none());
        assert!(value.get("expandable").is_none());
    }

    #[test]
    fn test_schema_has_token_budget() {
        let schema_value = schema();
        let tb = &schema_value["properties"]["token_budget"];
        assert!(tb.is_object());
        assert_eq!(tb["minimum"], 100);
        assert_eq!(tb["maximum"], 10000);
    }
}

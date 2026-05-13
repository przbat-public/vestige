//! Async pipeline orchestrating the eight cognitive stages.

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{
    CompetitionCandidate, EncodingContext, MemoryLifecycle, MemorySnapshot, MemoryState, Storage,
    TopicalContext,
};

use crate::cognitive::CognitiveEngine;

use super::args::SearchArgs;
use super::format::format_search_result;
use super::helpers::{compress_content, content_overlap, is_trivial_query, tag_jaccard};

/// Execute unified search with 8-stage cognitive pipeline.
///
/// Pipeline (matches `ARCHITECTURE.md` ordering):
///   0. Decompose compound queries (semicolons, conjunctions, question chains) and merge results
///   1. Hybrid search (keyword + semantic + RRF) with 3x over-fetch
///   2. Reranker (Jina Reranker v2 cross-encoder, trim to limit)
///   3. Temporal boosting (recency + validity windows)
///   4. Memory state accessibility filtering (Active/Dormant/Silent/Unavailable)
///   5. Context matching (Tulving 1973 encoding specificity, topic overlap)
///   6. Retrieval competition (Anderson 1994 retrieval-induced forgetting)
///   7. Spreading activation + predictive memory recording + reconsolidation labile marking
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

    // Validate retrieval_mode
    let retrieval_mode = match args.retrieval_mode.as_deref() {
        Some("precise") => "precise",
        Some("exhaustive") => "exhaustive",
        Some("balanced") | None => "balanced",
        Some(invalid) => {
            return Err(format!(
                "Invalid retrieval_mode '{}'. Must be 'precise', 'balanced', or 'exhaustive'.",
                invalid
            ));
        }
    };

    // Favor semantic search — research shows 0.3/0.7 outperforms equal weights
    let keyword_weight = 0.3_f32;
    let semantic_weight = 0.7_f32;

    // ====================================================================
    // STAGE 1: Hybrid search with Nx over-fetch for reranking pool
    //          + compound query decomposition for multi-part queries
    // ====================================================================
    let overfetch_multiplier = match retrieval_mode {
        "precise" => 1,
        "exhaustive" => 5,
        _ => 3,
    };
    let overfetch_limit = (limit * overfetch_multiplier).min(100);

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
        let mut best: std::collections::HashMap<String, vestige_core::SearchResult> =
            std::collections::HashMap::new();
        for batch in all_results {
            for r in batch {
                let id = r.node.id.clone();
                let score = r.combined_score;
                match best.get(&id) {
                    Some(existing) if existing.combined_score >= score => {}
                    _ => {
                        best.insert(id, r);
                    }
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
            storage_clone.hybrid_search(
                &query_clone,
                overfetch_limit,
                keyword_weight,
                semantic_weight,
            )
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
            storage_clone.hybrid_search(
                &query_clone,
                overfetch_limit,
                keyword_weight,
                semantic_weight,
            )
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
            if let Some(valid_until) = r.node.valid_until
                && valid_until < now
            {
                return false;
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

        if let Ok(reranked) = cog
            .reranker
            .rerank(&args.query, candidates, Some(limit as usize))
        {
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
            if !keep[i] {
                continue;
            }
            for j in (i + 1)..filtered_results.len() {
                if !keep[j] {
                    continue;
                }
                if content_overlap(
                    &filtered_results[i].node.content,
                    &filtered_results[j].node.content,
                ) > 0.85
                {
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
            result.combined_score = result.combined_score * 0.85
                + (result.combined_score * temporal_factor as f32) * 0.15;
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
                if max_score <= 0.0 {
                    continue;
                }

                let score_gap = (score_i - score_j).abs() / max_score;
                if score_gap > 0.15 {
                    continue;
                }

                let tag_overlap = tag_jaccard(
                    &filtered_results[i].node.tags,
                    &filtered_results[j].node.tags,
                );
                if tag_overlap < 0.5 {
                    continue;
                }

                let newer_idx =
                    if filtered_results[i].node.created_at > filtered_results[j].node.created_at {
                        i
                    } else {
                        j
                    };
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
        let retrieval_ctx =
            EncodingContext::new().with_topical(TopicalContext::with_topics(topics.clone()));
        if let Ok(cog) = cognitive.try_lock() {
            for result in &mut filtered_results {
                // Build encoding context from memory's tags
                let encoding_ctx = EncodingContext::new()
                    .with_topical(TopicalContext::with_topics(result.node.tags.clone()));
                let context_score = cog
                    .context_matcher
                    .match_contexts(&encoding_ctx, &retrieval_ctx);
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
            let reinstatement = cog
                .context_matcher
                .reinstate_context(&first.node.id, &current_ctx);
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
    // Skipped in precise mode (no need) and exhaustive mode (want all results)
    // ====================================================================
    let mut suppressed_count = 0_usize;
    if retrieval_mode == "balanced" && filtered_results.len() > 1 {
        // Pre-fetch all embeddings on the blocking pool, then run the
        // competition under the cognitive lock without further SQL.
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        let embeddings: Vec<Option<Vec<f32>>> = {
            let storage_emb = storage.clone();
            let ids: Vec<String> = filtered_results.iter().map(|r| r.node.id.clone()).collect();
            tokio::task::spawn_blocking(move || {
                ids.into_iter()
                    .map(|id| storage_emb.get_node_embedding(&id).ok().flatten())
                    .collect()
            })
            .await
            .map_err(|e| format!("search_unified embedding task panicked: {}", e))?
        };
        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        let embeddings: Vec<Option<Vec<f32>>> = vec![None; filtered_results.len()];

        if let Ok(mut cog) = cognitive.try_lock() {
            let candidates: Vec<CompetitionCandidate> = filtered_results
                .iter()
                .zip(embeddings.into_iter())
                .map(|(r, embedding)| CompetitionCandidate {
                    memory_id: r.node.id.clone(),
                    relevance_score: r.combined_score as f64,
                    similarity_to_query: r.semantic_score.unwrap_or(0.0) as f64,
                    embedding,
                })
                .collect();
            if let Some(result) = cog.competition_mgr.run_competition(&candidates, 0.7) {
                for suppressed_id in &result.suppressed_ids {
                    if let Some(r) = filtered_results
                        .iter_mut()
                        .find(|r| &r.node.id == suppressed_id)
                    {
                        r.combined_score *= 0.85;
                        suppressed_count += 1;
                    }
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
                let age_days =
                    (chrono::Utc::now() - result.node.created_at).num_hours() as f32 / 24.0;
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
        // Fetch all connection rows in one blocking hop.
        let storage_pi = storage.clone();
        let ids_for_task = result_ids.clone();
        let connections_per_id: Vec<Vec<vestige_core::ConnectionRecord>> =
            tokio::task::spawn_blocking(move || {
                ids_for_task
                    .into_iter()
                    .map(|id| {
                        storage_pi
                            .get_connections_for_memory(&id)
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .await
            .map_err(|e| format!("search_unified contradiction task panicked: {}", e))?;

        let mut penalties: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        for (id, connections) in result_ids.iter().zip(connections_per_id.into_iter()) {
            for conn in &connections {
                if conn.link_type == "contradiction" {
                    let other = if conn.source_id == *id {
                        &conn.target_id
                    } else {
                        &conn.source_id
                    };
                    if result_ids.contains(other) {
                        let id_time = filtered_results
                            .iter()
                            .find(|r| r.node.id == *id)
                            .map(|r| r.node.created_at);
                        let other_time = filtered_results
                            .iter()
                            .find(|r| r.node.id == *other)
                            .map(|r| r.node.created_at);
                        if let (Some(t1), Some(t2)) = (id_time, other_time) {
                            let older = if t1 < t2 { id } else { other };
                            penalties.entry(older.clone()).or_insert(0.20);
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
    if filtered_results.len() >= 3
        && let Some(top_score) = filtered_results.first().map(|r| r.combined_score)
        && top_score > 0.0
    {
        let threshold = top_score * 0.30;
        filtered_results.retain(|r| r.combined_score >= threshold);
    }
    let prune_removed = pre_prune_count - filtered_results.len();

    // ====================================================================
    // STAGE 6: Spreading activation — triple hybrid scoring
    //
    // Run spreading activation from the top result and use the activation
    // values to boost results that are reachable in the graph.
    // This creates a triple signal: keyword + semantic + graph.
    // Skipped in precise mode. Deeper (5 results) in exhaustive mode.
    // ====================================================================
    let activation_take = match retrieval_mode {
        "precise" => 0,
        "exhaustive" => 5,
        _ => 3,
    };
    let associations: Vec<Value> = if activation_take > 0 {
        if let Ok(mut cog) = cognitive.try_lock() {
            if let Some(first) = filtered_results.first() {
                let activated = cog.activation_network.activate(&first.node.id, 1.0);
                let activation_map: std::collections::HashMap<&str, f64> = activated
                    .iter()
                    .map(|a| (a.memory_id.as_str(), a.activation))
                    .collect();

                for result in filtered_results.iter_mut().skip(1) {
                    if let Some(&act) = activation_map.get(result.node.id.as_str()) {
                        result.combined_score *= 1.0 + (act as f32 * 0.20).min(0.20);
                    }
                }

                activated
                    .iter()
                    .take(activation_take)
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
    // Persist record_memory_access for every result on the blocking pool
    // first; the cognitive side-effects then run lock-bound with no SQL.
    {
        let storage_access = storage.clone();
        let access_ids: Vec<String> = filtered_results.iter().map(|r| r.node.id.clone()).collect();
        let _ = tokio::task::spawn_blocking(move || {
            for id in &access_ids {
                if let Err(e) = storage_access.record_memory_access(id) {
                    tracing::debug!(error = %e, memory_id = %id, "Failed to record memory access");
                }
            }
        })
        .await;
    }

    if let Ok(mut cog) = cognitive.try_lock() {
        // 7A. Record query for predictive memory
        let _ = cog.predictive_memory.record_query(&args.query, &[]);

        // 7B. Record each accessed memory for predictive/speculative models
        for result in &filtered_results {
            let _ = cog.predictive_memory.record_memory_access(
                &result.node.id,
                &result.node.content.chars().take(100).collect::<String>(),
                &result.node.tags,
            );

            cog.speculative_retriever.record_access(
                &result.node.id,
                None,                      // file_context
                Some(args.query.as_str()), // query_context
                None,                      // was_helpful (unknown yet)
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
        let max_budget: i32 = std::env::var("VESTIGE_MAX_TOKEN_BUDGET")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100000);
        let budget = budget.clamp(100, max_budget) as usize;
        let budget_chars = budget * 4;

        let total_size: usize = formatted
            .iter()
            .map(|r| serde_json::to_string(r).unwrap_or_default().len())
            .sum();

        // Step 2: compress content if over budget
        if total_size > budget_chars && detail_level != "brief" {
            let ratio = budget_chars as f64 / total_size as f64;
            for result in &mut formatted {
                if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
                    let compressed = compress_content(content, ratio);
                    result["content"] = serde_json::Value::String(compressed);
                }
            }
        }

        // Step 3: drop results that still don't fit
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
    let learning_mode = cognitive
        .try_lock()
        .ok()
        .map(|cog| cog.attention_signal.is_learning_mode())
        .unwrap_or(false);

    let mut response = serde_json::json!({
        "query": args.query,
        "method": "hybrid+cognitive",
        "retrievalMode": retrieval_mode,
        "detailLevel": detail_level,
        "total": formatted.len(),
        "results": formatted,
    });

    if formatted.is_empty() {
        response["hint"] = serde_json::json!(
            "No memories found. Use smart_ingest to add memories, or try a broader query."
        );
    }
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
            filtered_results
                .iter()
                .map(|r| r.node.confidence().point)
                .sum::<f64>()
                / filtered_results.len() as f64
        };
        let topic = args.query.split_whitespace().next().unwrap_or("unknown");
        cog.metacognition
            .record_search(filtered_results.len(), avg_conf, topic);

        let report = cog.metacognition.report();
        if report.total_queries_tracked >= 5 {
            response["metacognition"] = serde_json::json!({
                "hitRate": format!("{:.0}%", report.hit_rate * 100.0),
                "avgConfidence": format!("{:.2}", report.avg_confidence),
                "queriesTracked": report.total_queries_tracked,
            });
            if !report.knowledge_gaps.is_empty() {
                response["knowledgeGaps"] = serde_json::json!(
                    report
                        .knowledge_gaps
                        .iter()
                        .map(|g| &g.topic)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    Ok(response)
}

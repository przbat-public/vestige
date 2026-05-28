//! Scoring phase — every score adjustment between rerank and finalize.
//!
//! Stage 3   — Temporal boost (recency × validity).
//! Stage 3B  — Freshness-aware ranking when topics overlap.
//! Stage 4   — Memory state accessibility filter (Active/Dormant/Silent/Unavailable).
//! Stage 5   — Tulving (1973) context matching + reinstatement for top result.
//! Stage 5B  — Anderson et al. (1994) retrieval competition (balanced mode only).
//! Stage 5C  — MemRL utility-based ranking.
//! Stage 5D  — Emotional valence boost (McGaugh 2004 + Bower 1981 mood congruence).
//! Stage 5E-pre — Tulving (1972) memory tier adjustment (episodic recency, procedural stability).
//! Stage 5E  — Bayesian confidence boost (Gelman et al. 2013).
//! Stage 5F  — Proactive interference resolution (downrank older of a contradiction pair).
//! Stage 5G  — Score-adaptive pruning (drop results below 30% of top score).

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{
    CompetitionCandidate, EncodingContext, MemoryLifecycle, MemoryState, SearchResult, Storage,
    TopicalContext,
};

use crate::cognitive::CognitiveEngine;

use super::super::args::SearchArgs;
use super::super::helpers::tag_jaccard;
use super::{PipelineConfig, ScoringOutput};

pub(in crate::tools::search_unified) async fn run(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    config: &PipelineConfig,
    mut filtered_results: Vec<SearchResult>,
) -> Result<ScoringOutput, String> {
    apply_temporal_boost(cognitive, &mut filtered_results);
    apply_freshness_boost(&mut filtered_results);
    apply_state_accessibility(cognitive, &mut filtered_results);
    let reinstatement_info = apply_context_matching(cognitive, args, &mut filtered_results);
    let suppressed_count =
        apply_retrieval_competition(storage, cognitive, config, &mut filtered_results).await?;
    apply_utility_boost(&mut filtered_results);
    apply_emotional_boost(cognitive, args, &mut filtered_results);
    apply_memory_tier_adjustment(&mut filtered_results);
    apply_bayesian_confidence(&mut filtered_results);
    apply_proactive_interference(storage, &mut filtered_results).await?;

    // Re-sort once after every score modification settled.
    filtered_results.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let prune_removed = apply_adaptive_pruning(&mut filtered_results);

    Ok(ScoringOutput {
        results: filtered_results,
        suppressed_count,
        prune_removed,
        reinstatement_info,
        // Spreading-activation associations are produced in the finalize
        // phase after the final ordering is known.
        associations: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// STAGE 3 — temporal boost
// ---------------------------------------------------------------------------
fn apply_temporal_boost(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    filtered_results: &mut [SearchResult],
) {
    let Ok(cog) = cognitive.try_lock() else {
        crate::cognitive::try_lock_metrics::record_miss("search_scoring_temporal");
        return;
    };
    for result in filtered_results.iter_mut() {
        let recency = cog.temporal_searcher.recency_boost(result.node.created_at);
        let validity = cog.temporal_searcher.validity_boost(
            result.node.valid_from,
            result.node.valid_until,
            None,
        );
        // Blend: 85% relevance + 15% temporal signal.
        let temporal_factor = recency * validity;
        result.combined_score = result.combined_score * 0.85
            + (result.combined_score * temporal_factor as f32) * 0.15;
    }
}

// ---------------------------------------------------------------------------
// STAGE 3B — freshness-aware ranking
// ---------------------------------------------------------------------------
fn apply_freshness_boost(filtered_results: &mut [SearchResult]) {
    if filtered_results.len() <= 1 {
        return;
    }
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

// ---------------------------------------------------------------------------
// STAGE 4 — memory state accessibility
// ---------------------------------------------------------------------------
fn apply_state_accessibility(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    filtered_results: &mut [SearchResult],
) {
    let Ok(cog) = cognitive.try_lock() else {
        crate::cognitive::try_lock_metrics::record_miss("search_scoring_state");
        return;
    };
    for result in filtered_results.iter_mut() {
        let mut lifecycle = MemoryLifecycle::new();
        lifecycle.last_access = result.node.last_accessed;
        lifecycle.access_count = result.node.reps as u32;
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

// ---------------------------------------------------------------------------
// STAGE 5 — context matching + reinstatement
// ---------------------------------------------------------------------------
fn apply_context_matching(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    filtered_results: &mut [SearchResult],
) -> Option<Value> {
    if let Some(ref topics) = args.context_topics
        && !topics.is_empty()
    {
        let retrieval_ctx =
            EncodingContext::new().with_topical(TopicalContext::with_topics(topics.clone()));
        match cognitive.try_lock() {
            Ok(cog) => {
                for result in filtered_results.iter_mut() {
                    let encoding_ctx = EncodingContext::new()
                        .with_topical(TopicalContext::with_topics(result.node.tags.clone()));
                    let context_score = cog
                        .context_matcher
                        .match_contexts(&encoding_ctx, &retrieval_ctx);
                    // Blend: context match boosts relevance up to +30%.
                    result.combined_score *= 1.0 + (context_score as f32 * 0.3);
                }
            }
            Err(_) => {
                crate::cognitive::try_lock_metrics::record_miss("search_scoring_context_match");
            }
        }
    }

    // Reinstatement for the top result helps the agent see WHY this
    // memory matched — temporal/topical/session hints + related ids.
    match cognitive.try_lock() {
        Ok(cog) => {
            if let Some(first) = filtered_results.first() {
                let current_ctx = if let Some(ref topics) = args.context_topics {
                    EncodingContext::new().with_topical(TopicalContext::with_topics(topics.clone()))
                } else {
                    EncodingContext::new()
                };
                let reinstatement = cog
                    .context_matcher
                    .reinstate_context(&first.node.id, &current_ctx);
                return Some(serde_json::json!({
                    "memoryId": reinstatement.memory_id,
                    "temporalHint": reinstatement.temporal_hint,
                    "topicalHint": reinstatement.topical_hint,
                    "sessionHint": reinstatement.session_hint,
                    "relatedMemories": reinstatement.related_memories,
                }));
            }
        }
        Err(_) => {
            crate::cognitive::try_lock_metrics::record_miss("search_scoring_context_reinstate");
        }
    }
    None
}

// ---------------------------------------------------------------------------
// STAGE 5B — retrieval competition (Anderson et al. 1994)
// ---------------------------------------------------------------------------
async fn apply_retrieval_competition(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    config: &PipelineConfig,
    filtered_results: &mut [SearchResult],
) -> Result<usize, String> {
    let mut suppressed_count = 0_usize;
    if config.retrieval_mode != "balanced" || filtered_results.len() <= 1 {
        return Ok(suppressed_count);
    }

    // Pre-fetch every embedding on the blocking pool so the lock-bound
    // section below stays free of SQL.
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
    let embeddings: Vec<Option<Vec<f32>>> = {
        let _ = storage;
        vec![None; filtered_results.len()]
    };

    let Ok(mut cog) = cognitive.try_lock() else {
        crate::cognitive::try_lock_metrics::record_miss("search_scoring_competition");
        return Ok(suppressed_count);
    };
    let candidates: Vec<CompetitionCandidate> = filtered_results
        .iter()
        .zip(embeddings)
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
    Ok(suppressed_count)
}

// ---------------------------------------------------------------------------
// STAGE 5C — MemRL utility boost
// ---------------------------------------------------------------------------
fn apply_utility_boost(filtered_results: &mut [SearchResult]) {
    for result in filtered_results.iter_mut() {
        let utility = result.node.utility_score.unwrap_or(0.0) as f32;
        if utility > 0.0 {
            // Up to +15% for memories whose utility ratio reaches 1.0.
            result.combined_score *= 1.0 + (utility * 0.15);
        }
    }
}

// ---------------------------------------------------------------------------
// STAGE 5D — emotional valence + mood congruence
// ---------------------------------------------------------------------------
fn apply_emotional_boost(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    filtered_results: &mut [SearchResult],
) {
    let Ok(mut cog) = cognitive.try_lock() else {
        crate::cognitive::try_lock_metrics::record_miss("search_scoring_emotional");
        return;
    };
    cog.emotional_memory.evaluate_content(&args.query);

    for result in filtered_results.iter_mut() {
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

// ---------------------------------------------------------------------------
// STAGE 5E-pre — Tulving (1972) memory tier adjustment
// ---------------------------------------------------------------------------
fn apply_memory_tier_adjustment(filtered_results: &mut [SearchResult]) {
    for result in filtered_results.iter_mut() {
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
}

// ---------------------------------------------------------------------------
// STAGE 5E — Bayesian confidence
// ---------------------------------------------------------------------------
fn apply_bayesian_confidence(filtered_results: &mut [SearchResult]) {
    for result in filtered_results.iter_mut() {
        let conf = result.node.confidence();
        if conf.is_informed() {
            let boost = (conf.lower_95 as f32 - 0.5).max(0.0) * 0.15;
            result.combined_score *= 1.0 + boost;
        }
    }
}

// ---------------------------------------------------------------------------
// STAGE 5F — proactive interference resolution
// ---------------------------------------------------------------------------
async fn apply_proactive_interference(
    storage: &Arc<Storage>,
    filtered_results: &mut [SearchResult],
) -> Result<(), String> {
    let result_ids: Vec<String> = filtered_results.iter().map(|r| r.node.id.clone()).collect();
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

    let mut penalties: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for (id, connections) in result_ids.iter().zip(connections_per_id) {
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

    for result in filtered_results.iter_mut() {
        if let Some(penalty) = penalties.get(&result.node.id) {
            result.combined_score *= 1.0 - penalty;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// STAGE 5G — score-adaptive pruning
// ---------------------------------------------------------------------------
fn apply_adaptive_pruning(filtered_results: &mut Vec<SearchResult>) -> usize {
    let pre_prune_count = filtered_results.len();
    if filtered_results.len() >= 3
        && let Some(top_score) = filtered_results.first().map(|r| r.combined_score)
        && top_score > 0.0
    {
        let threshold = top_score * 0.30;
        filtered_results.retain(|r| r.combined_score >= threshold);
    }
    pre_prune_count - filtered_results.len()
}

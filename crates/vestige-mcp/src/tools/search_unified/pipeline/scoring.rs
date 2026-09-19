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

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{
    CompetitionCandidate, EncodingContext, MemoryLifecycle, MemoryState, SearchResult, Storage,
    TopicalContext, memory::FreshnessKey,
};

use crate::cognitive::CognitiveEngine;

use super::super::args::SearchArgs;
use super::super::helpers::looks_like_conflicting_versions;
use super::{PipelineConfig, ScoringOutput};

pub(in crate::tools::search_unified) async fn run(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    config: &PipelineConfig,
    mut filtered_results: Vec<SearchResult>,
) -> Result<ScoringOutput, String> {
    apply_temporal_boost(cognitive, &mut filtered_results).await;
    apply_state_accessibility(cognitive, &mut filtered_results).await;
    let reinstatement_info = apply_context_matching(cognitive, args, &mut filtered_results).await;
    let suppressed_count =
        apply_retrieval_competition(storage, cognitive, config, &mut filtered_results).await?;
    apply_utility_boost(&mut filtered_results);
    apply_emotional_boost(cognitive, args, &mut filtered_results).await;
    apply_memory_tier_adjustment(&mut filtered_results);
    apply_bayesian_confidence(&mut filtered_results);
    let contradiction_pairs = apply_proactive_interference(storage, &mut filtered_results).await?;

    // Every conflicting pair the read path could see is now known — from the
    // contradiction edges and from the content gate. The fresher member of each is
    // ordered first *before* the sort, so the emitted order and the emitted
    // `combined_score` agree.
    let conflict_pairs = apply_freshness_ordering(&mut filtered_results, &contradiction_pairs);
    sort_by_score_then_freshness(&mut filtered_results);

    // Pruning must not be what resolves a conflict. A capped superseded statement
    // can drop below the 30%-of-top cut, and when the cap lowered the top the
    // correction fell out with it — measured on FactConsolidation, that emptied one
    // conflict group. Recognised conflicts are exempt from the cut; the freshness
    // rule, not the score threshold, decides which version is read first.
    let protected: std::collections::HashSet<usize> =
        conflict_pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
    let prune_removed = apply_adaptive_pruning(&mut filtered_results, &protected);
    sort_by_score_then_freshness(&mut filtered_results);

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
// ORDERING — the pipeline's single ordering rule
// ---------------------------------------------------------------------------

/// Order results by score, then by `FreshnessKey`.
///
/// The score is the primary key and stays descending, so the emitted vector is
/// monotonic in the score it reports. Equal scores — which the multiplicative
/// boosts above produce routinely — fall back to the identity rule from
/// `vestige_core::memory`: the greater timestamp wins, and an exact tie is decided
/// by the memory id. Without that fallback the surviving order was the order the
/// candidates happened to arrive in, i.e. hash order upstream.
pub(in crate::tools::search_unified) fn sort_by_score_then_freshness(
    results: &mut Vec<SearchResult>,
) {
    let mut keyed: Vec<(f32, FreshnessKey, SearchResult)> = results
        .drain(..)
        .map(|r| {
            let key = FreshnessKey::from_node(&r.node);
            (r.combined_score, key, r)
        })
        .collect();
    keyed.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.compare(&a.1))
    });
    *results = keyed.into_iter().map(|(_, _, r)| r).collect();
}

/// One part in a thousand below `score`: enough to separate two f32 values at any
/// magnitude the pipeline produces, small enough that the reported score still
/// reads as the same number.
fn just_below(score: f32) -> f32 {
    if score > 0.0 {
        score * (1.0 - 1e-3)
    } else {
        score - 1e-6
    }
}

/// Resolve conflicting memories by the core freshness rule, as an ordering
/// constraint rather than a score nudge.
///
/// Two retrieved memories that state the same fact with different values are a
/// conflict, and the correction is worthless to the reader if the version it
/// replaces is read first. The read path used to express this as a +10% boost
/// gated on tag overlap and a score gap under 15%: tagless memories have a
/// Jaccard of 0.0, so the stage never fired for them, and a 10% multiplier cannot
/// overcome a larger score gap anyway (FactConsolidation measured the superseded
/// statement first in 74.6% of conflicts). Here the *superseded* member is capped
/// just below its correction instead, which is exactly an ordering constraint.
///
/// Capping and not reordering: the pipeline's last word is `sort_by_score_then_freshness`
/// (and `apply_adaptive_pruning` reads the same order), so the constraint has to be
/// expressible in the score. It only ever lowers a score and is idempotent, so
/// re-applying it after a later boost cannot oscillate.
///
/// `known_conflicts` are index pairs already established by a contradiction edge;
/// the rest are found by the cheap content gate in `helpers`.
///
/// Returns every pair it ordered, so the caller can keep them out of pruning.
pub(in crate::tools::search_unified) fn apply_freshness_ordering(
    results: &mut [SearchResult],
    known_conflicts: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let n = results.len();
    if n <= 1 {
        return Vec::new();
    }

    let mut pairs: Vec<(usize, usize)> = known_conflicts
        .iter()
        .copied()
        .filter(|&(a, b)| a < n && b < n && a != b)
        .collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if looks_like_conflicting_versions(&results[i].node.content, &results[j].node.content) {
                pairs.push((i, j));
            }
        }
    }
    if pairs.is_empty() {
        return Vec::new();
    }

    // Conflicting statements form groups (a fact can be superseded more than
    // once). Union-find keeps a chain A→B→C in one group, where ordering the
    // pairs independently would leave a transitive violation behind.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b) in &pairs {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        }
    }

    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }

    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        // Freshest first; `FreshnessKey::compare` is a total order, so this is
        // deterministic and cannot cycle however the members were retrieved.
        let mut by_freshness: Vec<(FreshnessKey, usize)> = members
            .iter()
            .map(|&i| (FreshnessKey::from_node(&results[i].node), i))
            .collect();
        by_freshness.sort_by(|a, b| b.0.compare(&a.0));

        // Walk down the chain capping each older member below the one above it;
        // the cap of the member above was itself capped, so the whole group ends
        // up strictly ordered by freshness.
        for pair in by_freshness.windows(2) {
            let (fresher, older) = (pair[0].1, pair[1].1);
            let cap = just_below(results[fresher].combined_score);
            if results[older].combined_score > cap {
                results[older].combined_score = cap;
            }
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// STAGE 3 — temporal boost
// ---------------------------------------------------------------------------
async fn apply_temporal_boost(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    filtered_results: &mut [SearchResult],
) {
    // Blocking, not `try_lock`: a stage that skips itself when the engine is busy
    // makes the ranking depend on who else happened to hold the lock, so the same
    // query returns different orders on different runs. Waiting costs latency and
    // buys determinism.
    let cog = cognitive.lock().await;
    for result in filtered_results.iter_mut() {
        // `CognitiveEngine::temporal_factor` folds recency × validity and
        // reports the neutral 1.0 in builds without `vector-search`, where
        // `TemporalSearcher` is not compiled in. The blend below then reduces
        // to the identity, which is what we want: no boost is better than a
        // boost computed from parameters this build does not have.
        let temporal_factor = cog.temporal_factor(
            result.node.created_at,
            result.node.valid_from,
            result.node.valid_until,
        );
        // Blend: 85% relevance + 15% temporal signal.
        result.combined_score =
            result.combined_score * 0.85 + (result.combined_score * temporal_factor as f32) * 0.15;
    }
}

// ---------------------------------------------------------------------------
// STAGE 4 — memory state accessibility
// ---------------------------------------------------------------------------
async fn apply_state_accessibility(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    filtered_results: &mut [SearchResult],
) {
    let cog = cognitive.lock().await;
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
async fn apply_context_matching(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    filtered_results: &mut [SearchResult],
) -> Option<Value> {
    if let Some(ref topics) = args.context_topics
        && !topics.is_empty()
    {
        let retrieval_ctx =
            EncodingContext::new().with_topical(TopicalContext::with_topics(topics.clone()));
        let cog = cognitive.lock().await;
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

    // Reinstatement for the top result helps the agent see WHY this
    // memory matched — temporal/topical/session hints + related ids.
    {
        let cog = cognitive.lock().await;
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

    let mut cog = cognitive.lock().await;
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
        // Collect first, mutate after: the lookup borrows `filtered_results`, and
        // the map must be dropped before the scores it points into are written.
        let suppressed_positions: Vec<usize> = {
            let position_of: std::collections::HashMap<&str, usize> = filtered_results
                .iter()
                .enumerate()
                .map(|(i, r)| (r.node.id.as_str(), i))
                .collect();
            result
                .suppressed_ids
                .iter()
                .filter_map(|id| position_of.get(id.as_str()).copied())
                .collect()
        };
        for i in suppressed_positions {
            filtered_results[i].combined_score *= 0.85;
            suppressed_count += 1;
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
async fn apply_emotional_boost(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    filtered_results: &mut [SearchResult],
) {
    let mut cog = cognitive.lock().await;
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
) -> Result<Vec<(usize, usize)>, String> {
    let n = filtered_results.len();
    if n <= 1 {
        return Ok(Vec::new());
    }

    // One query for the whole candidate set. The stage used to call
    // `get_connections_for_memory` once per result — N+1 round-trips through the
    // same connection — to look for at most a handful of contradiction edges.
    // `get_all_connections` is the existing bulk read (it is what the activation
    // network is built from), so the search path goes from 1 + N queries to 1.
    let storage_pi = storage.clone();
    let connections = tokio::task::spawn_blocking(move || storage_pi.get_all_connections())
        .await
        .map_err(|e| format!("search_unified contradiction task panicked: {}", e))?
        .unwrap_or_default();

    // id -> position, built once: the pairwise `Vec::contains` and the two
    // `iter().find()` scans per contradiction edge made this stage O(n²) in the
    // number of results.
    let position_of: std::collections::HashMap<&str, usize> = filtered_results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.node.id.as_str(), i))
        .collect();

    let mut penalties = vec![0.0_f32; n];
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for conn in &connections {
        if conn.link_type != "contradiction" {
            continue;
        }
        let (Some(&a), Some(&b)) = (
            position_of.get(conn.source_id.as_str()),
            position_of.get(conn.target_id.as_str()),
        ) else {
            continue;
        };
        let (a, b) = (a.min(b), a.max(b));
        if !seen.insert((a, b)) {
            continue;
        }

        // The core rule decides which side is current: `created_at` alone missed
        // valid time, so a memory anchored earlier but recorded later looked
        // older than it is.
        let ordering = FreshnessKey::from_node(&filtered_results[a].node)
            .compare(&FreshnessKey::from_node(&filtered_results[b].node));
        let (older, newer) = if ordering == std::cmp::Ordering::Greater {
            (b, a)
        } else {
            (a, b)
        };
        penalties[older] = penalties[older].max(0.20);
        pairs.push((older, newer));
    }

    for (result, penalty) in filtered_results.iter_mut().zip(penalties) {
        if penalty > 0.0 {
            result.combined_score *= 1.0 - penalty;
        }
    }
    Ok(pairs)
}

// ---------------------------------------------------------------------------
// STAGE 5G — score-adaptive pruning
// ---------------------------------------------------------------------------
fn apply_adaptive_pruning(
    filtered_results: &mut Vec<SearchResult>,
    protected: &std::collections::HashSet<usize>,
) -> usize {
    let pre_prune_count = filtered_results.len();
    if filtered_results.len() >= 3
        && let Some(top_score) = filtered_results.first().map(|r| r.combined_score)
        && top_score > 0.0
    {
        let threshold = top_score * 0.30;
        let mut kept = Vec::with_capacity(filtered_results.len());
        for (i, result) in filtered_results.drain(..).enumerate() {
            if result.combined_score >= threshold || protected.contains(&i) {
                kept.push(result);
            }
        }
        *filtered_results = kept;
    }
    pre_prune_count - filtered_results.len()
}

//! Retrieval phase — stages 0, 1, 2, 2B.
//!
//! Stage 0  — Trivial-query gate (Oblivion pattern).
//! Stage 1  — Hybrid search with N× over-fetch + compound query decomposition.
//!            Cheap filters (retention, similarity, temporal validity).
//! Stage 2  — Cross-encoder reranker (Jina v2) — trims pool to `limit`.
//! Stage 2B — Near-duplicate suppression (>0.85 word overlap).

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{SearchResult, Storage};

use crate::cognitive::CognitiveEngine;

use super::super::args::SearchArgs;
use super::super::helpers::{content_overlap_sets, content_token_set, is_trivial_query};
use super::{PipelineConfig, RetrievalOutput};

/// Run stages 0, 1, 2, 2B. Returns `Ok(Err(early_response))` when stage 0
/// short-circuits — the orchestrator emits the trivial-gate response
/// without hitting any later phase.
#[allow(
    clippy::result_large_err,
    reason = "Early-return value is the final JSON response, returned by value once."
)]
pub(in crate::tools::search_unified) async fn run(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    config: &PipelineConfig,
) -> Result<Result<RetrievalOutput, Value>, String> {
    // ====================================================================
    // STAGE 0: Read-path gating (Oblivion pattern)
    // ====================================================================
    if is_trivial_query(&args.query) {
        return Ok(Err(serde_json::json!({
            "results": [],
            "totalFound": 0,
            "gated": true,
            "reason": "Query classified as trivial — retrieval skipped"
        })));
    }

    // ====================================================================
    // STAGE 1: Hybrid search with over-fetch + compound decomposition
    // ====================================================================
    // Favour semantic search — empirically 0.3/0.7 outperforms equal weights.
    let keyword_weight = 0.3_f32;
    let semantic_weight = 0.7_f32;
    let overfetch_multiplier = match config.retrieval_mode {
        "precise" => 1,
        "exhaustive" => 5,
        _ => 3,
    };
    let overfetch_limit = (config.limit * overfetch_multiplier).min(100);

    // How many results the rerank stage is allowed to keep. With MMR enabled this is a
    // *pool* to select from rather than the final answer: diversity reordering can only
    // change which memories survive if it has more candidates than slots. It used to run
    // after the truncation to `config.limit` and keep the same count, so it permuted an
    // already-decided set — which is why the A/B measured exactly zero difference.
    let final_limit = config.limit as usize;
    let mmr_on = mmr_enabled();
    let keep_limit = if mmr_on {
        (final_limit * 4).clamp(final_limit, overfetch_limit as usize)
    } else {
        final_limit
    };

    let raw = hybrid_with_decompose(
        storage,
        &args.query,
        overfetch_limit,
        keyword_weight,
        semantic_weight,
    )
    .await?;

    // Cheap filters — retention floor, semantic floor, temporal validity.
    let now = chrono::Utc::now();
    let mut filtered_results: Vec<SearchResult> = raw
        .into_iter()
        .filter(|r| {
            if let Some(valid_until) = r.node.valid_until
                && valid_until < now
            {
                return false;
            }
            if r.node.retention_strength < config.min_retention {
                return false;
            }
            if let Some(sem_score) = r.semantic_score
                && sem_score < config.min_similarity
            {
                return false;
            }
            true
        })
        .collect();

    // ====================================================================
    // STAGE 2: Cross-encoder reranker
    //
    // Both rerankers live in `vestige_core::search` (gated by `vector-search`)
    // and reorder a hybrid candidate pool, so a keyword-only build has neither
    // the types nor the pool. It keeps the FTS5 order and truncates, which is
    // the same fallback the full build takes when no cross-encoder model has
    // been loaded — not a new failure mode.
    // ====================================================================
    #[cfg(not(feature = "vector-search"))]
    filtered_results.truncate(keep_limit);
    #[cfg(not(feature = "vector-search"))]
    let _ = cognitive; // used only by the gated reranker stage below
    #[cfg(feature = "vector-search")]
    if let Ok(mut cog) = cognitive.try_lock() {
        let candidates: Vec<_> = filtered_results
            .iter()
            .map(|r| (r.clone(), r.node.content.clone()))
            .collect();

        // Prefer ColBERT late-interaction reranking when it is loaded; the Jina
        // cross-encoder is the fallback (and the default when the feature is off
        // or the model failed to load).
        #[cfg(feature = "late-interaction")]
        let colbert_ranked: Option<Vec<SearchResult>> = cog.colbert.as_ref().and_then(|colbert| {
            vestige_core::search::rerank_late_interaction(
                colbert,
                &args.query,
                candidates.clone(),
                keep_limit,
            )
            .ok()
            .map(|ranked| ranked.into_iter().map(|r| r.item).collect())
        });
        #[cfg(not(feature = "late-interaction"))]
        let colbert_ranked: Option<Vec<SearchResult>> = None;

        if let Some(ranked) = colbert_ranked {
            filtered_results = ranked;
        } else if let Ok(reranked) = cog
            .reranker
            .rerank(&args.query, candidates, Some(keep_limit))
        {
            filtered_results = carry_rerank_scores(reranked);
        } else {
            filtered_results.truncate(keep_limit);
        }
    } else {
        crate::cognitive::try_lock_metrics::record_miss("search_rerank");
        tracing::debug!("Search stage 2: cognitive lock contention, skipping reranker");
        filtered_results.truncate(keep_limit);
    }

    // ====================================================================
    // STAGE 2B: Near-duplicate suppression (Yuan et al. 2026)
    //
    // Two results sharing >85% word overlap waste context tokens. Run
    // AFTER reranking so the higher-ranked variant survives.
    // ====================================================================
    let (mut filtered_results, dedup_removed) = suppress_near_duplicates(filtered_results);

    // ====================================================================
    // STAGE 2C: MMR diversity (multi-hop synthesis substrate)
    //
    // A pure relevance ranking can hand the answerer K near-duplicate memories about the
    // single most-relevant fact, starving multi-hop synthesis of the *other* facts it must
    // combine. MMR (Carbonell & Goldstein 1998) picks, at each step, the candidate that
    // maximises `lambda * relevance - (1 - lambda) * max_similarity_to_already_picked`.
    //
    // Opt-in + tunable so it can be A/B'd against the plain ranking:
    //   VESTIGE_MMR=on, VESTIGE_MMR_LAMBDA ∈ [0,1] (default 0.7).
    //
    // `mmr_select` lives in `vestige_core::search`, gated by `vector-search`
    // even though MMR is pure set selection. Without it the pool is truncated
    // instead — and because that silently ignores an explicit `VESTIGE_MMR=on`,
    // the operator is told once at startup (see `main`).
    // ====================================================================
    #[cfg(not(feature = "vector-search"))]
    {
        if mmr_on {
            tracing::debug!(
                "VESTIGE_MMR is set but this build has no `vector-search`; MMR skipped"
            );
        }
        if filtered_results.len() > final_limit {
            filtered_results.truncate(final_limit);
        }
    }
    #[cfg(feature = "vector-search")]
    if mmr_on && filtered_results.len() > 2 {
        filtered_results = mmr_reorder(filtered_results, mmr_lambda(), final_limit);
    } else if filtered_results.len() > final_limit {
        filtered_results.truncate(final_limit);
    }

    Ok(Ok(RetrievalOutput {
        results: filtered_results,
        dedup_removed,
    }))
}

/// Token sets for a candidate list, built once per candidate.
///
/// Both stages below compare pairs, and rebuilding the sets inside
/// `content_overlap` cost two builds per comparison: the dedup stage alone paid up
/// to n(n−1) builds per search with nothing removed (n = 6 → 30 comparison-pairs'
/// worth, `limit: 100` → 9 900), and MMR two more per similarity call. Here each
/// candidate is tokenised once — n sets. The overlap *values* are unchanged:
/// `content_overlap_sets` is the same Jaccard over the same tokens, it just stops
/// re-tokenising.
pub(in crate::tools::search_unified) fn candidate_token_sets(
    results: &[SearchResult],
) -> Vec<std::collections::HashSet<&str>> {
    results
        .iter()
        .map(|r| content_token_set(&r.node.content))
        .collect()
}

/// Near-duplicate suppression: drop a result when a higher-ranked one shares more
/// than 85% of its words (Yuan et al. 2026). Returns the survivors in their
/// original order and how many were removed.
pub(in crate::tools::search_unified) fn suppress_near_duplicates(
    results: Vec<SearchResult>,
) -> (Vec<SearchResult>, usize) {
    let n = results.len();
    if n <= 1 {
        return (results, 0);
    }
    let mut keep = vec![true; n];
    {
        // Scoped so the borrows into `results` end before it is consumed below.
        let sets = candidate_token_sets(&results);
        for i in 0..n {
            if !keep[i] {
                continue;
            }
            for j in (i + 1)..n {
                if !keep[j] {
                    continue;
                }
                if content_overlap_sets(&sets[i], &sets[j]) > 0.85 {
                    keep[j] = false;
                }
            }
        }
    }
    let mut deduped = Vec::with_capacity(n);
    for (idx, result) in results.into_iter().enumerate() {
        if keep[idx] {
            deduped.push(result);
        }
    }
    let removed = n - deduped.len();
    (deduped, removed)
}

/// Select `final_limit` out of the pool by Maximal Marginal Relevance, using
/// `combined_score` as relevance and word overlap as the redundancy penalty.
///
/// Selection runs over indices, not over `SearchResult`s, so the similarity
/// closure can read the precomputed token sets: `mmr_select` calls it O(n·k)
/// times, and the string form rebuilt two HashSets on every one of those calls.
/// This is the step that can change *which* memories are returned, not merely
/// their order, so the selection itself is unchanged — only its inputs are cached.
#[cfg(feature = "vector-search")]
pub(in crate::tools::search_unified) fn mmr_reorder(
    results: Vec<SearchResult>,
    lambda: f32,
    final_limit: usize,
) -> Vec<SearchResult> {
    let order = {
        let sets = candidate_token_sets(&results);
        let scored: Vec<(usize, f32)> = results
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.combined_score))
            .collect();
        vestige_core::search::mmr_select(
            scored,
            |a: &usize, b: &usize| content_overlap_sets(&sets[*a], &sets[*b]) as f32,
            lambda,
            final_limit,
        )
    };
    let mut slots: Vec<Option<SearchResult>> = results.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|i| {
            slots[i]
                .take()
                .expect("mmr_select returns each index at most once")
        })
        .collect()
}

/// Adapt `SearchResult` to the core merge trait.
///
/// `HasIdAndScore` lives in `vestige_core` and so does `SearchResult`, so the impl
/// cannot be written here (orphan rule). The newtype carries it instead, which
/// keeps the union/max-score/id-tiebreak rule in exactly one place —
/// `vestige_core::search::decompose::merge_results`.
#[cfg(feature = "vector-search")]
struct MergeCandidate(SearchResult);

#[cfg(feature = "vector-search")]
impl vestige_core::search::decompose::HasIdAndScore for MergeCandidate {
    fn id(&self) -> &str {
        &self.0.node.id
    }

    fn score(&self) -> f64 {
        self.0.combined_score as f64
    }
}

/// Merge per-sub-query batches: union by id, keep the highest score, and order by
/// score descending with the core module's deterministic tiebreak.
#[cfg(feature = "vector-search")]
pub(in crate::tools::search_unified) fn merge_search_batches(
    batches: Vec<Vec<SearchResult>>,
) -> Vec<SearchResult> {
    let wrapped: Vec<Vec<MergeCandidate>> = batches
        .into_iter()
        .map(|batch| batch.into_iter().map(MergeCandidate).collect())
        .collect();
    vestige_core::search::decompose::merge_results(wrapped)
        .into_iter()
        .map(|c| c.0)
        .collect()
}

/// Move the cross-encoder's score into `combined_score`, min-max normalised to the batch.
///
/// The reranker's *order* is the whole point of stage 2, and the scoring stage re-sorts by
/// `combined_score` once every adjustment settles — so a score that stays in the
/// `RerankedResult` never reaches the ranking. The previous code dropped it (`.map(|rr|
/// rr.item)`), which meant the surviving order was the hybrid one from before the rerank
/// and the cross-encoder was decorative. Normalising keeps the batch's relative order
/// while staying on the same 0..1 scale the rest of the pipeline manipulates.
#[cfg(feature = "vector-search")]
fn carry_rerank_scores(
    reranked: Vec<vestige_core::search::RerankedResult<SearchResult>>,
) -> Vec<SearchResult> {
    let min = reranked
        .iter()
        .map(|r| r.score)
        .fold(f32::INFINITY, f32::min);
    let max = reranked
        .iter()
        .map(|r| r.score)
        .fold(f32::NEG_INFINITY, f32::max);
    let span = if (max - min).abs() < f32::EPSILON {
        1.0
    } else {
        max - min
    };

    reranked
        .into_iter()
        .map(|mut rr| {
            rr.item.combined_score = (rr.score - min) / span;
            rr.item
        })
        .collect()
}

/// Whether MMR diversity reordering is enabled (`VESTIGE_MMR`). Off by default
/// so the plain relevance ranking is the measurable baseline.
fn mmr_enabled() -> bool {
    use std::sync::OnceLock;
    static E: OnceLock<bool> = OnceLock::new();
    *E.get_or_init(|| {
        std::env::var("VESTIGE_MMR")
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

/// MMR relevance/diversity trade-off `λ` from `VESTIGE_MMR_LAMBDA` (default
/// 0.7 — relevance-leaning). Clamped to `[0,1]`; bad values fall back.
#[cfg(feature = "vector-search")]
fn mmr_lambda() -> f32 {
    use std::sync::OnceLock;
    static L: OnceLock<f32> = OnceLock::new();
    *L.get_or_init(|| {
        std::env::var("VESTIGE_MMR_LAMBDA")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .unwrap_or(0.7)
    })
}

/// Issue the retrieval, decomposing compound queries when supported.
///
/// Retrieval goes through [`crate::retrieval::hybrid_search`] rather than
/// `Storage::hybrid_search` directly: that facade is the one place that knows
/// the no-embeddings build falls back to FTS5 keyword search (and, because
/// `decompose_query` is also gated behind `vector-search`, skips compound
/// decomposition). Calling the storage method here is what made
/// `--no-default-features` uncompilable.
async fn hybrid_with_decompose(
    storage: &Arc<Storage>,
    query: &str,
    overfetch_limit: i32,
    keyword_weight: f32,
    semantic_weight: f32,
) -> Result<Vec<SearchResult>, String> {
    #[cfg(feature = "vector-search")]
    {
        let decomposition = vestige_core::search::decompose::decompose_query(query);
        if decomposition.is_compound {
            // One blocking task per sub-query, all started before the first is
            // awaited: the sub-queries are independent reads of the same store, so
            // awaiting them in sequence made a compound query cost the sum of its
            // parts. Batches are collected in sub-query order, so the merge below
            // sees exactly the batches the sequential loop produced.
            let mut handles = Vec::with_capacity(decomposition.sub_queries.len());
            for sub_query in &decomposition.sub_queries {
                let storage_clone = Arc::clone(storage);
                let sq = sub_query.clone();
                handles.push(tokio::task::spawn_blocking(move || {
                    crate::retrieval::hybrid_search(
                        &storage_clone,
                        &sq,
                        overfetch_limit,
                        keyword_weight,
                        semantic_weight,
                    )
                }));
            }
            let mut all_results: Vec<Vec<SearchResult>> = Vec::with_capacity(handles.len());
            for handle in handles {
                let sub_results = handle
                    .await
                    .map_err(|e| format!("Search task panicked: {}", e))?
                    .map_err(|e| e.to_string())?;
                all_results.push(sub_results);
            }

            // Merge: union by node_id, keep max combined_score per id — the rule
            // (and its id tiebreak) lives in the core module.
            return Ok(merge_search_batches(all_results));
        }
    }

    let storage_clone = Arc::clone(storage);
    let query_clone = query.to_string();
    tokio::task::spawn_blocking(move || {
        crate::retrieval::hybrid_search(
            &storage_clone,
            &query_clone,
            overfetch_limit,
            keyword_weight,
            semantic_weight,
        )
    })
    .await
    .map_err(|e| format!("Search task panicked: {}", e))?
    .map_err(|e| e.to_string())
}

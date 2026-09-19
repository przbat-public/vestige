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
use super::super::helpers::{content_overlap, is_trivial_query};
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
    // ====================================================================
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
                config.limit as usize,
            )
            .ok()
            .map(|ranked| ranked.into_iter().map(|r| r.item).collect())
        });
        #[cfg(not(feature = "late-interaction"))]
        let colbert_ranked: Option<Vec<SearchResult>> = None;

        if let Some(ranked) = colbert_ranked {
            filtered_results = ranked;
        } else if let Ok(reranked) =
            cog.reranker
                .rerank(&args.query, candidates, Some(config.limit as usize))
        {
            filtered_results = reranked.into_iter().map(|rr| rr.item).collect();
        } else {
            filtered_results.truncate(config.limit as usize);
        }
    } else {
        crate::cognitive::try_lock_metrics::record_miss("search_rerank");
        tracing::debug!("Search stage 2: cognitive lock contention, skipping reranker");
        filtered_results.truncate(config.limit as usize);
    }

    // ====================================================================
    // STAGE 2B: Near-duplicate suppression (Yuan et al. 2026)
    //
    // Two results sharing >85% word overlap waste context tokens. Run
    // AFTER reranking so the higher-ranked variant survives.
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
    // STAGE 2C: MMR diversity (multi-hop synthesis substrate)
    //
    // A pure relevance ranking can hand the answerer K near-duplicate
    // memories about the single most-relevant fact, starving multi-hop
    // synthesis of the *other* facts it must combine. MMR (Carbonell &
    // Goldstein 1998) reorders the survivors to balance relevance against
    // novelty, so the downstream context budget keeps a diverse set.
    // Opt-in + tunable so it can be A/B'd against the plain ranking:
    //   VESTIGE_MMR=on, VESTIGE_MMR_LAMBDA ∈ [0,1] (default 0.7).
    // ====================================================================
    if mmr_enabled() && filtered_results.len() > 2 {
        let lambda = mmr_lambda();
        let scored: Vec<(SearchResult, f32)> = filtered_results
            .into_iter()
            .map(|r| {
                let score = r.combined_score;
                (r, score)
            })
            .collect();
        let keep = scored.len();
        filtered_results = vestige_core::search::mmr_select(
            scored,
            |a, b| content_overlap(&a.node.content, &b.node.content) as f32,
            lambda,
            keep,
        );
    }

    Ok(Ok(RetrievalOutput {
        results: filtered_results,
        dedup_removed,
    }))
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

/// Issue the hybrid search, decomposing compound queries when supported.
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
            // Run separate searches for each sub-query, then merge.
            let mut all_results: Vec<Vec<SearchResult>> = Vec::new();
            for sub_query in &decomposition.sub_queries {
                let storage_clone = Arc::clone(storage);
                let sq = sub_query.clone();
                let sub_results = tokio::task::spawn_blocking(move || {
                    storage_clone.hybrid_search(
                        &sq,
                        overfetch_limit,
                        keyword_weight,
                        semantic_weight,
                    )
                })
                .await
                .map_err(|e| format!("Search task panicked: {}", e))?
                .map_err(|e| e.to_string())?;
                all_results.push(sub_results);
            }

            // Merge: union by node_id, keep max combined_score per id.
            let mut best: std::collections::HashMap<String, SearchResult> =
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
            return Ok(merged);
        }
    }

    let storage_clone = Arc::clone(storage);
    let query_clone = query.to_string();
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
    .map_err(|e| e.to_string())
}

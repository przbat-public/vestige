//! Finalize phase — stages 6, 7, formatting, token budget, metacognition.
//!
//! Stage 6  — Spreading activation from the top result; boosts reachable
//!            results and produces the `associations` block.
//! Stage 7  — Side effects: Testing-Effect strengthening, predictive memory
//!            recording, reconsolidation labile marking.
//! Format   — `format_search_result` per detail level.
//! Budget   — Three-tier compression (LightMem pattern): fit → compress → spill.
//! Meta     — Metacognition tracking (hit rate, confidence, knowledge gaps).

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{MemorySnapshot, SearchResult, Storage};

use crate::cognitive::CognitiveEngine;

use super::super::args::SearchArgs;
use super::super::format::format_search_result;
use super::super::helpers::compress_content;
use super::{PipelineConfig, ScoringOutput};

/// Run the final phase. Receives the scored result set plus the
/// `dedup_removed` counter from retrieval (passed through the orchestrator
/// so it surfaces in the response payload).
pub(in crate::tools::search_unified) async fn run(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    config: &PipelineConfig,
    mut scoring: ScoringOutput,
    dedup_removed: usize,
) -> Result<Value, String> {
    apply_spreading_activation(cognitive, config, &mut scoring).await;
    strengthen_on_access(storage, &scoring).await;
    record_side_effects(storage, cognitive, args, &scoring).await;

    let formatted: Vec<Value> = scoring
        .results
        .iter()
        .map(|r| format_search_result(r, config.detail_level))
        .collect();

    let (formatted, budget_expandable, budget_tokens_used) =
        enforce_token_budget(formatted, args, config.detail_level);

    let learning_mode = cognitive.lock().await.attention_signal.is_learning_mode();

    let mut response = serde_json::json!({
        "query": args.query,
        "method": "hybrid+cognitive",
        "retrievalMode": config.retrieval_mode,
        "detailLevel": config.detail_level,
        "total": formatted.len(),
        "results": formatted,
    });

    if formatted.is_empty() {
        response["hint"] = serde_json::json!(
            "No memories found. Use smart_ingest to add memories, or try a broader query."
        );
    }
    if !scoring.associations.is_empty() {
        response["associations"] = serde_json::json!(scoring.associations);
    }
    if let Some(ri) = scoring.reinstatement_info {
        response["contextReinstatement"] = ri;
    }
    if scoring.suppressed_count > 0 {
        response["competitionSuppressed"] = serde_json::json!(scoring.suppressed_count);
    }
    if dedup_removed > 0 {
        response["deduplicated"] = serde_json::json!(dedup_removed);
    }
    if scoring.prune_removed > 0 {
        response["pruned"] = serde_json::json!(scoring.prune_removed);
    }
    if learning_mode {
        response["learningModeDetected"] = serde_json::json!(true);
    }
    if !budget_expandable.is_empty() {
        response["expandable"] = serde_json::json!(budget_expandable);
    }
    if let Some(budget) = args.token_budget {
        response["tokenBudget"] = serde_json::json!(budget);
    }
    if let Some(used) = budget_tokens_used {
        response["tokensUsed"] = serde_json::json!(used);
    }

    record_metacognition(cognitive, args, &scoring.results, &mut response).await;

    Ok(response)
}

// ---------------------------------------------------------------------------
// STAGE 6 — spreading activation
// ---------------------------------------------------------------------------
async fn apply_spreading_activation(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    config: &PipelineConfig,
    scoring: &mut ScoringOutput,
) {
    let activation_take = match config.retrieval_mode {
        "precise" => 0,
        "exhaustive" => 5,
        _ => 3,
    };
    if activation_take == 0 {
        return;
    }
    // Clone the activation-network Arc out of the engine and drop the
    // engine lock immediately. Spreading activation then runs holding only
    // its own write lock — other tools can grab the engine concurrently.
    // Both locks are taken by waiting: a `try_lock` here used to skip the
    // stage silently, which made the final order depend on lock timing.
    let net = {
        let cog = cognitive.lock().await;
        Arc::clone(&cog.activation_network)
    };
    let Some(first) = scoring.results.first() else {
        return;
    };

    let activated = net.write().await.activate(&first.node.id, 1.0);
    let activation_map: std::collections::HashMap<&str, f64> = activated
        .iter()
        .map(|a| (a.memory_id.as_str(), a.activation))
        .collect();

    apply_activation_boost(&mut scoring.results, &activation_map);

    scoring.associations = activated
        .iter()
        .take(activation_take)
        .map(|a| {
            serde_json::json!({
                "memoryId": a.memory_id,
                "activation": a.activation,
                "distance": a.distance,
            })
        })
        .collect();
}

/// Apply the ACT-R activation bump, then restore the reported order.
///
/// The bump is multiplicative and lands *after* the scoring phase's sort, so it
/// can move a result past its neighbour — and, when the neighbour is the
/// correction that supersedes it, past the freshness constraint too. The order is
/// therefore rebuilt here, in the same two steps the scoring phase uses, instead
/// of leaving the response with a `combinedScore` column that does not match the
/// row order. Split out as a pure function so that contract is testable without a
/// live activation network.
///
/// The 20 % ceiling is deliberately conservative:
///
///   * Anderson's ACT-R activation can dominate retrieval probability,
///     but that's *recall* probability, not a post-rank score scalar.
///     A 20 % bump on `combined_score` is loud enough to re-order
///     ties without overpowering the FTS/semantic ranking signal.
///   * Activation is normalised to `[0, 1.0]` by the network; values
///     above 1.0 occasionally appear at the anchor's direct
///     neighbours when multiple high-weight edges converge — the
///     `min(0.20)` clamp guarantees the bump is a re-rank, not a
///     re-rewrite, even in those cases.
///
/// If you want to tune this, prefer changing the activation network's
/// decay (in `ActivationNetwork::activate`) rather than this knob —
/// it propagates the effect end-to-end and stays consistent with the
/// dream / metacognition pipelines that also read activation.
pub(in crate::tools::search_unified) fn apply_activation_boost(
    results: &mut Vec<SearchResult>,
    activation_map: &std::collections::HashMap<&str, f64>,
) {
    // `skip(1)`: the anchor is the source of the activation, not a neighbour.
    for result in results.iter_mut().skip(1) {
        if let Some(&act) = activation_map.get(result.node.id.as_str()) {
            result.combined_score *= 1.0 + (act as f32 * 0.20).min(0.20);
        }
    }
    super::scoring::apply_freshness_ordering(results, &[]);
    super::scoring::sort_by_score_then_freshness(results);
}

// ---------------------------------------------------------------------------
// Auto-strengthen on access (Testing Effect, Roediger & Karpicke 2006)
// ---------------------------------------------------------------------------
async fn strengthen_on_access(storage: &Arc<Storage>, scoring: &ScoringOutput) {
    let storage_clone = Arc::clone(storage);
    let ids_owned: Vec<String> = scoring.results.iter().map(|r| r.node.id.clone()).collect();
    let _ = tokio::task::spawn_blocking(move || {
        let ids: Vec<&str> = ids_owned.iter().map(|s| s.as_str()).collect();
        storage_clone.strengthen_batch_on_access(&ids)
    })
    .await;
}

// ---------------------------------------------------------------------------
// STAGE 7 — predictive memory + reconsolidation labile marking
// ---------------------------------------------------------------------------
async fn record_side_effects(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    scoring: &ScoringOutput,
) {
    let storage_access = storage.clone();
    let access_ids: Vec<String> = scoring.results.iter().map(|r| r.node.id.clone()).collect();
    let _ = tokio::task::spawn_blocking(move || {
        for id in &access_ids {
            if let Err(e) = storage_access.record_memory_access(id) {
                tracing::debug!(error = %e, memory_id = %id, "Failed to record memory access");
            }
        }
    })
    .await;

    let mut cog = cognitive.lock().await;
    let _ = cog.predictive_memory.record_query(&args.query, &[]);

    for result in &scoring.results {
        let _ = cog.predictive_memory.record_memory_access(
            &result.node.id,
            &result.node.content.chars().take(100).collect::<String>(),
            &result.node.tags,
        );

        cog.speculative_retriever.record_access(
            &result.node.id,
            None,
            Some(args.query.as_str()),
            None,
        );

        // 5-min reconsolidation window so subsequent edits update both
        // the snapshot and the working memory.
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

// ---------------------------------------------------------------------------
// LightMem-pattern token budget: fit → compress → spill into expandable
// ---------------------------------------------------------------------------
fn enforce_token_budget(
    mut formatted: Vec<Value>,
    args: &SearchArgs,
    detail_level: &str,
) -> (Vec<Value>, Vec<String>, Option<usize>) {
    let mut budget_expandable: Vec<String> = Vec::new();
    let Some(budget) = args.token_budget else {
        return (formatted, budget_expandable, None);
    };

    let max_budget: i32 = std::env::var("VESTIGE_MAX_TOKEN_BUDGET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let budget = budget.clamp(100, max_budget) as usize;
    let budget_chars = budget * 4;

    let total_size: usize = formatted
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default().len())
        .sum();

    if total_size > budget_chars && detail_level != "brief" {
        let ratio = budget_chars as f64 / total_size as f64;
        for result in &mut formatted {
            if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
                let compressed = compress_content(content, ratio);
                result["content"] = serde_json::Value::String(compressed);
            }
        }
    }

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

    (budgeted, budget_expandable, Some(used / 4))
}

// ---------------------------------------------------------------------------
// Metacognition (hit rate, confidence, knowledge gaps)
// ---------------------------------------------------------------------------
async fn record_metacognition(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &SearchArgs,
    results: &[vestige_core::SearchResult],
    response: &mut Value,
) {
    let mut cog = cognitive.lock().await;
    let avg_conf = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .map(|r| r.node.confidence().point)
            .sum::<f64>()
            / results.len() as f64
    };
    let topic = args.query.split_whitespace().next().unwrap_or("unknown");
    cog.metacognition
        .record_search(results.len(), avg_conf, topic);

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

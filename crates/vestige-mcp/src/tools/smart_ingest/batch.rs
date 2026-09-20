//! Batch-mode ingestion (up to 20 items per call).

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{ContentType, ImportanceContext, IngestInput, Storage};

use crate::cognitive::CognitiveEngine;

use super::anchors;
use super::args::BatchItem;
use super::post_ingest::run_post_ingest;
use super::self_contained::{detect_with_anchors, reject_response};

/// Execute batch mode: process up to 20 items, each with full cognitive pipeline.
///
/// Unlike the old `session_checkpoint` tool, batch mode runs the full cognitive
/// pre-ingest (importance scoring, intent detection) and post-ingest (synaptic
/// tagging, novelty update, hippocampal indexing) pipelines per item.
///
/// `actor` is the caller-supplied identity (`agent` / `session_id`) recorded on
/// every revision the batch appends, so a memory written by a batch carries the
/// same "who wrote this" as one written singly.
pub(super) async fn execute_batch(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    items: Vec<BatchItem>,
    global_force_create: bool,
    actor: Option<&str>,
) -> Result<Value, String> {
    if items.is_empty() {
        return Err("Items array cannot be empty".to_string());
    }
    if items.len() > 20 {
        return Err("Maximum 20 items per batch".to_string());
    }

    let mut results = Vec::new();
    let mut created = 0u32;
    // Only the `embeddings` + `vector-search` ingest branch below increments
    // `updated`, so the documented no-embeddings build mutates it nowhere.
    // Keep the counter (the response always reports it) and silence the
    // resulting `unused_mut` for that configuration only.
    #[cfg_attr(
        not(all(feature = "embeddings", feature = "vector-search")),
        allow(unused_mut)
    )]
    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;
    let mut rejected = 0u32;

    for (i, mut item) in items.into_iter().enumerate() {
        // An empty item is the gate's second refusal case, and it is refused
        // here in the same vocabulary as the first: `rejected`, never
        // `skipped`. "Skipped" reads as a queue that may be drained later,
        // which is exactly the wrong expectation for content that must not be
        // stored at all.
        if item.content.trim().is_empty() {
            results.push(serde_json::json!({
                "index": i,
                "status": "rejected",
                "decision": "reject",
                "stored": false,
                "reason": "Empty content — there is nothing to remember",
                "guidance": "Nothing was written. Save the lesson or the decision this was meant \
                             to record instead (see AGENTS.md, Mandatory Save Gates)."
            }));
            rejected += 1;
            continue;
        }

        // Skip content > 1MB
        if item.content.len() > 1_000_000 {
            results.push(serde_json::json!({
                "index": i,
                "status": "skipped",
                "reason": "Content too large (max 1MB)"
            }));
            skipped += 1;
            continue;
        }

        // Extract per-item force_create before consuming other fields
        let item_force_create = item.force_create.unwrap_or(false);
        let explicit_anchors = std::mem::take(&mut item.code_refs);

        // ================================================================
        // COGNITIVE PRE-INGEST (per item) — see signal-routing note on the
        // single-mode path above. emotional_arousal vs importance_composite
        // must stay separate. Uses blocking lock for the same reason as the
        // single-mode path.
        // ================================================================
        let mut tags = item.tags.unwrap_or_default();

        let (importance_composite, emotional_arousal) = {
            let cog = cognitive.lock().await;
            let context = ImportanceContext::current();
            let importance = cog
                .importance_signals
                .compute_importance(&item.content, &context);

            let intent_result = cog.intent_detector.detect_intent();
            if intent_result.confidence > 0.5 {
                let intent_tag = format!("intent:{:?}", intent_result.primary_intent);
                let intent_tag = if intent_tag.len() > 50 {
                    format!("{}...", &intent_tag[..intent_tag.floor_char_boundary(47)])
                } else {
                    intent_tag
                };
                tags.push(intent_tag);
            }

            let _content_type = ContentType::detect(&item.content);

            (importance.composite, importance.arousal)
        };

        // ============================================================
        // PREPROCESSING PIPELINE (per batch item)
        // ============================================================
        #[cfg(feature = "preprocessing")]
        let (
            item_content,
            batch_pp_tags,
            batch_pp_from,
            batch_pp_until,
            _batch_pp_rels,
            batch_pp_prov,
        ) = {
            let config = vestige_core::preprocessing::PreprocessingConfig::default();
            let pp = vestige_core::preprocessing::preprocess(&item.content, &config);
            (
                pp.content,
                pp.auto_tags,
                pp.valid_from,
                pp.valid_until,
                pp.relations,
                Some(pp.provenance.to_json()),
            )
        };
        // The tuple is annotated because with `preprocessing` off the last slot
        // is a bare `None`, and nothing downstream pinned its type: the closure
        // at `batch_pp_prov.map(...)` calls `as_object_mut()`, which serde_json
        // implements for `Value` but not through an unconstrained type variable.
        // That was the `E0282` in the no-default-features build.
        #[cfg(not(feature = "preprocessing"))]
        let (item_content, batch_pp_tags, batch_pp_from, batch_pp_until, batch_pp_prov): (
            String,
            Vec<String>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
        ) = (item.content.clone(), Vec::new(), None, None, None);

        tags.extend(batch_pp_tags);

        // Anchors, exactly as in single mode: explicit first, then the paths the
        // stored text names. Collected before the gate so the gate can see which
        // of its `bare_code_reference` findings this write already answered.
        let item_anchors = anchors::resolve(anchors::collect(&explicit_anchors, &item_content));
        let anchored_paths = anchors::anchored_paths(&item_anchors);

        // The same gate the single-item path runs, on the preprocessed text and
        // for the same reason: a pronoun that survived coreference rewriting is
        // one the rewriter could not resolve. It has to run here as well, or
        // batch mode would be an open door past the one refusal that exists.
        let self_contained = detect_with_anchors(
            &item_content,
            batch_pp_from.is_some() || batch_pp_until.is_some(),
            &anchored_paths,
        );
        if let Some(mut refusal) = reject_response(&self_contained) {
            refusal["index"] = serde_json::json!(i);
            refusal["status"] = serde_json::json!("rejected");
            results.push(refusal);
            rejected += 1;
            continue;
        }

        let provenance_with_importance = batch_pp_prov.map(|mut p| {
            if let Some(obj) = p.as_object_mut() {
                obj.insert(
                    "importance_composite".to_string(),
                    serde_json::Value::from(importance_composite),
                );
            }
            p
        });

        let input = IngestInput {
            content: item_content,
            node_type: item.node_type.unwrap_or_else(|| "fact".to_string()),
            source: item.source,
            sentiment_score: 0.0,
            sentiment_magnitude: emotional_arousal,
            tags,
            valid_from: batch_pp_from,
            valid_until: batch_pp_until,
            provenance: provenance_with_importance,
            self_contained: Some(self_contained.marker()),
            self_contained_findings: self_contained.findings_json(),
            actor: actor.map(str::to_string),
            anchors: item_anchors,
            ..Default::default()
        };

        // ================================================================
        // INGEST (storage lock per item)
        // ================================================================

        // Check force_create: global flag OR per-item flag
        let item_force = global_force_create || item_force_create;
        if item_force {
            let storage_item = storage.clone();
            let input_item = input.clone();
            let ingest_outcome =
                tokio::task::spawn_blocking(move || storage_item.ingest(input_item))
                    .await
                    .map_err(|e| format!("batch ingest task panicked: {}", e))?;
            match ingest_outcome {
                Ok(node) => {
                    let node_id = node.id.clone();
                    let node_content = node.content.clone();
                    let node_type = node.node_type.clone();

                    created += 1;
                    run_post_ingest(
                        cognitive,
                        &node_id,
                        &node_content,
                        &node_type,
                        importance_composite,
                    );

                    let mut saved = serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": "create",
                        "nodeId": node_id,
                        "importanceScore": importance_composite,
                        "reason": "Forced creation - skipped similarity check"
                    });
                    if !input.anchors.is_empty() {
                        saved["anchors"] = anchors::response_anchors(&input.anchors);
                    }
                    results.push(saved);
                }
                Err(e) => {
                    errors += 1;
                    results.push(serde_json::json!({
                        "index": i,
                        "status": "error",
                        "reason": e.to_string()
                    }));
                }
            }
            continue;
        }

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        {
            let storage_si = storage.clone();
            let input_si = input.clone();
            let ingest_outcome =
                tokio::task::spawn_blocking(move || storage_si.smart_ingest(input_si))
                    .await
                    .map_err(|e| format!("batch smart_ingest task panicked: {}", e))?;
            match ingest_outcome {
                Ok(result) => {
                    let node_id = result.node.id.clone();
                    let node_content = result.node.content.clone();
                    let node_type = result.node.node_type.clone();

                    match result.decision.as_str() {
                        "create" | "supersede" | "replace" => created += 1,
                        "update" | "reinforce" | "merge" | "add_context" => updated += 1,
                        _ => created += 1,
                    }

                    // Post-ingest cognitive side effects
                    run_post_ingest(
                        cognitive,
                        &node_id,
                        &node_content,
                        &node_type,
                        importance_composite,
                    );

                    let mut saved = serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": result.decision,
                        "nodeId": node_id,
                        "similarity": result.similarity,
                        "importanceScore": importance_composite,
                        "reason": result.reason
                    });
                    if !input.anchors.is_empty() {
                        saved["anchors"] = anchors::response_anchors(&input.anchors);
                    }
                    results.push(saved);
                }
                Err(e) => {
                    errors += 1;
                    results.push(serde_json::json!({
                        "index": i,
                        "status": "error",
                        "reason": e.to_string()
                    }));
                }
            }
        }

        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        {
            let storage_clone = storage.clone();
            let input_clone = input.clone();
            let ingest_outcome =
                tokio::task::spawn_blocking(move || storage_clone.ingest(input_clone))
                    .await
                    .map_err(|e| format!("batch ingest fallback task panicked: {}", e))?;
            match ingest_outcome {
                Ok(node) => {
                    let node_id = node.id.clone();
                    let node_content = node.content.clone();
                    let node_type = node.node_type.clone();

                    created += 1;
                    run_post_ingest(
                        cognitive,
                        &node_id,
                        &node_content,
                        &node_type,
                        importance_composite,
                    );

                    let mut saved = serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": "create",
                        "nodeId": node_id,
                        "importanceScore": importance_composite,
                        "reason": "Embeddings not available - used regular ingest"
                    });
                    if !input.anchors.is_empty() {
                        saved["anchors"] = anchors::response_anchors(&input.anchors);
                    }
                    results.push(saved);
                }
                Err(e) => {
                    errors += 1;
                    results.push(serde_json::json!({
                        "index": i,
                        "status": "error",
                        "reason": e.to_string()
                    }));
                }
            }
        }
    }

    Ok(serde_json::json!({
        "success": errors == 0,
        "mode": "batch",
        "summary": {
            "total": results.len(),
            "created": created,
            "updated": updated,
            "skipped": skipped,
            // Refusals are reported separately from `skipped` (too large) and
            // from `errors`: a refusal is the gate doing its job, so a batch of
            // one good item and one refusal is a success with a rejected item,
            // not a failure.
            "rejected": rejected,
            "errors": errors
        },
        "results": results
    }))
}

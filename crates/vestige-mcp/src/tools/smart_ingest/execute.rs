//! Single-mode `execute` entry point (also dispatches to batch mode).

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{ContentType, ImportanceContext, IngestInput, Storage};

use crate::cognitive::CognitiveEngine;

use super::args::{SmartIngestArgs, actor_of};
use super::batch::execute_batch;
use super::compound::detect_compound_content;
#[cfg(feature = "preprocessing")]
use super::post_ingest::create_relation_edges;
use super::post_ingest::{run_post_ingest, run_post_ingest_with_neighbors};
use super::write_preparation::{FileRefs, prepare};

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let mut args: SmartIngestArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    // Who is writing. Built before any field of `args` is consumed, and used for
    // every revision this call appends — the actor is a property of the write,
    // not of each individual step of it.
    let actor = actor_of(args.agent.as_deref(), args.session_id.as_deref());

    // Detect mode: batch (items present) vs single (content present)
    if let Some(items) = args.items {
        let global_force = args.force_create.unwrap_or(false);
        return execute_batch(storage, cognitive, items, global_force, actor.as_deref()).await;
    }

    // Anchors are collected before the gate runs, because the gate's
    // `bare_code_reference` rule fires on a path *without* an anchor and has to
    // see the anchors this write is actually going to store. Both steps live in
    // `write_preparation`, which is also what every other write path calls, so
    // the ordering cannot drift between them.
    let explicit_anchors = std::mem::take(&mut args.code_refs);

    // Single mode: content is required
    let content = args.content.ok_or(
        "Missing 'content' field. Provide 'content' for single mode or 'items' for batch mode.",
    )?;

    // Validate content. Empty content is refused here rather than turned into a
    // `Reject` decision: it never reaches the gate (there is nothing to
    // analyse), and the transport already carries it as a tool error, which is
    // the louder refusal of the two. The message still names the alternative,
    // because "Content cannot be empty" alone invites the caller to pad the
    // string until it is accepted.
    if content.trim().is_empty() {
        return Err(
            "Content cannot be empty — there is nothing to remember, and nothing was written. \
             Save the lesson or the decision this was meant to record instead (see AGENTS.md, \
             Mandatory Save Gates)."
                .to_string(),
        );
    }

    if content.len() > 1_000_000 {
        return Err("Content too large (max 1MB)".to_string());
    }

    // Detect compound content and generate advisory
    let compound_warning = detect_compound_content(&content);

    // ====================================================================
    // COGNITIVE PRE-INGEST: importance scoring + intent detection + content analysis
    //
    // NOTE on signal routing (fixed in v3.2.1):
    //   - `sentiment_magnitude` on IngestInput is EMOTIONAL arousal (0..1).
    //     Used downstream for emotional sentiment boost in FSRS, hippocampal
    //     index, synaptic tagging — must reflect real emotional content.
    //   - `importance_composite` is a 4-channel composite (novelty + arousal
    //     + reward + attention). It governs CREATE vs UPDATE vs SUPERSEDE in
    //     prediction error gating and is preserved in provenance for audit.
    //   - Pre v3.2.1 the composite was stored as sentiment_magnitude, which
    //     overdrove emotional boosts for any high-importance memory regardless
    //     of actual emotion. Now we split them.
    // ====================================================================
    let mut tags = args.tags.unwrap_or_default();

    // We block on the cognitive lock for pre-ingest. Pre v3.2.1 this used
    // try_lock and silently dropped importance scoring + intent detection
    // when another async task held the engine — producing memories whose
    // quality signals defaulted to zero. The ingest itself is going to
    // serialize on Storage anyway; serializing on the cognitive engine for
    // a few hundred microseconds of CPU work is a strictly worse trade-off
    // than silently corrupting the quality channels.
    let (importance_composite, emotional_arousal) = {
        let cog = cognitive.lock().await;
        // 4A. Full 4-channel importance scoring (Anderson 1983, Yonelinas 2002,
        //     LaBar & Cabeza 2006).
        let context = ImportanceContext::current();
        let importance = cog
            .importance_signals
            .compute_importance(&content, &context);

        // 4B. Intent detection → auto-tag
        let intent_result = cog.intent_detector.detect_intent();
        if intent_result.confidence > 0.5 {
            let intent_tag = format!("intent:{:?}", intent_result.primary_intent);
            // Truncate long intent tags
            let intent_tag = if intent_tag.len() > 50 {
                format!("{}...", &intent_tag[..intent_tag.floor_char_boundary(47)])
            } else {
                intent_tag
            };
            tags.push(intent_tag);
        }

        // 4D. Adaptive embedding — detect content type for logging
        let _content_type = ContentType::detect(&content);

        (importance.composite, importance.arousal)
    };

    // ====================================================================
    // PREPROCESSING PIPELINE: coreference, entities, temporal, relations, provenance
    // ====================================================================
    #[cfg(feature = "preprocessing")]
    let (content, pp_tags, pp_valid_from, pp_valid_until, pp_relations, pp_provenance) = {
        let config = vestige_core::preprocessing::PreprocessingConfig {
            session_id: args.session_id.clone(),
            agent: args.agent.clone(),
            existing_valid_from: None,
            existing_valid_until: None,
        };
        let pp = vestige_core::preprocessing::preprocess(&content, &config);
        (
            pp.content,
            pp.auto_tags,
            pp.valid_from,
            pp.valid_until,
            pp.relations,
            Some(pp.provenance.to_json()),
        )
    };
    #[cfg(not(feature = "preprocessing"))]
    let (pp_tags, pp_valid_from, pp_valid_until, pp_provenance): (
        Vec<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<serde_json::Value>,
    ) = (Vec::new(), None, None, None);
    #[cfg(not(feature = "preprocessing"))]
    let pp_relations: Vec<()> = Vec::new();

    // Both write-time steps, over the text that is actually stored: the
    // preprocessed content, which is what a reader will see and what the paths
    // in it refer to. Anchors are resolved (and hashed) before the ingest so no
    // repository walk happens inside the SQLite write transaction; the gate then
    // runs on that same text with the anchored paths in hand, so a path this
    // write anchored is not reported as a bare reference.
    let prepared = prepare(
        &content,
        FileRefs::from(explicit_anchors.as_slice()),
        pp_valid_from.is_some() || pp_valid_until.is_some(),
    );

    // The gate's one non-negotiable outcome, and the only place it can be
    // enforced correctly: three write paths leave this function below (forced
    // create, prediction-error `smart_ingest`, the no-embeddings fallback), and
    // a check inside any single one of them would leave the other two free to
    // store a copy of the repository. Nothing has been written yet — the first
    // write is the storage call after this return.
    if let Some(refusal) = prepared.refusal {
        return Ok(refusal);
    }

    // Merge auto-tags from preprocessing with user-provided tags
    tags.extend(pp_tags);

    // Inject importance_composite into provenance for audit/observability.
    // sentiment_magnitude carries emotional arousal (its proper semantics).
    let provenance_with_importance = pp_provenance.map(|mut p| {
        if let Some(obj) = p.as_object_mut() {
            obj.insert(
                "importance_composite".to_string(),
                serde_json::Value::from(importance_composite),
            );
        }
        p
    });

    let input = IngestInput {
        content: content.clone(),
        node_type: args.node_type.unwrap_or_else(|| "fact".to_string()),
        source: args.source,
        sentiment_score: 0.0,
        sentiment_magnitude: emotional_arousal,
        tags,
        valid_from: pp_valid_from,
        valid_until: pp_valid_until,
        provenance: provenance_with_importance,
        // The gate ran on this content, so the verdict is recorded with the
        // memory instead of only in a response the caller may not keep. Without
        // it, a memory that needs its conversation is indistinguishable from one
        // that stands alone the moment the response is gone.
        self_contained: Some(prepared.marker),
        self_contained_findings: prepared.findings.clone(),
        actor,
        anchors: prepared.anchors.clone(),
        ..Default::default()
    };

    // Store relations for post-ingest graph edge creation
    let _pp_relations = pp_relations;

    // ====================================================================
    // INGEST (storage lock)
    // ====================================================================

    // Check if force_create is enabled
    if args.force_create.unwrap_or(false) {
        // Run duplicate-similarity probe + ingest together on the blocking
        // pool — both touch SQLite, both can briefly block the runtime.
        //
        // Embedding strategy: we embed `input.content` exactly once and
        // hand that vector to both the nearest-neighbour probe and the
        // ingest write. Pre-2026-05-20 this path did two full embeds
        // back-to-back (one inside `semantic_search_raw`, one inside
        // `ingest`) — a measurable cost when forced creation is used in
        // bulk imports.
        let storage_fc = storage.clone();
        let input_fc = input.clone();
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        let (nearest_sim, node) = tokio::task::spawn_blocking(
            move || -> Result<(Option<f64>, vestige_core::KnowledgeNode), String> {
                let embedding = if storage_fc.embedding_service_ready() {
                    storage_fc.embed_text(&input_fc.content).ok()
                } else {
                    None
                };
                let nearest_sim = match embedding.as_ref() {
                    Some(emb) => storage_fc
                        .semantic_search_by_embedding(&emb.vector, 1)
                        .ok()
                        .and_then(|results| results.into_iter().next())
                        .map(|(_, score)| score as f64),
                    None => None,
                };
                let node = match embedding.as_ref() {
                    Some(emb) => storage_fc.ingest_with_embedding(input_fc, emb),
                    None => storage_fc.ingest(input_fc),
                }
                .map_err(|e| e.to_string())?;
                Ok((nearest_sim, node))
            },
        )
        .await
        .map_err(|e| format!("smart_ingest force_create task panicked: {}", e))??;
        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        let (nearest_sim, node): (Option<f64>, vestige_core::KnowledgeNode) =
            tokio::task::spawn_blocking(move || {
                let node = storage_fc.ingest(input_fc).map_err(|e| e.to_string())?;
                Ok::<_, String>((None, node))
            })
            .await
            .map_err(|e| format!("smart_ingest force_create task panicked: {}", e))??;

        let node_id = node.id.clone();
        let node_content = node.content.clone();
        let node_type = node.node_type.clone();
        let has_embedding = node.has_embedding.unwrap_or(false);

        run_post_ingest(
            cognitive,
            &node_id,
            &node_content,
            &node_type,
            importance_composite,
        );

        // Force-create has no prediction-error gate, but the relation
        // extractor STILL needs to publish causal/semantic edges so the
        // knowledge graph stays consistent with the smart-ingest path.
        // Without this, force-created memories were invisible in the
        // causal-chain tool until the next dream cycle.
        #[cfg(feature = "preprocessing")]
        create_relation_edges(cognitive, storage, &node_id, &_pp_relations).await;

        let mut response = serde_json::json!({
            "success": true,
            "decision": "create",
            "nodeId": node_id,
            "message": "Memory created (force_create=true)",
            "hasEmbedding": has_embedding,
            "predictionError": 1.0,
            "importanceScore": importance_composite,
            "reason": "Forced creation - skipped similarity check"
        });
        if let Some(warning) = &compound_warning {
            response["compound_content_warning"] = serde_json::json!(warning);
        }
        if let Some(anchors) = prepared.response_anchors() {
            response["anchors"] = anchors;
        }
        // Attached only when the gate found something: `ok: true` on every
        // response would be noise that hides the flagged ones.
        if let Some(marker) = prepared.response_marker() {
            response["self_contained"] = marker;
        }
        if let Some(sim) = nearest_sim
            && sim > 0.9
        {
            response["near_duplicate_warning"] = serde_json::json!(format!(
                "Content is {:.0}% similar to an existing memory. This was created because force_create=true, but consider using memory(action='edit') instead.",
                sim * 100.0
            ));
        }
        return Ok(response);
    }

    // Use smart ingest with prediction error gating
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        // Run smart_ingest on the blocking pool — it touches SQLite and the
        // vector index. `smart_ingest` now returns the nearest-neighbour IDs
        // it already evaluated for the prediction-error gate, so we don't
        // need a second `semantic_search_raw` here (which would embed
        // `input.content` for a third time after the gate's own embed and
        // the post-ingest write-side embed inside `ingest`).
        let storage_si = storage.clone();
        let input_si = input.clone();
        let result = tokio::task::spawn_blocking(
            move || -> Result<vestige_core::SmartIngestResult, String> {
                storage_si.smart_ingest(input_si).map_err(|e| e.to_string())
            },
        )
        .await
        .map_err(|e| format!("smart_ingest task panicked: {}", e))??;
        // Trim to the 5 closest, matching the previous behaviour where the
        // MCP layer requested `limit: 5` from `semantic_search_raw`. The
        // gate considers up to 10 (for higher-quality merge/supersede
        // decisions); the post-ingest cognitive side effects only care
        // about the top 5.
        let neighbor_ids: Vec<String> = result.neighbor_ids.iter().take(5).cloned().collect();
        let node_id = result.node.id.clone();
        let node_content = result.node.content.clone();
        let node_type = result.node.node_type.clone();
        let has_embedding = result.node.has_embedding.unwrap_or(false);

        // Post-ingest cognitive side effects + prospective indexing
        run_post_ingest_with_neighbors(
            cognitive,
            &node_id,
            &node_content,
            &node_type,
            importance_composite,
            &neighbor_ids,
        );

        // Create activation-network edges from extracted relations
        #[cfg(feature = "preprocessing")]
        create_relation_edges(cognitive, storage, &node_id, &_pp_relations).await;

        let mut response = serde_json::json!({
            "success": true,
            "decision": result.decision,
            "nodeId": node_id,
            "message": format!("Smart ingest complete: {}", result.reason),
            "hasEmbedding": has_embedding,
            "similarity": result.similarity,
            "predictionError": result.prediction_error,
            "supersededId": result.superseded_id,
            "importanceScore": importance_composite,
            "reason": result.reason,
            "explanation": match result.decision.as_str() {
                "create" => "Created new memory - content was different enough from existing memories",
                "update" => "Updated existing memory - content was similar to an existing memory",
                "reinforce" => "Reinforced existing memory - content was nearly identical",
                "supersede" => "Superseded old memory - new content is an improvement/correction",
                "merge" => "Merged with related memories - content connects multiple topics",
                "replace" => "Replaced existing memory content entirely",
                "add_context" => "Added new content as context to existing memory",
                _ => "Memory processed successfully"
            }
        });
        if let Some(warning) = &compound_warning {
            response["compound_content_warning"] = serde_json::json!(warning);
        }
        if let Some(anchors) = prepared.response_anchors() {
            response["anchors"] = anchors;
        }
        // Attached only when the gate found something: `ok: true` on every
        // response would be noise that hides the flagged ones.
        if let Some(marker) = prepared.response_marker() {
            response["self_contained"] = marker;
        }
        // A reported contradiction travels with the response, or the writer never
        // learns that the memory it just saved may deny one it already had. The
        // report names the memory, how similar the two are, how strong the
        // evidence is, what fired it and what to do — because the one thing the
        // write path deliberately did not do is retire anything.
        if let Some(contradiction) = &result.contradiction
            && let Ok(report) = serde_json::to_value(contradiction)
        {
            response["contradiction"] = report;
        }
        // Near-duplicate advisory threshold.
        //
        // The prediction-error gate stores anything above 0.75 cosine as its own
        // memory and links it to the neighbour (nothing rewrites an existing
        // memory's text any more), so `decision == "create"` with a high
        // similarity is the normal outcome for "same project, different claim"
        // and exactly the case a writer should look at: it may have meant to
        // edit. The gate reports the similarity on the create path for this.
        const NEAR_DUPLICATE_ADVISORY_THRESHOLD: f32 = 0.6;
        if let Some(sim) = result.similarity
            && sim >= NEAR_DUPLICATE_ADVISORY_THRESHOLD
            && result.decision == "create"
        {
            response["near_duplicate_warning"] = serde_json::json!(format!(
                "Content is {:.0}% similar to an existing memory but was created as new. Consider using memory(action='edit') if you intended to update an existing memory.",
                sim * 100.0
            ));
        }
        Ok(response)
    }

    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let storage_clone = storage.clone();
        let input_clone = input.clone();
        let node = tokio::task::spawn_blocking(move || {
            storage_clone.ingest(input_clone).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("smart_ingest fallback task panicked: {}", e))??;
        let node_id = node.id.clone();
        let node_content = node.content.clone();
        let node_type = node.node_type.clone();

        run_post_ingest(
            cognitive,
            &node_id,
            &node_content,
            &node_type,
            importance_composite,
        );

        let mut response = serde_json::json!({
            "success": true,
            "decision": "create",
            "nodeId": node_id,
            "message": "Memory created (smart ingest requires embeddings feature)",
            "hasEmbedding": false,
            "predictionError": 1.0,
            "importanceScore": importance_composite,
            "reason": "Embeddings not available - used regular ingest"
        });
        if let Some(warning) = &compound_warning {
            response["compound_content_warning"] = serde_json::json!(warning);
        }
        if let Some(anchors) = prepared.response_anchors() {
            response["anchors"] = anchors;
        }
        // Attached only when the gate found something: `ok: true` on every
        // response would be noise that hides the flagged ones.
        if let Some(marker) = prepared.response_marker() {
            response["self_contained"] = marker;
        }
        Ok(response)
    }
}

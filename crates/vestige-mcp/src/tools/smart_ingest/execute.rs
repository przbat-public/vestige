//! Single-mode `execute` entry point (also dispatches to batch mode).

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{ContentType, ImportanceContext, IngestInput, Storage};

use crate::cognitive::CognitiveEngine;

use super::args::SmartIngestArgs;
use super::batch::execute_batch;
use super::compound::detect_compound_content;
#[cfg(feature = "preprocessing")]
use super::post_ingest::create_relation_edges;
use super::post_ingest::{run_post_ingest, run_post_ingest_with_neighbors};

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: SmartIngestArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    // Detect mode: batch (items present) vs single (content present)
    if let Some(items) = args.items {
        let global_force = args.force_create.unwrap_or(false);
        return execute_batch(storage, cognitive, items, global_force).await;
    }

    // Single mode: content is required
    let content = args.content.ok_or(
        "Missing 'content' field. Provide 'content' for single mode or 'items' for batch mode.",
    )?;

    // Validate content
    if content.trim().is_empty() {
        return Err("Content cannot be empty".to_string());
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
        // Near-duplicate advisory threshold.
        //
        // The prediction-error gate routes anything with cosine >= 0.75 to
        // Update/Reinforce/Supersede/Merge under the default config
        // (prefer_updates=true). So a `decision == "create"` outcome with
        // sim > 0.9 is effectively impossible — the old `> 0.9` check was
        // dead code. Lower the bar to 0.6 (still high enough to surface
        // "you almost hit Update territory but the gate landed on Create"
        // cases that warrant a manual look).
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
        Ok(response)
    }
}

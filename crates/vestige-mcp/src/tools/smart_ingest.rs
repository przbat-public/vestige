//! Smart Ingest Tool
//!
//! Intelligent memory ingestion with Prediction Error Gating.
//! Automatically decides whether to create, update, or supersede memories
//! based on semantic similarity to existing content.
//!
//! This solves the "bad vs good similar memory" problem by:
//! - Detecting when new content is similar to existing memories
//! - Updating existing memories when appropriate (low prediction error)
//! - Creating new memories when content is substantially different (high PE)
//! - Superseding demoted/outdated memories with better alternatives
//!
//! v1.5.0: Enhanced with cognitive pipeline:
//!   Pre-ingest: importance scoring (4-channel) + intent detection → auto-tag
//!   Post-ingest: synaptic tagging + novelty model update + hippocampal indexing

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{
    ContentType, ImportanceContext, ImportanceEvent, ImportanceEventType, IngestInput, Storage,
};

/// Input schema for smart_ingest tool
///
/// Supports two modes:
/// - **Single mode**: provide `content` (required) + optional fields
/// - **Batch mode**: provide `items` array (max 20), each with full cognitive pipeline
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "content": {
                "type": "string",
                "description": "The content to remember. MUST be atomic: one fact, one decision, one event per call. Multi-topic content triggers compound_content_warning — split into batch items instead. (Single mode)"
            },
            "node_type": {
                "type": "string",
                "description": "Type of knowledge: fact, concept, event, person, place, note, pattern, decision",
                "default": "fact"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Tags for categorization"
            },
            "source": {
                "type": "string",
                "description": "Source or reference for this knowledge"
            },
            "forceCreate": {
                "type": "boolean",
                "description": "Force creation of a new memory even if similar content exists",
                "default": false
            },
            "session_id": {
                "type": "string",
                "description": "Session/conversation identifier for provenance tracking"
            },
            "agent": {
                "type": "string",
                "description": "Agent identifier (e.g. 'cursor', 'claude') for provenance tracking"
            },
            "items": {
                "type": "array",
                "description": "Batch mode: array of items to save (max 20). Each runs through full cognitive pipeline with Prediction Error Gating. Use at session end or before context compaction.",
                "maxItems": 20,
                "items": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The content to remember"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tags for categorization"
                        },
                        "node_type": {
                            "type": "string",
                            "description": "Type: fact, concept, event, person, place, note, pattern, decision",
                            "default": "fact"
                        },
                        "source": {
                            "type": "string",
                            "description": "Source reference"
                        },
                        "forceCreate": {
                            "type": "boolean",
                            "description": "Force creation of this item even if similar content exists",
                            "default": false
                        }
                    },
                    "required": ["content"]
                }
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmartIngestArgs {
    content: Option<String>,
    #[serde(alias = "node_type")]
    node_type: Option<String>,
    tags: Option<Vec<String>>,
    source: Option<String>,
    #[serde(alias = "force_create")]
    force_create: Option<bool>,
    items: Option<Vec<BatchItem>>,
    /// Session identifier for provenance tracking (e.g. conversation ID)
    #[serde(alias = "session_id")]
    session_id: Option<String>,
    /// Agent identifier for provenance tracking (e.g. "cursor", "claude")
    agent: Option<String>,
}

/// A single item in batch mode
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchItem {
    content: String,
    tags: Option<Vec<String>>,
    #[serde(alias = "node_type")]
    node_type: Option<String>,
    source: Option<String>,
    #[serde(alias = "force_create")]
    force_create: Option<bool>,
}

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
        let storage_fc = storage.clone();
        let input_fc = input.clone();
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        let (nearest_sim, node) = tokio::task::spawn_blocking(
            move || -> Result<(Option<f64>, vestige_core::KnowledgeNode), String> {
                let nearest_sim = storage_fc
                    .semantic_search_raw(&input_fc.content, 1)
                    .ok()
                    .and_then(|results| results.into_iter().next())
                    .map(|(_, score)| score as f64);
                let node = storage_fc.ingest(input_fc).map_err(|e| e.to_string())?;
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
        // Run nearest-neighbor lookup + smart_ingest together on the
        // blocking pool — both call into SQLite/vector-search.
        let storage_si = storage.clone();
        let input_si = input.clone();
        let (neighbor_ids, result) = tokio::task::spawn_blocking(
            move || -> Result<(Vec<String>, vestige_core::SmartIngestResult), String> {
                let neighbor_ids: Vec<String> = storage_si
                    .semantic_search_raw(&input_si.content, 5)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect();
                let result = storage_si
                    .smart_ingest(input_si)
                    .map_err(|e| e.to_string())?;
                Ok((neighbor_ids, result))
            },
        )
        .await
        .map_err(|e| format!("smart_ingest task panicked: {}", e))??;
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

/// Execute batch mode: process up to 20 items, each with full cognitive pipeline.
///
/// Unlike the old `session_checkpoint` tool, batch mode runs the full cognitive
/// pre-ingest (importance scoring, intent detection) and post-ingest (synaptic
/// tagging, novelty update, hippocampal indexing) pipelines per item.
async fn execute_batch(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    items: Vec<BatchItem>,
    global_force_create: bool,
) -> Result<Value, String> {
    if items.is_empty() {
        return Err("Items array cannot be empty".to_string());
    }
    if items.len() > 20 {
        return Err("Maximum 20 items per batch".to_string());
    }

    let mut results = Vec::new();
    let mut created = 0u32;
    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    for (i, item) in items.into_iter().enumerate() {
        // Skip empty content
        if item.content.trim().is_empty() {
            results.push(serde_json::json!({
                "index": i,
                "status": "skipped",
                "reason": "Empty content"
            }));
            skipped += 1;
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
        #[cfg(not(feature = "preprocessing"))]
        let (item_content, batch_pp_tags, batch_pp_from, batch_pp_until, batch_pp_prov) =
            (item.content.clone(), Vec::<String>::new(), None, None, None);

        tags.extend(batch_pp_tags);

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

                    results.push(serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": "create",
                        "nodeId": node_id,
                        "importanceScore": importance_composite,
                        "reason": "Forced creation - skipped similarity check"
                    }));
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

                    results.push(serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": result.decision,
                        "nodeId": node_id,
                        "similarity": result.similarity,
                        "importanceScore": importance_composite,
                        "reason": result.reason
                    }));
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

                    results.push(serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": "create",
                        "nodeId": node_id,
                        "importanceScore": importance_composite,
                        "reason": "Embeddings not available - used regular ingest"
                    }));
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
            "errors": errors
        },
        "results": results
    }))
}

/// Detects content that likely contains multiple distinct topics and should be split
/// into atomic memories. Returns a warning message if compound content is detected.
fn detect_compound_content(content: &str) -> Option<String> {
    let len = content.len();
    if len < 300 {
        return None;
    }

    let mut signals: Vec<&str> = Vec::new();

    let lines: Vec<&str> = content.lines().collect();
    let paragraph_count = content
        .split("\n\n")
        .filter(|p| p.trim().len() > 30)
        .count();
    if paragraph_count >= 3 {
        signals.push("multiple paragraphs covering different topics");
    }

    let speaker_pattern_count = lines
        .iter()
        .filter(|l| {
            let trimmed = l.trim();
            // "Speaker: text" or "Speaker Name: text"
            if let Some(colon_pos) = trimmed.find(':') {
                let before_colon = &trimmed[..colon_pos];
                colon_pos < 40
                    && !before_colon.is_empty()
                    && before_colon
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
                    && trimmed.len() > colon_pos + 5
            } else {
                false
            }
        })
        .count();
    if speaker_pattern_count >= 3 {
        signals.push("conversation transcript (multiple 'Speaker: text' lines)");
    }

    let bullet_count = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("- ")
                || t.starts_with("* ")
                || t.starts_with("• ")
                || (t.len() > 3
                    && t.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && (t.contains(". ") || t.contains(") ")))
        })
        .count();
    if bullet_count >= 4 {
        signals.push("bulleted list with multiple distinct items");
    }

    let topic_shift_indicators = [
        "also,",
        "additionally,",
        "on another note",
        "separately,",
        "moving on",
        "another thing",
        "by the way",
        "btw,",
        "oh and",
        "also worth noting",
        "furthermore,",
        "in other news",
        "on a different topic",
    ];
    let topic_shifts = lines
        .iter()
        .filter(|l| {
            let lower = l.to_lowercase();
            topic_shift_indicators.iter().any(|ind| lower.contains(ind))
        })
        .count();
    if topic_shifts >= 2 {
        signals.push("topic-shift phrases detected");
    }

    if signals.is_empty() {
        return None;
    }

    let reason = signals.join("; ");
    Some(format!(
        "⚠️ COMPOUND CONTENT DETECTED: This memory contains {}. \
         For better search recall, split into separate atomic memories using batch mode. \
         Each memory should contain ONE fact, decision, or event. \
         Example: instead of one memory with 5 bullet points, use `items` array with 5 separate entries.",
        reason
    ))
}

/// Cognitive post-ingest side effects: synaptic tagging, novelty update, hippocampal indexing.
///
/// Uses try_lock() for non-blocking access. If cognitive is locked, side effects are skipped.
fn run_post_ingest(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    node_id: &str,
    content: &str,
    node_type: &str,
    importance_composite: f64,
) {
    run_post_ingest_with_neighbors(
        cognitive,
        node_id,
        content,
        node_type,
        importance_composite,
        &[],
    );
}

/// Extended post-ingest that also creates activation-network edges
/// to the new memory's vector-space neighbors (prospective indexing).
fn run_post_ingest_with_neighbors(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    node_id: &str,
    content: &str,
    node_type: &str,
    importance_composite: f64,
    neighbor_ids: &[String],
) {
    if let Ok(mut cog) = cognitive.try_lock() {
        if importance_composite > 0.3 {
            cog.synaptic_tagging.tag_memory(node_id);
            if importance_composite > 0.7 {
                let event = ImportanceEvent::for_memory(node_id, ImportanceEventType::NoveltySpike);
                let _capture = cog.synaptic_tagging.trigger_prp(event);
            }
        }

        cog.importance_signals.learn_content(content);

        if let Err(e) =
            cog.hippocampal_index
                .index_memory(node_id, content, node_type, Utc::now(), None)
        {
            tracing::warn!(error = %e, node_id = %node_id, "Failed to index memory in hippocampal index");
        }

        cog.cross_project
            .record_project_memory(node_id, "default", None);

        // Prospective indexing: link new memory to its vector-space neighbors
        // in the spreading-activation network so future searches reach it via
        // graph traversal, not only vector similarity.
        for neighbor in neighbor_ids {
            cog.activation_network.add_edge(
                node_id.to_string(),
                neighbor.clone(),
                vestige_core::neuroscience::spreading_activation::LinkType::Semantic,
                0.5,
            );
        }
    }
}

/// Create activation-network edges from extracted relation triples.
///
/// For each relation (subject → predicate → object), creates a directed edge
/// from the memory node to any existing memories that contain the subject or object
/// entity. This builds the knowledge graph incrementally at ingest time
/// rather than waiting for dream consolidation.
#[cfg(feature = "preprocessing")]
async fn create_relation_edges(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    storage: &Arc<Storage>,
    node_id: &str,
    relations: &[vestige_core::preprocessing::relations::ExtractedRelation],
) {
    if relations.is_empty() {
        return;
    }

    // Run every keyword_search on the blocking pool first; then take the
    // cognitive lock and add edges synchronously. Keeps SQLite calls off
    // the async runtime and minimises lock-hold time.
    let storage_clone = storage.clone();
    let objects: Vec<String> = relations.iter().map(|r| r.object.clone()).collect();
    let matches_per_relation = tokio::task::spawn_blocking(move || {
        objects
            .into_iter()
            .map(|object| {
                storage_clone
                    .keyword_search(&object, 3, 0.0)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
    })
    .await;

    let Ok(matches_per_relation) = matches_per_relation else {
        return;
    };

    if let Ok(mut cog) = cognitive.try_lock() {
        for matches in matches_per_relation {
            for matched in matches {
                if matched.id != node_id {
                    cog.activation_network.add_edge(
                        node_id.to_string(),
                        matched.id.clone(),
                        vestige_core::neuroscience::spreading_activation::LinkType::Causal,
                        0.6,
                    );
                }
            }
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    /// Create a test storage instance with a temporary database
    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    #[tokio::test]
    async fn test_smart_ingest_empty_content_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_smart_ingest_basic_content_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "This is a test fact to remember."
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert!(value["nodeId"].is_string());
        assert!(value["decision"].is_string());
    }

    #[tokio::test]
    async fn test_smart_ingest_force_create() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Force create test content.",
            "forceCreate": true
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["decision"], "create");
        assert!(
            value["reason"].as_str().unwrap().contains("Forced")
                || value["reason"]
                    .as_str()
                    .unwrap()
                    .contains("Embeddings not available")
        );
    }

    #[test]
    fn test_schema_has_required_fields() {
        let schema_value = schema();
        assert_eq!(schema_value["type"], "object");
        assert!(schema_value["properties"]["content"].is_object());
        assert!(schema_value["properties"]["forceCreate"].is_object());
        assert!(schema_value["properties"]["items"].is_object());
        // v1.7: no top-level required — content for single mode, items for batch mode
        assert!(schema_value.get("required").is_none() || schema_value["required"].is_null());
    }

    #[tokio::test]
    async fn test_smart_ingest_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_smart_ingest_whitespace_only_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "   \t\n  " });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_smart_ingest_too_large_fails() {
        let (storage, _dir) = test_storage().await;
        let large = "x".repeat(1_000_001);
        let args = serde_json::json!({ "content": large });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
    }

    #[tokio::test]
    async fn test_smart_ingest_exactly_1mb_succeeds() {
        let (storage, _dir) = test_storage().await;
        let content = "x".repeat(1_000_000);
        let args = serde_json::json!({ "content": content });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_smart_ingest_with_node_type() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "A concept to remember",
            "node_type": "concept"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_smart_ingest_with_tags_and_source() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Tagged and sourced memory",
            "tags": ["test", "smart-ingest"],
            "source": "unit-test"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_smart_ingest_response_has_importance_score() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "Important memory content" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        let value = result.unwrap();
        assert!(value["importanceScore"].is_number());
    }

    #[tokio::test]
    async fn test_smart_ingest_missing_content_field_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "tags": ["test"] });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("content"));
    }

    // ========================================================================
    // TESTS PORTED FROM ingest.rs (v1.7.0 merge)
    // ========================================================================

    #[tokio::test]
    async fn test_smart_ingest_with_all_optional_fields() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Complex memory with all metadata.",
            "node_type": "decision",
            "tags": ["architecture", "design"],
            "source": "team meeting notes"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert!(value["nodeId"].is_string());
    }

    #[tokio::test]
    async fn test_smart_ingest_default_node_type_is_fact() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "Default type test content." });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let node_id = result.unwrap()["nodeId"].as_str().unwrap().to_string();
        let node = storage.get_node(&node_id).unwrap().unwrap();
        assert_eq!(node.node_type, "fact");
    }

    #[test]
    fn test_schema_has_optional_fields() {
        let schema_value = schema();
        assert!(schema_value["properties"]["node_type"].is_object());
        assert!(schema_value["properties"]["tags"].is_object());
        assert!(schema_value["properties"]["source"].is_object());
    }

    #[tokio::test]
    async fn test_smart_ingest_with_source() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "MCP protocol version 2024-11-05 is the current standard.",
            "source": "https://modelcontextprotocol.io/spec"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }

    // ========================================================================
    // BATCH MODE TESTS (ported from checkpoint.rs, v1.7.0 merge)
    // ========================================================================

    #[tokio::test]
    async fn test_batch_empty_items_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({ "items": [] })),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_batch_ingest() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [
                    { "content": "First batch item", "tags": ["test"] },
                    { "content": "Second batch item", "tags": ["test"] }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["mode"], "batch");
        assert_eq!(value["summary"]["total"], 2);
    }

    #[tokio::test]
    async fn test_batch_skips_empty_content() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [
                    { "content": "Valid item" },
                    { "content": "" },
                    { "content": "Another valid item" }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["skipped"], 1);
    }

    #[tokio::test]
    async fn test_batch_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_batch_exceeds_20_items_fails() {
        let (storage, _dir) = test_storage().await;
        let items: Vec<serde_json::Value> = (0..21)
            .map(|i| serde_json::json!({ "content": format!("Item {}", i) }))
            .collect();
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({ "items": items })),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum 20 items"));
    }

    #[tokio::test]
    async fn test_batch_exactly_20_items_succeeds() {
        let (storage, _dir) = test_storage().await;
        let items: Vec<serde_json::Value> = (0..20)
            .map(|i| serde_json::json!({ "content": format!("Item {}", i) }))
            .collect();
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({ "items": items })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["total"], 20);
    }

    #[tokio::test]
    async fn test_batch_skips_whitespace_only_content() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [
                    { "content": "   \t\n  " },
                    { "content": "Valid content" }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["skipped"], 1);
        assert_eq!(value["summary"]["created"], 1);
    }

    #[tokio::test]
    async fn test_batch_single_item_succeeds() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [{ "content": "Single item" }]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["total"], 1);
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_batch_items_with_all_fields() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [{
                    "content": "Full fields item",
                    "tags": ["test", "batch"],
                    "node_type": "decision",
                    "source": "test-suite"
                }]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["created"], 1);
    }

    #[tokio::test]
    async fn test_batch_results_array_matches_items() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [
                    { "content": "First" },
                    { "content": "" },
                    { "content": "Third" }
                ]
            })),
        )
        .await;
        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["index"], 0);
        assert_eq!(results[1]["index"], 1);
        assert_eq!(results[1]["status"], "skipped");
        assert_eq!(results[2]["index"], 2);
    }

    #[tokio::test]
    async fn test_batch_success_true_when_only_skipped() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [
                    { "content": "" },
                    { "content": "   " }
                ]
            })),
        )
        .await;
        let value = result.unwrap();
        assert_eq!(value["success"], true); // skipped ≠ errors
        assert_eq!(value["summary"]["errors"], 0);
        assert_eq!(value["summary"]["skipped"], 2);
    }

    #[tokio::test]
    async fn test_batch_has_importance_scores() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [{ "content": "Important batch memory content" }]
            })),
        )
        .await;
        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert!(results[0]["importanceScore"].is_number());
    }

    #[tokio::test]
    async fn test_batch_force_create_global() {
        let (storage, _dir) = test_storage().await;
        // Three items with very similar content + global forceCreate
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "forceCreate": true,
                "items": [
                    { "content": "Physics question about quantum mechanics and wave functions" },
                    { "content": "Physics question about quantum mechanics and wave equations" },
                    { "content": "Physics question about quantum mechanics and wave behavior" }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["mode"], "batch");
        // All 3 should be created separately, not merged
        assert_eq!(value["summary"]["created"], 3);
        assert_eq!(value["summary"]["updated"], 0);
        // Each result should say "Forced creation"
        let results = value["results"].as_array().unwrap();
        for r in results {
            assert_eq!(r["decision"], "create");
            assert!(r["reason"].as_str().unwrap().contains("Forced"));
        }
    }

    #[tokio::test]
    async fn test_batch_force_create_per_item() {
        let (storage, _dir) = test_storage().await;
        // Mix of forced and non-forced items
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "items": [
                    { "content": "Forced item one", "forceCreate": true },
                    { "content": "Normal item two" },
                    { "content": "Forced item three", "forceCreate": true }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        // Forced items should say "Forced creation"
        assert_eq!(results[0]["decision"], "create");
        assert!(results[0]["reason"].as_str().unwrap().contains("Forced"));
        // Non-forced item gets normal processing
        assert_eq!(results[1]["status"], "saved");
        // Third forced item
        assert_eq!(results[2]["decision"], "create");
        assert!(results[2]["reason"].as_str().unwrap().contains("Forced"));
    }

    #[tokio::test]
    async fn test_no_content_no_items_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "tags": ["orphan"] });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("content"));
    }

    #[test]
    fn test_detect_compound_short_content_returns_none() {
        assert!(detect_compound_content("Short text").is_none());
        assert!(detect_compound_content("This is under 300 chars").is_none());
    }

    #[test]
    fn test_detect_compound_multi_paragraph() {
        let content = "First paragraph about topic A: we discovered that the search engine \
                        has a fundamental issue with how it handles compound queries containing semicolons.\n\n\
                        Second paragraph about topic B: the deployment pipeline needs to be reconfigured \
                        because the staging environment is running out of disk space on the worker nodes.\n\n\
                        Third paragraph about topic C: John mentioned in the standup that he prefers \
                        using dark mode and wants us to add theme support to the internal dashboard tool.";
        assert!(
            content.len() >= 300,
            "Test content must be >=300 chars, got {}",
            content.len()
        );
        let result = detect_compound_content(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("COMPOUND CONTENT DETECTED"));
    }

    #[test]
    fn test_detect_compound_speaker_pattern() {
        let content = "Alice: I think we should deploy on Friday because the staging tests passed and the team is ready.\n\
                        Bob: That works for me, but let's make sure to run the full integration test suite first before we proceed.\n\
                        Alice: Sure, I'll set up the CI pipeline today and configure the deployment scripts for the new environment.\n\
                        Carol: Can we also add a staging verification step before production? Last time we had issues with config.";
        assert!(
            content.len() >= 300,
            "Test content must be >=300 chars, got {}",
            content.len()
        );
        let result = detect_compound_content(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("conversation transcript"));
    }

    #[test]
    fn test_detect_compound_bullet_list() {
        let content = "Session summary with multiple learnings from today's work session:\n\
                        - Fixed the authentication bug in login flow where tokens were not refreshed properly\n\
                        - Decided to migrate from MySQL to PostgreSQL for better JSON support and NOTIFY features\n\
                        - John prefers dark mode in all editors and wants the dashboard to support theme switching\n\
                        - Deployment deadline moved to next Friday because of the infrastructure migration blocking us\n\
                        - Added rate limiting to the API gateway to prevent abuse from unauthenticated clients";
        assert!(
            content.len() >= 300,
            "Test content must be >=300 chars, got {}",
            content.len()
        );
        let result = detect_compound_content(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("bulleted list"));
    }

    #[test]
    fn test_detect_compound_single_fact_no_warning() {
        let content = "The hybrid_search function in vestige-core uses triple scoring: BM25 for lexical match, \
                        semantic embeddings for meaning match, and Reciprocal Rank Fusion to combine them. \
                        The default weights are 0.4 for BM25 and 0.6 for semantic. This is configured in \
                        the search module at crates/vestige-core/src/search/hybrid.rs.";
        assert!(detect_compound_content(content).is_none());
    }

    #[tokio::test]
    async fn test_compound_warning_in_response() {
        let (storage, _dir) = test_storage().await;
        let compound = "Alice: We discussed the deployment plan for the new microservice architecture and decided on Friday.\n\
                         Bob: Let's do it Friday, but I want to make sure all the integration tests pass before we push to production.\n\
                         Carol: I agree with that plan and I'll prepare the rollback scripts just in case something goes wrong.\n\
                         Dave: Make sure the staging environment passes all health checks first and monitoring is configured properly.";
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({ "content": compound })),
        )
        .await;
        let value = result.unwrap();
        assert!(value["compound_content_warning"].is_string());
    }

    #[tokio::test]
    async fn test_no_compound_warning_for_atomic() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({ "content": "Single atomic fact about Rust memory safety." })),
        )
        .await;
        let value = result.unwrap();
        assert!(
            value.get("compound_content_warning").is_none()
                || value["compound_content_warning"].is_null()
        );
    }
}

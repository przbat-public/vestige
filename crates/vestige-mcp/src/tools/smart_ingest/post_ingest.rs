//! Cognitive post-ingest side effects (synaptic tagging, novelty, indexing).

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;

use vestige_core::{ImportanceEvent, ImportanceEventType, Storage};

use crate::cognitive::CognitiveEngine;

/// Cognitive post-ingest side effects: synaptic tagging, novelty update, hippocampal indexing.
///
/// Uses try_lock() for non-blocking access. If cognitive is locked, side effects are skipped.
pub(super) fn run_post_ingest(
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
pub(super) fn run_post_ingest_with_neighbors(
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
pub(super) async fn create_relation_edges(
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

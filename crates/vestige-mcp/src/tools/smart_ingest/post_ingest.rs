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
        // graph traversal, not only vector similarity. The activation
        // network has its own lock now — `try_write()` is non-blocking so
        // we don't stall ingest if a search is currently propagating
        // activation; just skip and record the miss.
        if let Ok(mut net) = cog.activation_network.try_write() {
            for neighbor in neighbor_ids {
                net.add_edge(
                    node_id.to_string(),
                    neighbor.clone(),
                    vestige_core::neuroscience::spreading_activation::LinkType::Semantic,
                    0.5,
                );
            }
        } else {
            crate::cognitive::try_lock_metrics::record_miss("ingest_post");
        }
    } else {
        crate::cognitive::try_lock_metrics::record_miss("ingest_post");
    }
}

/// Create activation-network edges (and persistent connections) from
/// extracted relation triples.
///
/// For each relation (subject → predicate → object), creates a directed edge
/// from the new memory to any existing memories whose content matches the
/// object phrase. The edge's `LinkType` comes from
/// [`vestige_core::preprocessing::relations::predicate_to_link_type`] — so
/// "X causes Y" produces a `Causal` edge, "X manages Y" a `Semantic` edge,
/// and "X contains Y" a `PartOf` edge.
///
/// # Why persist
///
/// Until 2026-05-22 these edges only ever lived in the in-memory
/// `activation_network`, so a restart wiped every causal link extracted
/// during the session. That defeated the whole point of building a
/// knowledge graph at ingest time. We now also call
/// [`Storage::save_connection`] so the edge survives restarts and powers
/// the `causal_chain` / `explore_connections` MCP tools after a cold
/// boot.
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

    // Resolve every relation's object to existing memory IDs on the
    // blocking pool, carrying the per-relation `LinkType` through so the
    // edge writer below can emit the correct link kind. Pre 2026-05-22
    // this returned a flat `Vec<Vec<KnowledgeNode>>` and lost the
    // verb/link-type association — so every edge ended up as `Causal`
    // regardless of the source predicate.
    let storage_clone = storage.clone();
    let lookups: Vec<(
        vestige_core::neuroscience::spreading_activation::LinkType,
        String,
    )> = relations
        .iter()
        .map(|r| (r.link_type, r.object.clone()))
        .collect();
    let resolved = tokio::task::spawn_blocking(move || {
        lookups
            .into_iter()
            .map(|(link_type, object)| {
                let matches = storage_clone
                    .keyword_search(&object, 3, 0.0)
                    .unwrap_or_default();
                (link_type, matches)
            })
            .collect::<Vec<_>>()
    })
    .await;

    let Ok(resolved) = resolved else {
        return;
    };

    // Persist edges first (cheap SQLite UPSERTs); the in-memory
    // activation network is best-effort under `try_write`. Pre 2026-05-22
    // we only updated the in-memory network so every causal edge died on
    // restart.
    persist_relation_edges(storage, node_id, &resolved).await;

    let net = {
        let Ok(cog) = cognitive.try_lock() else {
            crate::cognitive::try_lock_metrics::record_miss("ingest_post");
            return;
        };
        Arc::clone(&cog.activation_network)
    };
    let Ok(mut net) = net.try_write() else {
        crate::cognitive::try_lock_metrics::record_miss("ingest_post");
        return;
    };
    for (link_type, matches) in resolved {
        for matched in matches {
            if matched.id != node_id {
                net.add_edge(node_id.to_string(), matched.id.clone(), link_type, 0.6);
            }
        }
    }
}

/// Default strength for ingest-time relation edges. Mirrors the value used
/// for the in-memory activation network so the in-RAM and on-disk graphs
/// agree right after ingest.
#[cfg(feature = "preprocessing")]
const RELATION_EDGE_STRENGTH: f64 = 0.6;

/// Persist relation-derived edges into the SQLite `memory_connections`
/// table so they survive restarts and feed `explore_connections` / dream.
///
/// Failures are logged at `warn` but never propagated — graph edges are a
/// best-effort enrichment, not a correctness boundary. The blocking
/// inserts run on Tokio's blocking pool to avoid stalling the async
/// runtime on SQLite's writer mutex.
#[cfg(feature = "preprocessing")]
async fn persist_relation_edges(
    storage: &Arc<Storage>,
    node_id: &str,
    resolved: &[(
        vestige_core::neuroscience::spreading_activation::LinkType,
        Vec<vestige_core::KnowledgeNode>,
    )],
) {
    use chrono::Utc;
    use vestige_core::ConnectionRecord;

    let mut edges: Vec<ConnectionRecord> = Vec::new();
    let now = Utc::now();
    for (link_type, matches) in resolved {
        let link_label = link_type_label(*link_type);
        for matched in matches {
            if matched.id == node_id {
                continue;
            }
            edges.push(ConnectionRecord {
                source_id: node_id.to_string(),
                target_id: matched.id.clone(),
                strength: RELATION_EDGE_STRENGTH,
                link_type: link_label.to_string(),
                created_at: now,
                last_activated: now,
                activation_count: 0,
            });
        }
    }
    if edges.is_empty() {
        return;
    }
    let storage_w = storage.clone();
    let result = tokio::task::spawn_blocking(move || {
        for edge in &edges {
            if let Err(e) = storage_w.save_connection(edge) {
                tracing::warn!(
                    error = %e,
                    source = %edge.source_id,
                    target = %edge.target_id,
                    link_type = %edge.link_type,
                    "failed to persist relation edge",
                );
            }
        }
    })
    .await;
    if let Err(e) = result {
        tracing::warn!(error = %e, "persist_relation_edges task panicked");
    }
}

/// Canonical string label for a [`LinkType`] used in `memory_connections.link_type`.
///
/// Keeps the persistent labels stable even if the enum's `Debug` or
/// `Serialize` representations change — the dashboard and dream
/// traversal already grep on these strings.
#[cfg(feature = "preprocessing")]
fn link_type_label(link_type: vestige_core::neuroscience::spreading_activation::LinkType) -> &'static str {
    use vestige_core::neuroscience::spreading_activation::LinkType;
    match link_type {
        LinkType::Causal => "causal",
        LinkType::Semantic => "semantic",
        LinkType::Temporal => "temporal",
        LinkType::Spatial => "spatial",
        LinkType::PartOf => "part_of",
        LinkType::UserDefined => "user_defined",
    }
}

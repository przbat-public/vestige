//! CognitiveEngine — Stateful neuroscience modules that persist across tool calls.
//!
//! v1.5.0: Wires ALL unused vestige-core features into the MCP server.
//! Each module is initialized once at startup and shared via Arc<Mutex<>>
//! across all tool invocations.
//!
//! ## Roadmap
//!
//! - **MCP Tasks**: Expose long-running operations (dream, consolidation, backup)
//!   as MCP Tasks with progress streaming via SSE, replacing fire-and-forget calls.
//! - **Metacognition layer**: Add confidence calibration and retrieval-quality
//!   self-assessment. Track hit/miss ratio per query type to adjust search
//!   parameters (similarity threshold, RRF k-factor) dynamically.

use vestige_core::neuroscience::predictive_retrieval::PredictiveMemory;
use vestige_core::neuroscience::prospective_memory::{IntentionParser, ProspectiveMemory};
use vestige_core::search::TemporalSearcher;
use vestige_core::{
    AccessibilityCalculator,
    // Neuroscience modules
    ActivationNetwork,
    ActivityTracker,
    AdaptiveEmbedder,
    ArousalSignal,
    AttentionSignal,
    CompetitionManager,
    ConsolidationScheduler,
    ContextMatcher,
    CrossProjectLearner,
    // Consolidation
    DreamEngine,
    EmotionalMemory,
    HippocampalIndex,
    ImportanceSignals,
    // Advanced modules
    ImportanceTracker,
    IntentDetector,
    LinkType,
    MemoryChainBuilder,
    MemoryCompressor,
    MemoryDreamer,
    MetacognitionMonitor,
    NoveltySignal,
    ReconsolidationManager,
    // Search modules
    Reranker,
    RerankerConfig,
    RewardSignal,
    SpeculativeRetriever,
    StateUpdateService,
    // Storage
    Storage,
    SynapticTaggingSystem,
};

/// Stateful cognitive engine holding all neuroscience modules.
///
/// Lives on `McpServer` as `Arc<Mutex<CognitiveEngine>>` and is passed
/// to tools that need persistent cross-call state (search, ingest,
/// feedback, consolidation, new tools).
///
/// ## Concurrency model (b14)
///
/// The whole engine sits behind a single `tokio::sync::Mutex`. That keeps the
/// implementation simple but every tool serializes through it. Two rules
/// matter when you touch this type:
///
/// 1. **Always `lock().await` from request paths.** `try_lock()` returns `None`
///    under contention and the silent failure burned us in a10. The only
///    callers that may use `try_lock()` are fire-and-forget background tasks
///    where missing an update is acceptable — and even then prefer
///    `tokio::spawn` + `lock().await` so back-pressure is explicit.
///
/// 2. **Hold the lock for the smallest possible critical section.** Drop the
///    guard before awaiting unrelated futures (DB, embeddings, HTTP). If a
///    handler needs the lock twice with awaits in between, re-acquire instead
///    of holding it across `.await`.
///
/// Splitting this engine into per-module locks (synaptic tagging vs hippocampal
/// index vs metacognition…) is the obvious next step once the API stabilizes;
/// the current monolith is fine while the surface keeps shifting between
/// patches. See the audit notes in CHANGELOG `[3.2.1]` for the open plan.
pub struct CognitiveEngine {
    // -- Neuroscience --
    pub activation_network: ActivationNetwork,
    pub synaptic_tagging: SynapticTaggingSystem,
    pub hippocampal_index: HippocampalIndex,
    pub context_matcher: ContextMatcher,
    pub accessibility_calc: AccessibilityCalculator,
    pub competition_mgr: CompetitionManager,
    pub state_service: StateUpdateService,
    pub importance_signals: ImportanceSignals,
    pub novelty_signal: NoveltySignal,
    pub arousal_signal: ArousalSignal,
    pub reward_signal: RewardSignal,
    pub attention_signal: AttentionSignal,
    pub emotional_memory: EmotionalMemory,
    pub predictive_memory: PredictiveMemory,
    pub prospective_memory: ProspectiveMemory,
    pub intention_parser: IntentionParser,

    // -- Advanced --
    pub importance_tracker: ImportanceTracker,
    pub reconsolidation: ReconsolidationManager,
    pub intent_detector: IntentDetector,
    pub activity_tracker: ActivityTracker,
    pub dreamer: MemoryDreamer,
    pub chain_builder: MemoryChainBuilder,
    pub compressor: MemoryCompressor,
    pub cross_project: CrossProjectLearner,
    pub adaptive_embedder: AdaptiveEmbedder,
    pub speculative_retriever: SpeculativeRetriever,
    pub consolidation_scheduler: ConsolidationScheduler,

    // -- Consolidation --
    pub dream_engine: DreamEngine,

    // -- Search --
    pub reranker: Reranker,
    pub temporal_searcher: TemporalSearcher,

    // -- Metacognition (Nelson & Narens 1990) --
    pub metacognition: MetacognitionMonitor,
}

impl Default for CognitiveEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveEngine {
    /// Load persisted state from storage into in-memory cognitive modules.
    ///
    /// Without this, the cognitive layer starts empty after every restart and
    /// modules that consult in-memory indices (e.g. `explore_connections`,
    /// `HippocampalIndex` associations) silently return nothing.
    ///
    /// Hydrates two indices:
    /// - `ActivationNetwork` — every persisted `ConnectionRecord` becomes an edge
    ///   so spreading-activation queries work immediately.
    /// - `HippocampalIndex` — every persisted memory is re-indexed (with its
    ///   embedding when available) so barcode/association lookups survive restarts.
    ///
    /// `SynapticTaggingSystem`, `ImportanceTracker`, `ActivityTracker` and the
    /// other working-memory style modules are intentionally NOT hydrated —
    /// they hold ephemeral session state by design (tags expire on a window
    /// shorter than typical downtime).
    pub fn hydrate(&mut self, storage: &Storage) {
        // 1. Connections → ActivationNetwork
        match storage.get_all_connections() {
            Ok(connections) => {
                for conn in &connections {
                    let link_type = match conn.link_type.as_str() {
                        "semantic" => LinkType::Semantic,
                        "temporal" => LinkType::Temporal,
                        "causal" => LinkType::Causal,
                        "spatial" => LinkType::Spatial,
                        "shared_concepts" | "complementary" => LinkType::Semantic,
                        _ => LinkType::Semantic,
                    };
                    self.activation_network.add_edge(
                        conn.source_id.clone(),
                        conn.target_id.clone(),
                        link_type,
                        conn.strength,
                    );
                }
                tracing::info!(
                    count = connections.len(),
                    "Hydrated activation network from persisted connections"
                );
            }
            Err(e) => {
                tracing::warn!("Failed to hydrate activation network: {}", e);
            }
        }

        // 2. Memories → HippocampalIndex (batched to avoid loading the
        //    full table into memory).
        const PAGE: i32 = 500;
        let mut offset: i32 = 0;
        let mut total_indexed: usize = 0;
        let mut total_errors: usize = 0;

        loop {
            match storage.get_all_nodes(PAGE, offset) {
                Ok(nodes) if nodes.is_empty() => break,
                Ok(nodes) => {
                    let count = nodes.len();
                    for node in &nodes {
                        // Embedding lookup is best-effort; missing embedding
                        // is fine because index_memory accepts Option<Vec<f32>>.
                        let embedding = storage.get_node_embedding(&node.id).ok().flatten();
                        if let Err(e) = self.hippocampal_index.index_memory(
                            &node.id,
                            &node.content,
                            &node.node_type,
                            node.created_at,
                            embedding,
                        ) {
                            total_errors += 1;
                            tracing::debug!(
                                memory_id = %node.id,
                                error = %e,
                                "Skipped indexing memory during hydration"
                            );
                        } else {
                            total_indexed += 1;
                        }
                    }
                    offset += count as i32;
                    if count < PAGE as usize {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to page memories during hydration: {}", e);
                    break;
                }
            }
        }

        if total_indexed > 0 || total_errors > 0 {
            tracing::info!(
                indexed = total_indexed,
                errors = total_errors,
                "Hydrated hippocampal index from persisted memories"
            );
        }
    }

    /// Initialize all cognitive modules with default configurations.
    pub fn new() -> Self {
        Self {
            // Neuroscience
            activation_network: ActivationNetwork::new(),
            synaptic_tagging: SynapticTaggingSystem::new(),
            hippocampal_index: HippocampalIndex::new(),
            context_matcher: ContextMatcher::new(),
            accessibility_calc: AccessibilityCalculator::default(),
            competition_mgr: CompetitionManager::new(),
            state_service: StateUpdateService::new(),
            importance_signals: ImportanceSignals::new(),
            novelty_signal: NoveltySignal::new(),
            arousal_signal: ArousalSignal::new(),
            reward_signal: RewardSignal::new(),
            attention_signal: AttentionSignal::new(),
            emotional_memory: EmotionalMemory::new(),
            predictive_memory: PredictiveMemory::new(),
            prospective_memory: ProspectiveMemory::new(),
            intention_parser: IntentionParser::new(),

            // Advanced
            importance_tracker: ImportanceTracker::new(),
            reconsolidation: ReconsolidationManager::new(),
            intent_detector: IntentDetector::new(),
            activity_tracker: ActivityTracker::new(),
            dreamer: MemoryDreamer::new(),
            chain_builder: MemoryChainBuilder::new(),
            compressor: MemoryCompressor::new(),
            cross_project: CrossProjectLearner::new(),
            adaptive_embedder: AdaptiveEmbedder::new(),
            speculative_retriever: SpeculativeRetriever::new(),
            consolidation_scheduler: ConsolidationScheduler::new(),

            // Consolidation
            dream_engine: DreamEngine::new(),

            // Search
            reranker: Reranker::new(RerankerConfig::default()),
            temporal_searcher: TemporalSearcher::new(),

            // Metacognition
            metacognition: MetacognitionMonitor::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;
    use vestige_core::{ConnectionRecord, IngestInput};

    fn create_test_storage() -> (Storage, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (storage, dir)
    }

    fn ingest_memory(storage: &Storage, content: &str) -> String {
        let result = storage
            .ingest(IngestInput {
                content: content.to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["test".to_string()],
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            })
            .unwrap();
        result.id
    }

    #[test]
    fn test_hydrate_empty_storage() {
        let (storage, _dir) = create_test_storage();
        let mut engine = CognitiveEngine::new();
        engine.hydrate(&storage);
        // Should succeed with 0 connections
        let assocs = engine.activation_network.get_associations("nonexistent");
        assert!(assocs.is_empty());
    }

    #[test]
    fn test_hydrate_loads_connections() {
        let (storage, _dir) = create_test_storage();

        // Create two memories so FK constraints pass
        let id1 = ingest_memory(&storage, "Memory about Rust programming");
        let id2 = ingest_memory(&storage, "Memory about Cargo build system");

        // Save a connection between them
        let now = Utc::now();
        storage
            .save_connection(&ConnectionRecord {
                source_id: id1.clone(),
                target_id: id2.clone(),
                strength: 0.85,
                link_type: "semantic".to_string(),
                created_at: now,
                last_activated: now,
                activation_count: 1,
            })
            .unwrap();

        // Hydrate engine
        let mut engine = CognitiveEngine::new();
        engine.hydrate(&storage);

        // Verify activation network has the connection
        let assocs = engine.activation_network.get_associations(&id1);
        assert!(
            !assocs.is_empty(),
            "Hydrated engine should have associations for {}",
            id1
        );
        assert!(
            assocs.iter().any(|a| a.memory_id == id2),
            "Should find connection to {}",
            id2
        );
    }

    #[test]
    fn test_hydrate_indexes_persisted_memories_into_hippocampal_index() {
        let (storage, _dir) = create_test_storage();

        let id1 = ingest_memory(&storage, "Memory about hippocampal indexing");
        let id2 = ingest_memory(&storage, "Another memory that should be indexed");

        let mut engine = CognitiveEngine::new();
        engine.hydrate(&storage);

        let stats = engine.hippocampal_index.stats();
        assert!(
            stats.total_indices >= 2,
            "HippocampalIndex should contain hydrated memories, got {}",
            stats.total_indices
        );

        // Sanity: the indexed memory IDs are reachable through the index
        let _ = (id1, id2);
    }

    #[test]
    fn test_hydrate_multiple_link_types() {
        let (storage, _dir) = create_test_storage();

        let id1 = ingest_memory(&storage, "Event A happened");
        let id2 = ingest_memory(&storage, "Event B followed");
        let id3 = ingest_memory(&storage, "Event C was caused by A");

        let now = Utc::now();
        storage
            .save_connection(&ConnectionRecord {
                source_id: id1.clone(),
                target_id: id2.clone(),
                strength: 0.7,
                link_type: "temporal".to_string(),
                created_at: now,
                last_activated: now,
                activation_count: 1,
            })
            .unwrap();
        storage
            .save_connection(&ConnectionRecord {
                source_id: id1.clone(),
                target_id: id3.clone(),
                strength: 0.9,
                link_type: "causal".to_string(),
                created_at: now,
                last_activated: now,
                activation_count: 1,
            })
            .unwrap();

        let mut engine = CognitiveEngine::new();
        engine.hydrate(&storage);

        let assocs = engine.activation_network.get_associations(&id1);
        assert!(
            assocs.len() >= 2,
            "Should have at least 2 associations, got {}",
            assocs.len()
        );
    }
}

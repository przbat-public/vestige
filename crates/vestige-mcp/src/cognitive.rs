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

use std::sync::Arc;
use tokio::sync::RwLock;

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
///
/// ## Lock miss telemetry
///
/// Hot paths that *do* keep using `try_lock()` (rerank, scoring, post-ingest,
/// finalize, codebase tools…) intentionally treat contention as
/// "skip this enrichment". To make those skips observable, every such call
/// site reports a counter via [`try_lock_metrics::record_miss`]. The
/// `system_status` MCP tool exposes the snapshot under
/// `cognitive.tryLockMisses`, so we can tell *which* enrichment was being
/// skipped instead of staring at a constant retention number wondering why
/// activations look flat. Added 2026-05-19.
pub struct CognitiveEngine {
    // -- Neuroscience --
    //
    // `activation_network` and `dreamer` live behind their own `RwLock` so
    // hot paths can clone the `Arc`, drop the engine guard, then lock the
    // inner network/dreamer independently. This removes them from the
    // serialization chokepoint that the rest of the engine still uses (see
    // CHANGELOG `[3.2.1]`). The other modules stay protected by the engine
    // `Mutex` until their APIs settle.
    pub activation_network: Arc<RwLock<ActivationNetwork>>,
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
    pub dreamer: Arc<RwLock<MemoryDreamer>>,
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
    /// ColBERT late-interaction reranker. Loaded on demand in `main` when
    /// `VESTIGE_LATE_INTERACTION` + `VESTIGE_COLBERT_MODEL_DIR` are set; `None`
    /// otherwise, in which case the Jina cross-encoder remains the reranker.
    #[cfg(feature = "late-interaction")]
    pub colbert: Option<vestige_core::search::ColbertEmbedder>,
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
        //
        // `hydrate` runs synchronously from `spawn_blocking` (see main.rs).
        // `blocking_write()` is the right primitive here: it'd panic from
        // an async task, but in a blocking context it just acquires the
        // write lock. Since nothing else has the Arc yet there's no
        // contention.
        match storage.get_all_connections() {
            Ok(connections) => {
                let mut net = self.activation_network.blocking_write();
                for conn in &connections {
                    let link_type = match conn.link_type.as_str() {
                        "semantic" => LinkType::Semantic,
                        "temporal" => LinkType::Temporal,
                        "causal" => LinkType::Causal,
                        "spatial" => LinkType::Spatial,
                        "shared_concepts" | "complementary" => LinkType::Semantic,
                        _ => LinkType::Semantic,
                    };
                    net.add_edge(
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
            activation_network: Arc::new(RwLock::new(ActivationNetwork::new())),
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
            dreamer: Arc::new(RwLock::new(MemoryDreamer::new())),
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
            #[cfg(feature = "late-interaction")]
            colbert: None,
            temporal_searcher: TemporalSearcher::new(),

            // Metacognition
            metacognition: MetacognitionMonitor::new(),
        }
    }
}

/// Per-stage telemetry for `try_lock()` skips in cognitive hot paths.
///
/// `try_lock()` returning `None` is *not* an error — it's how we keep request
/// latency bounded when the engine is busy. But silently dropping the
/// enrichment used to be invisible from the outside: search would return
/// fewer activated neighbours, scores would shift, and there was no signal
/// at all in logs or metrics. Now every call site that bails on `try_lock()`
/// bumps a counter here, and `system_status` surfaces the totals so operators
/// can spot a saturated cognitive engine.
///
/// The counters are best-effort `Relaxed` atomics — we never read them on a
/// hot path, only when `system_status` snapshots them.
pub mod try_lock_metrics {
    use std::sync::atomic::{AtomicU64, Ordering};

    macro_rules! stages {
        ($($name:ident => $key:literal),* $(,)?) => {
            $(static $name: AtomicU64 = AtomicU64::new(0);)*

            /// Stable, snake_case labels exposed in `system_status`.
            pub const STAGES: &[&str] = &[$($key),*];

            /// Bump the counter for `stage`. Unknown stages are silently
            /// ignored so a typo never breaks a request path.
            pub fn record_miss(stage: &'static str) {
                match stage {
                    $($key => { $name.fetch_add(1, Ordering::Relaxed); })*
                    _ => {}
                }
            }

            /// Snapshot of every counter as `(label, count)` pairs.
            pub fn snapshot() -> Vec<(&'static str, u64)> {
                vec![$(($key, $name.load(Ordering::Relaxed))),*]
            }

            #[cfg(test)]
            pub fn reset() {
                $($name.store(0, Ordering::Relaxed);)*
            }
        };
    }

    stages! {
        SEARCH_RETRIEVAL => "search_retrieval",
        SEARCH_RERANK => "search_rerank",
        SEARCH_SCORING => "search_scoring",
        SEARCH_FINALIZE => "search_finalize",
        INGEST_PRE => "ingest_pre",
        INGEST_POST => "ingest_post",
        MEMORY_PROMOTE => "memory_promote",
        MEMORY_DEMOTE => "memory_demote",
        INTENTION_SET => "intention_set",
        INTENTION_CHECK => "intention_check",
        CODEBASE_PATTERN => "codebase_pattern",
        CODEBASE_DECISION => "codebase_decision",
        CODEBASE_DECISION_V2 => "codebase_decision_v2",
        CODEBASE_GET_CONTEXT => "codebase_get_context",
        CROSS_REFERENCE => "cross_reference",
        DEEP_REFERENCE => "deep_reference",
        SYSTEM_STATUS => "system_status",
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
        // Should succeed with 0 connections. No async runtime in this
        // test, so use `try_read()` which is non-blocking and never panics
        // outside a reactor.
        let net = engine.activation_network.try_read().unwrap();
        let assocs = net.get_associations("nonexistent");
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
        let net = engine.activation_network.try_read().unwrap();
        let assocs = net.get_associations(&id1);
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

    /// activation_network must expose an `Arc<RwLock<…>>` handle so callers
    /// can clone it out of the engine, drop the engine guard, then lock the
    /// network independently. Without that pattern every search/ingest path
    /// serializes through the giant engine mutex — exactly the contention
    /// the audit flagged in CHANGELOG `[3.2.1]`.
    #[tokio::test(start_paused = false)]
    async fn activation_network_handle_can_outlive_engine_guard() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let engine = Arc::new(Mutex::new(CognitiveEngine::new()));

        // Clone the handle out of the engine, then drop the engine guard.
        let net_handle = {
            let cog = engine.lock().await;
            Arc::clone(&cog.activation_network)
        };

        // Now hold a read lock on the activation network while ANOTHER task
        // acquires the engine. The point of splitting locks is that the two
        // operations don't deadlock or serialize. A second engine lock must
        // be obtainable within a few ms even while we hold the network
        // lock.
        let read_guard = net_handle.read().await;
        let other = tokio::time::timeout(std::time::Duration::from_millis(50), engine.lock()).await;
        assert!(
            other.is_ok(),
            "engine lock must be obtainable while a separate \
             activation_network read lock is held — otherwise the split is cosmetic"
        );
        drop(read_guard);
    }

    /// Same guarantee for the dreamer — its `synthesize_*` paths are read-
    /// only and used during dream cycles that can take seconds. Holding the
    /// engine mutex for that long would block every other tool call.
    #[tokio::test(start_paused = false)]
    async fn dreamer_handle_can_outlive_engine_guard() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let engine = Arc::new(Mutex::new(CognitiveEngine::new()));
        let dreamer_handle = {
            let cog = engine.lock().await;
            Arc::clone(&cog.dreamer)
        };
        let read_guard = dreamer_handle.read().await;
        let other = tokio::time::timeout(std::time::Duration::from_millis(50), engine.lock()).await;
        assert!(
            other.is_ok(),
            "engine lock must be obtainable while a separate dreamer read lock is held"
        );
        drop(read_guard);
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

        let net = engine.activation_network.try_read().unwrap();
        let assocs = net.get_associations(&id1);
        assert!(
            assocs.len() >= 2,
            "Should have at least 2 associations, got {}",
            assocs.len()
        );
    }
}

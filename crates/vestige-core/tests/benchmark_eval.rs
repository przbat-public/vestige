//! Retrieval tests over the real `Storage` pipeline, plus two pure-maths
//! properties of the decay and activation helpers.
//!
//! What this file does: writes a small hand-built corpus through
//! `Storage::ingest` into a temporary SQLite database, runs each query through
//! the production retrieval path (`Storage::keyword_search` for FTS5/BM25 and,
//! with the `embeddings` + `vector-search` features on, `Storage::hybrid_search`
//! for BM25 + HNSW cosine fused by RRF) and asserts that the memory carrying the
//! query's distinctive term is retrieved. The metrics are computed from the
//! product's own result lists, so a regression in FTS5, the HNSW index, RRF or
//! the ingest path can fail these tests.
//!
//! What this file is NOT: a LoCoMo/LongMemEval benchmark, despite the earlier
//! header claiming that format. The corpus is six memories written here, so the
//! printed recall@5 / MRR / precision@3 numbers describe only this corpus and
//! must not be quoted as product quality. The previous revision of this file
//! also computed those metrics with a private scorer
//! (`word_hits * 2.0 + tag_hits`) over `KnowledgeNode`s built in memory and then
//! asserted `recall@5 >= 0.75` / `mrr >= 0.50`: a regression anywhere in the
//! retrieval pipeline could not have failed it, and a green run said nothing
//! about the product.

use std::path::PathBuf;

use tempfile::TempDir;
use vestige_core::memory::StrengthDecay;
use vestige_core::neuroscience::spreading_activation::ActivationNetwork;
use vestige_core::{IngestInput, Storage};

/// One retrieval query: the text, the corpus entry it must find, and the term
/// (or term combination) that makes that entry identifiable among the corpus.
struct RetrievalQuery {
    text: &'static str,
    /// Index into the scenario corpus of the one memory that must be retrieved.
    target: usize,
    /// Term or term combination carried by the target memory and by no other
    /// corpus entry; the failure message names it so a broken assertion points
    /// at the discriminating input rather than at "recall".
    distinctive_terms: &'static str,
}

struct RetrievalScenario {
    name: &'static str,
    /// `(content, node_type, tags)` per memory, in a stable order.
    corpus: Vec<(&'static str, &'static str, Vec<&'static str>)>,
    queries: Vec<RetrievalQuery>,
}

/// Which production retrieval path a scenario was run through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetrievalPath {
    /// `Storage::keyword_search` - FTS5/BM25 only.
    Keyword,
    /// `Storage::hybrid_search` - BM25 + HNSW fused by RRF.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    Hybrid,
}

impl RetrievalPath {
    fn label(self) -> &'static str {
        match self {
            RetrievalPath::Keyword => "keyword_search (FTS5/BM25)",
            #[cfg(all(feature = "embeddings", feature = "vector-search"))]
            RetrievalPath::Hybrid => "hybrid_search (BM25 + HNSW + RRF)",
        }
    }
}

/// Every retrieval path this build can exercise.
fn retrieval_paths() -> Vec<RetrievalPath> {
    let mut paths = vec![RetrievalPath::Keyword];
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    paths.push(RetrievalPath::Hybrid);
    paths
}

/// The mock embedder must be on *before* the first `Storage::new`: without it
/// the first ingest lazily initializes the real ONNX model, which downloads
/// ~547 MB on a cold cache. Unlike `tests/e2e`, the unit-test override in
/// `embeddings::local` is `#[cfg(test)]` and therefore not visible to this
/// integration test, so the environment switch is the only lever here.
///
/// Set once per process and never restored: every test in this binary wants the
/// mock embedder, and restoring it mid-run would race the other test threads
/// against a half-configured service.
fn enable_mock_embeddings() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: `call_once` runs this exactly once, before any test in this
        // binary opens a `Storage`, and the value is never changed afterwards,
        // so no other thread can observe a concurrent write.
        unsafe {
            std::env::set_var(vestige_core::embeddings::MOCK_EMBEDDINGS_ENV, "1");
        }
    });
}

/// A `Storage` on a fresh temporary SQLite file inside its own subdirectory.
///
/// Each store gets its own directory because `Storage` also persists an HNSW
/// sidecar next to the database file: two stores sharing a directory would share
/// that sidecar too, and the second path under test would start with the first
/// path's index. The directory (and the sidecar) is removed when the returned
/// `TempDir` drops.
fn open_store(dir: &TempDir, name: &str) -> Storage {
    enable_mock_embeddings();
    let store_dir = dir.path().join(name);
    std::fs::create_dir_all(&store_dir).expect("per-store temp dir must be creatable");
    let db_path: PathBuf = store_dir.join("retrieval.db");
    Storage::new(Some(db_path)).expect("temporary Storage must open")
}

/// Ingest the scenario corpus and return the ids in corpus order.
fn ingest_corpus(storage: &Storage, scenario: &RetrievalScenario) -> Vec<String> {
    let ids: Vec<String> = scenario
        .corpus
        .iter()
        .map(|(content, node_type, tags)| {
            let input = IngestInput {
                content: (*content).to_string(),
                node_type: (*node_type).to_string(),
                tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
                ..IngestInput::default()
            };
            storage
                .ingest(input)
                .unwrap_or_else(|error| {
                    panic!(
                        "ingest of {:?} failed: {error}",
                        &content[..40.min(content.len())]
                    )
                })
                .id
        })
        .collect();

    let stored = storage
        .count_nodes_filtered(None, None, None)
        .expect("counting ingested memories must not error");
    assert_eq!(
        stored,
        scenario.corpus.len(),
        "[{}] every corpus memory must persist as exactly one row (got {stored})",
        scenario.name
    );

    ids
}

/// Run one query through the production retrieval path and return the ids in
/// the order the product ranked them.
fn retrieve(storage: &Storage, path: RetrievalPath, query: &str, limit: i32) -> Vec<String> {
    match path {
        RetrievalPath::Keyword => storage
            .keyword_search(query, limit, 0.0)
            .expect("keyword_search must not error")
            .into_iter()
            .map(|node| node.id)
            .collect(),
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        RetrievalPath::Hybrid => storage
            .hybrid_search(
                query,
                limit,
                vestige_core::DEFAULT_HYBRID_KEYWORD_WEIGHT,
                vestige_core::DEFAULT_HYBRID_SEMANTIC_WEIGHT,
            )
            .expect("hybrid_search must not error")
            .into_iter()
            .map(|result| result.node.id)
            .collect(),
    }
}

/// Metrics computed from the product's result lists, not from a scorer written
/// here. With one relevant memory per query, recall@5 is 1.0 exactly when that
/// memory appears in the top 5, which every caller asserts per query.
struct Metrics {
    recall_at_5: f64,
    mrr: f64,
    precision_at_3: f64,
}

/// Exercise one scenario through one production path. Returns the metrics for
/// printing; the per-query assertion inside is what pins retrieval.
fn run_scenario(storage: &Storage, scenario: &RetrievalScenario, path: RetrievalPath) -> Metrics {
    // `ingest_corpus` asserts that the store holds exactly one row per corpus
    // memory before any query runs.
    let ids = ingest_corpus(storage, scenario);

    let mut recall_sum = 0.0;
    let mut mrr_sum = 0.0;
    let mut precision_sum = 0.0;

    for query in &scenario.queries {
        let ranked = retrieve(storage, path, query.text, 5);
        let top_5: Vec<&String> = ranked.iter().take(5).collect();
        let top_3: Vec<&String> = ranked.iter().take(3).collect();
        let target_id = &ids[query.target];
        let rank = ranked.iter().position(|id| id == target_id);

        println!(
            "[{} / {}] query {:?} -> rank {:?}; top 5: {:?}",
            scenario.name,
            path.label(),
            query.text,
            rank.map(|position| position + 1),
            top_5
                .iter()
                .map(|id| {
                    let index = ids
                        .iter()
                        .position(|known| known == *id)
                        .unwrap_or(usize::MAX);
                    scenario.corpus.get(index).map_or("?", |entry| {
                        entry.0.split('.').next().unwrap_or(entry.0).trim()
                    })
                })
                .collect::<Vec<_>>()
        );

        assert!(
            top_5.contains(&target_id),
            "[{} / {}] the memory identified by {:?} must be retrieved for {:?}; got {} results",
            scenario.name,
            path.label(),
            query.distinctive_terms,
            query.text,
            ranked.len()
        );

        recall_sum += 1.0;
        precision_sum += if top_3.contains(&target_id) {
            1.0 / 3.0
        } else {
            0.0
        };
        if let Some(position) = rank {
            mrr_sum += 1.0 / (position as f64 + 1.0);
        }
    }

    let query_count = scenario.queries.len() as f64;
    Metrics {
        recall_at_5: recall_sum / query_count,
        mrr: mrr_sum / query_count,
        precision_at_3: precision_sum / query_count,
    }
}

/// Assert and report a scenario's metrics. `recall@5 = 1.0` is implied by the
/// per-query assertions above (one relevant memory per query); it is asserted
/// again so the aggregate cannot drift if a query is edited to have none.
fn assert_and_report(scenario: &RetrievalScenario, path: RetrievalPath, metrics: &Metrics) {
    println!(
        "[{} / {}] recall@5 = {:.2}, MRR = {:.3}, precision@3 = {:.3} over {} queries and {} corpus memories",
        scenario.name,
        path.label(),
        metrics.recall_at_5,
        metrics.mrr,
        metrics.precision_at_3,
        scenario.queries.len(),
        scenario.corpus.len()
    );
    assert_eq!(
        metrics.recall_at_5,
        1.0,
        "[{} / {}] every query has exactly one relevant memory and each is asserted individually",
        scenario.name,
        path.label()
    );
}

/// Run every path this build supports and report both.
fn run_all_paths(scenario: &RetrievalScenario, dir: &TempDir) {
    for (index, path) in retrieval_paths().into_iter().enumerate() {
        // Each path gets its own database so the ingest under test is the only
        // state the retrieval sees (no index warmed by a previous path).
        let storage = open_store(dir, &format!("path-{index}"));
        let metrics = run_scenario(&storage, scenario, path);
        assert_and_report(scenario, path, &metrics);
    }
}

// ==========================================================================
// Scenario 1: Bug-fix knowledge retrieval
//
// One query per bug-fix memory, each containing that memory's distinctive
// token (the error type, the failure mode). This is the bread-and-butter case
// of a developer memory system: the fix must come back for a description of
// the symptom.
// ==========================================================================
#[test]
fn test_bug_fix_memories_are_retrieved_by_error_description() {
    let scenario = RetrievalScenario {
        name: "Bug-Fix Retrieval",
        corpus: vec![
            (
                "BUG FIX: ConnectionResetError when calling auth API. Root cause: connection pool timeout set to 5s, upstream latency was 8s. Fix: increased timeout to 15s.",
                "fact",
                vec!["bug-fix", "auth", "timeout"],
            ),
            (
                "BUG FIX: OOM crash in image processing pipeline. Root cause: unbounded buffer accumulation. Fix: added backpressure with bounded channel.",
                "fact",
                vec!["bug-fix", "oom", "pipeline"],
            ),
            (
                "Architecture decision: chose PostgreSQL over MongoDB for transaction guarantees.",
                "decision",
                vec!["architecture", "database"],
            ),
            (
                "BUG FIX: Race condition in session management. Two requests updating the same session concurrently. Fix: added optimistic locking with version column.",
                "fact",
                vec!["bug-fix", "race-condition", "session"],
            ),
            (
                "User preference: always use structured logging with correlation IDs.",
                "note",
                vec!["preference", "logging"],
            ),
            (
                "BUG FIX: Memory leak in WebSocket handler. Event listeners not cleaned up on disconnect. Fix: added cleanup in onClose handler.",
                "fact",
                vec!["bug-fix", "memory-leak", "websocket"],
            ),
        ],
        queries: vec![
            RetrievalQuery {
                text: "connection timeout error auth API",
                target: 0,
                distinctive_terms: "auth API",
            },
            RetrievalQuery {
                text: "out of memory crash pipeline backpressure",
                target: 1,
                distinctive_terms: "backpressure",
            },
            RetrievalQuery {
                text: "race condition session concurrent locking",
                target: 3,
                distinctive_terms: "locking",
            },
            RetrievalQuery {
                text: "websocket memory leak event listener cleanup",
                target: 5,
                distinctive_terms: "WebSocket",
            },
        ],
    };

    let dir = TempDir::new().expect("temp dir");
    run_all_paths(&scenario, &dir);
}

// ==========================================================================
// Scenario 2: Decision knowledge retrieval
//
// The "why did we do X?" case: each query carries the distinctive technology
// name of exactly one decision memory.
// ==========================================================================
#[test]
fn test_decision_memories_are_retrieved_by_rationale_query() {
    let scenario = RetrievalScenario {
        name: "Decision Retrieval",
        corpus: vec![
            (
                "DECISION: Use Redis for session storage instead of PostgreSQL. Rationale: sub-millisecond reads, built-in TTL for session expiry.",
                "decision",
                vec!["decision", "redis", "session"],
            ),
            (
                "DECISION: Migrate from REST to gRPC for service-to-service communication. Rationale: type safety, streaming, lower latency.",
                "decision",
                vec!["decision", "grpc", "api"],
            ),
            (
                "Team standup notes from Monday: discussed sprint velocity.",
                "event",
                vec!["meeting", "standup"],
            ),
            (
                "DECISION: Deploy on Kubernetes instead of ECS. Rationale: multi-cloud portability, Helm ecosystem.",
                "decision",
                vec!["decision", "kubernetes", "deployment"],
            ),
            (
                "Code review feedback: use more descriptive variable names.",
                "note",
                vec!["code-review"],
            ),
            (
                "DECISION: Chose TypeScript over Python for the API layer. Rationale: shared types with frontend, better IDE support.",
                "decision",
                vec!["decision", "typescript", "api"],
            ),
        ],
        queries: vec![
            RetrievalQuery {
                text: "why Redis for sessions",
                target: 0,
                distinctive_terms: "Redis",
            },
            RetrievalQuery {
                text: "gRPC migration decision API",
                target: 1,
                distinctive_terms: "gRPC",
            },
            RetrievalQuery {
                text: "Kubernetes deployment decision",
                target: 3,
                distinctive_terms: "Kubernetes",
            },
            RetrievalQuery {
                text: "TypeScript Python API decision language",
                target: 5,
                distinctive_terms: "TypeScript",
            },
        ],
    };

    let dir = TempDir::new().expect("temp dir");
    run_all_paths(&scenario, &dir);
}

// ==========================================================================
// Scenario 3: Cross-domain retrieval
//
// The query names three topics that meet in exactly one memory; the other
// corpus entries each carry one of them. The assertion is the same shape as
// above - the spanning memory must be in the top 5 - but here the competing
// entries share part of the query vocabulary, so this is the case where a
// ranking regression would show up first. It deliberately does *not* assert
// recall@5 = 1.0 for a multi-relevant query: only the spanning memory is
// asserted, and the printed metrics cover it.
// ==========================================================================
#[test]
fn test_cross_domain_query_retrieves_the_memory_spanning_the_topics() {
    let scenario = RetrievalScenario {
        name: "Cross-Domain Retrieval",
        corpus: vec![
            (
                "Rust's borrow checker prevents data races at compile time.",
                "fact",
                vec!["rust", "safety"],
            ),
            (
                "PostgreSQL MVCC uses snapshots to provide transaction isolation.",
                "fact",
                vec!["postgresql", "database"],
            ),
            (
                "The CAP theorem states that distributed systems can only guarantee two of: consistency, availability, partition tolerance.",
                "concept",
                vec!["distributed-systems", "cap-theorem"],
            ),
            (
                "Rust async runtime Tokio uses a work-stealing scheduler for efficient task distribution.",
                "fact",
                vec!["rust", "async", "tokio"],
            ),
            (
                "PostgreSQL's LISTEN/NOTIFY provides real-time change notifications without polling.",
                "fact",
                vec!["postgresql", "realtime"],
            ),
            (
                "Building a distributed Rust service with PostgreSQL backend requires careful connection pool management.",
                "fact",
                vec!["rust", "postgresql", "distributed"],
            ),
        ],
        queries: vec![
            RetrievalQuery {
                text: "Rust PostgreSQL distributed service",
                target: 5,
                distinctive_terms: "rust + postgresql + distributed (all three in one memory)",
            },
            RetrievalQuery {
                text: "database transaction isolation",
                target: 1,
                distinctive_terms: "transaction isolation",
            },
        ],
    };

    let dir = TempDir::new().expect("temp dir");
    for (index, path) in retrieval_paths().into_iter().enumerate() {
        let storage = open_store(&dir, &format!("cross-domain-{index}"));
        let metrics = run_scenario(&storage, &scenario, path);
        println!(
            "[{} / {}] recall@5 = {:.2}, MRR = {:.3}, precision@3 = {:.3} (only the spanning memory is asserted per query)",
            scenario.name,
            path.label(),
            metrics.recall_at_5,
            metrics.mrr,
            metrics.precision_at_3
        );
        assert!(
            metrics.recall_at_5 > 0.0,
            "[{} / {}] the asserted targets must be found",
            scenario.name,
            path.label()
        );
    }
}

// ==========================================================================
// Pure-maths properties of the decay and activation helpers
//
// These two tests never touch Storage: they pin the shape of
// `StrengthDecay::retrieval_at` and of `ActivationNetwork::activate` themselves.
// They are named after the property, not after a comparison with another
// system, because no other system is exercised here.
// ==========================================================================

/// `StrengthDecay::retrieval_at` must be a decreasing function of elapsed time:
/// a memory just encoded is fully retained, retention at 1 day exceeds
/// retention at 30 days, and both are still above zero.
#[test]
fn test_strength_decay_curve_decreases_with_elapsed_time() {
    let decay = StrengthDecay::new(5.0, 0.0);
    let horizons = [0.0_f64, 1.0, 5.0, 30.0, 365.0];
    let retentions: Vec<f64> = horizons
        .iter()
        .map(|days| decay.retrieval_at(*days))
        .collect();

    println!("StrengthDecay(stability 5d) retention at {horizons:?} = {retentions:?}");

    assert_eq!(
        retentions[0], 1.0,
        "a memory that has not aged must be fully retained"
    );
    assert!(
        retentions.windows(2).all(|pair| pair[1] < pair[0]),
        "retention must decrease strictly with elapsed time: {horizons:?} -> {retentions:?}"
    );
    assert!(
        retentions.iter().all(|retention| *retention > 0.0),
        "retention must stay positive: {retentions:?}"
    );
}

/// `ActivationNetwork::activate` must reach two hops with decaying activation:
/// activation spreads from a seed through its direct neighbour to the second
/// hop, and the direct neighbour carries more activation than the node reached
/// through it.
#[test]
fn test_activation_network_reaches_two_hops_with_decaying_activation() {
    use vestige_core::neuroscience::spreading_activation::LinkType;

    let mut net = ActivationNetwork::default();
    net.add_edge("auth".into(), "session".into(), LinkType::Semantic, 0.8);
    net.add_edge("session".into(), "redis".into(), LinkType::Semantic, 0.7);

    let activated = net.activate("auth", 1.0);
    let ids: Vec<&str> = activated.iter().map(|a| a.memory_id.as_str()).collect();

    println!(
        "activated from 'auth': {:?}",
        activated
            .iter()
            .map(|a| (a.memory_id.as_str(), a.activation, a.distance))
            .collect::<Vec<_>>()
    );

    assert!(
        ids.contains(&"session"),
        "direct neighbour 'session' must be activated; got {ids:?}"
    );
    assert!(
        ids.contains(&"redis"),
        "2-hop neighbour 'redis' must be activated via session; got {ids:?}"
    );

    let session_act = activated
        .iter()
        .find(|a| a.memory_id == "session")
        .expect("session must be present")
        .activation;
    let redis_act = activated
        .iter()
        .find(|a| a.memory_id == "redis")
        .expect("redis must be present")
        .activation;
    assert!(
        session_act > redis_act,
        "activation must decay with distance: session ({session_act:.4}) > redis ({redis_act:.4})"
    );
}

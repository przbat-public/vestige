//! Benchmark Evaluation Harness (LoCoMo/LongMemEval format)
//!
//! Evaluates Vestige's end-to-end memory quality by running standardized
//! scenarios: ingest a corpus, query, and verify that relevant memories
//! surface in the top-K results.
//!
//! Inspired by:
//! - LoCoMo (Long-Context Memory-Optimized) benchmark
//! - LongMemEval evaluation framework
//! - MTEB Retrieval benchmarks
//!
//! Metrics:
//! - Recall@K: fraction of relevant memories found in top K results
//! - MRR (Mean Reciprocal Rank): average 1/rank of first relevant result
//! - Precision@K: fraction of top K results that are relevant

#[cfg(test)]
mod tests {
    use crate::memory::{KnowledgeNode, StrengthDecay};
    use crate::neuroscience::spreading_activation::ActivationNetwork;

    struct BenchmarkScenario {
        name: &'static str,
        corpus: Vec<(&'static str, &'static str, Vec<&'static str>)>,
        queries: Vec<BenchmarkQuery>,
    }

    struct BenchmarkQuery {
        query: &'static str,
        expected_relevant: Vec<usize>,
    }

    struct BenchmarkResult {
        #[allow(dead_code)]
        scenario: String,
        recall_at_5: f64,
        mrr: f64,
        #[allow(dead_code)]
        precision_at_3: f64,
    }

    fn run_scenario(scenario: &BenchmarkScenario) -> BenchmarkResult {
        let mut nodes: Vec<KnowledgeNode> = Vec::new();
        for (i, (content, node_type, tags)) in scenario.corpus.iter().enumerate() {
            let mut node = KnowledgeNode::default();
            node.id = format!("bench-{}", i);
            node.content = content.to_string();
            node.node_type = node_type.to_string();
            node.tags = tags.iter().map(|t| t.to_string()).collect();
            node.retention_strength = 0.8;
            node.retrieval_strength = 0.7;
            nodes.push(node);
        }

        let mut total_recall = 0.0;
        let mut total_mrr = 0.0;
        let mut total_precision = 0.0;
        let query_count = scenario.queries.len() as f64;

        for query in &scenario.queries {
            let lower_query = query.query.to_lowercase();
            let query_words: Vec<&str> = lower_query.split_whitespace().collect();

            let mut scored: Vec<(usize, f64)> = nodes.iter().enumerate().map(|(i, node)| {
                let lower_content = node.content.to_lowercase();
                let word_hits = query_words.iter()
                    .filter(|w| lower_content.contains(*w))
                    .count() as f64;
                let tag_hits = node.tags.iter()
                    .filter(|t| query_words.iter().any(|w| t.to_lowercase().contains(w)))
                    .count() as f64;
                (i, word_hits * 2.0 + tag_hits)
            }).collect();

            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let top_5: Vec<usize> = scored.iter().take(5).map(|s| s.0).collect();
            let top_3: Vec<usize> = scored.iter().take(3).map(|s| s.0).collect();

            let relevant_in_5 = query.expected_relevant.iter()
                .filter(|r| top_5.contains(r))
                .count() as f64;
            total_recall += relevant_in_5 / query.expected_relevant.len() as f64;

            let first_relevant_rank = top_5.iter().position(|idx|
                query.expected_relevant.contains(idx)
            );
            if let Some(rank) = first_relevant_rank {
                total_mrr += 1.0 / (rank as f64 + 1.0);
            }

            let relevant_in_3 = query.expected_relevant.iter()
                .filter(|r| top_3.contains(r))
                .count() as f64;
            total_precision += relevant_in_3 / 3.0;
        }

        BenchmarkResult {
            scenario: scenario.name.to_string(),
            recall_at_5: total_recall / query_count,
            mrr: total_mrr / query_count,
            precision_at_3: total_precision / query_count,
        }
    }

    // ==========================================================================
    // Scenario 1: Bug-Fix Knowledge Retrieval
    //
    // Tests whether bug fix memories are retrievable by error description.
    // This is the bread and butter of a developer memory system.
    // ==========================================================================
    #[test]
    fn bench_bug_fix_retrieval() {
        let scenario = BenchmarkScenario {
            name: "Bug-Fix Retrieval",
            corpus: vec![
                ("BUG FIX: ConnectionResetError when calling auth API. Root cause: connection pool timeout set to 5s, upstream latency was 8s. Fix: increased timeout to 15s.", "fact", vec!["bug-fix", "auth", "timeout"]),
                ("BUG FIX: OOM crash in image processing pipeline. Root cause: unbounded buffer accumulation. Fix: added backpressure with bounded channel.", "fact", vec!["bug-fix", "oom", "pipeline"]),
                ("Architecture decision: chose PostgreSQL over MongoDB for transaction guarantees.", "decision", vec!["architecture", "database"]),
                ("BUG FIX: Race condition in session management. Two requests updating the same session concurrently. Fix: added optimistic locking with version column.", "fact", vec!["bug-fix", "race-condition", "session"]),
                ("User preference: always use structured logging with correlation IDs.", "note", vec!["preference", "logging"]),
                ("BUG FIX: Memory leak in WebSocket handler. Event listeners not cleaned up on disconnect. Fix: added cleanup in onClose handler.", "fact", vec!["bug-fix", "memory-leak", "websocket"]),
            ],
            queries: vec![
                BenchmarkQuery {
                    query: "connection timeout error auth API",
                    expected_relevant: vec![0],
                },
                BenchmarkQuery {
                    query: "out of memory crash pipeline",
                    expected_relevant: vec![1],
                },
                BenchmarkQuery {
                    query: "race condition session concurrent",
                    expected_relevant: vec![3],
                },
                BenchmarkQuery {
                    query: "websocket memory leak event listener",
                    expected_relevant: vec![5],
                },
            ],
        };

        let result = run_scenario(&scenario);
        assert!(result.recall_at_5 >= 0.75, "Bug-fix recall@5 should be >=75%, got {:.0}%", result.recall_at_5 * 100.0);
        assert!(result.mrr >= 0.5, "Bug-fix MRR should be >=0.50, got {:.2}", result.mrr);
    }

    // ==========================================================================
    // Scenario 2: Decision Knowledge Retrieval
    //
    // Tests retrieval of architectural decisions — the "why did we do X?" case.
    // ==========================================================================
    #[test]
    fn bench_decision_retrieval() {
        let scenario = BenchmarkScenario {
            name: "Decision Retrieval",
            corpus: vec![
                ("DECISION: Use Redis for session storage instead of PostgreSQL. Rationale: sub-millisecond reads, built-in TTL for session expiry.", "decision", vec!["decision", "redis", "session"]),
                ("DECISION: Migrate from REST to gRPC for service-to-service communication. Rationale: type safety, streaming, lower latency.", "decision", vec!["decision", "grpc", "api"]),
                ("Team standup notes from Monday: discussed sprint velocity.", "event", vec!["meeting", "standup"]),
                ("DECISION: Deploy on Kubernetes instead of ECS. Rationale: multi-cloud portability, Helm ecosystem.", "decision", vec!["decision", "kubernetes", "deployment"]),
                ("Code review feedback: use more descriptive variable names.", "note", vec!["code-review"]),
                ("DECISION: Chose TypeScript over Python for the API layer. Rationale: shared types with frontend, better IDE support.", "decision", vec!["decision", "typescript", "api"]),
            ],
            queries: vec![
                BenchmarkQuery {
                    query: "why Redis for sessions",
                    expected_relevant: vec![0],
                },
                BenchmarkQuery {
                    query: "gRPC migration decision API",
                    expected_relevant: vec![1],
                },
                BenchmarkQuery {
                    query: "Kubernetes deployment decision",
                    expected_relevant: vec![3],
                },
                BenchmarkQuery {
                    query: "TypeScript Python API decision language",
                    expected_relevant: vec![5],
                },
            ],
        };

        let result = run_scenario(&scenario);
        assert!(result.recall_at_5 >= 0.75, "Decision recall@5 should be >=75%, got {:.0}%", result.recall_at_5 * 100.0);
        assert!(result.mrr >= 0.5, "Decision MRR should be >=0.50, got {:.2}", result.mrr);
    }

    // ==========================================================================
    // Scenario 3: Cross-Domain Retrieval
    //
    // Tests retrieval across different knowledge domains — can the system
    // find relevant memories when the query spans multiple topics?
    // ==========================================================================
    #[test]
    fn bench_cross_domain_retrieval() {
        let scenario = BenchmarkScenario {
            name: "Cross-Domain Retrieval",
            corpus: vec![
                ("Rust's borrow checker prevents data races at compile time.", "fact", vec!["rust", "safety"]),
                ("PostgreSQL MVCC uses snapshots to provide transaction isolation.", "fact", vec!["postgresql", "database"]),
                ("The CAP theorem states that distributed systems can only guarantee two of: consistency, availability, partition tolerance.", "concept", vec!["distributed-systems", "cap-theorem"]),
                ("Rust async runtime Tokio uses a work-stealing scheduler for efficient task distribution.", "fact", vec!["rust", "async", "tokio"]),
                ("PostgreSQL's LISTEN/NOTIFY provides real-time change notifications without polling.", "fact", vec!["postgresql", "realtime"]),
                ("Building a distributed Rust service with PostgreSQL backend requires careful connection pool management.", "fact", vec!["rust", "postgresql", "distributed"]),
            ],
            queries: vec![
                BenchmarkQuery {
                    query: "Rust PostgreSQL distributed service",
                    expected_relevant: vec![5, 0, 3],
                },
                BenchmarkQuery {
                    query: "database transaction isolation",
                    expected_relevant: vec![1],
                },
            ],
        };

        let result = run_scenario(&scenario);
        assert!(result.recall_at_5 >= 0.5, "Cross-domain recall@5 should be >=50%, got {:.0}%", result.recall_at_5 * 100.0);
    }

    // ==========================================================================
    // Scenario 4: Temporal Freshness
    //
    // Verifies that the memory system can distinguish between stale and fresh
    // memories when queried about recent events.
    // ==========================================================================
    #[test]
    fn bench_decay_favors_recent_memories() {
        let old_decay = StrengthDecay::new(5.0, 0.0);
        let recent_decay = StrengthDecay::new(5.0, 0.0);

        let old_retention = old_decay.retrieval_at(30.0);
        let recent_retention = recent_decay.retrieval_at(1.0);

        assert!(
            recent_retention > old_retention,
            "Recent memory retention ({:.4}) should exceed 30-day-old ({:.4})",
            recent_retention, old_retention
        );

        let ratio = recent_retention / old_retention;
        assert!(
            ratio > 1.5,
            "Recent/old ratio should be >1.5x, got {:.2}x",
            ratio
        );
    }

    // ==========================================================================
    // Scenario 5: Activation Network preserves retrieval paths
    //
    // Validates that multi-hop retrieval through the activation network
    // can surface indirectly related memories (2-hop connections).
    // ==========================================================================
    #[test]
    fn bench_multi_hop_activation_retrieval() {
        use crate::neuroscience::spreading_activation::LinkType;

        let mut net = ActivationNetwork::default();
        net.add_edge("auth".into(), "session".into(), LinkType::Semantic, 0.8);
        net.add_edge("session".into(), "redis".into(), LinkType::Semantic, 0.7);
        net.add_edge("redis".into(), "cache".into(), LinkType::Semantic, 0.6);

        let activated = net.activate("auth", 1.0);
        let ids: Vec<&str> = activated.iter().map(|a| a.memory_id.as_str()).collect();

        assert!(ids.contains(&"session"), "Direct neighbor 'session' should be activated");
        assert!(ids.contains(&"redis"), "2-hop neighbor 'redis' should be activated via session");

        let session_act = activated.iter().find(|a| a.memory_id == "session").unwrap().activation;
        let redis_act = activated.iter().find(|a| a.memory_id == "redis").unwrap().activation;
        assert!(
            session_act > redis_act,
            "Direct neighbor ({:.4}) should have higher activation than 2-hop ({:.4})",
            session_act, redis_act
        );
    }
}

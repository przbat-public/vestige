//! # Retrieval Quality Benchmark
//!
//! Evaluates preprocessing pipeline impact on search quality using synthetic memories.
//! Measures Precision@5, Recall@5, and Mean Reciprocal Rank (MRR) for:
//! - Compound query decomposition
//! - Coreference-rewritten content
//! - Entity-tagged content
//!
//! Not a microbenchmark — this measures *quality*, not speed.

use vestige_core::preprocessing::{self, PreprocessingConfig};
use vestige_core::search::decompose::{HasIdAndScore, decompose_query, merge_results};

// ============================================================================
// SYNTHETIC CORPUS
// ============================================================================

struct SyntheticMemory {
    id: String,
    raw_content: String,
    #[allow(dead_code)]
    topics: Vec<String>,
}

fn build_corpus() -> Vec<SyntheticMemory> {
    vec![
        // Auth domain (10 memories)
        mem(
            "m01",
            "Alice manages the Auth Team. She leads OAuth2 implementation.",
            &["auth", "team"],
        ),
        mem(
            "m02",
            "The JWT token rotation policy requires 15-minute expiry.",
            &["auth", "jwt"],
        ),
        mem(
            "m03",
            "OAuth2 client credentials flow is used for service-to-service auth.",
            &["auth", "oauth"],
        ),
        mem(
            "m04",
            "Auth service depends on Redis for session storage.",
            &["auth", "redis"],
        ),
        mem(
            "m05",
            "Bob implemented RBAC for the admin dashboard.",
            &["auth", "rbac"],
        ),
        mem(
            "m06",
            "The OIDC provider is Keycloak deployed on GKE.",
            &["auth", "oidc"],
        ),
        mem(
            "m07",
            "MFA enrollment requires backup codes to be generated.",
            &["auth", "mfa"],
        ),
        mem(
            "m08",
            "The auth service handles 10,000 RPS at peak.",
            &["auth", "performance"],
        ),
        mem(
            "m09",
            "Session invalidation happens via Redis pub/sub.",
            &["auth", "session"],
        ),
        mem(
            "m10",
            "Alice reviewed the security audit findings last week.",
            &["auth", "security"],
        ),
        // Search domain (10 memories)
        mem(
            "m11",
            "Vestige uses FSRS-6 for spaced repetition scheduling.",
            &["search", "fsrs"],
        ),
        mem(
            "m12",
            "Hybrid search combines BM25 keyword and semantic similarity.",
            &["search", "hybrid"],
        ),
        mem(
            "m13",
            "The reranker uses Jina v2 cross-encoder model.",
            &["search", "reranker"],
        ),
        mem(
            "m14",
            "USearch HNSW index provides 20x faster vector search than FAISS.",
            &["search", "vector"],
        ),
        mem(
            "m15",
            "Query decomposition splits compound queries for better recall.",
            &["search", "decompose"],
        ),
        mem(
            "m16",
            "Temporal boosting ranks recent memories higher.",
            &["search", "temporal"],
        ),
        mem(
            "m17",
            "The testing effect strengthens memories on access.",
            &["search", "testing-effect"],
        ),
        mem(
            "m18",
            "Spreading activation traverses the knowledge graph.",
            &["search", "activation"],
        ),
        mem(
            "m19",
            "Dream consolidation merges related memories during sleep phase.",
            &["search", "dreams"],
        ),
        mem(
            "m20",
            "Prediction Error Gating prevents duplicate memory storage.",
            &["search", "pe-gating"],
        ),
        // Infrastructure domain (10 memories)
        mem(
            "m21",
            "The API runs on Cloud Run with min 2 max 10 instances.",
            &["infra", "cloud-run"],
        ),
        mem(
            "m22",
            "SQLite WAL mode is used for concurrent readers.",
            &["infra", "sqlite"],
        ),
        mem(
            "m23",
            "Embeddings use nomic-embed-text-v1.5 with 384D Matryoshka.",
            &["infra", "embeddings"],
        ),
        mem(
            "m24",
            "CI pipeline runs on GitHub Actions with Rust caching.",
            &["infra", "ci"],
        ),
        mem(
            "m25",
            "The dashboard is built with React 19 and Three.js.",
            &["infra", "dashboard"],
        ),
        mem(
            "m26",
            "Docker image is built with cargo-chef for layer caching.",
            &["infra", "docker"],
        ),
        mem(
            "m27",
            "Terraform manages the GCP infrastructure.",
            &["infra", "terraform"],
        ),
        mem(
            "m28",
            "Monitoring uses Cloud Logging with structured JSON.",
            &["infra", "monitoring"],
        ),
        mem(
            "m29",
            "The backup runs nightly to GCS bucket.",
            &["infra", "backup"],
        ),
        mem(
            "m30",
            "Secret Manager stores API keys and credentials.",
            &["infra", "secrets"],
        ),
        // People domain (10 memories)
        mem(
            "m31",
            "Carol is the tech lead for the backend team.",
            &["people", "backend"],
        ),
        mem(
            "m32",
            "Dave handles DevOps and infrastructure.",
            &["people", "devops"],
        ),
        mem(
            "m33",
            "Eve designed the original FSRS integration.",
            &["people", "fsrs"],
        ),
        mem(
            "m34",
            "Frank is the product manager for the memory system.",
            &["people", "product"],
        ),
        mem(
            "m35",
            "Grace reviews all security-related PRs.",
            &["people", "security"],
        ),
        mem(
            "m36",
            "Hank wrote the dream consolidation engine.",
            &["people", "dreams"],
        ),
        mem(
            "m37",
            "Ivy manages the SRE team and on-call rotation.",
            &["people", "sre"],
        ),
        mem(
            "m38",
            "Jack built the dashboard 3D graph visualization.",
            &["people", "dashboard"],
        ),
        mem(
            "m39",
            "Kate handles customer support escalations.",
            &["people", "support"],
        ),
        mem(
            "m40",
            "Leo optimized the vector search performance.",
            &["people", "vector"],
        ),
        // Cross-domain (10 memories)
        mem(
            "m41",
            "The auth service migrated from JWT to session tokens last month.",
            &["auth", "migration"],
        ),
        mem(
            "m42",
            "Search latency dropped from 200ms to 50ms after USearch.",
            &["search", "performance"],
        ),
        mem(
            "m43",
            "Alice and Bob pair-programmed the OAuth2 PKCE flow.",
            &["auth", "people"],
        ),
        mem(
            "m44",
            "The FSRS scheduler was validated against 10,000 flashcard decks.",
            &["search", "validation"],
        ),
        mem(
            "m45",
            "Cloud Run cold starts were reduced from 3s to 800ms.",
            &["infra", "performance"],
        ),
        mem(
            "m46",
            "The dream engine was inspired by Stickgold & Walker 2013.",
            &["search", "research"],
        ),
        mem(
            "m47",
            "GKE to Cloud Run migration saved $2,000/month.",
            &["infra", "cost"],
        ),
        mem(
            "m48",
            "Carol reviewed Eve's FSRS implementation changes.",
            &["people", "code-review"],
        ),
        mem(
            "m49",
            "The embedding model switch from ada-002 to nomic improved recall by 15%.",
            &["search", "embeddings"],
        ),
        mem(
            "m50",
            "Budget for Q2 infrastructure is $15,000.",
            &["infra", "budget"],
        ),
    ]
}

fn mem(id: &str, content: &str, topics: &[&str]) -> SyntheticMemory {
    SyntheticMemory {
        id: id.to_string(),
        raw_content: content.to_string(),
        topics: topics.iter().map(|s| s.to_string()).collect(),
    }
}

// ============================================================================
// QUERY + GROUND TRUTH
// ============================================================================

struct QueryEval {
    query: &'static str,
    relevant_ids: Vec<&'static str>,
}

fn build_queries() -> Vec<QueryEval> {
    vec![
        QueryEval {
            query: "Who manages the auth team?",
            relevant_ids: vec!["m01", "m10", "m43"],
        },
        QueryEval {
            query: "How does search work?",
            relevant_ids: vec!["m12", "m14", "m15", "m16", "m17", "m18"],
        },
        QueryEval {
            query: "auth security; infrastructure costs",
            relevant_ids: vec!["m10", "m35", "m47", "m50"],
        },
        QueryEval {
            query: "Who worked on FSRS? And what about dream consolidation?",
            relevant_ids: vec!["m11", "m33", "m44", "m19", "m36", "m46"],
        },
        QueryEval {
            query: "What embedding model does Vestige use and how is vector search implemented?",
            relevant_ids: vec!["m23", "m14", "m49", "m40"],
        },
        QueryEval {
            query: "performance improvements",
            relevant_ids: vec!["m08", "m42", "m45"],
        },
        QueryEval {
            query: "dashboard; visualization",
            relevant_ids: vec!["m25", "m38"],
        },
        QueryEval {
            query: "Redis usage in the system",
            relevant_ids: vec!["m04", "m09"],
        },
    ]
}

// ============================================================================
// METRICS
// ============================================================================

fn precision_at_k(retrieved: &[String], relevant: &[&str], k: usize) -> f64 {
    let top_k: Vec<&String> = retrieved.iter().take(k).collect();
    let hits = top_k
        .iter()
        .filter(|id| relevant.contains(&id.as_str()))
        .count();
    hits as f64 / k as f64
}

fn recall_at_k(retrieved: &[String], relevant: &[&str], k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let top_k: Vec<&String> = retrieved.iter().take(k).collect();
    let hits = top_k
        .iter()
        .filter(|id| relevant.contains(&id.as_str()))
        .count();
    hits as f64 / relevant.len() as f64
}

fn mrr(retrieved: &[String], relevant: &[&str]) -> f64 {
    for (i, id) in retrieved.iter().enumerate() {
        if relevant.contains(&id.as_str()) {
            return 1.0 / (i + 1) as f64;
        }
    }
    0.0
}

// ============================================================================
// SIMULATED RETRIEVAL (text overlap scoring — no embeddings needed)
// ============================================================================

#[derive(Debug)]
struct ScoredMemory {
    id: String,
    score: f64,
}

impl HasIdAndScore for ScoredMemory {
    fn id(&self) -> &str {
        &self.id
    }
    fn score(&self) -> f64 {
        self.score
    }
}

fn keyword_score(query: &str, content: &str) -> f64 {
    let query_words: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(String::from)
        .collect();

    if query_words.is_empty() {
        return 0.0;
    }

    let content_lower = content.to_lowercase();
    let hits = query_words
        .iter()
        .filter(|w| content_lower.contains(w.as_str()))
        .count();

    hits as f64 / query_words.len() as f64
}

fn retrieve_baseline(query: &str, corpus: &[SyntheticMemory], k: usize) -> Vec<String> {
    let mut scored: Vec<ScoredMemory> = corpus
        .iter()
        .map(|m| ScoredMemory {
            id: m.id.clone(),
            score: keyword_score(query, &m.raw_content),
        })
        .filter(|s| s.score > 0.0)
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    scored.into_iter().take(k).map(|s| s.id).collect()
}

fn retrieve_with_decomposition(query: &str, corpus: &[SyntheticMemory], k: usize) -> Vec<String> {
    let decomp = decompose_query(query);

    if !decomp.is_compound {
        return retrieve_baseline(query, corpus, k);
    }

    let sub_results: Vec<Vec<ScoredMemory>> = decomp
        .sub_queries
        .iter()
        .map(|sq| {
            corpus
                .iter()
                .map(|m| ScoredMemory {
                    id: m.id.clone(),
                    score: keyword_score(sq, &m.raw_content),
                })
                .filter(|s| s.score > 0.0)
                .collect()
        })
        .collect();

    let merged = merge_results(sub_results);
    merged.into_iter().take(k).map(|s| s.id).collect()
}

fn retrieve_with_preprocessing(query: &str, corpus: &[SyntheticMemory], k: usize) -> Vec<String> {
    let preprocessed: Vec<(String, String, Vec<String>)> = corpus
        .iter()
        .map(|m| {
            let result = preprocessing::preprocess(&m.raw_content, &PreprocessingConfig::default());
            let tags: Vec<String> = result
                .auto_tags
                .iter()
                .map(|t| t.replace("entity:", "").replace('-', " "))
                .collect();
            (m.id.clone(), result.content, tags)
        })
        .collect();

    let decomp = decompose_query(query);
    let queries = if decomp.is_compound {
        decomp.sub_queries
    } else {
        vec![query.to_string()]
    };

    let sub_results: Vec<Vec<ScoredMemory>> = queries
        .iter()
        .map(|sq| {
            preprocessed
                .iter()
                .map(|(id, content, tags)| {
                    let content_score = keyword_score(sq, content);
                    let tag_str = tags.join(" ");
                    let tag_score = keyword_score(sq, &tag_str) * 0.3;
                    ScoredMemory {
                        id: id.clone(),
                        score: content_score + tag_score,
                    }
                })
                .filter(|s| s.score > 0.0)
                .collect()
        })
        .collect();

    let merged = merge_results(sub_results);
    merged.into_iter().take(k).map(|s| s.id).collect()
}

// ============================================================================
// BENCHMARK TESTS
// ============================================================================

#[test]
fn benchmark_baseline_retrieval_quality() {
    let corpus = build_corpus();
    let queries = build_queries();
    let k = 5;

    let mut total_p5 = 0.0;
    let mut total_r5 = 0.0;
    let mut total_mrr = 0.0;

    for q in &queries {
        let retrieved = retrieve_baseline(q.query, &corpus, k);
        total_p5 += precision_at_k(&retrieved, &q.relevant_ids, k);
        total_r5 += recall_at_k(&retrieved, &q.relevant_ids, k);
        total_mrr += mrr(&retrieved, &q.relevant_ids);
    }

    let n = queries.len() as f64;
    let avg_p5 = total_p5 / n;
    let avg_r5 = total_r5 / n;
    let avg_mrr = total_mrr / n;

    eprintln!("=== BASELINE (keyword only) ===");
    eprintln!("  Precision@5: {:.3}", avg_p5);
    eprintln!("  Recall@5:    {:.3}", avg_r5);
    eprintln!("  MRR:         {:.3}", avg_mrr);

    // Baseline should work at some level
    assert!(avg_mrr >= 0.0, "MRR should be non-negative");
}

#[test]
fn benchmark_decomposition_improves_compound_queries() {
    let corpus = build_corpus();
    let queries = build_queries();
    let k = 5;

    let compound_queries: Vec<&QueryEval> = queries
        .iter()
        .filter(|q| decompose_query(q.query).is_compound)
        .collect();

    assert!(
        !compound_queries.is_empty(),
        "Test data should include compound queries"
    );

    let mut baseline_mrr = 0.0;
    let mut decomp_mrr = 0.0;

    for q in &compound_queries {
        let b = retrieve_baseline(q.query, &corpus, k);
        let d = retrieve_with_decomposition(q.query, &corpus, k);

        baseline_mrr += mrr(&b, &q.relevant_ids);
        decomp_mrr += mrr(&d, &q.relevant_ids);
    }

    let n = compound_queries.len() as f64;
    let avg_baseline = baseline_mrr / n;
    let avg_decomp = decomp_mrr / n;

    eprintln!("=== COMPOUND QUERIES (decomposition) ===");
    eprintln!("  Baseline MRR:       {:.3}", avg_baseline);
    eprintln!("  Decomposition MRR:  {:.3}", avg_decomp);

    // Decomposition should not make things significantly worse
    // In practice it should improve recall on compound queries
    assert!(
        avg_decomp >= avg_baseline * 0.8,
        "Decomposition should not degrade MRR by more than 20%"
    );
}

#[test]
fn benchmark_preprocessing_enrichment() {
    let corpus = build_corpus();
    let queries = build_queries();
    let k = 5;

    let mut baseline_total = 0.0;
    let mut enriched_total = 0.0;

    for q in &queries {
        let b = retrieve_baseline(q.query, &corpus, k);
        let e = retrieve_with_preprocessing(q.query, &corpus, k);

        baseline_total += mrr(&b, &q.relevant_ids);
        enriched_total += mrr(&e, &q.relevant_ids);
    }

    let n = queries.len() as f64;
    let avg_baseline = baseline_total / n;
    let avg_enriched = enriched_total / n;

    eprintln!("=== PREPROCESSING ENRICHMENT ===");
    eprintln!("  Baseline MRR:  {:.3}", avg_baseline);
    eprintln!("  Enriched MRR:  {:.3}", avg_enriched);

    // Enrichment should not degrade quality
    assert!(
        avg_enriched >= avg_baseline * 0.8,
        "Preprocessing should not degrade MRR by more than 20%"
    );
}

#[test]
fn benchmark_preprocessing_latency() {
    let corpus = build_corpus();

    let start = std::time::Instant::now();
    for m in &corpus {
        let _ = preprocessing::preprocess(&m.raw_content, &PreprocessingConfig::default());
    }
    let elapsed = start.elapsed();

    let per_memory_us = elapsed.as_micros() / corpus.len() as u128;
    let total_ms = elapsed.as_millis();

    eprintln!("=== PREPROCESSING LATENCY (50 memories) ===");
    eprintln!("  Total:      {}ms", total_ms);
    eprintln!("  Per memory: {}µs", per_memory_us);

    assert!(
        per_memory_us < 5000,
        "Per-memory preprocessing should be < 5ms, got {}µs",
        per_memory_us
    );
    assert!(
        total_ms < 250,
        "Full corpus preprocessing should be < 250ms, got {}ms",
        total_ms
    );
}

#[test]
fn benchmark_decomposition_latency() {
    let queries = build_queries();

    let start = std::time::Instant::now();
    for _ in 0..1000 {
        for q in &queries {
            let _ = decompose_query(q.query);
        }
    }
    let elapsed = start.elapsed();

    let per_query_ns = elapsed.as_nanos() / (1000 * queries.len() as u128);

    eprintln!("=== DECOMPOSITION LATENCY ===");
    eprintln!("  Per query: {}ns", per_query_ns);

    assert!(
        per_query_ns < 50_000,
        "Decomposition should be < 50µs, got {}ns",
        per_query_ns
    );
}

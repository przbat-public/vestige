//! # LoCoMo Benchmark Harness for Vestige
//!
//! Evaluates Vestige's long-term conversational memory against the LoCoMo benchmark
//! (Maharana et al., ACL 2024). Uses the official `locomo10.json` dataset: 10 conversations,
//! ~300 turns each across ~27 sessions, with 1,540 annotated QA pairs.
//!
//! ## Two-phase evaluation
//!
//! **Phase 1 (this binary)**: Retrieval-only — ingest sessions, search, measure whether
//! evidence passages surface in top-K. No LLM needed. Outputs `retrieval_results.json`.
//!
//! **Phase 2 (`evaluate.py`)**: LLM judge — generates answers from retrieved context,
//! scores with GPT-4o. Produces the comparable LoCoMo LLM Judge Score.
//!
//! ## Competitor scores (LoCoMo LLM Judge)
//!
//! | System        | Overall |
//! |---------------|---------|
//! | Memobase      |  75.78% |
//! | Zep (updated) |  75.14% |
//! | Mem0-Graph    |  68.44% |
//! | Mem0          |  66.88% |
//! | LangMem       |  58.10% |
//! | OpenAI        |  52.90% |

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use vestige_core::memory::IngestInput;
use vestige_core::storage::Storage;

// ============================================================================
// LoCoMo dataset types
// ============================================================================

#[derive(Deserialize)]
struct LoCoMoSample {
    #[serde(default)]
    sample_id: String,
    qa: Vec<QAPair>,
    conversation: serde_json::Value,
}

#[derive(Deserialize, Clone)]
struct QAPair {
    question: String,
    #[serde(default)]
    answer: serde_json::Value,
    #[serde(default)]
    #[allow(dead_code)]
    adversarial_answer: Option<String>,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    category: u32,
}

#[derive(Debug, Clone)]
struct SessionChunk {
    session_key: String,
    timestamp: String,
    turns: Vec<Turn>,
}

#[derive(Debug, Clone)]
struct Turn {
    speaker: String,
    dia_id: String,
    text: String,
}

// ============================================================================
// Output types
// ============================================================================

#[derive(Serialize)]
struct BenchmarkOutput {
    vestige_version: String,
    dataset: String,
    total_conversations: usize,
    total_questions: usize,
    skipped_adversarial: usize,
    ingestion_time_secs: f64,
    search_time_secs: f64,
    retrieval_metrics: RetrievalMetrics,
    per_category: HashMap<String, RetrievalMetrics>,
    results: Vec<QuestionResult>,
}

#[derive(Serialize, Default, Clone)]
struct RetrievalMetrics {
    recall_at_5: f64,
    recall_at_10: f64,
    mrr: f64,
    count: usize,
}

#[derive(Serialize)]
struct QuestionResult {
    sample_id: String,
    question: String,
    ground_truth: String,
    category: u32,
    category_name: String,
    evidence_ids: Vec<String>,
    retrieved_contexts: Vec<String>,
    retrieved_scores: Vec<f32>,
    evidence_found_in_top_5: bool,
    evidence_found_in_top_10: bool,
    reciprocal_rank: f64,
}

// ============================================================================
// Dataset parsing
// ============================================================================

fn parse_sessions(conversation: &serde_json::Value) -> Vec<SessionChunk> {
    let obj = match conversation.as_object() {
        Some(o) => o,
        None => return vec![],
    };

    let mut session_keys: Vec<String> = obj
        .keys()
        .filter(|k| k.starts_with("session_") && !k.contains("date_time") && !k.contains("summary") && !k.contains("observation"))
        .filter(|k| obj.get(k.as_str()).map_or(false, |v| v.is_array()))
        .cloned()
        .collect();

    session_keys.sort_by(|a, b| {
        let num_a: u32 = a.strip_prefix("session_").and_then(|s| s.parse().ok()).unwrap_or(0);
        let num_b: u32 = b.strip_prefix("session_").and_then(|s| s.parse().ok()).unwrap_or(0);
        num_a.cmp(&num_b)
    });

    session_keys
        .into_iter()
        .map(|key| {
            let ts_key = format!("{}_date_time", key);
            let timestamp = obj
                .get(&ts_key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let turns: Vec<Turn> = obj
                .get(&key)
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|turn| {
                            let speaker = turn.get("speaker")?.as_str()?.to_string();
                            let dia_id = turn.get("dia_id")?.as_str()?.to_string();
                            let text = turn.get("text")?.as_str()?.to_string();
                            Some(Turn { speaker, dia_id, text })
                        })
                        .collect()
                })
                .unwrap_or_default();

            SessionChunk { session_key: key, timestamp, turns }
        })
        .collect()
}

fn session_to_content(session: &SessionChunk) -> String {
    let mut content = String::new();
    if !session.timestamp.is_empty() {
        content.push_str(&format!("[{}]\n", session.timestamp));
    }
    for turn in &session.turns {
        content.push_str(&format!("{}: {}\n", turn.speaker, turn.text));
    }
    content
}

fn session_dia_ids(session: &SessionChunk) -> Vec<String> {
    session.turns.iter().map(|t| t.dia_id.clone()).collect()
}

fn category_name(cat: u32) -> &'static str {
    match cat {
        1 => "single_hop",
        2 => "temporal",
        3 => "multi_hop",
        4 => "open_domain",
        5 => "adversarial",
        _ => "unknown",
    }
}

fn answer_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

// ============================================================================
// Retrieval evaluation
// ============================================================================

fn evaluate_retrieval(
    evidence_ids: &[String],
    retrieved_dia_ids: &[Vec<String>],
    k: usize,
) -> (bool, f64) {
    if evidence_ids.is_empty() {
        return (false, 0.0);
    }

    let top_k = &retrieved_dia_ids[..retrieved_dia_ids.len().min(k)];

    let mut found = false;
    let mut first_rank = None;

    for (rank, chunk_ids) in top_k.iter().enumerate() {
        if evidence_ids.iter().any(|eid| chunk_ids.contains(eid)) {
            if first_rank.is_none() {
                first_rank = Some(rank);
            }
            found = true;
            break;
        }
    }

    let rr = first_rank.map(|r| 1.0 / (r as f64 + 1.0)).unwrap_or(0.0);
    (found, rr)
}

// ============================================================================
// Main benchmark runner
// ============================================================================

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    let dataset_path = args.get(1).map(PathBuf::from).unwrap_or_else(|| {
        let default = PathBuf::from("benchmarks/locomo/data/locomo10.json");
        if default.exists() {
            default
        } else {
            eprintln!("Usage: locomo-bench <path-to-locomo10.json> [output.json]");
            eprintln!();
            eprintln!("Download the dataset:");
            eprintln!("  mkdir -p benchmarks/locomo/data");
            eprintln!("  curl -L -o benchmarks/locomo/data/locomo10.json \\");
            eprintln!("    https://raw.githubusercontent.com/snap-research/locomo/main/data/locomo10.json");
            std::process::exit(1);
        }
    });

    let output_path = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("benchmarks/locomo/retrieval_results.json"));

    eprintln!("=== Vestige LoCoMo Benchmark ===");
    eprintln!("Dataset: {}", dataset_path.display());
    eprintln!("Output:  {}", output_path.display());
    eprintln!();

    // Load dataset
    let data_str = std::fs::read_to_string(&dataset_path).expect("Failed to read dataset");
    let samples: Vec<LoCoMoSample> = serde_json::from_str(&data_str).expect("Failed to parse JSON");

    eprintln!("Loaded {} conversations", samples.len());

    let mut all_results: Vec<QuestionResult> = Vec::new();
    let mut total_ingest_time = 0.0_f64;
    let mut total_search_time = 0.0_f64;
    let mut skipped_adversarial = 0_usize;

    for (conv_idx, sample) in samples.iter().enumerate() {
        let sample_id = if sample.sample_id.is_empty() {
            format!("conv-{}", conv_idx)
        } else {
            sample.sample_id.clone()
        };

        let sessions = parse_sessions(&sample.conversation);
        if sessions.is_empty() {
            eprintln!("  [{}] No sessions found, skipping", sample_id);
            continue;
        }

        eprintln!(
            "  [{}/{}] {} — {} sessions, {} QA pairs",
            conv_idx + 1,
            samples.len(),
            sample_id,
            sessions.len(),
            sample.qa.len()
        );

        // Create temporary storage for this conversation
        let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db_path = tmp_dir.path().join("locomo.db");
        let storage = Storage::new(Some(db_path)).expect("Failed to create storage");

        // Track which dia_ids are in each ingested memory
        let mut memory_id_to_dia_ids: HashMap<String, Vec<String>> = HashMap::new();

        // --- INGEST PHASE ---
        let ingest_start = Instant::now();
        for session in &sessions {
            let content = session_to_content(session);
            if content.trim().is_empty() {
                continue;
            }

            let dia_ids = session_dia_ids(session);
            let tags: Vec<String> = vec![
                format!("session:{}", session.session_key),
                format!("conversation:{}", sample_id),
            ];

            let input = IngestInput {
                content,
                node_type: "event".to_string(),
                source: Some(format!("locomo/{}/{}", sample_id, session.session_key)),
                tags,
                ..Default::default()
            };

            match storage.ingest(input) {
                Ok(node) => {
                    memory_id_to_dia_ids.insert(node.id.clone(), dia_ids);
                }
                Err(e) => {
                    eprintln!("    WARN: ingest failed for {}: {}", session.session_key, e);
                }
            }
        }
        let ingest_elapsed = ingest_start.elapsed().as_secs_f64();
        total_ingest_time += ingest_elapsed;
        eprintln!("    Ingested {} sessions in {:.1}s", sessions.len(), ingest_elapsed);

        // --- SEARCH PHASE ---
        let search_start = Instant::now();

        let non_adversarial: Vec<&QAPair> = sample
            .qa
            .iter()
            .filter(|qa| qa.category != 5)
            .collect();

        let adversarial_count = sample.qa.len() - non_adversarial.len();
        skipped_adversarial += adversarial_count;

        for qa in &non_adversarial {
            let results = storage.hybrid_search(&qa.question, 10, 0.4, 0.6);

            let (retrieved_contexts, retrieved_scores, retrieved_dia_id_sets): (
                Vec<String>,
                Vec<f32>,
                Vec<Vec<String>>,
            ) = match results {
                Ok(res) => {
                    let contexts: Vec<String> =
                        res.iter().map(|r| r.node.content.clone()).collect();
                    let scores: Vec<f32> = res.iter().map(|r| r.combined_score).collect();
                    let dia_sets: Vec<Vec<String>> = res
                        .iter()
                        .map(|r| {
                            memory_id_to_dia_ids
                                .get(&r.node.id)
                                .cloned()
                                .unwrap_or_default()
                        })
                        .collect();
                    (contexts, scores, dia_sets)
                }
                Err(e) => {
                    eprintln!("    WARN: search failed: {}", e);
                    (vec![], vec![], vec![])
                }
            };

            let (found_5, rr) = evaluate_retrieval(&qa.evidence, &retrieved_dia_id_sets, 5);
            let (found_10, _) = evaluate_retrieval(&qa.evidence, &retrieved_dia_id_sets, 10);

            all_results.push(QuestionResult {
                sample_id: sample_id.clone(),
                question: qa.question.clone(),
                ground_truth: answer_to_string(&qa.answer),
                category: qa.category,
                category_name: category_name(qa.category).to_string(),
                evidence_ids: qa.evidence.clone(),
                retrieved_contexts,
                retrieved_scores,
                evidence_found_in_top_5: found_5,
                evidence_found_in_top_10: found_10,
                reciprocal_rank: rr,
            });
        }

        let search_elapsed = search_start.elapsed().as_secs_f64();
        total_search_time += search_elapsed;
        eprintln!(
            "    Searched {} questions in {:.1}s ({:.0}ms/query)",
            non_adversarial.len(),
            search_elapsed,
            (search_elapsed / non_adversarial.len().max(1) as f64) * 1000.0
        );
    }

    // --- COMPUTE METRICS ---
    let total_questions = all_results.len();

    let overall = compute_metrics(&all_results);

    let mut per_category: HashMap<String, RetrievalMetrics> = HashMap::new();
    for cat in [1, 2, 3, 4] {
        let name = category_name(cat).to_string();
        let subset: Vec<&QuestionResult> = all_results.iter().filter(|r| r.category == cat).collect();
        if !subset.is_empty() {
            per_category.insert(name, compute_metrics_refs(&subset));
        }
    }

    // --- PRINT RESULTS ---
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════╗");
    eprintln!("║             VESTIGE LoCoMo RETRIEVAL RESULTS        ║");
    eprintln!("╠══════════════════════════════════════════════════════╣");
    eprintln!(
        "║  Questions: {:>4} (skipped {} adversarial)          ║",
        total_questions, skipped_adversarial
    );
    eprintln!("╠══════════════════════════════════════════════════════╣");
    eprintln!(
        "║  Recall@5:     {:>6.2}%                               ║",
        overall.recall_at_5 * 100.0
    );
    eprintln!(
        "║  Recall@10:    {:>6.2}%                               ║",
        overall.recall_at_10 * 100.0
    );
    eprintln!(
        "║  MRR:          {:>6.4}                                ║",
        overall.mrr
    );
    eprintln!("╠══════════════════════════════════════════════════════╣");

    for (name, metrics) in &per_category {
        eprintln!(
            "║  {:12} R@5={:5.1}%  R@10={:5.1}%  MRR={:.3}  n={} ║",
            name,
            metrics.recall_at_5 * 100.0,
            metrics.recall_at_10 * 100.0,
            metrics.mrr,
            metrics.count
        );
    }

    eprintln!("╠══════════════════════════════════════════════════════╣");
    eprintln!(
        "║  Ingest: {:.1}s  Search: {:.1}s  Total: {:.1}s          ║",
        total_ingest_time,
        total_search_time,
        total_ingest_time + total_search_time
    );
    eprintln!("╚══════════════════════════════════════════════════════╝");

    // --- WRITE OUTPUT ---
    let output = BenchmarkOutput {
        vestige_version: "3.2.0".to_string(),
        dataset: "locomo10".to_string(),
        total_conversations: samples.len(),
        total_questions,
        skipped_adversarial,
        ingestion_time_secs: total_ingest_time,
        search_time_secs: total_search_time,
        retrieval_metrics: overall,
        per_category,
        results: all_results,
    };

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string_pretty(&output).expect("Failed to serialize output");
    std::fs::write(&output_path, &json).expect("Failed to write output");
    eprintln!();
    eprintln!("Results written to {}", output_path.display());
    eprintln!("Run `python benchmarks/locomo/evaluate.py` for LLM judge scores.");
}

fn compute_metrics(results: &[QuestionResult]) -> RetrievalMetrics {
    if results.is_empty() {
        return RetrievalMetrics::default();
    }
    let n = results.len() as f64;
    RetrievalMetrics {
        recall_at_5: results.iter().filter(|r| r.evidence_found_in_top_5).count() as f64 / n,
        recall_at_10: results.iter().filter(|r| r.evidence_found_in_top_10).count() as f64 / n,
        mrr: results.iter().map(|r| r.reciprocal_rank).sum::<f64>() / n,
        count: results.len(),
    }
}

fn compute_metrics_refs(results: &[&QuestionResult]) -> RetrievalMetrics {
    if results.is_empty() {
        return RetrievalMetrics::default();
    }
    let n = results.len() as f64;
    RetrievalMetrics {
        recall_at_5: results.iter().filter(|r| r.evidence_found_in_top_5).count() as f64 / n,
        recall_at_10: results.iter().filter(|r| r.evidence_found_in_top_10).count() as f64 / n,
        mrr: results.iter().map(|r| r.reciprocal_rank).sum::<f64>() / n,
        count: results.len(),
    }
}

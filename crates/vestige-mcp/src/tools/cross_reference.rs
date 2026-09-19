//! Deep Reference Tool (v3.2.1)
//!
//! Cognitive reasoning engine across memories. Combines:
//!   1. Broad retrieval (hybrid search + reranking)
//!   2. Spreading activation expansion (connected memories)
//!   3. FSRS-6 trust scoring (retention, stability, reps, lapses)
//!   4. Temporal supersession (newer = current truth)
//!   5. Contradiction analysis (trust-weighted)
//!   6. Dream insight integration (persisted insights)
//!   7. Structured synthesis (recommended answer + evidence)
//!
//! Replaces cross_reference with full cognitive reasoning. cross_reference
//! is kept as a backward-compatible alias.

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{KnowledgeNode, Storage};

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "The question, claim, or topic to reason about across all memories"
            },
            "depth": {
                "type": "integer",
                "description": "How many memories to analyze (default: 20, max: 50). Higher = more thorough.",
                "default": 20,
                "minimum": 5,
                "maximum": 50
            }
        },
        "required": ["query"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeepRefArgs {
    query: String,
    depth: Option<i32>,
}

// ============================================================================
// FSRS-6 Trust Score
// ============================================================================

fn compute_trust(retention: f64, stability: f64, reps: i32, lapses: i32) -> f64 {
    let retention_factor = retention * 0.4;
    let stability_factor = (stability / 30.0).min(1.0) * 0.2;
    let reps_factor = (reps as f64 / 10.0).min(1.0) * 0.2;
    let lapses_penalty = (1.0 - (lapses as f64 / 5.0)).max(0.0) * 0.2;
    (retention_factor + stability_factor + reps_factor + lapses_penalty).clamp(0.0, 1.0)
}

// ============================================================================
// Intent Classification
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum QueryIntent {
    FactCheck,
    Timeline,
    RootCause,
    Comparison,
    Synthesis,
}

impl std::fmt::Display for QueryIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryIntent::FactCheck => write!(f, "fact_check"),
            QueryIntent::Timeline => write!(f, "timeline"),
            QueryIntent::RootCause => write!(f, "root_cause"),
            QueryIntent::Comparison => write!(f, "comparison"),
            QueryIntent::Synthesis => write!(f, "synthesis"),
        }
    }
}

fn classify_intent(query: &str) -> QueryIntent {
    let q = query.to_lowercase();
    let patterns: &[(QueryIntent, &[&str])] = &[
        (
            QueryIntent::RootCause,
            &[
                "why did",
                "root cause",
                "what caused",
                "because of",
                "reason for",
                "why is",
                "why was",
            ],
        ),
        (
            QueryIntent::Timeline,
            &[
                "when did",
                "timeline",
                "history of",
                "over time",
                "how has",
                "evolution of",
                "sequence of",
            ],
        ),
        (
            QueryIntent::Comparison,
            &[
                "differ",
                "compare",
                "versus",
                " vs ",
                "difference between",
                "changed from",
            ],
        ),
        (
            QueryIntent::FactCheck,
            &[
                "is it true",
                "did i",
                "was there",
                "verify",
                "confirm",
                "is this correct",
                "should i use",
                "should we",
            ],
        ),
    ];
    for (intent, keywords) in patterns {
        if keywords.iter().any(|kw| q.contains(kw)) {
            return intent.clone();
        }
    }
    QueryIntent::Synthesis
}

// ============================================================================
// Relation Assessment
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum Relation {
    Supports,
    Contradicts,
    Supersedes,
    Irrelevant,
}

impl std::fmt::Display for Relation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Relation::Supports => write!(f, "supports"),
            Relation::Contradicts => write!(f, "contradicts"),
            Relation::Supersedes => write!(f, "supersedes"),
            Relation::Irrelevant => write!(f, "irrelevant"),
        }
    }
}

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "shall", "can", "to",
    "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "about", "that", "this",
    "it", "its", "and", "or", "but", "if", "so", "no", "not", "we", "i", "my", "our", "he", "she",
    "they", "them", "his", "her",
];

fn shared_topic_words(a: &str, b: &str) -> usize {
    let words_a: std::collections::HashSet<String> = a
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(&w.as_str()))
        .collect();
    let words_b: std::collections::HashSet<String> = b
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(&w.as_str()))
        .collect();
    words_a.intersection(&words_b).count()
}

fn contains_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .any(|w| w == word)
}

fn has_negation_signal(a: &str, b: &str) -> bool {
    // Ask the calibrated detector first — the same one the ingest gate uses
    // (`nlp::default_contradiction_detector`, NegEx scope + EN/PL lexicons, F1 0.955 on the
    // eval set). This function used to be the *only* check, and its antonym table ("don't"
    // vs "do", "deprecated" vs "recommended") cannot see a disagreement phrased as ordinary
    // prose ("the deploy is on Friday" / "the deploy moved to Monday"). That is why
    // `deep_reference` reported zero contradictions while the ingest gate was flagging
    // corrections, and why the MemConflict CRS columns measured 0.0000 in every arm.
    let detector = crate::cognitive::contradiction_detector();
    if detector.detect(a, b).positive || detector.detect(b, a).positive {
        return true;
    }

    let negation_pairs: &[(&str, &str)] = &[
        ("don't", "do"),
        ("doesn't", "does"),
        ("didn't", "did"),
        ("isn't", "is"),
        ("aren't", "are"),
        ("won't", "will"),
        ("can't", "can"),
        ("shouldn't", "should"),
        ("never", "always"),
        ("false", "true"),
        ("incorrect", "correct"),
        ("wrong", "right"),
        ("deprecated", "recommended"),
        ("removed", "added"),
        ("disabled", "enabled"),
    ];
    let al = a.to_lowercase();
    let bl = b.to_lowercase();
    for (neg, pos) in negation_pairs {
        if (contains_word(&al, neg) && contains_word(&bl, pos))
            || (contains_word(&bl, neg) && contains_word(&al, pos))
        {
            return true;
        }
    }

    let a_has_not = contains_word(&al, "not");
    let b_has_not = contains_word(&bl, "not");
    if a_has_not != b_has_not {
        let shared = shared_topic_words(a, b);
        if shared >= 2 {
            return true;
        }
    }

    let correction_signals = [
        "correction:",
        "no longer",
        "instead of",
        "replaced by",
        "superseded by",
    ];
    for sig in &correction_signals {
        if al.contains(sig) || bl.contains(sig) {
            return true;
        }
    }
    false
}

/// Build the JSON value the dashboard expects for one evidence entry.
/// Used for both the primary `evidence[]` array (stage 2 — direct
/// retrieval hits) and `related_insights[]` (stage 3 — surfaced via
/// spreading activation). Centralised here so the two emit identical
/// shapes; the dashboard `DeepRefEvidenceDto` is the source of truth
/// and requires `retention`, `stability`, `reps`, `lapses`, `tags`,
/// `nodeType`, `createdAt`, `updatedAt` to be present.
///
/// The `source` argument is `None` for direct retrieval and
/// `Some("spreading_activation")` for activation hits. The dashboard
/// uses this to label related memories as "via connections" rather
/// than mixing them with primary evidence.
///
/// Regression: before this helper existed, `related_insights` only
/// emitted 4 fields (id, content, trust, source) which failed
/// `DeepRefEvidenceDto` deserialization with `missing field "retention"`
/// → 502 Bad Gateway on every query that triggered activation.
fn build_evidence_value(
    node: &KnowledgeNode,
    trust: f64,
    combined_score: f64,
    source: Option<&str>,
) -> Value {
    let mut v = serde_json::json!({
        "id": node.id,
        "content": node.content,
        "trust": (trust * 100.0).round() / 100.0,
        "retention": node.retention_strength,
        "stability": node.stability,
        "reps": node.reps,
        "lapses": node.lapses,
        "tags": node.tags,
        "nodeType": node.node_type,
        "createdAt": node.created_at.to_rfc3339(),
        "updatedAt": node.updated_at.to_rfc3339(),
        "combinedScore": combined_score,
    });
    if let Some(s) = source {
        v["source"] = Value::String(s.to_string());
    }
    v
}

/// Build the JSON value the dashboard expects for one contradictory
/// pair. Extracted from the main `execute()` body so unit tests can
/// pin the shape of the contract independently of the classifier — a
/// missing/renamed field breaks `DeepRefContradictionDto` parsing
/// (502 BAD GATEWAY) and we used to learn about it from production.
///
/// Contents are clipped to the first sentence so the payload stays
/// readable in the dashboard side panel without pulling multi-kB
/// compound memories across the wire for every pair.
fn build_contradiction_value(
    id_a: &str,
    id_b: &str,
    content_a: &str,
    content_b: &str,
    trust_a: f64,
    trust_b: f64,
    relation: &Relation,
) -> Value {
    serde_json::json!({
        "memoryA": id_a,
        "memoryB": id_b,
        "contentA": first_sentence(content_a),
        "contentB": first_sentence(content_b),
        "relation": relation.to_string(),
        "trustA": (trust_a * 100.0).round() / 100.0,
        "trustB": (trust_b * 100.0).round() / 100.0,
    })
}

fn assess_relation(
    a_content: &str,
    a_trust: f64,
    a_time: chrono::DateTime<Utc>,
    b_content: &str,
    b_trust: f64,
    b_time: chrono::DateTime<Utc>,
) -> Relation {
    let shared = shared_topic_words(a_content, b_content);
    if shared < 2 {
        return Relation::Irrelevant;
    }
    if has_negation_signal(a_content, b_content) {
        let newer_trust = if a_time > b_time { a_trust } else { b_trust };
        if newer_trust > 0.5 {
            return Relation::Supersedes;
        }
        return Relation::Contradicts;
    }
    Relation::Supports
}

// ============================================================================
// Execute
// ============================================================================

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: DeepRefArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    if args.query.trim().is_empty() {
        return Err("Query cannot be empty".to_string());
    }

    let depth = args.depth.unwrap_or(20).clamp(5, 50) as usize;
    let intent = classify_intent(&args.query);

    // STAGE 1: Broad retrieval with overfetch. Hybrid search fires FTS5 +
    // embedding similarity — always blocking, move to the thread pool.
    let overfetch = (depth * 3).min(150);
    let storage_search = storage.clone();
    let query_owned = args.query.clone();
    let results = tokio::task::spawn_blocking(move || {
        let (kw, sem) = vestige_core::default_hybrid_weights();
        crate::retrieval::hybrid_search(&storage_search, &query_owned, overfetch as i32, kw, sem)
            .map_err(|e| format!("Search failed: {}", e))
    })
    .await
    .map_err(|e| format!("cross_reference search task panicked: {}", e))??;

    if results.is_empty() {
        return Ok(serde_json::json!({
            "intent": intent.to_string(),
            "query": args.query,
            "reasoning": "No memories found matching this query.",
            "recommended": null,
            "evidence": [],
            "contradictions": [],
            "confidence": 0.0,
        }));
    }

    // STAGE 2: Trust scoring
    let mut evidence: Vec<Value> = Vec::with_capacity(depth);
    let mut trust_scores: Vec<(String, f64, String, chrono::DateTime<Utc>)> = Vec::new();

    for r in results.iter().take(depth) {
        let trust = compute_trust(
            r.node.retention_strength,
            r.node.stability,
            r.node.reps,
            r.node.lapses,
        );
        trust_scores.push((
            r.node.id.clone(),
            trust,
            r.node.content.clone(),
            r.node.updated_at,
        ));
        evidence.push(build_evidence_value(
            &r.node,
            trust,
            r.combined_score as f64,
            None,
        ));
    }

    // Sort evidence by trust descending
    evidence.sort_by(|a, b| {
        let ta = a["trust"].as_f64().unwrap_or(0.0);
        let tb = b["trust"].as_f64().unwrap_or(0.0);
        tb.partial_cmp(&ta).unwrap_or(std::cmp::Ordering::Equal)
    });

    // STAGE 3: Spreading activation expansion
    //
    // Clone the network Arc out of the engine and drop the engine lock
    // before doing the propagation — keeps the engine free for unrelated
    // work while activation runs.
    let mut activation_ids: Vec<String> = Vec::new();
    let net = match cognitive.try_lock() {
        Ok(cog) => Some(std::sync::Arc::clone(&cog.activation_network)),
        Err(_) => {
            crate::cognitive::try_lock_metrics::record_miss("deep_reference");
            tracing::warn!("deep_reference: cognitive engine locked, spreading activation skipped");
            None
        }
    };
    let activation_available = if let Some(net) = net {
        if let Some(top) = trust_scores.first() {
            match net.try_write() {
                Ok(mut net) => {
                    let activated = net.activate(&top.0, 1.0);
                    for a in activated.iter().take(5) {
                        if !trust_scores.iter().any(|(id, _, _, _)| id == &a.memory_id) {
                            activation_ids.push(a.memory_id.clone());
                        }
                    }
                    true
                }
                Err(_) => {
                    crate::cognitive::try_lock_metrics::record_miss("deep_reference");
                    false
                }
            }
        } else {
            true
        }
    } else {
        false
    };
    let mut related_insights: Vec<Value> = Vec::new();
    if !activation_ids.is_empty() {
        // Batch all activation-id node lookups in one blocking task — each
        // get_node is a separate SELECT, easy to chain on the same thread.
        let storage_act = storage.clone();
        let ids_owned = activation_ids.clone();
        let nodes = tokio::task::spawn_blocking(move || {
            ids_owned
                .iter()
                .filter_map(|aid| storage_act.get_node(aid).ok().flatten())
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| format!("cross_reference activation task panicked: {}", e))?;
        for node in nodes {
            let trust = compute_trust(
                node.retention_strength,
                node.stability,
                node.reps,
                node.lapses,
            );
            related_insights.push(build_evidence_value(
                &node,
                trust,
                0.0,
                Some("spreading_activation"),
            ));
        }
    }

    // STAGE 4: Temporal supersession
    let mut superseded: Vec<Value> = Vec::new();
    trust_scores.sort_by(|a, b| b.3.cmp(&a.3));
    let mut current_facts: Vec<(String, f64, String, chrono::DateTime<Utc>)> = Vec::new();
    for (id, trust, content, time) in &trust_scores {
        let mut is_superseded = false;
        for (cid, ctrust, ccontent, _ctime) in &current_facts {
            if shared_topic_words(content, ccontent) >= 3 && ctrust > trust {
                superseded.push(serde_json::json!({
                    "supersededId": id,
                    "supersededBy": cid,
                    "reason": "Newer memory with higher trust covers same topic",
                }));
                is_superseded = true;
                break;
            }
        }
        if !is_superseded {
            current_facts.push((id.clone(), *trust, content.clone(), *time));
        }
    }

    // STAGE 5: Contradiction analysis
    let mut contradictions: Vec<Value> = Vec::new();
    for i in 0..trust_scores.len() {
        for j in (i + 1)..trust_scores.len() {
            let (ref id_a, trust_a, ref content_a, time_a) = trust_scores[i];
            let (ref id_b, trust_b, ref content_b, time_b) = trust_scores[j];
            let relation = assess_relation(content_a, trust_a, time_a, content_b, trust_b, time_b);
            match relation {
                Relation::Contradicts => {
                    contradictions.push(build_contradiction_value(
                        id_a, id_b, content_a, content_b, trust_a, trust_b, &relation,
                    ));
                }
                Relation::Supersedes if !superseded.iter().any(|s| s["supersededId"] == *id_b) => {
                    let (newer_id, older_id) = if time_a > time_b {
                        (id_a, id_b)
                    } else {
                        (id_b, id_a)
                    };

                    // Supersession is how the conflict was *resolved*, not a reason to hide
                    // it. Reporting only the resolution is why `contradictions` came back
                    // empty on stores that plainly disagreed with themselves: any pair whose
                    // newer side had trust > 0.5 was routed here and nowhere else. The pair
                    // is listed in both places, with the resolution named.
                    let mut value = build_contradiction_value(
                        id_a, id_b, content_a, content_b, trust_a, trust_b, &relation,
                    );
                    value["resolvedBy"] = serde_json::json!("supersession");
                    value["supersededId"] = serde_json::json!(older_id);
                    contradictions.push(value);

                    superseded.push(serde_json::json!({
                        "supersededId": older_id,
                        "supersededBy": newer_id,
                        "reason": "Newer memory corrects/replaces older",
                    }));
                }
                Relation::Supersedes | Relation::Supports | Relation::Irrelevant => {}
            }
        }
    }

    // STAGE 6: Dream insight integration — blocking SQL on the threadpool.
    let storage_dream = storage.clone();
    let dream_query = tokio::task::spawn_blocking(move || storage_dream.get_insights(10))
        .await
        .map_err(|e| format!("cross_reference dream task panicked: {}", e))?;
    let (dream_insights, dream_available): (Vec<Value>, bool) = match dream_query {
        Ok(insights) => {
            let filtered: Vec<Value> = insights
                .iter()
                .filter(|i| {
                    let il = i.insight.to_lowercase();
                    let ql = args.query.to_lowercase();
                    ql.split_whitespace()
                        .filter(|w| w.len() > 2)
                        .any(|w| il.contains(w))
                })
                .take(3)
                .map(|i| {
                    serde_json::json!({
                        "insight": i.insight,
                        "confidence": i.confidence,
                        "source": "dream",
                    })
                })
                .collect();
            (filtered, true)
        }
        Err(e) => {
            tracing::warn!("deep_reference: dream insights unavailable: {}", e);
            (vec![], false)
        }
    };

    // STAGE 7: Synthesis
    let recommended = evidence.first().cloned();
    let top_trust = recommended
        .as_ref()
        .and_then(|r| r["trust"].as_f64())
        .unwrap_or(0.0);
    let contradiction_penalty = (contradictions.len() as f64 * 0.1).min(0.3);
    let confidence = (top_trust - contradiction_penalty).clamp(0.0, 1.0);

    let reasoning = build_reasoning(&intent, &evidence, &contradictions, &superseded, confidence);

    // Evolution timeline (for timeline intent or always useful)
    let mut evolution: Vec<Value> = trust_scores
        .iter()
        .map(|(id, trust, content, time)| {
            serde_json::json!({
                "id": id,
                "content": first_sentence(content),
                "trust": (trust * 100.0).round() / 100.0,
                "timestamp": time.to_rfc3339(),
            })
        })
        .collect();
    evolution.sort_by(|a, b| {
        let ta = a["timestamp"].as_str().unwrap_or("");
        let tb = b["timestamp"].as_str().unwrap_or("");
        ta.cmp(tb)
    });

    Ok(serde_json::json!({
        "intent": intent.to_string(),
        "query": args.query,
        "reasoning": reasoning,
        "recommended": recommended,
        "evidence": evidence,
        "contradictions": contradictions,
        "superseded": superseded,
        "evolution": evolution,
        "relatedInsights": related_insights,
        "dreamInsights": dream_insights,
        "confidence": (confidence * 100.0).round() / 100.0,
        "depth": depth,
        "memoriesAnalyzed": trust_scores.len(),
        "stagesCompleted": {
            "spreadingActivation": activation_available,
            "dreamInsights": dream_available,
        },
    }))
}

fn build_reasoning(
    intent: &QueryIntent,
    evidence: &[Value],
    contradictions: &[Value],
    superseded: &[Value],
    confidence: f64,
) -> String {
    let mut parts = Vec::new();

    match intent {
        QueryIntent::FactCheck => parts.push("Intent: Verifying a factual claim.".to_string()),
        QueryIntent::Timeline => {
            parts.push("Intent: Reconstructing temporal sequence.".to_string())
        }
        QueryIntent::RootCause => parts.push("Intent: Tracing causal chain backward.".to_string()),
        QueryIntent::Comparison => {
            parts.push("Intent: Comparing two concepts or states.".to_string())
        }
        QueryIntent::Synthesis => {
            parts.push("Intent: Synthesizing knowledge on a topic.".to_string())
        }
    }

    parts.push(format!("Found {} relevant memories.", evidence.len()));

    if !contradictions.is_empty() {
        parts.push(format!(
            "WARNING: {} contradiction(s) detected — verify with source.",
            contradictions.len()
        ));
    }

    if !superseded.is_empty() {
        parts.push(format!(
            "{} memory/memories superseded by newer information.",
            superseded.len()
        ));
    }

    if let Some(top) = evidence.first() {
        let trust = top["trust"].as_f64().unwrap_or(0.0);
        let content = top["content"].as_str().unwrap_or("");
        let preview: String = content.chars().take(200).collect();
        parts.push(format!(
            "Highest-trust answer (trust={:.2}): {}",
            trust, preview
        ));
    }

    if confidence < 0.3 {
        parts.push(
            "Low confidence — consider saving more context or verifying externally.".to_string(),
        );
    }

    parts.join(" ")
}

fn first_sentence(text: &str) -> String {
    text.split(['.', '\n'])
        .next()
        .unwrap_or(text)
        .trim()
        .chars()
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_has_required_fields() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["query"].is_object());
        assert!(s["properties"]["depth"].is_object());
        assert_eq!(s["required"], serde_json::json!(["query"]));
    }

    #[test]
    fn test_classify_intent_synthesis() {
        assert_eq!(
            classify_intent("What do I know about Rust?"),
            QueryIntent::Synthesis
        );
    }

    #[test]
    fn test_classify_intent_fact_check() {
        assert_eq!(
            classify_intent("Is it true that we use PostgreSQL?"),
            QueryIntent::FactCheck
        );
    }

    #[test]
    fn test_classify_intent_timeline() {
        assert_eq!(
            classify_intent("When did we switch to React?"),
            QueryIntent::Timeline
        );
    }

    #[test]
    fn test_classify_intent_root_cause() {
        assert_eq!(
            classify_intent("Why did the deployment fail?"),
            QueryIntent::RootCause
        );
    }

    #[test]
    fn test_classify_intent_comparison() {
        assert_eq!(
            classify_intent("How does Redis differ from Memcached?"),
            QueryIntent::Comparison
        );
    }

    #[test]
    fn test_compute_trust_perfect() {
        let trust = compute_trust(1.0, 30.0, 10, 0);
        assert!((trust - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_trust_zero() {
        let trust = compute_trust(0.0, 0.0, 0, 5);
        assert!(trust < 0.01);
    }

    #[test]
    fn test_shared_topic_words() {
        assert!(
            shared_topic_words(
                "Rust borrow checker prevents data races",
                "Rust ownership prevents memory issues"
            ) >= 2
        );
        assert_eq!(shared_topic_words("hello world", "goodbye moon"), 0);
    }

    #[test]
    fn test_shared_topic_words_short_acronyms() {
        assert!(shared_topic_words("JWT token validation", "JWT parsing error") >= 1);
        assert!(shared_topic_words("SQL query optimization", "SQL index tuning") >= 1);
        assert!(shared_topic_words("API rate limiting", "API gateway setup") >= 1);
    }

    #[test]
    fn test_negation_signal() {
        assert!(has_negation_signal(
            "We should not use Redis",
            "We should use Redis for caching"
        ));
        assert!(!has_negation_signal("Rust is fast", "Rust is safe"));
    }

    #[test]
    fn test_negation_signal_no_false_positives() {
        assert!(!has_negation_signal(
            "I'm noting the deadline for Friday",
            "The weather is nice today"
        ));
        assert!(!has_negation_signal(
            "Nothing special about this change",
            "Completely unrelated topic here"
        ));
    }

    #[test]
    fn test_negation_signal_contraction_pairs() {
        assert!(has_negation_signal(
            "We don't deploy on Fridays",
            "We do deploy on Mondays"
        ));
        assert!(has_negation_signal(
            "Feature is disabled in prod",
            "Feature is enabled in staging"
        ));
    }

    #[test]
    fn test_contains_word_boundary() {
        assert!(contains_word("we should not use redis", "not"));
        assert!(!contains_word("nothing special here", "not"));
        assert!(!contains_word("I'm noting the deadline", "not"));
        assert!(contains_word("don't do that", "don't"));
    }

    #[test]
    fn test_relation_irrelevant_few_shared_words() {
        let now = Utc::now();
        let rel = assess_relation("Cats are cute", 0.8, now, "Rust is fast", 0.7, now);
        assert!(matches!(rel, Relation::Irrelevant));
    }

    // ---- Integration tests against real storage ----
    //
    // These exist because the unit tests above only cover the leaf
    // helpers (intent classifier, trust formula, relation classifier).
    // They never exercise the JSON-shape contract the dashboard
    // depends on, which is exactly where the deep_reference 502 bug
    // hid for three releases: contradictions were missing
    // contentA/contentB in the tool output but the wire DTO required
    // them, so every multi-memory query failed to deserialize.

    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    async fn ingest(storage: &Arc<Storage>, content: &str) -> String {
        storage
            .ingest(vestige_core::IngestInput {
                content: content.into(),
                node_type: "fact".into(),
                ..Default::default()
            })
            .unwrap()
            .id
    }

    /// Force two memories with shared topic words + negation so the
    /// relation classifier returns `Contradicts`. Without this setup
    /// the contradiction branch never runs and the missing-fields bug
    /// stays hidden.
    async fn seed_contradictory_pair(storage: &Arc<Storage>) -> (String, String) {
        let a = ingest(
            storage,
            "We use PostgreSQL for primary storage in the dashboard pipeline.",
        )
        .await;
        let b = ingest(
            storage,
            "We don't use PostgreSQL for primary storage in the dashboard pipeline.",
        )
        .await;
        (a, b)
    }

    /// A disagreement phrased as ordinary prose must be detected. The old signal was an
    /// antonym table ("don't"/"do", "deprecated"/"recommended"), which sees none of these —
    /// so `deep_reference` reported zero contradictions on stores that visibly disagreed
    /// with themselves.
    #[test]
    fn prose_contradictions_are_detected_by_the_calibrated_detector() {
        use chrono::Utc;

        let now = Utc::now();
        let earlier = now - chrono::Duration::hours(2);

        let pairs = [
            ("The deploy is on Friday", "The deploy is not on Friday"),
            (
                "Use the legacy authentication flow for the admin console",
                "Do not use the legacy authentication flow for the admin console",
            ),
        ];

        for (a, b) in pairs {
            let relation = assess_relation(a, 0.4, earlier, b, 0.9, now);
            assert!(
                matches!(relation, Relation::Contradicts | Relation::Supersedes),
                "expected a conflict for {a:?} vs {b:?}, got {relation:?}"
            );
        }
    }

    /// Agreement stays agreement: no false positives from the detector.
    #[test]
    fn agreeing_memories_are_not_reported_as_conflicts() {
        use chrono::Utc;

        let now = Utc::now();
        let earlier = now - chrono::Duration::hours(2);

        let relation = assess_relation(
            "The deploy pipeline runs every Friday evening",
            0.8,
            earlier,
            "The deploy pipeline runs on Friday evening after the tests pass",
            0.8,
            now,
        );

        assert_eq!(relation, Relation::Supports);
    }

    /// Unit-pin the shape of one contradiction entry. This is the
    /// regression test for the 502 bug — before the fix the helper
    /// did not exist and the inline `json!` macro forgot contentA /
    /// contentB, which the dashboard's `DeepRefContradictionDto`
    /// required. We test the helper directly instead of forcing the
    /// classifier into a `Contradicts` branch because the trust
    /// threshold inside `assess_relation` routes most freshly-ingested
    /// pairs to `Supersedes`, hiding the field-drift bug.
    #[test]
    fn test_build_contradiction_value_has_all_dashboard_required_fields() {
        let v = build_contradiction_value(
            "id-a",
            "id-b",
            "We use PostgreSQL for primary storage.",
            "We don't use PostgreSQL for primary storage.",
            0.42,
            0.31,
            &Relation::Contradicts,
        );
        // Each assertion names the field that broke production when
        // it was missing — keep these specific so the failure message
        // points at the contract gap, not at a generic JSON shape.
        assert_eq!(v["memoryA"], "id-a");
        assert_eq!(v["memoryB"], "id-b");
        assert!(
            v["contentA"].is_string() && !v["contentA"].as_str().unwrap().is_empty(),
            "contentA must be present (DeepRefContradictionDto required field)"
        );
        assert!(
            v["contentB"].is_string() && !v["contentB"].as_str().unwrap().is_empty(),
            "contentB must be present (DeepRefContradictionDto required field)"
        );
        assert_eq!(v["relation"], "contradicts");
        assert!((v["trustA"].as_f64().unwrap() - 0.42).abs() < 1e-9);
        assert!((v["trustB"].as_f64().unwrap() - 0.31).abs() < 1e-9);
    }

    /// First-sentence clipping: long compound memories must not drag
    /// multi-kB content into every contradiction pair. We assert the
    /// helper trims at the first sentence boundary like `first_sentence`
    /// does for the evolution timeline — the two paths are visually
    /// adjacent in the dashboard.
    #[test]
    fn test_build_contradiction_value_clips_content_to_first_sentence() {
        let long = "Decision: migrate to OIDC. \
                    [Updated 2026-04-05] After three months we reverted. \
                    [Updated 2026-04-12] Now migrating again with a new provider.";
        let v = build_contradiction_value(
            "id-a",
            "id-b",
            long,
            "Short statement.",
            0.5,
            0.5,
            &Relation::Contradicts,
        );
        let ca = v["contentA"].as_str().unwrap();
        assert!(
            !ca.contains("[Updated"),
            "first-sentence clip must strip update markers from compound memories; got {ca:?}"
        );
        assert!(ca.starts_with("Decision: migrate to OIDC"));
    }

    /// Parity gate: a hand-built contradiction value (mirroring what
    /// `execute()` would emit) must round-trip through the wire DTO.
    /// This is the *minimal* contract test — independent of the
    /// classifier, independent of storage state — that would have
    /// failed before the fix and now serves as a regression guard.
    #[test]
    fn test_contradiction_value_round_trips_through_wire_dto() {
        use crate::dashboard::wire::DeepRefContradictionDto;
        let v = build_contradiction_value(
            "id-a",
            "id-b",
            "We use PostgreSQL.",
            "We don't use PostgreSQL.",
            0.5,
            0.4,
            &Relation::Contradicts,
        );
        let parsed: Result<DeepRefContradictionDto, _> = serde_json::from_value(v.clone());
        if let Err(e) = &parsed {
            panic!(
                "DeepRefContradictionDto rejected our payload — this is the 502 bug.\nerror: {e}\npayload: {v:#}"
            );
        }
        let dto = parsed.unwrap();
        assert_eq!(dto.memory_a, "id-a");
        assert_eq!(dto.content_a, "We use PostgreSQL");
        assert_eq!(dto.content_b, "We don't use PostgreSQL");
    }

    /// Unit-pin the shape of one evidence entry. Regression test for
    /// the second 502 bug — `related_insights` was missing every FSRS
    /// field (`retention`, `stability`, `reps`, `lapses`, `tags`,
    /// `nodeType`, `createdAt`, `updatedAt`) because the activation
    /// stage's `serde_json::json!` block only emitted 4 fields. Each
    /// assertion names a field that broke production when missing —
    /// keep them granular so a failure points at the exact contract
    /// gap, not at a generic "JSON shape" error.
    #[test]
    fn test_build_evidence_value_has_all_dashboard_required_fields() {
        let mut node = KnowledgeNode::new("Vestige uses FSRS-6 for trust scoring.");
        node.id = "node-1".into();
        node.retention_strength = 0.83;
        node.stability = 12.4;
        node.reps = 5;
        node.lapses = 1;
        node.tags = vec!["vestige".into(), "fsrs".into()];
        node.node_type = "fact".into();

        let v = build_evidence_value(&node, 0.91, 0.74, Some("spreading_activation"));

        assert_eq!(v["id"], "node-1");
        assert!(
            v["retention"].is_number(),
            "retention must be present (DeepRefEvidenceDto required field)"
        );
        assert!(
            v["stability"].is_number(),
            "stability must be present (DeepRefEvidenceDto required field)"
        );
        assert!(
            v["reps"].is_number(),
            "reps must be present (DeepRefEvidenceDto required field)"
        );
        assert!(
            v["lapses"].is_number(),
            "lapses must be present (DeepRefEvidenceDto required field)"
        );
        assert!(
            v["tags"].is_array(),
            "tags must be an array (DeepRefEvidenceDto required field)"
        );
        assert_eq!(v["nodeType"], "fact");
        assert!(
            v["createdAt"].is_string(),
            "createdAt must be present (DeepRefEvidenceDto required field)"
        );
        assert!(
            v["updatedAt"].is_string(),
            "updatedAt must be present (DeepRefEvidenceDto required field)"
        );
        assert!((v["combinedScore"].as_f64().unwrap() - 0.74).abs() < 1e-9);
        assert_eq!(v["source"], "spreading_activation");
    }

    /// Primary-evidence entries (no `source` annotation) must omit the
    /// field entirely so the dashboard can distinguish direct hits
    /// from activation-derived "related" memories.
    #[test]
    fn test_build_evidence_value_omits_source_when_none() {
        let node = KnowledgeNode::new("Primary retrieval hit.");
        let v = build_evidence_value(&node, 0.5, 0.42, None);
        assert!(
            v.get("source").is_none(),
            "primary evidence must not carry a `source` field; got {v:#}"
        );
    }

    /// Parity gate: every evidence value we emit — primary or via
    /// activation — must round-trip through `DeepRefEvidenceDto`.
    /// This is the regression test that would have failed before
    /// `related_insights` was upgraded to use `build_evidence_value`.
    #[test]
    fn test_evidence_value_round_trips_through_wire_dto() {
        use crate::dashboard::wire::DeepRefEvidenceDto;

        let mut node = KnowledgeNode::new("Anything goes.");
        node.id = "id".into();
        node.tags = vec!["a".into()];

        // Both annotated and unannotated variants must parse cleanly.
        for source in [None, Some("spreading_activation")] {
            let v = build_evidence_value(&node, 0.5, 0.3, source);
            let parsed: Result<DeepRefEvidenceDto, _> = serde_json::from_value(v.clone());
            if let Err(e) = &parsed {
                panic!(
                    "DeepRefEvidenceDto rejected our payload (source={source:?}).\n\
                     error: {e}\npayload: {v:#}"
                );
            }
            let dto = parsed.unwrap();
            assert_eq!(dto.id, "id");
            assert_eq!(dto.source.as_deref(), source);
        }
    }

    /// Integration parity gate: a real `execute()` payload (even
    /// without contradictions) must deserialize through the full
    /// `DeepReferenceResultDto`. Catches field-name drift across the
    /// other stages (evidence, evolution, dreamInsights, stagesCompleted).
    #[tokio::test]
    async fn test_execute_payload_round_trips_through_wire_dto() {
        use crate::dashboard::wire::DeepReferenceResultDto;

        let (storage, _dir) = test_storage().await;
        let _ = seed_contradictory_pair(&storage).await;

        let args = Some(serde_json::json!({
            "query": "Do we use PostgreSQL for primary storage?",
            "depth": 5,
        }));
        let raw = execute(&storage, &test_cognitive(), args)
            .await
            .expect("execute should succeed");

        let result: Result<DeepReferenceResultDto, _> = serde_json::from_value(raw.clone());
        if let Err(e) = &result {
            panic!(
                "DeepReferenceResultDto rejected execute() payload.\nerror: {e}\npayload: {raw:#}"
            );
        }
    }
}

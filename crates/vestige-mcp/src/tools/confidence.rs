//! Confidence scoring for opinions and beliefs.
//!
//! Assigns multi-dimensional confidence scores to memories, distinguishing
//! hard facts from opinions, verified knowledge from assumptions.
//!
//! Based on: Kahneman (2011) confidence calibration, Tetlock (2015)
//! superforecasting, Mercier & Sperber (2017) argumentative theory.

use std::sync::Arc;

use serde_json::Value;

use vestige_core::Storage;

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["score", "audit", "calibrate"],
                "description": "score: evaluate a single memory's confidence. audit: find poorly-calibrated memories. calibrate: compare self-assessment with evidence."
            },
            "memory_id": {
                "type": "string",
                "description": "Memory ID to score (required for 'score')"
            },
            "limit": {
                "type": "integer",
                "description": "Max results for audit (default: 20)",
                "default": 20
            }
        },
        "required": ["action"]
    })
}

pub async fn execute(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args = args.ok_or("Arguments required")?;
    let action = args.get("action")
        .and_then(|v| v.as_str())
        .ok_or("action is required")?;
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20) as i32;

    match action {
        "score" => {
            let memory_id = args.get("memory_id")
                .and_then(|v| v.as_str())
                .ok_or("memory_id required for 'score'")?;

            let node = storage.get_node(memory_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Memory not found: {memory_id}"))?;
            let score = compute_confidence(&node);

            Ok(serde_json::json!({
                "action": "score",
                "memory_id": memory_id,
                "confidence": score,
                "content_preview": truncate(&node.content, 150),
            }))
        }

        "audit" => {
            let memories = storage.get_all_nodes(limit * 5, 0)
                .map_err(|e| e.to_string())?;

            let mut scored: Vec<_> = memories.iter()
                .map(|m| {
                    let score = compute_confidence(m);
                    (m, score)
                })
                .collect();

            let low_confidence: Vec<Value> = scored.iter()
                .filter(|(_, s)| s.overall < 0.4)
                .take(limit as usize)
                .map(|(m, s)| serde_json::json!({
                    "id": m.id,
                    "content": truncate(&m.content, 120),
                    "confidence": s,
                    "recommendation": confidence_recommendation(s),
                }))
                .collect();

            let overconfident: Vec<Value> = scored.iter()
                .filter(|(m, _)| {
                    m.retrieval_strength > 0.8 && m.storage_strength < 0.3
                })
                .take(limit as usize)
                .map(|(m, s)| serde_json::json!({
                    "id": m.id,
                    "content": truncate(&m.content, 120),
                    "confidence": s,
                    "issue": "High retrieval but weak storage — illusory confidence",
                }))
                .collect();

            scored.sort_by(|a, b| a.1.overall.partial_cmp(&b.1.overall)
                .unwrap_or(std::cmp::Ordering::Equal));

            let distribution = ConfidenceDistribution {
                very_low: scored.iter().filter(|(_, s)| s.overall < 0.2).count(),
                low: scored.iter().filter(|(_, s)| s.overall >= 0.2 && s.overall < 0.4).count(),
                medium: scored.iter().filter(|(_, s)| s.overall >= 0.4 && s.overall < 0.6).count(),
                high: scored.iter().filter(|(_, s)| s.overall >= 0.6 && s.overall < 0.8).count(),
                very_high: scored.iter().filter(|(_, s)| s.overall >= 0.8).count(),
            };

            Ok(serde_json::json!({
                "action": "audit",
                "totalAnalyzed": scored.len(),
                "distribution": {
                    "very_low": distribution.very_low,
                    "low": distribution.low,
                    "medium": distribution.medium,
                    "high": distribution.high,
                    "very_high": distribution.very_high,
                },
                "lowConfidence": low_confidence,
                "overconfident": overconfident,
                "averageConfidence": if scored.is_empty() { 0.0 } else {
                    scored.iter().map(|(_, s)| s.overall).sum::<f64>() / scored.len() as f64
                },
            }))
        }

        "calibrate" => {
            let memories = storage.get_all_nodes(limit * 3, 0)
                .map_err(|e| e.to_string())?;

            let opinions: Vec<_> = memories.iter()
                .filter(|m| is_opinion(&m.content) || m.node_type == "decision" || m.node_type == "pattern")
                .collect();

            let facts: Vec<_> = memories.iter()
                .filter(|m| !is_opinion(&m.content) && m.node_type == "fact")
                .collect();

            let opinion_avg_retention = if opinions.is_empty() { 0.0 } else {
                opinions.iter().map(|m| m.retention_strength).sum::<f64>() / opinions.len() as f64
            };
            let fact_avg_retention = if facts.is_empty() { 0.0 } else {
                facts.iter().map(|m| m.retention_strength).sum::<f64>() / facts.len() as f64
            };

            let opinions_needing_review: Vec<Value> = opinions.iter()
                .filter(|m| m.retention_strength < 0.4 && m.reps < 3)
                .take(limit as usize)
                .map(|m| serde_json::json!({
                    "id": m.id,
                    "content": truncate(&m.content, 150),
                    "retention": format!("{:.2}", m.retention_strength),
                    "reviews": m.reps,
                    "type": m.node_type,
                }))
                .collect();

            Ok(serde_json::json!({
                "action": "calibrate",
                "totalMemories": memories.len(),
                "opinions": opinions.len(),
                "facts": facts.len(),
                "opinionAvgRetention": format!("{:.3}", opinion_avg_retention),
                "factAvgRetention": format!("{:.3}", fact_avg_retention),
                "calibrationGap": format!("{:.3}", (opinion_avg_retention - fact_avg_retention).abs()),
                "opinionsNeedingReview": opinions_needing_review,
            }))
        }

        _ => Err(format!("Unknown action: {action}. Use: score, audit, calibrate")),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct ConfidenceScore {
    overall: f64,
    encoding_strength: f64,
    retrieval_reliability: f64,
    temporal_validity: f64,
    evidence_level: String,
}

struct ConfidenceDistribution {
    very_low: usize,
    low: usize,
    medium: usize,
    high: usize,
    very_high: usize,
}

fn compute_confidence(node: &vestige_core::KnowledgeNode) -> ConfidenceScore {
    let encoding = node.storage_strength.clamp(0.0, 1.0);

    let retrieval = if node.reps > 0 {
        let base = node.retrieval_strength;
        let review_bonus = (node.reps as f64 / 10.0).min(0.3);
        (base + review_bonus).clamp(0.0, 1.0)
    } else {
        node.retrieval_strength * 0.5
    };

    let now = chrono::Utc::now();
    let temporal = if let Some(until) = node.valid_until {
        if until <= now { 0.1 } else { 1.0 }
    } else {
        let age_days = (now - node.created_at).num_days() as f64;
        if age_days > 365.0 { 0.6 } else if age_days > 90.0 { 0.8 } else { 1.0 }
    };

    let evidence = classify_evidence(&node.content, &node.node_type);

    let overall = encoding * 0.3 + retrieval * 0.3 + temporal * 0.2 +
        match evidence.as_str() {
            "verified_fact" => 0.2,
            "documented" => 0.15,
            "experience" => 0.1,
            "opinion" => 0.05,
            _ => 0.1,
        };

    ConfidenceScore {
        overall: overall.clamp(0.0, 1.0),
        encoding_strength: encoding,
        retrieval_reliability: retrieval,
        temporal_validity: temporal,
        evidence_level: evidence,
    }
}

fn classify_evidence(content: &str, node_type: &str) -> String {
    let lower = content.to_lowercase();
    match node_type {
        "fact" if lower.contains("verified") || lower.contains("confirmed") || lower.contains("tested") => {
            "verified_fact".to_string()
        }
        "fact" | "event" => "documented".to_string(),
        "decision" | "pattern" => "experience".to_string(),
        _ if is_opinion(&lower) => "opinion".to_string(),
        _ => "documented".to_string(),
    }
}

fn is_opinion(content: &str) -> bool {
    let lower = content.to_lowercase();
    let markers = [
        "i think", "i believe", "probably", "might be", "could be",
        "seems like", "in my opinion", "i prefer", "i feel", "arguably",
        "maybe", "possibly", "i suspect", "likely", "unlikely",
    ];
    markers.iter().any(|m| lower.contains(m))
}

fn confidence_recommendation(score: &ConfidenceScore) -> String {
    if score.overall < 0.2 {
        "Very low confidence — verify this before relying on it".to_string()
    } else if score.overall < 0.4 {
        "Low confidence — consider reviewing the source material".to_string()
    } else if score.encoding_strength < 0.3 {
        "Weak encoding — needs more review/practice to solidify".to_string()
    } else if score.temporal_validity < 0.5 {
        "May be outdated — check if this is still current".to_string()
    } else {
        "Reasonable confidence".to_string()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max)])
    }
}

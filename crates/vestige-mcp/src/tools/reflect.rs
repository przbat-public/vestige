//! Reflect tool — deliberate metacognitive insight generation.
//!
//! Unlike `dream` (unconscious sleep consolidation), `reflect` performs active
//! self-examination: contradiction detection, knowledge gap analysis,
//! pattern recognition, and confidence calibration.
//!
//! Based on: Flavell (1979) metacognition, Schön (1983) reflection-in-action,
//! Nelson & Narens (1990) metamemory monitoring.

use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::{Duration, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::cognitive::CognitiveEngine;
use vestige_core::{InsightRecord, Storage};

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "focus": {
                "type": "string",
                "description": "Optional topic to focus reflection on (e.g. 'rust', 'architecture')"
            },
            "depth": {
                "type": "string",
                "enum": ["quick", "standard", "deep"],
                "description": "Reflection depth: quick (50 memories), standard (200), deep (500)",
                "default": "standard"
            }
        }
    })
}

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let focus = args.as_ref()
        .and_then(|a| a.get("focus"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let depth = args.as_ref()
        .and_then(|a| a.get("depth"))
        .and_then(|v| v.as_str())
        .unwrap_or("standard");

    let memory_limit = match depth {
        "quick" => 50,
        "deep" => 500,
        _ => 200,
    };

    let memories = if let Some(ref query) = focus {
        let results = storage.hybrid_search(query, memory_limit, 0.2, 0.8)
            .map_err(|e| e.to_string())?;
        results.into_iter()
            .filter_map(|r| storage.get_node(&r.node.id).ok().flatten())
            .collect::<Vec<_>>()
    } else {
        storage.get_all_nodes(memory_limit, 0).map_err(|e| e.to_string())?
    };

    if memories.len() < 3 {
        return Ok(serde_json::json!({
            "status": "insufficient_memories",
            "message": format!("Need at least 3 memories to reflect. Current: {}", memories.len())
        }));
    }

    let now = Utc::now();

    // 1. Contradiction detection — find memories with opposing content on same topic
    let mut contradictions = Vec::new();
    let tags_map: std::collections::HashMap<String, Vec<usize>> = {
        let mut m = std::collections::HashMap::new();
        for (i, mem) in memories.iter().enumerate() {
            for tag in &mem.tags {
                m.entry(tag.clone()).or_insert_with(Vec::new).push(i);
            }
        }
        m
    };

    for (_tag, indices) in &tags_map {
        if indices.len() < 2 { continue; }
        for i in 0..indices.len().min(10) {
            for j in (i + 1)..indices.len().min(10) {
                let a = &memories[indices[i]];
                let b = &memories[indices[j]];
                if content_conflicts(&a.content, &b.content) {
                    contradictions.push(serde_json::json!({
                        "memory_a": a.id,
                        "memory_b": b.id,
                        "snippet_a": truncate(&a.content, 120),
                        "snippet_b": truncate(&b.content, 120),
                    }));
                }
            }
        }
    }

    // 2. Knowledge gaps — topics with few memories or low retention
    let mut topic_health: std::collections::HashMap<String, (usize, f64)> = std::collections::HashMap::new();
    for mem in &memories {
        for tag in &mem.tags {
            let entry = topic_health.entry(tag.clone()).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += mem.retention_strength;
        }
    }
    let mut knowledge_gaps: Vec<Value> = topic_health.iter()
        .filter(|(_, (count, total_ret))| {
            let avg = total_ret / *count as f64;
            avg < 0.4 || *count < 3
        })
        .map(|(tag, (count, total_ret))| {
            let avg_ret = total_ret / *count as f64;
            serde_json::json!({
                "topic": tag,
                "memory_count": count,
                "avg_retention": format!("{:.2}", avg_ret),
                "risk": if avg_ret < 0.2 { "critical" } else if avg_ret < 0.4 { "low" } else { "sparse" }
            })
        })
        .collect();
    knowledge_gaps.sort_by(|a, b| {
        let ar: f64 = a["avg_retention"].as_str().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        let br: f64 = b["avg_retention"].as_str().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        ar.partial_cmp(&br).unwrap_or(std::cmp::Ordering::Equal)
    });
    knowledge_gaps.truncate(15);

    // 3. Stale decisions — decisions older than 30 days that haven't been reviewed
    let thirty_days_ago = now - Duration::days(30);
    let stale_decisions: Vec<Value> = memories.iter()
        .filter(|m| {
            m.node_type == "decision"
                && m.last_accessed < thirty_days_ago
                && m.retention_strength < 0.5
        })
        .take(10)
        .map(|m| serde_json::json!({
            "id": m.id,
            "content": truncate(&m.content, 150),
            "days_since_access": (now - m.last_accessed).num_days(),
            "retention": format!("{:.2}", m.retention_strength),
        }))
        .collect();

    // 4. Confidence calibration — compare retrieval vs storage strength
    let overconfident: Vec<Value> = memories.iter()
        .filter(|m| m.retrieval_strength > 0.7 && m.storage_strength < 0.3)
        .take(10)
        .map(|m| serde_json::json!({
            "id": m.id,
            "content": truncate(&m.content, 120),
            "retrieval_strength": format!("{:.2}", m.retrieval_strength),
            "storage_strength": format!("{:.2}", m.storage_strength),
            "risk": "frequently retrieved but weakly encoded — may be unreliable"
        }))
        .collect();

    // 5. Pattern clusters — use spreading activation to find dense regions
    let patterns = {
        let cog = cognitive.lock().await;
        let mut found = Vec::new();
        for mem in memories.iter().take(20) {
            let associations = cog.activation_network.get_associations(&mem.id);
            if associations.len() >= 3 {
                found.push(serde_json::json!({
                    "hub_memory": mem.id,
                    "hub_content": truncate(&mem.content, 100),
                    "connections": associations.len(),
                    "top_associations": associations.iter()
                        .take(5)
                        .map(|a| serde_json::json!({
                            "id": a.memory_id,
                            "weight": a.association_strength
                        }))
                        .collect::<Vec<_>>(),
                }));
            }
        }
        found.sort_by(|a, b| {
            b["connections"].as_u64().cmp(&a["connections"].as_u64())
        });
        found.truncate(10);
        found
    };

    // 6. Synthesize actionable insights
    let mut insights = Vec::new();

    if !contradictions.is_empty() {
        insights.push(format!(
            "Found {} potential contradictions in your memories. Review them to resolve conflicting knowledge.",
            contradictions.len()
        ));
    }
    if !stale_decisions.is_empty() {
        insights.push(format!(
            "{} decisions haven't been reviewed in 30+ days. They may be outdated.",
            stale_decisions.len()
        ));
    }
    if !overconfident.is_empty() {
        insights.push(format!(
            "{} memories are frequently retrieved but weakly encoded — high risk of recall errors.",
            overconfident.len()
        ));
    }
    let critical_gaps: Vec<_> = knowledge_gaps.iter()
        .filter(|g| g["risk"] == "critical")
        .collect();
    if !critical_gaps.is_empty() {
        let topics: Vec<_> = critical_gaps.iter()
            .take(5)
            .filter_map(|g| g["topic"].as_str())
            .collect();
        insights.push(format!(
            "Critical knowledge decay in: {}. These topics need reinforcement.",
            topics.join(", ")
        ));
    }

    // Persist insights
    for insight_text in &insights {
        let record = InsightRecord {
            id: Uuid::new_v4().to_string(),
            insight: insight_text.clone(),
            source_memories: vec![],
            confidence: 0.7,
            novelty_score: 0.5,
            insight_type: "reflection".to_string(),
            generated_at: now,
            tags: focus.as_deref().map(|f| vec![f.to_string()]).unwrap_or_default(),
            feedback: None,
            applied_count: 0,
        };
        if let Err(e) = storage.save_insight(&record) {
            tracing::warn!(error = %e, "Failed to persist reflection insight");
        }
    }

    Ok(serde_json::json!({
        "status": "reflected",
        "memoriesAnalyzed": memories.len(),
        "focus": focus,
        "depth": depth,
        "contradictions": contradictions,
        "knowledgeGaps": knowledge_gaps,
        "staleDecisions": stale_decisions,
        "overconfidentMemories": overconfident,
        "patternClusters": patterns,
        "insights": insights,
        "stats": {
            "contradictions_found": contradictions.len(),
            "knowledge_gaps": knowledge_gaps.len(),
            "stale_decisions": stale_decisions.len(),
            "overconfident": overconfident.len(),
            "pattern_clusters": patterns.len(),
            "insights_generated": insights.len(),
        }
    }))
}

fn content_conflicts(a: &str, b: &str) -> bool {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    let negation_pairs = [
        ("should", "should not"), ("should", "shouldn't"),
        ("always", "never"), ("do", "don't"), ("do", "do not"),
        ("use", "avoid"), ("enable", "disable"),
        ("recommended", "not recommended"), ("deprecated", "recommended"),
        ("works", "doesn't work"), ("works", "broken"),
    ];

    for (pos, neg) in &negation_pairs {
        if (a_lower.contains(pos) && b_lower.contains(neg))
            || (a_lower.contains(neg) && b_lower.contains(pos))
        {
            return true;
        }
    }
    false
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_detection() {
        assert!(content_conflicts("You should use Rust", "You should not use Rust"));
        assert!(content_conflicts("Always run tests", "Never run tests"));
        assert!(!content_conflicts("Rust is great", "Rust is performant"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert!(truncate("a very long string that goes on", 10).len() <= 14);
    }
}

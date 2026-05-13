//! Integration — validate insights, store new nodes, generate report.

use std::collections::HashSet;
use std::time::Instant;

use super::engine::DreamEngine;
use super::types::{
    CreativeConnection, CreativeConnectionType, DreamInsight, DreamPhase, PhaseResult, TriagedMemory,
};

impl DreamEngine {
    pub(super) fn phase_integration(
        &self,
        connections: &[CreativeConnection],
        triaged: &[TriagedMemory],
    ) -> (Vec<DreamInsight>, PhaseResult) {
        let start = Instant::now();
        let mut insights = Vec::new();
        let mut actions = Vec::new();

        // Validate connections: keep only those above threshold
        let valid_connections: Vec<&CreativeConnection> = connections
            .iter()
            .filter(|c| c.confidence >= self.validation_threshold)
            .collect();

        actions.push(format!(
            "Validated {}/{} connections (threshold: {})",
            valid_connections.len(),
            connections.len(),
            self.validation_threshold
        ));

        // Convert validated connections to insights
        for conn in &valid_connections {
            insights.push(DreamInsight {
                insight: conn.insight.clone(),
                source_memory_ids: vec![conn.memory_a_id.clone(), conn.memory_b_id.clone()],
                confidence: conn.confidence,
                novelty: self.estimate_novelty(conn, triaged),
                insight_type: match conn.connection_type {
                    CreativeConnectionType::CrossDomain => "CrossDomain".to_string(),
                    CreativeConnectionType::Causal => "Causal".to_string(),
                    CreativeConnectionType::Complementary => "Complementary".to_string(),
                    CreativeConnectionType::Contradictory => "Contradiction".to_string(),
                },
            });
        }

        // Deduplicate insights involving the same memory pairs
        let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
        insights.retain(|i| {
            if i.source_memory_ids.len() >= 2 {
                let pair = (
                    i.source_memory_ids[0]
                        .clone()
                        .min(i.source_memory_ids[1].clone()),
                    i.source_memory_ids[0]
                        .clone()
                        .max(i.source_memory_ids[1].clone()),
                );
                seen_pairs.insert(pair)
            } else {
                true
            }
        });

        // Sort by confidence * novelty (most interesting first)
        insights.sort_by(|a, b| {
            let score_a = a.confidence * a.novelty;
            let score_b = b.confidence * b.novelty;
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Cap at 20 insights
        insights.truncate(20);

        actions.push(format!("Generated {} dream insights", insights.len()));

        // Summary statistics
        let avg_retention: f64 = if triaged.is_empty() {
            0.0
        } else {
            triaged.iter().map(|m| m.retention_strength).sum::<f64>() / triaged.len() as f64
        };
        actions.push(format!(
            "Average retention across dreamed memories: {:.2}",
            avg_retention
        ));

        let phase = PhaseResult {
            phase: DreamPhase::Integration,
            duration_ms: start.elapsed().as_millis() as u64,
            memories_processed: triaged.len(),
            actions,
        };

        (insights, phase)
    }

    fn estimate_novelty(&self, conn: &CreativeConnection, triaged: &[TriagedMemory]) -> f64 {
        // Novelty is higher when:
        // 1. The memories are from different time periods
        // 2. The memories have different tags
        // 3. Cross-domain connections are inherently more novel

        let mem_a = triaged.iter().find(|m| m.id == conn.memory_a_id);
        let mem_b = triaged.iter().find(|m| m.id == conn.memory_b_id);

        let mut novelty: f64 = match conn.connection_type {
            CreativeConnectionType::CrossDomain => 0.7,
            CreativeConnectionType::Contradictory => 0.8,
            CreativeConnectionType::Causal => 0.5,
            CreativeConnectionType::Complementary => 0.4,
        };

        if let (Some(a), Some(b)) = (mem_a, mem_b) {
            // Time distance bonus
            let time_gap_days = (a.created_at - b.created_at).num_days().unsigned_abs();
            if time_gap_days > 7 {
                novelty += 0.1;
            }

            // Tag diversity bonus
            let tags_a: HashSet<&String> = a.tags.iter().collect();
            let tags_b: HashSet<&String> = b.tags.iter().collect();
            if tags_a.is_disjoint(&tags_b) {
                novelty += 0.1;
            }
        }

        novelty.min(1.0)
    }
}

//! REM — creative cross-domain pairing, pattern extraction, emotional processing.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::neuroscience::emotional_memory::EmotionalMemory;

use super::engine::DreamEngine;
use super::types::{
    CreativeConnection, CreativeConnectionType, DreamPhase, PhaseResult, TriageCategory,
    TriagedMemory,
};

impl DreamEngine {
    pub(super) fn phase_rem(
        &self,
        triaged: &[TriagedMemory],
        emotional_memory: &mut EmotionalMemory,
    ) -> (Vec<CreativeConnection>, usize, PhaseResult) {
        let start = Instant::now();
        let mut connections = Vec::new();
        let mut actions = Vec::new();
        let mut emotional_processed = 0;

        // Group memories by primary tag for cross-domain pairing
        let mut tag_groups: HashMap<String, Vec<&TriagedMemory>> = HashMap::new();
        for tm in triaged {
            let primary_tag = tm
                .tags
                .first()
                .cloned()
                .unwrap_or_else(|| "untagged".to_string());
            tag_groups.entry(primary_tag).or_default().push(tm);
        }

        let tag_keys: Vec<String> = tag_groups.keys().cloned().collect();

        // Cross-domain pairing: compare memories between different tag groups
        for i in 0..tag_keys.len() {
            for j in (i + 1)..tag_keys.len() {
                let group_a = &tag_groups[&tag_keys[i]];
                let group_b = &tag_groups[&tag_keys[j]];

                // Sample pairs (max 5 per group pair to keep bounded)
                let max_pairs = 5;
                let mut pair_count = 0;

                for mem_a in group_a.iter().take(3) {
                    for mem_b in group_b.iter().take(3) {
                        if pair_count >= max_pairs {
                            break;
                        }

                        // Check for shared words (simple content similarity)
                        let similarity = self.content_similarity(&mem_a.content, &mem_b.content);

                        if similarity > self.min_insight_confidence {
                            let conn_type = self.classify_connection(mem_a, mem_b, similarity);
                            let insight = self.generate_connection_insight(
                                mem_a,
                                mem_b,
                                &tag_keys[i],
                                &tag_keys[j],
                                conn_type,
                            );

                            connections.push(CreativeConnection {
                                memory_a_id: mem_a.id.clone(),
                                memory_b_id: mem_b.id.clone(),
                                insight,
                                confidence: similarity,
                                connection_type: conn_type,
                            });
                            pair_count += 1;
                        }
                    }
                }
            }
        }

        actions.push(format!(
            "Cross-domain pairing: {} tag groups, {} connections found",
            tag_keys.len(),
            connections.len()
        ));

        // Emotional processing: reduce intensity of error/frustration memories
        for tm in triaged {
            if tm.category == TriageCategory::Emotional && tm.emotional_valence < -0.3 {
                // Process negative emotional memories — extract the lesson, reduce raw emotion
                // In practice: the insight extraction above captures the lesson,
                // and we record the emotional processing for the engine
                emotional_memory.record_encoding(&tm.id, tm.emotional_valence * 0.7, 0.3);
                emotional_processed += 1;
            }
        }

        if emotional_processed > 0 {
            actions.push(format!(
                "Emotional processing: {} negative memories had intensity reduced",
                emotional_processed
            ));
        }

        // Pattern extraction: find repeated patterns across memories
        let pattern_count = self.extract_patterns(triaged, &mut connections);
        if pattern_count > 0 {
            actions.push(format!(
                "Pattern extraction: {} shared patterns found",
                pattern_count
            ));
        }

        let phase = PhaseResult {
            phase: DreamPhase::Rem,
            duration_ms: start.elapsed().as_millis() as u64,
            memories_processed: triaged.len(),
            actions,
        };

        (connections, emotional_processed, phase)
    }

    pub(super) fn content_similarity(&self, a: &str, b: &str) -> f64 {
        let words_a: HashSet<&str> = a
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 3)
            .collect();
        let words_b: HashSet<&str> = b
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 3)
            .collect();

        if words_a.is_empty() || words_b.is_empty() {
            return 0.0;
        }

        let intersection = words_a.intersection(&words_b).count() as f64;
        let union = words_a.union(&words_b).count() as f64;
        intersection / union // Jaccard similarity
    }

    fn classify_connection(
        &self,
        a: &TriagedMemory,
        b: &TriagedMemory,
        similarity: f64,
    ) -> CreativeConnectionType {
        // Check for contradiction (opposing sentiments about similar content)
        if (a.emotional_valence - b.emotional_valence).abs() > 1.0 && similarity > 0.4 {
            return CreativeConnectionType::Contradictory;
        }

        // Check for causal (temporal ordering + one references the other's topic)
        if a.created_at < b.created_at && similarity > 0.3 {
            let time_gap = (b.created_at - a.created_at).num_hours();
            if time_gap < 24 {
                return CreativeConnectionType::Causal;
            }
        }

        // Cross-domain if different primary tags
        if a.tags.first() != b.tags.first() {
            return CreativeConnectionType::CrossDomain;
        }

        CreativeConnectionType::Complementary
    }

    fn generate_connection_insight(
        &self,
        a: &TriagedMemory,
        b: &TriagedMemory,
        tag_a: &str,
        tag_b: &str,
        conn_type: CreativeConnectionType,
    ) -> String {
        let a_summary = if a.content.len() > 60 {
            &a.content[..60]
        } else {
            &a.content
        };
        let b_summary = if b.content.len() > 60 {
            &b.content[..60]
        } else {
            &b.content
        };

        match conn_type {
            CreativeConnectionType::CrossDomain => {
                format!(
                    "Cross-domain pattern between [{}] and [{}]: '{}...' connects to '{}...'",
                    tag_a, tag_b, a_summary, b_summary
                )
            }
            CreativeConnectionType::Causal => {
                format!(
                    "Possible causal link: '{}...' may have led to '{}...'",
                    a_summary, b_summary
                )
            }
            CreativeConnectionType::Complementary => {
                format!(
                    "Complementary knowledge: '{}...' and '{}...' fill gaps in each other",
                    a_summary, b_summary
                )
            }
            CreativeConnectionType::Contradictory => {
                format!(
                    "Contradiction detected: '{}...' vs '{}...' — may need resolution",
                    a_summary, b_summary
                )
            }
        }
    }

    fn extract_patterns(
        &self,
        triaged: &[TriagedMemory],
        connections: &mut Vec<CreativeConnection>,
    ) -> usize {
        // Find memories that share common n-word sequences (patterns)
        let mut bigram_index: HashMap<(String, String), Vec<usize>> = HashMap::new();

        for (idx, tm) in triaged.iter().enumerate() {
            let words: Vec<String> = tm
                .content
                .split_whitespace()
                .map(|w| w.to_lowercase())
                .filter(|w| w.len() > 3)
                .collect();

            for window in words.windows(2) {
                let key = (window[0].clone(), window[1].clone());
                bigram_index.entry(key).or_default().push(idx);
            }
        }

        // Find bigrams shared by 3+ memories (indicates a pattern)
        let mut pattern_count = 0;
        for (bigram, indices) in &bigram_index {
            if indices.len() >= 3 && indices.len() <= 10 {
                pattern_count += 1;
                // Create a connection between the first and last memory sharing this pattern
                if let (Some(&first), Some(&last)) = (indices.first(), indices.last())
                    && first != last
                {
                    connections.push(CreativeConnection {
                        memory_a_id: triaged[first].id.clone(),
                        memory_b_id: triaged[last].id.clone(),
                        insight: format!(
                            "Shared pattern '{}  {}' found across {} memories",
                            bigram.0,
                            bigram.1,
                            indices.len()
                        ),
                        confidence: (indices.len() as f64 / triaged.len() as f64).min(1.0),
                        connection_type: CreativeConnectionType::CrossDomain,
                    });
                }
            }
        }

        pattern_count
    }
}

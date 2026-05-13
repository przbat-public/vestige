//! NREM1 — light sleep / triage. Score, categorize, build replay queue.

use std::collections::HashMap;
use std::time::Instant;

use crate::memory::KnowledgeNode;
use crate::neuroscience::emotional_memory::EmotionalMemory;
use crate::neuroscience::importance_signals::ImportanceSignals;

use super::engine::DreamEngine;
use super::types::{DreamPhase, PhaseResult, TriageCategory, TriagedMemory};

impl DreamEngine {
    pub(super) fn phase_nrem1(
        &self,
        memories: &[KnowledgeNode],
        emotional_memory: &mut EmotionalMemory,
        importance_signals: &ImportanceSignals,
    ) -> (Vec<TriagedMemory>, Vec<String>, PhaseResult) {
        let start = Instant::now();
        let mut triaged = Vec::with_capacity(memories.len());
        let mut actions = Vec::new();

        for node in memories {
            // Score importance using 4-channel model
            let ctx = crate::neuroscience::importance_signals::Context::current();
            let score = importance_signals.compute_importance(&node.content, &ctx);
            let importance = score.composite;

            // Evaluate emotional content
            let emotional = emotional_memory.evaluate_content(&node.content);

            // Categorize
            let category = self.categorize_memory(node, importance, &emotional.category);

            triaged.push(TriagedMemory {
                id: node.id.clone(),
                content: node.content.clone(),
                importance,
                category,
                tags: node.tags.clone(),
                created_at: node.created_at,
                retention_strength: node.retention_strength,
                emotional_valence: emotional.valence,
                is_flashbulb: emotional.is_flashbulb,
            });
        }

        // Sort by importance (highest first)
        triaged.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Build replay queue: 70% high-value, 30% random noise floor
        let high_value_count = (triaged.len() as f64 * self.high_value_ratio).ceil() as usize;
        let random_count = triaged.len().saturating_sub(high_value_count);

        let mut replay_queue: Vec<String> = triaged
            .iter()
            .take(high_value_count)
            .map(|m| m.id.clone())
            .collect();

        // Add random noise floor from the remaining memories
        if random_count > 0 {
            let remaining: Vec<&TriagedMemory> = triaged.iter().skip(high_value_count).collect();
            // Simple deterministic shuffle using content hash
            let mut noise: Vec<&TriagedMemory> = remaining;
            noise.sort_by_key(|m| {
                let hash: u64 =
                    m.id.bytes()
                        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
                hash
            });
            for m in noise.iter().take(random_count) {
                replay_queue.push(m.id.clone());
            }
        }

        // Count categories
        let mut cat_counts: HashMap<&str, usize> = HashMap::new();
        for t in &triaged {
            let label = match t.category {
                TriageCategory::Emotional => "emotional",
                TriageCategory::FutureRelevant => "future_relevant",
                TriageCategory::Rewarded => "rewarded",
                TriageCategory::Novel => "novel",
                TriageCategory::Standard => "standard",
            };
            *cat_counts.entry(label).or_insert(0) += 1;
        }

        actions.push(format!("Scored {} memories", triaged.len()));
        actions.push(format!("Categories: {:?}", cat_counts));
        actions.push(format!(
            "Replay queue: {} high-value + {} noise = {} total",
            high_value_count.min(triaged.len()),
            replay_queue
                .len()
                .saturating_sub(high_value_count.min(triaged.len())),
            replay_queue.len()
        ));

        let flashbulb_count = triaged.iter().filter(|m| m.is_flashbulb).count();
        if flashbulb_count > 0 {
            actions.push(format!("Flashbulb memories detected: {}", flashbulb_count));
        }

        let phase = PhaseResult {
            phase: DreamPhase::Nrem1,
            duration_ms: start.elapsed().as_millis() as u64,
            memories_processed: triaged.len(),
            actions,
        };

        (triaged, replay_queue, phase)
    }
}

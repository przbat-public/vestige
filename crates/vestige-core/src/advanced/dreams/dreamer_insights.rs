//! Phase 4 of the dream cycle: generate insights from clustered memories.
//! Insights are scored on novelty and cluster cohesion; only those above the
//! configured novelty floor survive.
//!
//! Extracted from `dreamer.rs`. Inherent methods on [`MemoryDreamer`].

use std::collections::{HashMap, HashSet};

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::constants::MIN_MEMORIES_FOR_INSIGHT;
use super::dreamer::MemoryDreamer;
use super::types::{DreamMemory, DreamStats, InsightType, SynthesizedInsight};

impl MemoryDreamer {
    pub(super) fn generate_insights(
        &self,
        memories: &[&DreamMemory],
        clusters: &[Vec<String>],
        stats: &mut DreamStats,
    ) -> Vec<SynthesizedInsight> {
        let mut insights = Vec::new();
        let memory_map: HashMap<_, _> = memories.iter().map(|m| (&m.id, *m)).collect();

        for cluster in clusters {
            stats.candidates_considered += 1;

            let cluster_memories: Vec<_> = cluster
                .iter()
                .filter_map(|id| memory_map.get(&id).copied())
                .collect();

            if cluster_memories.len() < MIN_MEMORIES_FOR_INSIGHT {
                continue;
            }

            if let Some(insight) = self.generate_insight_from_cluster(&cluster_memories)
                && insight.novelty_score >= self.config.min_novelty
            {
                insights.push(insight);
            }

            if insights.len() >= self.config.max_insights {
                break;
            }
        }

        insights
    }

    fn generate_insight_from_cluster(
        &self,
        memories: &[&DreamMemory],
    ) -> Option<SynthesizedInsight> {
        if memories.is_empty() {
            return None;
        }

        let all_tags: HashSet<_> = memories
            .iter()
            .flat_map(|m| m.tags.iter().cloned())
            .collect();

        // A tag is "common" if more than half the cluster carries it.
        let common_tags: Vec<_> = all_tags
            .iter()
            .filter(|t| {
                memories.iter().filter(|m| m.tags.contains(*t)).count() > memories.len() / 2
            })
            .cloned()
            .collect();

        let (insight_text, insight_type) = self.synthesize_insight_text(memories, &common_tags);
        let novelty = self.calculate_novelty(&insight_text, memories);
        let confidence = self.calculate_insight_confidence(memories);

        Some(SynthesizedInsight {
            id: format!("insight-{}", Uuid::new_v4()),
            insight: insight_text,
            source_memories: memories.iter().map(|m| m.id.clone()).collect(),
            confidence,
            novelty_score: novelty,
            insight_type,
            generated_at: Utc::now(),
            tags: common_tags,
        })
    }

    fn synthesize_insight_text(
        &self,
        memories: &[&DreamMemory],
        common_tags: &[String],
    ) -> (String, InsightType) {
        let time_range = memories
            .iter()
            .map(|m| m.created_at)
            .fold((Utc::now(), Utc::now() - Duration::days(365)), |acc, t| {
                (acc.0.min(t), acc.1.max(t))
            });

        let time_span_days = (time_range.1 - time_range.0).num_days();

        if time_span_days > 30 {
            let insight = format!(
                "Pattern observed over {} days in '{}': recurring theme across {} related memories",
                time_span_days,
                common_tags.first().map(|s| s.as_str()).unwrap_or("topic"),
                memories.len()
            );
            (insight, InsightType::TemporalTrend)
        } else if common_tags.len() >= 2 {
            let insight = format!(
                "Connection between '{}' and '{}' found across {} memories",
                common_tags.first().map(|s| s.as_str()).unwrap_or("A"),
                common_tags.get(1).map(|s| s.as_str()).unwrap_or("B"),
                memories.len()
            );
            (insight, InsightType::HiddenConnection)
        } else if memories.len() >= 3 {
            let insight = format!(
                "Recurring pattern in '{}': {} instances identified with common characteristics",
                common_tags.first().map(|s| s.as_str()).unwrap_or("topic"),
                memories.len()
            );
            (insight, InsightType::RecurringPattern)
        } else {
            let insight = format!(
                "Synthesis: {} related memories about '{}' suggest broader understanding",
                memories.len(),
                common_tags.first().map(|s| s.as_str()).unwrap_or("topic")
            );
            (insight, InsightType::Synthesis)
        }
    }

    fn calculate_novelty(&self, insight: &str, source_memories: &[&DreamMemory]) -> f64 {
        // Novelty = how many insight words don't appear in the source set.
        let insight_words: HashSet<_> = insight
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() > 3)
            .collect();

        let source_words: HashSet<_> = source_memories
            .iter()
            .flat_map(|m| m.content.split_whitespace())
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() > 3)
            .collect();

        let novel_words = insight_words.difference(&source_words).count();
        let total_words = insight_words.len();

        if total_words == 0 {
            return 0.3;
        }

        let word_novelty = (novel_words as f64 / total_words as f64) * 0.5;
        let source_bonus = ((source_memories.len() as f64 - 2.0) * 0.1).clamp(0.0, 0.3);

        (word_novelty + source_bonus + 0.2).min(1.0)
    }

    fn calculate_insight_confidence(&self, memories: &[&DreamMemory]) -> f64 {
        // Three additive factors, capped at 0.95.
        let count_factor = (memories.len() as f64 / 5.0).min(1.0) * 0.4;

        let avg_access =
            memories.iter().map(|m| m.access_count as f64).sum::<f64>() / memories.len() as f64;
        let access_factor = (avg_access / 10.0).min(1.0) * 0.3;

        let tag_overlap = self.average_tag_overlap(memories);
        let tag_factor = tag_overlap * 0.3;

        (count_factor + access_factor + tag_factor).min(0.95)
    }

    fn average_tag_overlap(&self, memories: &[&DreamMemory]) -> f64 {
        if memories.len() < 2 {
            return 0.0;
        }

        let mut total_overlap = 0.0;
        let mut comparisons = 0;

        for i in 0..memories.len() {
            for j in (i + 1)..memories.len() {
                total_overlap += self.tag_similarity(&memories[i].tags, &memories[j].tags);
                comparisons += 1;
            }
        }

        if comparisons == 0 {
            0.0
        } else {
            total_overlap / comparisons as f64
        }
    }
}

//! Metacognition Layer — Self-monitoring of memory retrieval quality
//!
//! Tracks search effectiveness and dynamically adjusts retrieval parameters.
//! Inspired by Nelson & Narens (1990) metacognitive monitoring framework.
//!
//! Key metrics:
//! - Hit rate: fraction of searches that return ≥1 result
//! - Utility rate: fraction of promoted vs total retrieved memories
//! - Coverage: ratio of unique memories surfaced vs total memory count
//! - Knowledge gaps: topics with consistently empty or low-confidence results

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

const HISTORY_CAPACITY: usize = 100;

/// Metacognitive monitor for search quality
#[derive(Debug, Clone)]
pub struct MetacognitionMonitor {
    history: VecDeque<SearchOutcome>,
    gap_tracker: Vec<GapSignal>,
}

/// Outcome of a single search query
#[derive(Debug, Clone)]
struct SearchOutcome {
    had_results: bool,
    result_count: usize,
    avg_confidence: f64,
    query_topic: String,
}

/// A topic that consistently yields poor results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapSignal {
    pub topic: String,
    pub miss_count: u32,
    pub total_queries: u32,
}

/// Aggregated metacognitive metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetacognitionReport {
    pub hit_rate: f64,
    pub avg_result_count: f64,
    pub avg_confidence: f64,
    pub total_queries_tracked: usize,
    pub knowledge_gaps: Vec<GapSignal>,
}

impl MetacognitionMonitor {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
            gap_tracker: Vec::new(),
        }
    }

    /// Record a search outcome
    pub fn record_search(&mut self, result_count: usize, avg_confidence: f64, query_topic: &str) {
        let outcome = SearchOutcome {
            had_results: result_count > 0,
            result_count,
            avg_confidence,
            query_topic: query_topic.to_string(),
        };

        if self.history.len() >= HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(outcome);

        // Update gap tracker
        if result_count == 0 || avg_confidence < 0.3 {
            if let Some(gap) = self.gap_tracker.iter_mut().find(|g| g.topic == query_topic) {
                gap.miss_count += 1;
                gap.total_queries += 1;
            } else {
                self.gap_tracker.push(GapSignal {
                    topic: query_topic.to_string(),
                    miss_count: 1,
                    total_queries: 1,
                });
            }
        } else if let Some(gap) = self.gap_tracker.iter_mut().find(|g| g.topic == query_topic) {
            gap.total_queries += 1;
        }
    }

    /// Generate a metacognition report
    pub fn report(&self) -> MetacognitionReport {
        let total = self.history.len();
        if total == 0 {
            return MetacognitionReport {
                hit_rate: 0.0,
                avg_result_count: 0.0,
                avg_confidence: 0.0,
                total_queries_tracked: 0,
                knowledge_gaps: Vec::new(),
            };
        }

        let hits = self.history.iter().filter(|o| o.had_results).count();
        let hit_rate = hits as f64 / total as f64;
        let avg_results = self.history.iter().map(|o| o.result_count).sum::<usize>() as f64 / total as f64;
        let avg_conf = self.history.iter().map(|o| o.avg_confidence).sum::<f64>() / total as f64;

        // Gaps: topics with >50% miss rate and at least 3 queries
        let gaps: Vec<GapSignal> = self.gap_tracker.iter()
            .filter(|g| g.total_queries >= 3 && (g.miss_count as f64 / g.total_queries as f64) > 0.5)
            .cloned()
            .collect();

        MetacognitionReport {
            hit_rate,
            avg_result_count: avg_results,
            avg_confidence: avg_conf,
            total_queries_tracked: total,
            knowledge_gaps: gaps,
        }
    }

    /// Suggest search parameter adjustments based on recent performance
    pub fn suggest_adjustments(&self) -> SearchAdjustments {
        let report = self.report();
        SearchAdjustments {
            expand_limit: report.hit_rate < 0.7,
            lower_min_similarity: report.avg_result_count < 2.0 && report.total_queries_tracked >= 5,
            use_hyde: report.hit_rate < 0.5 && report.total_queries_tracked >= 10,
        }
    }
}

impl Default for MetacognitionMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Dynamic parameter adjustments suggested by the metacognition layer
#[derive(Debug, Clone)]
pub struct SearchAdjustments {
    pub expand_limit: bool,
    pub lower_min_similarity: bool,
    pub use_hyde: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_monitor_returns_zero_report() {
        let monitor = MetacognitionMonitor::new();
        let report = monitor.report();
        assert_eq!(report.hit_rate, 0.0);
        assert_eq!(report.total_queries_tracked, 0);
    }

    #[test]
    fn tracks_hit_rate() {
        let mut monitor = MetacognitionMonitor::new();
        monitor.record_search(3, 0.8, "rust");
        monitor.record_search(0, 0.0, "obscure-topic");
        monitor.record_search(2, 0.7, "python");

        let report = monitor.report();
        assert!((report.hit_rate - 0.6667).abs() < 0.01, "Hit rate: {}", report.hit_rate);
        assert_eq!(report.total_queries_tracked, 3);
    }

    #[test]
    fn detects_knowledge_gaps() {
        let mut monitor = MetacognitionMonitor::new();
        for _ in 0..5 {
            monitor.record_search(0, 0.0, "quantum-computing");
        }
        monitor.record_search(5, 0.9, "rust-patterns");

        let report = monitor.report();
        assert_eq!(report.knowledge_gaps.len(), 1);
        assert_eq!(report.knowledge_gaps[0].topic, "quantum-computing");
    }

    #[test]
    fn suggests_expansion_on_low_hit_rate() {
        let mut monitor = MetacognitionMonitor::new();
        for _ in 0..10 {
            monitor.record_search(0, 0.0, "topic");
        }
        let adj = monitor.suggest_adjustments();
        assert!(adj.expand_limit, "Should suggest expanding limit");
        assert!(adj.use_hyde, "Should suggest HyDE");
    }

    #[test]
    fn history_bounded_at_capacity() {
        let mut monitor = MetacognitionMonitor::new();
        for i in 0..200 {
            monitor.record_search(1, 0.5, &format!("topic-{}", i));
        }
        assert!(monitor.history.len() <= HISTORY_CAPACITY);
    }
}

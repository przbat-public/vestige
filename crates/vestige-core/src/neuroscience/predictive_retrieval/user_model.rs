//! Persistent model of user interests, query history and behavioural
//! patterns that feeds the predictive engine.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{QueryPattern, SessionContext, TemporalPatterns};
use super::{INTEREST_DECAY_RATE, INTEREST_LEARNING_RATE, MAX_QUERY_HISTORY};

// ============================================================================
// USER MODEL
// ============================================================================

/// Model of user interests and behavior for prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModel {
    /// Topic interest weights (topic -> weight 0.0-1.0)
    pub interests: HashMap<String, f64>,
    /// Recent queries for pattern analysis
    pub recent_queries: VecDeque<QueryPattern>,
    /// Temporal patterns in user behavior
    pub temporal_patterns: TemporalPatterns,
    /// Current session context
    pub session_context: SessionContext,
    /// Co-access patterns (memory_id -> Vec<(memory_id, count)>)
    pub co_access_patterns: HashMap<String, Vec<(String, u32)>>,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
    /// Total number of interactions tracked
    pub total_interactions: u64,
}

impl Default for UserModel {
    fn default() -> Self {
        Self {
            interests: HashMap::new(),
            recent_queries: VecDeque::with_capacity(MAX_QUERY_HISTORY),
            temporal_patterns: TemporalPatterns::new(),
            session_context: SessionContext::new(),
            co_access_patterns: HashMap::new(),
            last_updated: Utc::now(),
            total_interactions: 0,
        }
    }
}

impl UserModel {
    /// Create a new user model
    pub fn new() -> Self {
        Self::default()
    }

    /// Update interest weight for a topic
    pub fn update_interest(&mut self, topic: &str, weight: f64) {
        let normalized_topic = topic.to_lowercase();
        let current = self
            .interests
            .entry(normalized_topic.clone())
            .or_insert(0.0);

        // Exponential moving average for smooth updates
        *current = *current * (1.0 - INTEREST_LEARNING_RATE) + weight * INTEREST_LEARNING_RATE;

        // Clamp to valid range
        *current = current.clamp(0.0, 1.0);

        // Update temporal patterns
        self.temporal_patterns
            .record_activity(Utc::now(), &normalized_topic, weight);
        self.last_updated = Utc::now();
        self.total_interactions += 1;
    }

    /// Record a query for pattern analysis
    pub fn record_query(&mut self, query: &str, tags: &[&str]) {
        let pattern = QueryPattern {
            query: query.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            timestamp: Utc::now(),
            accessed_results: Vec::new(),
            was_satisfied: None,
        };

        self.recent_queries.push_back(pattern);

        // Maintain capacity
        while self.recent_queries.len() > MAX_QUERY_HISTORY {
            self.recent_queries.pop_front();
        }

        // Update session context
        self.session_context.add_query(query.to_string());

        // Update interests based on query topics
        for tag in tags {
            self.update_interest(tag, 0.5);
        }

        self.last_updated = Utc::now();
    }

    /// Record that a memory was accessed
    pub fn record_memory_access(&mut self, memory_id: &str, tags: &[String]) {
        // Update session
        self.session_context
            .add_accessed_memory(memory_id.to_string());

        // Update interests based on accessed memory tags
        for tag in tags {
            self.update_interest(tag, 0.7);
        }

        // Update co-access patterns
        // Collect IDs first to avoid borrow issues
        let existing_ids: Vec<String> = self
            .session_context
            .accessed_memories
            .iter()
            .filter(|id| *id != memory_id)
            .cloned()
            .collect();

        for existing_id in existing_ids {
            // Bidirectional co-access
            self.record_co_access(&existing_id, memory_id);
            self.record_co_access(memory_id, &existing_id);
        }

        self.last_updated = Utc::now();
    }

    /// Record co-access between two memories
    fn record_co_access(&mut self, from: &str, to: &str) {
        let patterns = self.co_access_patterns.entry(from.to_string()).or_default();

        if let Some(existing) = patterns.iter_mut().find(|(id, _)| id == to) {
            existing.1 += 1;
        } else {
            patterns.push((to.to_string(), 1));
        }

        // Sort by count and keep top patterns
        patterns.sort_by(|a, b| b.1.cmp(&a.1));
        patterns.truncate(50);
    }

    /// Apply decay to interest weights (call periodically)
    pub fn apply_decay(&mut self) {
        for weight in self.interests.values_mut() {
            *weight *= INTEREST_DECAY_RATE;
        }

        // Remove very low weights
        self.interests.retain(|_, w| *w > 0.01);

        self.last_updated = Utc::now();
    }

    /// Get top interests
    pub fn top_interests(&self, limit: usize) -> Vec<(String, f64)> {
        let mut interests: Vec<_> = self
            .interests
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        interests.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        interests.truncate(limit);
        interests
    }

    /// Get co-access candidates for a memory
    pub fn get_co_access_candidates(&self, memory_id: &str) -> Vec<(String, f64)> {
        self.co_access_patterns
            .get(memory_id)
            .map(|patterns| {
                let total: u32 = patterns.iter().map(|(_, c)| c).sum();
                patterns
                    .iter()
                    .map(|(id, count)| (id.clone(), *count as f64 / total as f64))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if session should be reset
    pub fn should_reset_session(&self) -> bool {
        !self.session_context.is_active()
    }

    /// Reset the session context
    pub fn reset_session(&mut self) {
        self.session_context = SessionContext::new();
    }
}

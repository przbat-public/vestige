//! Reward signal — dopamine-like reinforcement tracker (was-this-helpful?).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{MAX_IMPORTANCE, MAX_OUTCOME_HISTORY, MIN_IMPORTANCE};

// ============================================================================
// REWARD SIGNAL (Dopamine-like: Positive Outcomes)
// ============================================================================

/// Reward signal inspired by dopamine's role in reinforcement learning.
///
/// In the brain, dopamine release in the nucleus accumbens reinforces behaviors
/// that lead to positive outcomes. This signal tracks which memories have been
/// associated with successful outcomes and should be prioritized.
///
/// ## How It Works
///
/// 1. Records outcomes when memories are used (helpful, not helpful, etc.)
/// 2. Learns patterns that predict positive outcomes
/// 3. Gives higher importance to memories with track record of success
#[derive(Debug)]
pub struct RewardSignal {
    /// Outcome history: memory_id -> outcomes
    outcome_history: Arc<RwLock<HashMap<String, Outcome>>>,
    /// Learned reward patterns
    reward_patterns: Arc<RwLock<Vec<RewardPattern>>>,
}

impl Default for RewardSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl RewardSignal {
    /// Create a new reward signal tracker
    pub fn new() -> Self {
        Self {
            outcome_history: Arc::new(RwLock::new(HashMap::new())),
            reward_patterns: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record an outcome for a memory
    pub fn record_outcome(&self, memory_id: &str, outcome_type: OutcomeType) {
        if let Ok(mut history) = self.outcome_history.write() {
            let outcome = history
                .entry(memory_id.to_string())
                .or_insert_with(|| Outcome::new(memory_id));

            outcome.record(outcome_type);

            // Prune old entries if needed
            if history.len() > MAX_OUTCOME_HISTORY {
                self.prune_old_outcomes(&mut history);
            }
        }
    }

    /// Record outcome with context for pattern learning
    pub fn record_outcome_with_context(
        &self,
        memory_id: &str,
        outcome_type: OutcomeType,
        context_tags: &[String],
    ) {
        self.record_outcome(memory_id, outcome_type.clone());

        // Learn pattern from this outcome
        if matches!(
            outcome_type,
            OutcomeType::Helpful | OutcomeType::VeryHelpful
        ) {
            self.learn_pattern(context_tags, 1.0);
        } else if matches!(outcome_type, OutcomeType::NotHelpful | OutcomeType::Harmful) {
            self.learn_pattern(context_tags, -0.5);
        }
    }

    /// Compute reward score for a memory
    pub fn compute(&self, memory_id: &str) -> f64 {
        let history = match self.outcome_history.read() {
            Ok(h) => h,
            Err(_) => {
                tracing::warn!("RewardSignal: outcome_history RwLock poisoned");
                return 0.5;
            }
        };

        match history.get(memory_id) {
            Some(outcome) => outcome.reward_score(),
            None => 0.5, // No history = neutral
        }
    }

    /// Compute reward score with context-based prediction
    pub fn compute_with_context(&self, memory_id: &str, context_tags: &[String]) -> f64 {
        let base_score = self.compute(memory_id);
        let pattern_score = self.compute_pattern_score(context_tags);

        // Combine historical performance with pattern prediction
        (base_score * 0.7 + pattern_score * 0.3).clamp(MIN_IMPORTANCE, MAX_IMPORTANCE)
    }

    /// Get explanation for reward score
    pub fn explain(&self, memory_id: &str) -> RewardExplanation {
        let score = self.compute(memory_id);
        let history = self.outcome_history.read().ok();

        let (helpful_count, total_count, last_outcome) = match &history {
            Some(h) => match h.get(memory_id) {
                Some(outcome) => (
                    outcome.helpful_count,
                    outcome.total_count,
                    outcome.last_outcome.clone(),
                ),
                None => (0, 0, None),
            },
            None => (0, 0, None),
        };

        RewardExplanation {
            score,
            helpful_count,
            total_count,
            helpfulness_ratio: if total_count > 0 {
                helpful_count as f64 / total_count as f64
            } else {
                0.5
            },
            last_outcome,
        }
    }

    /// Get top performing memories
    pub fn get_top_performers(&self, limit: usize) -> Vec<(String, f64)> {
        let history = match self.outcome_history.read() {
            Ok(h) => h,
            Err(_) => return vec![],
        };

        let mut scores: Vec<_> = history
            .iter()
            .map(|(id, outcome)| (id.clone(), outcome.reward_score()))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }

    fn learn_pattern(&self, tags: &[String], reward: f64) {
        if let Ok(mut patterns) = self.reward_patterns.write() {
            // Check for existing pattern
            for pattern in patterns.iter_mut() {
                if pattern.matches(tags) {
                    pattern.update(reward);
                    return;
                }
            }

            // Create new pattern
            patterns.push(RewardPattern::new(tags, reward));

            // Limit pattern count
            if patterns.len() > 1000 {
                patterns.sort_by(|a, b| {
                    b.strength
                        .partial_cmp(&a.strength)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                patterns.truncate(500);
            }
        }
    }

    fn compute_pattern_score(&self, tags: &[String]) -> f64 {
        let patterns = match self.reward_patterns.read() {
            Ok(p) => p,
            Err(_) => return 0.5,
        };

        let matching: Vec<_> = patterns.iter().filter(|p| p.matches(tags)).collect();

        if matching.is_empty() {
            return 0.5;
        }

        let total_strength: f64 = matching.iter().map(|p| p.strength.abs()).sum();
        if total_strength == 0.0 {
            return 0.5;
        }

        let weighted_sum: f64 = matching
            .iter()
            .map(|p| p.strength * (0.5 + p.strength.signum() * 0.5))
            .sum();

        (weighted_sum / total_strength).clamp(0.0, 1.0)
    }

    fn prune_old_outcomes(&self, history: &mut HashMap<String, Outcome>) {
        // Remove outcomes with lowest scores and oldest access
        let mut entries: Vec<_> = history
            .iter()
            .map(|(k, v)| (k.clone(), v.reward_score(), v.last_accessed))
            .collect();

        entries.sort_by(|a, b| {
            // Sort by score, then by recency
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.2.cmp(&a.2))
        });

        // Keep top entries
        let keep_count = MAX_OUTCOME_HISTORY * 4 / 5;
        let remove: HashSet<_> = entries
            .into_iter()
            .skip(keep_count)
            .map(|(id, _, _)| id)
            .collect();

        history.retain(|k, _| !remove.contains(k));
    }
}

/// Outcome tracking for a single memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    /// Memory ID
    pub memory_id: String,
    /// Total number of times this memory was used
    pub total_count: u32,
    /// Number of times marked as helpful
    pub helpful_count: u32,
    /// Number of times marked as very helpful
    pub very_helpful_count: u32,
    /// Number of times marked as not helpful
    pub not_helpful_count: u32,
    /// Number of times marked as harmful
    pub harmful_count: u32,
    /// Last outcome type
    pub last_outcome: Option<OutcomeType>,
    /// Last time this memory was accessed
    pub last_accessed: DateTime<Utc>,
    /// When tracking started
    pub created_at: DateTime<Utc>,
}

impl Outcome {
    fn new(memory_id: &str) -> Self {
        let now = Utc::now();
        Self {
            memory_id: memory_id.to_string(),
            total_count: 0,
            helpful_count: 0,
            very_helpful_count: 0,
            not_helpful_count: 0,
            harmful_count: 0,
            last_outcome: None,
            last_accessed: now,
            created_at: now,
        }
    }

    fn record(&mut self, outcome: OutcomeType) {
        self.total_count += 1;
        self.last_accessed = Utc::now();

        match &outcome {
            OutcomeType::Helpful => self.helpful_count += 1,
            OutcomeType::VeryHelpful => {
                self.helpful_count += 1;
                self.very_helpful_count += 1;
            }
            OutcomeType::NotHelpful => self.not_helpful_count += 1,
            OutcomeType::Harmful => self.harmful_count += 1,
            OutcomeType::Neutral => {}
        }

        self.last_outcome = Some(outcome);
    }

    fn reward_score(&self) -> f64 {
        if self.total_count == 0 {
            return 0.5;
        }

        // Weighted scoring
        let positive = self.helpful_count as f64 + self.very_helpful_count as f64 * 0.5;
        let negative = self.not_helpful_count as f64 + self.harmful_count as f64 * 2.0;

        let ratio = positive / (positive + negative + 1.0);

        // Apply confidence based on sample size
        let confidence = 1.0 - (1.0 / (self.total_count as f64 + 1.0));

        // Blend with neutral based on confidence
        (0.5 * (1.0 - confidence) + ratio * confidence).clamp(MIN_IMPORTANCE, MAX_IMPORTANCE)
    }
}

/// Types of outcomes that can be recorded
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OutcomeType {
    /// Memory was very helpful
    VeryHelpful,
    /// Memory was helpful
    Helpful,
    /// Memory was neutral/neither helpful nor harmful
    Neutral,
    /// Memory was not helpful
    NotHelpful,
    /// Memory was harmful/misleading
    Harmful,
}

/// Learned pattern that predicts reward
#[derive(Debug, Clone)]
struct RewardPattern {
    /// Tags that define this pattern
    tags: HashSet<String>,
    /// Strength of this pattern (-1.0 to 1.0)
    strength: f64,
    /// Number of times this pattern was observed
    observations: u32,
}

impl RewardPattern {
    fn new(tags: &[String], initial_reward: f64) -> Self {
        Self {
            tags: tags.iter().cloned().collect(),
            strength: initial_reward.clamp(-1.0, 1.0),
            observations: 1,
        }
    }

    fn matches(&self, tags: &[String]) -> bool {
        let tag_set: HashSet<_> = tags.iter().cloned().collect();
        let overlap = self.tags.intersection(&tag_set).count();
        overlap >= self.tags.len().min(tag_set.len()).max(1) / 2
    }

    fn update(&mut self, reward: f64) {
        self.observations += 1;
        // Exponential moving average
        let alpha = 2.0 / (self.observations as f64 + 1.0);
        self.strength = self.strength * (1.0 - alpha) + reward * alpha;
    }
}

/// Explanation of reward score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardExplanation {
    /// The computed reward score
    pub score: f64,
    /// Number of times marked helpful
    pub helpful_count: u32,
    /// Total number of uses
    pub total_count: u32,
    /// Ratio of helpful to total
    pub helpfulness_ratio: f64,
    /// Most recent outcome
    pub last_outcome: Option<OutcomeType>,
}

impl RewardExplanation {
    /// Generate human-readable explanation
    pub fn explain(&self) -> String {
        if self.total_count == 0 {
            "No usage history yet. Default neutral score.".to_string()
        } else {
            format!(
                "Helpful {}/{} times ({:.0}%). Score: {:.2}",
                self.helpful_count,
                self.total_count,
                self.helpfulness_ratio * 100.0,
                self.score
            )
        }
    }
}

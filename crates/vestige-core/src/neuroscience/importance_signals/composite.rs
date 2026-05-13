//! Composite ImportanceSignals — combines novelty/arousal/reward/attention into
//! one weighted score plus configuration helpers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::arousal::{ArousalExplanation, ArousalSignal};
use super::attention::{AccessPattern, AttentionExplanation, AttentionSignal, Session};
use super::novelty::{NoveltyExplanation, NoveltySignal};
use super::reward::{OutcomeType, RewardExplanation, RewardSignal};
use super::{
    Context, DEFAULT_AROUSAL_WEIGHT, DEFAULT_ATTENTION_WEIGHT, DEFAULT_NOVELTY_WEIGHT,
    DEFAULT_REWARD_WEIGHT, MAX_IMPORTANCE, MIN_IMPORTANCE,
};

// ============================================================================
// COMPOSITE IMPORTANCE SIGNALS
// ============================================================================

/// Multi-dimensional importance scoring inspired by neuromodulator systems.
///
/// Combines four independent importance signals into a composite score:
/// - Novelty (Dopamine): How surprising/unexpected is this content?
/// - Arousal (Norepinephrine): How emotionally intense is this content?
/// - Reward (Dopamine): How often has this content been helpful?
/// - Attention (Acetylcholine): Is the user actively focused/learning?
///
/// Each signal contributes to the final importance score with configurable weights.
#[derive(Debug)]
pub struct ImportanceSignals {
    /// Novelty signal (dopamine-like: prediction error, surprise)
    pub novelty: NoveltySignal,
    /// Arousal signal (norepinephrine-like: emotional intensity)
    pub arousal: ArousalSignal,
    /// Reward signal (dopamine-like: positive outcomes)
    pub reward: RewardSignal,
    /// Attention signal (acetylcholine-like: focus, learning mode)
    pub attention: AttentionSignal,
    /// Weights for composite calculation
    weights: CompositeWeights,
}

impl Default for ImportanceSignals {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportanceSignals {
    /// Create a new importance signals system
    pub fn new() -> Self {
        Self {
            novelty: NoveltySignal::new(),
            arousal: ArousalSignal::new(),
            reward: RewardSignal::new(),
            attention: AttentionSignal::new(),
            weights: CompositeWeights::default(),
        }
    }

    /// Create with custom weights
    pub fn with_weights(mut self, weights: CompositeWeights) -> Self {
        self.weights = weights;
        self
    }

    /// Compute composite importance for content
    pub fn compute_importance(&self, content: &str, context: &Context) -> ImportanceScore {
        let novelty = self.novelty.compute(content, context);
        let arousal = self.arousal.compute(content);

        // For reward and attention, we need additional context
        let reward = context
            .recent_memory_ids
            .first()
            .map(|id| self.reward.compute(id))
            .unwrap_or(0.5);

        let access_pattern = AccessPattern::default();
        let attention = self.attention.compute(&access_pattern);

        self.compute_composite(novelty, arousal, reward, attention, content, context)
    }

    /// Compute composite importance with explicit values
    pub fn compute_importance_explicit(
        &self,
        content: &str,
        context: &Context,
        memory_id: Option<&str>,
        access_pattern: Option<&AccessPattern>,
    ) -> ImportanceScore {
        let novelty = self.novelty.compute(content, context);
        let arousal = self.arousal.compute(content);

        let reward = memory_id.map(|id| self.reward.compute(id)).unwrap_or(0.5);

        let attention = access_pattern
            .map(|p| self.attention.compute(p))
            .unwrap_or(0.5);

        self.compute_composite(novelty, arousal, reward, attention, content, context)
    }

    /// Update novelty model (learning)
    pub fn learn_content(&mut self, content: &str) {
        self.novelty.update_model(content);
    }

    /// Record outcome for reward learning
    pub fn record_outcome(&self, memory_id: &str, outcome: OutcomeType) {
        self.reward.record_outcome(memory_id, outcome);
    }

    /// Record session for attention tracking
    pub fn record_session(&self, session: Session) {
        self.attention.record_session_activity(session);
    }

    /// Get current weights
    pub fn weights(&self) -> &CompositeWeights {
        &self.weights
    }

    /// Set weights
    pub fn set_weights(&mut self, weights: CompositeWeights) {
        self.weights = weights;
    }

    fn compute_composite(
        &self,
        novelty: f64,
        arousal: f64,
        reward: f64,
        attention: f64,
        content: &str,
        context: &Context,
    ) -> ImportanceScore {
        // Weighted composite
        let composite = novelty * self.weights.novelty
            + arousal * self.weights.arousal
            + reward * self.weights.reward
            + attention * self.weights.attention;

        // Encoding boost: high importance = stronger encoding
        let encoding_boost = 1.0 + (composite - 0.5) * 0.6; // 0.7 to 1.3

        // Consolidation priority based on score
        let consolidation_priority = if composite > 0.8 {
            ConsolidationPriority::Critical
        } else if composite > 0.6 {
            ConsolidationPriority::High
        } else if composite > 0.4 {
            ConsolidationPriority::Normal
        } else {
            ConsolidationPriority::Low
        };

        // Build explanations
        let novelty_explanation = self.novelty.explain(content, context);
        let arousal_explanation = self.arousal.explain(content);
        let reward_explanation = context
            .recent_memory_ids
            .first()
            .map(|id| self.reward.explain(id))
            .unwrap_or(RewardExplanation {
                score: 0.5,
                helpful_count: 0,
                total_count: 0,
                helpfulness_ratio: 0.5,
                last_outcome: None,
            });
        let attention_explanation = self.attention.explain(&AccessPattern::default());

        ImportanceScore {
            composite: composite.clamp(MIN_IMPORTANCE, MAX_IMPORTANCE),
            novelty,
            arousal,
            reward,
            attention,
            encoding_boost,
            consolidation_priority,
            weights_used: self.weights.clone(),
            novelty_explanation: Some(novelty_explanation),
            arousal_explanation: Some(arousal_explanation),
            reward_explanation: Some(reward_explanation),
            attention_explanation: Some(attention_explanation),
            computed_at: Utc::now(),
        }
    }
}

/// Weights for composite importance calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeWeights {
    /// Weight for novelty signal
    pub novelty: f64,
    /// Weight for arousal signal
    pub arousal: f64,
    /// Weight for reward signal
    pub reward: f64,
    /// Weight for attention signal
    pub attention: f64,
}

impl Default for CompositeWeights {
    fn default() -> Self {
        Self {
            novelty: DEFAULT_NOVELTY_WEIGHT,
            arousal: DEFAULT_AROUSAL_WEIGHT,
            reward: DEFAULT_REWARD_WEIGHT,
            attention: DEFAULT_ATTENTION_WEIGHT,
        }
    }
}

impl CompositeWeights {
    /// Create with custom weights (will be normalized)
    pub fn new(novelty: f64, arousal: f64, reward: f64, attention: f64) -> Self {
        let total = novelty + arousal + reward + attention;
        if total == 0.0 {
            return Self::default();
        }

        Self {
            novelty: novelty / total,
            arousal: arousal / total,
            reward: reward / total,
            attention: attention / total,
        }
    }

    /// Validate that weights sum to approximately 1.0
    pub fn is_valid(&self) -> bool {
        let sum = self.novelty + self.arousal + self.reward + self.attention;
        (sum - 1.0).abs() < 0.01
    }

    /// Normalize weights to sum to 1.0
    pub fn normalize(&mut self) {
        let total = self.novelty + self.arousal + self.reward + self.attention;
        if total > 0.0 {
            self.novelty /= total;
            self.arousal /= total;
            self.reward /= total;
            self.attention /= total;
        }
    }
}

/// Composite importance score with full breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceScore {
    /// Final composite importance score (0.0 to 1.0)
    pub composite: f64,
    /// Novelty component score
    pub novelty: f64,
    /// Arousal component score
    pub arousal: f64,
    /// Reward component score
    pub reward: f64,
    /// Attention component score
    pub attention: f64,
    /// How much to boost encoding strength (typically 0.7 to 1.3)
    pub encoding_boost: f64,
    /// Priority for memory consolidation
    pub consolidation_priority: ConsolidationPriority,
    /// Weights used in calculation
    pub weights_used: CompositeWeights,
    /// Detailed novelty explanation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub novelty_explanation: Option<NoveltyExplanation>,
    /// Detailed arousal explanation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arousal_explanation: Option<ArousalExplanation>,
    /// Detailed reward explanation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_explanation: Option<RewardExplanation>,
    /// Detailed attention explanation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_explanation: Option<AttentionExplanation>,
    /// When this score was computed
    pub computed_at: DateTime<Utc>,
}

impl ImportanceScore {
    /// Get human-readable summary of the score
    pub fn summary(&self) -> String {
        format!(
            "Importance: {:.2} (N:{:.0}% A:{:.0}% R:{:.0}% At:{:.0}%) - {} priority",
            self.composite,
            self.novelty * 100.0,
            self.arousal * 100.0,
            self.reward * 100.0,
            self.attention * 100.0,
            match self.consolidation_priority {
                ConsolidationPriority::Critical => "CRITICAL",
                ConsolidationPriority::High => "High",
                ConsolidationPriority::Normal => "Normal",
                ConsolidationPriority::Low => "Low",
            }
        )
    }

    /// Get explanation for why this content is important
    pub fn explain(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref novelty) = self.novelty_explanation {
            parts.push(format!("Novelty: {}", novelty.explain()));
        }

        if let Some(ref arousal) = self.arousal_explanation {
            parts.push(format!("Arousal: {}", arousal.explain()));
        }

        if let Some(ref reward) = self.reward_explanation {
            parts.push(format!("Reward: {}", reward.explain()));
        }

        if let Some(ref attention) = self.attention_explanation {
            parts.push(format!("Attention: {}", attention.explain()));
        }

        parts.join("\n")
    }

    /// Get the dominant signal (highest contributor)
    pub fn dominant_signal(&self) -> &'static str {
        let weighted = [
            (self.novelty * self.weights_used.novelty, "Novelty"),
            (self.arousal * self.weights_used.arousal, "Arousal"),
            (self.reward * self.weights_used.reward, "Reward"),
            (self.attention * self.weights_used.attention, "Attention"),
        ];

        weighted
            .iter()
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|x| x.1)
            .unwrap_or("Unknown")
    }
}

/// Priority levels for memory consolidation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConsolidationPriority {
    /// Low priority - process last, may be pruned
    Low,
    /// Normal priority - standard processing
    Normal,
    /// High priority - process early, preserve longer
    High,
    /// Critical priority - process immediately, never prune
    Critical,
}

impl ConsolidationPriority {
    /// Get decay rate modifier (lower = slower decay)
    pub fn decay_modifier(&self) -> f64 {
        match self {
            ConsolidationPriority::Critical => 0.5, // 50% slower decay
            ConsolidationPriority::High => 0.75,    // 25% slower decay
            ConsolidationPriority::Normal => 1.0,   // Normal decay
            ConsolidationPriority::Low => 1.25,     // 25% faster decay
        }
    }

    /// Get retrieval boost
    pub fn retrieval_boost(&self) -> f64 {
        match self {
            ConsolidationPriority::Critical => 1.3,
            ConsolidationPriority::High => 1.15,
            ConsolidationPriority::Normal => 1.0,
            ConsolidationPriority::Low => 0.9,
        }
    }
}

// ============================================================================
// INTEGRATION HELPERS
// ============================================================================

/// Configuration for importance-aware encoding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceEncodingConfig {
    /// Minimum importance for enhanced encoding
    pub enhanced_encoding_threshold: f64,
    /// Maximum encoding boost factor
    pub max_encoding_boost: f64,
    /// Whether to use importance for initial stability
    pub importance_affects_stability: bool,
    /// Base stability modifier per importance point
    pub stability_modifier_per_importance: f64,
}

impl Default for ImportanceEncodingConfig {
    fn default() -> Self {
        Self {
            enhanced_encoding_threshold: 0.6,
            max_encoding_boost: 1.5,
            importance_affects_stability: true,
            stability_modifier_per_importance: 0.5,
        }
    }
}

/// Configuration for importance-aware consolidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceConsolidationConfig {
    /// Process high-importance memories first
    pub prioritize_high_importance: bool,
    /// Minimum importance to avoid pruning
    pub pruning_protection_threshold: f64,
    /// Boost replay frequency for high-importance memories
    pub replay_importance_scaling: bool,
}

impl Default for ImportanceConsolidationConfig {
    fn default() -> Self {
        Self {
            prioritize_high_importance: true,
            pruning_protection_threshold: 0.7,
            replay_importance_scaling: true,
        }
    }
}

/// Configuration for importance-aware retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceRetrievalConfig {
    /// Weight of importance in ranking (0.0 to 1.0)
    pub importance_ranking_weight: f64,
    /// Boost retrieval score based on consolidation priority
    pub apply_priority_boost: bool,
    /// Include importance breakdown in results
    pub include_importance_breakdown: bool,
}

impl Default for ImportanceRetrievalConfig {
    fn default() -> Self {
        Self {
            importance_ranking_weight: 0.2,
            apply_priority_boost: true,
            include_importance_breakdown: true,
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

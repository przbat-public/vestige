//! # Multi-channel Importance Signaling System
//!
//! Inspired by how neuromodulators in the brain signal different types of importance:
//!
//! - **Dopamine (Novelty & Reward)**: Signals prediction errors and positive outcomes
//! - **Norepinephrine (Arousal)**: Signals emotional intensity and urgency
//! - **Acetylcholine (Attention)**: Signals focus states and active learning
//! - **Serotonin**: Modulates overall system responsiveness
//!
//! ## The Four Importance Channels
//!
//! 1. **NoveltySignal**: Detects prediction errors - when something doesn't match expectations
//! 2. **ArousalSignal**: Detects emotional intensity through sentiment and keywords
//! 3. **RewardSignal**: Tracks which memories lead to positive outcomes
//! 4. **AttentionSignal**: Detects when the user is actively focused/learning
//!
//! ## Why This Matters
//!
//! Different types of content deserve different treatment:
//! - Novel information needs stronger initial encoding
//! - Emotional content naturally sticks better (flashbulb memories)
//! - Rewarding patterns should be reinforced
//! - Focused learning sessions create stronger memories
//!
//! ## Module Layout (split from a 2.4K-line monolith for navigation)
//!
//! - `novelty`   — `NoveltySignal`, `NoveltyExplanation`, internal `PredictionModel`
//! - `arousal`   — `ArousalSignal`, `EmotionalMarker`, `MarkerType`, `ArousalExplanation`
//! - `sentiment` — `SentimentAnalyzer`, `SentimentResult` (used by arousal)
//! - `reward`    — `RewardSignal`, `Outcome`, `OutcomeType`, `RewardExplanation`
//! - `attention` — `AttentionSignal`, `AccessPattern`, `Session`, `AttentionExplanation`
//! - `composite` — `ImportanceSignals` orchestrator + scoring/config types
//!
//! Public API is preserved through `pub use` re-exports.
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! let signals = ImportanceSignals::new();
//!
//! // Analyze content for importance
//! let content = "CRITICAL: Production database migration failed with data loss!";
//! let context = Context::current();
//!
//! let score = signals.compute_importance(content, &context);
//!
//! // Get transparent breakdown
//! println!("Novelty:   {:.2} - {}", score.novelty, score.explain_novelty());
//! println!("Arousal:   {:.2} - {}", score.arousal, score.explain_arousal());
//! println!("Reward:    {:.2} - {}", score.reward, score.explain_reward());
//! println!("Attention: {:.2} - {}", score.attention, score.explain_attention());
//! println!("Composite: {:.2}", score.composite);
//!
//! // Use score for encoding decisions
//! if score.encoding_boost > 1.0 {
//!     println!("Boosting encoding strength by {:.0}%", (score.encoding_boost - 1.0) * 100.0);
//! }
//! ```
//!
//! ## Biological Inspiration
//!
//! In the brain, neuromodulator systems work together:
//!
//! | System | Neuromodulator | Memory Effect |
//! |--------|---------------|---------------|
//! | Novelty | Dopamine (VTA/SNc) | Enhances hippocampal plasticity |
//! | Arousal | Norepinephrine (LC) | Strengthens amygdala-mediated encoding |
//! | Reward | Dopamine (NAcc) | Reinforces successful patterns |
//! | Attention | Acetylcholine (BF) | Gates learning in cortical circuits |
//!
//! This system translates these biological mechanisms into computational signals
//! that determine memory encoding strength, consolidation priority, and retrieval ranking.

use chrono::{DateTime, Utc};

// ============================================================================
// CONFIGURATION CONSTANTS
// ============================================================================

/// Default weight for novelty signal in composite score
pub(crate) const DEFAULT_NOVELTY_WEIGHT: f64 = 0.25;

/// Default weight for arousal signal in composite score
pub(crate) const DEFAULT_AROUSAL_WEIGHT: f64 = 0.30;

/// Default weight for reward signal in composite score
pub(crate) const DEFAULT_REWARD_WEIGHT: f64 = 0.25;

/// Default weight for attention signal in composite score
pub(crate) const DEFAULT_ATTENTION_WEIGHT: f64 = 0.20;

/// Minimum importance score (never drops to zero)
pub(crate) const MIN_IMPORTANCE: f64 = 0.05;

/// Maximum importance score
pub(crate) const MAX_IMPORTANCE: f64 = 1.0;

/// Default novelty threshold for prediction model
pub(crate) const DEFAULT_NOVELTY_THRESHOLD: f64 = 0.3;

/// Maximum patterns to track in prediction model
pub(crate) const MAX_PREDICTION_PATTERNS: usize = 10_000;

/// Decay rate for pattern frequencies
pub(crate) const PATTERN_DECAY_RATE: f64 = 0.99;

/// Maximum outcome history entries
pub(crate) const MAX_OUTCOME_HISTORY: usize = 5_000;

/// Session inactivity timeout for learning mode detection (minutes)
pub(crate) const LEARNING_MODE_TIMEOUT_MINUTES: i64 = 30;

// ============================================================================
// CONTEXT
// ============================================================================

/// Context for importance computation
///
/// Provides environmental information that affects importance scoring.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Current session ID
    pub session_id: Option<String>,
    /// Current project or domain
    pub project: Option<String>,
    /// Recent queries made
    pub recent_queries: Vec<String>,
    /// Current time (for temporal patterns)
    pub timestamp: Option<DateTime<Utc>>,
    /// Whether user is in an active learning session
    pub learning_session_active: bool,
    /// Current emotional context (e.g., "stressed", "focused", "casual")
    pub emotional_context: Option<String>,
    /// Tags relevant to current context
    pub context_tags: Vec<String>,
    /// Recent memory IDs accessed
    pub recent_memory_ids: Vec<String>,
}

impl Context {
    /// Create a new context with current timestamp
    pub fn current() -> Self {
        Self {
            timestamp: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the project context
    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    /// Add a recent query
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.recent_queries.push(query.into());
        self
    }

    /// Set learning session status
    pub fn with_learning_session(mut self, active: bool) -> Self {
        self.learning_session_active = active;
        self
    }

    /// Set emotional context
    pub fn with_emotional_context(mut self, context: impl Into<String>) -> Self {
        self.emotional_context = Some(context.into());
        self
    }

    /// Add context tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.context_tags = tags;
        self
    }
}

// ============================================================================
// SUBMODULES
// ============================================================================

mod arousal;
mod attention;
mod composite;
mod novelty;
mod reward;
mod sentiment;

pub use arousal::{ArousalExplanation, ArousalSignal, EmotionalMarker, MarkerType};
pub use attention::{AccessPattern, AttentionExplanation, AttentionSignal, Session};
pub use composite::{
    CompositeWeights, ConsolidationPriority, ImportanceConsolidationConfig,
    ImportanceEncodingConfig, ImportanceRetrievalConfig, ImportanceScore, ImportanceSignals,
};
pub use novelty::{NoveltyExplanation, NoveltySignal};
pub use reward::{Outcome, OutcomeType, RewardExplanation, RewardSignal};
pub use sentiment::{SentimentAnalyzer, SentimentResult};

#[cfg(test)]
mod tests;

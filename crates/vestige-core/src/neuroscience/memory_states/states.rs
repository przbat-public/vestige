//! Core state vocabulary: the [`MemoryState`] enum, transition reasons,
//! transition records, and competition events.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::{
    ACCESSIBILITY_ACTIVE, ACCESSIBILITY_DORMANT, ACCESSIBILITY_SILENT, ACCESSIBILITY_UNAVAILABLE,
};

// ============================================================================
// MEMORY STATE ENUM
// ============================================================================

/// The accessibility state of a memory.
///
/// Memories transition between these states based on:
/// - Time since last access
/// - Strength of retrieval cues
/// - Competition with similar memories
///
/// # State Accessibility
///
/// | State       | Multiplier | Description                          |
/// |-------------|------------|--------------------------------------|
/// | Active      | 1.0        | Currently being processed            |
/// | Dormant     | 0.7        | Easily retrievable with partial cues |
/// | Silent      | 0.3        | Requires strong/specific cues        |
/// | Unavailable | 0.05       | Temporarily blocked                  |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MemoryState {
    /// Currently being processed, high accessibility.
    ///
    /// This is the state immediately after a memory is created or accessed.
    /// The memory is in "working memory" and immediately available.
    #[default]
    Active,

    /// Easily retrievable with partial cues, moderate accessibility.
    ///
    /// Like remembering a friend's name when you see their face.
    /// The memory is well-consolidated and doesn't require much effort to retrieve.
    Dormant,

    /// Exists but requires strong/specific cues to retrieve.
    ///
    /// Like childhood memories that only surface with specific triggers.
    /// The memory exists but needs substantial cue overlap to be activated.
    Silent,

    /// Temporarily inaccessible due to interference or suppression.
    ///
    /// The memory is blocked, often because:
    /// - A similar memory "won" a retrieval competition
    /// - The user actively suppressed the memory
    /// - Too many similar memories are competing
    ///
    /// This state is reversible - the memory can become accessible again
    /// once interference is resolved or suppression expires.
    Unavailable,
}

impl MemoryState {
    /// Get the base accessibility multiplier for this state.
    ///
    /// This multiplier should be factored into retrieval ranking:
    /// `effective_score = raw_score * accessibility_multiplier`
    ///
    /// # Returns
    ///
    /// A value between 0.0 and 1.0 representing the state's base accessibility.
    #[inline]
    pub fn accessibility_multiplier(&self) -> f64 {
        match self {
            MemoryState::Active => ACCESSIBILITY_ACTIVE,
            MemoryState::Dormant => ACCESSIBILITY_DORMANT,
            MemoryState::Silent => ACCESSIBILITY_SILENT,
            MemoryState::Unavailable => ACCESSIBILITY_UNAVAILABLE,
        }
    }

    /// Check if this state allows normal retrieval.
    ///
    /// Active and Dormant memories can be retrieved with normal cues.
    /// Silent memories require stronger cues (higher similarity threshold).
    /// Unavailable memories are blocked until suppression expires.
    #[inline]
    pub fn is_retrievable(&self) -> bool {
        matches!(self, MemoryState::Active | MemoryState::Dormant)
    }

    /// Check if this state requires strong cues for retrieval.
    #[inline]
    pub fn requires_strong_cue(&self) -> bool {
        matches!(self, MemoryState::Silent)
    }

    /// Check if this state blocks retrieval.
    #[inline]
    pub fn is_blocked(&self) -> bool {
        matches!(self, MemoryState::Unavailable)
    }

    /// Get a human-readable description of the state.
    pub fn description(&self) -> &'static str {
        match self {
            MemoryState::Active => "Currently in working memory, immediately accessible",
            MemoryState::Dormant => "Well-consolidated, easily retrievable with partial cues",
            MemoryState::Silent => "Exists but requires strong or specific cues to surface",
            MemoryState::Unavailable => "Temporarily blocked due to interference or suppression",
        }
    }

    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryState::Active => "active",
            MemoryState::Dormant => "dormant",
            MemoryState::Silent => "silent",
            MemoryState::Unavailable => "unavailable",
        }
    }

    /// Parse from string name.
    pub fn parse_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "active" => MemoryState::Active,
            "dormant" => MemoryState::Dormant,
            "silent" => MemoryState::Silent,
            "unavailable" => MemoryState::Unavailable,
            _ => MemoryState::Dormant, // Safe default
        }
    }
}

impl std::fmt::Display for MemoryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// STATE TRANSITION REASON
// ============================================================================

/// The reason for a state transition.
///
/// Tracking reasons provides transparency about why memories
/// change accessibility over time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateTransitionReason {
    /// Memory was just created or accessed
    Access,
    /// Time-based decay (natural forgetting)
    TimeDecay,
    /// Strong cue reactivated a Silent memory
    CueReactivation {
        /// Similarity score of the cue that triggered reactivation
        cue_similarity: f64,
    },
    /// Lost a retrieval competition to another memory
    CompetitionLoss {
        /// ID of the winning memory
        winner_id: String,
        /// How similar the memories were
        similarity: f64,
    },
    /// Competition resolved, interference no longer blocking
    InterferenceResolved,
    /// User explicitly suppressed the memory
    UserSuppression {
        /// Optional reason provided by user
        reason: Option<String>,
    },
    /// Suppression period expired
    SuppressionExpired,
    /// Manual state override (e.g., admin action)
    ManualOverride {
        /// Who made the change
        actor: Option<String>,
    },
    /// System initialization or migration
    SystemInit,
}

impl StateTransitionReason {
    /// Get a human-readable description of the reason.
    pub fn description(&self) -> String {
        match self {
            StateTransitionReason::Access => "Memory was accessed or created".to_string(),
            StateTransitionReason::TimeDecay => "Natural decay over time".to_string(),
            StateTransitionReason::CueReactivation { cue_similarity } => {
                format!(
                    "Reactivated by strong cue (similarity: {:.2})",
                    cue_similarity
                )
            }
            StateTransitionReason::CompetitionLoss {
                winner_id,
                similarity,
            } => {
                format!(
                    "Lost retrieval competition to {} (similarity: {:.2})",
                    winner_id, similarity
                )
            }
            StateTransitionReason::InterferenceResolved => {
                "Interference from competing memories resolved".to_string()
            }
            StateTransitionReason::UserSuppression { reason } => match reason {
                Some(r) => format!("User suppressed: {}", r),
                None => "User suppressed memory".to_string(),
            },
            StateTransitionReason::SuppressionExpired => "Suppression period expired".to_string(),
            StateTransitionReason::ManualOverride { actor } => match actor {
                Some(a) => format!("Manual override by {}", a),
                None => "Manual state override".to_string(),
            },
            StateTransitionReason::SystemInit => "System initialization".to_string(),
        }
    }
}

// ============================================================================
// STATE TRANSITION
// ============================================================================

/// A recorded state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateTransition {
    /// The previous state
    pub from_state: MemoryState,
    /// The new state
    pub to_state: MemoryState,
    /// When the transition occurred
    pub timestamp: DateTime<Utc>,
    /// Why the transition happened
    pub reason: StateTransitionReason,
}

impl StateTransition {
    /// Create a new state transition record.
    pub fn new(
        from_state: MemoryState,
        to_state: MemoryState,
        reason: StateTransitionReason,
    ) -> Self {
        Self {
            from_state,
            to_state,
            timestamp: Utc::now(),
            reason,
        }
    }
}

// ============================================================================
// COMPETITION EVENT
// ============================================================================

/// Records a retrieval competition event.
///
/// When similar memories compete during retrieval, we track:
/// - Which memories competed
/// - Who won
/// - The suppression applied to losers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionEvent {
    /// ID of the query/cue that triggered competition
    pub query_id: Option<String>,
    /// The query/cue text (for debugging)
    pub query_text: Option<String>,
    /// ID of the winning memory
    pub winner_id: String,
    /// IDs of memories that lost (and were suppressed)
    pub loser_ids: Vec<String>,
    /// Similarity scores between winner and each loser
    pub loser_similarities: Vec<f64>,
    /// When the competition occurred
    pub timestamp: DateTime<Utc>,
    /// How long suppression lasts for losers
    pub suppression_duration: Duration,
}

impl CompetitionEvent {
    /// Create a new competition event.
    pub fn new(
        winner_id: String,
        loser_ids: Vec<String>,
        loser_similarities: Vec<f64>,
        suppression_duration: Duration,
    ) -> Self {
        Self {
            query_id: None,
            query_text: None,
            winner_id,
            loser_ids,
            loser_similarities,
            timestamp: Utc::now(),
            suppression_duration,
        }
    }

    /// Add query information to the event.
    pub fn with_query(mut self, query_id: Option<String>, query_text: Option<String>) -> Self {
        self.query_id = query_id;
        self.query_text = query_text;
        self
    }
}

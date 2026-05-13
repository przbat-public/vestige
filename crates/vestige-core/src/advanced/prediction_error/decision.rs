//! Decision payloads emitted by the prediction-error gate.

use serde::{Deserialize, Serialize};

/// Decision made by the prediction error gate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateDecision {
    /// Create a new memory (high prediction error)
    Create {
        /// Reason for creating new
        reason: CreateReason,
        /// Prediction error score (0.0 = identical, 1.0 = completely different)
        prediction_error: f32,
        /// Related memories that were considered
        related_memory_ids: Vec<String>,
    },

    /// Update an existing memory (low prediction error)
    Update {
        /// ID of memory to update
        target_id: String,
        /// How similar the content is (0.0 - 1.0)
        similarity: f32,
        /// Type of update to perform
        update_type: UpdateType,
        /// Prediction error score
        prediction_error: f32,
    },

    /// Supersede an existing memory (correction/improvement)
    Supersede {
        /// ID of memory being superseded
        old_memory_id: String,
        /// Similarity to old memory
        similarity: f32,
        /// Why this supersedes the old one
        supersede_reason: SupersedeReason,
        /// Prediction error score
        prediction_error: f32,
    },

    /// Merge with multiple existing memories
    Merge {
        /// IDs of memories to merge with
        memory_ids: Vec<String>,
        /// Average similarity
        avg_similarity: f32,
        /// Merge strategy
        strategy: MergeStrategy,
    },
}

impl GateDecision {
    /// Get the prediction error score
    pub fn prediction_error(&self) -> f32 {
        match self {
            Self::Create {
                prediction_error, ..
            } => *prediction_error,
            Self::Update {
                prediction_error, ..
            } => *prediction_error,
            Self::Supersede {
                prediction_error, ..
            } => *prediction_error,
            Self::Merge { avg_similarity, .. } => 1.0 - avg_similarity,
        }
    }

    /// Check if this is a create decision
    pub fn is_create(&self) -> bool {
        matches!(self, Self::Create { .. })
    }

    /// Check if this is an update decision
    pub fn is_update(&self) -> bool {
        matches!(self, Self::Update { .. })
    }

    /// Get target ID if updating or superseding
    pub fn target_id(&self) -> Option<&str> {
        match self {
            Self::Update { target_id, .. } => Some(target_id),
            Self::Supersede { old_memory_id, .. } => Some(old_memory_id),
            _ => None,
        }
    }
}

/// Reasons for creating a new memory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CreateReason {
    /// No similar memories exist
    NoSimilarMemories,
    /// Content is substantially different from all candidates
    HighPredictionError,
    /// Different domain/topic despite surface similarity
    DifferentDomain,
    /// Explicitly requested new memory (not update)
    ExplicitCreate,
    /// First memory in the system
    FirstMemory,
}

/// Types of updates to existing memories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateType {
    /// Append new information
    Append,
    /// Replace content entirely
    Replace,
    /// Merge content intelligently
    Merge,
    /// Add as related context
    AddContext,
    /// Strengthen existing memory (same content, reinforcement)
    Reinforce,
}

/// Reasons for superseding an existing memory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SupersedeReason {
    /// New content is a correction of old
    Correction,
    /// New content is an improvement/update
    Improvement,
    /// Old content is marked as outdated
    Outdated,
    /// User explicitly indicated this is better
    UserIndicated,
    /// New content has higher confidence/authority
    HigherConfidence,
}

/// Strategies for merging multiple memories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Combine all content
    Combine,
    /// Keep most recent, link to older
    KeepRecent,
    /// Create summary of all
    Summarize,
    /// Create hierarchy (parent with children)
    Hierarchical,
}

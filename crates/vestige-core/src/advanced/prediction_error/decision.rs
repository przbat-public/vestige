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

    /// The new content may deny something an existing memory records, and the
    /// evidence is strong enough to say so — and not strong enough to act on it.
    ///
    /// The memory is written and linked to the one it may contradict; the older
    /// memory keeps its text and its standing, because retiring it means writing
    /// `valid_until`, a claim about the past that every later reader inherits,
    /// and a lexical heuristic cannot support that claim. Measured on a real
    /// thirteen-memory store, the strictest automatic rule still marked 39 of
    /// 156 ordered pairs as corrections: two memories about one project negate
    /// different things with the same words ("pseudocode does not compile"
    /// against "each teaching stage compiles cleanly"). Retirement is therefore
    /// explicit — `temporal(action="invalidate")`, or a write that passes
    /// `supersedes`.
    Contradiction {
        /// ID of the memory that may be contradicted.
        existing_id: String,
        /// Similarity to that memory (0.0 - 1.0).
        similarity: f32,
        /// Confidence of the contradiction evidence (0.0 - 1.0).
        confidence: f32,
        /// What fired the detector, for the caller to judge.
        evidence: Vec<GateFinding>,
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

    /// Refuse the candidate: nothing is written.
    ///
    /// Deliberately narrow. It exists for content the repository already owns —
    /// a code block, a directory tree, copied source, a coverage figure, a
    /// version number — because storing a copy buys a second source of truth
    /// that rots with the next commit. Everything *fixable* (a dangling
    /// reference, a relative date, a bare path) is written and flagged instead:
    /// trading junk for silent loss would be the worse failure, and a memory
    /// nobody can see is a silent loss.
    Reject {
        /// Why this content must not be stored, worded for the caller to show.
        reason: String,
        /// The findings behind the refusal, if the detector reported any. A
        /// content-level refusal names no single span, so `reason` carries it
        /// and this may be empty.
        findings: Vec<GateFinding>,
    },
}

/// One reason a candidate was refused before it reached storage.
///
/// The same shape the write-time gate reports, repeated here so a decision can
/// be serialized and audited without borrowing the detector's tables: `kind`
/// names the rule, `span` is the text that fired it, `hint` says what to write
/// instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateFinding {
    /// Stable rule identifier (e.g. `derivable_from_repo`).
    pub kind: String,
    /// The exact text that fired the rule.
    pub span: String,
    /// What to write instead.
    pub hint: String,
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
            // A flagged contradiction says "these two look like they disagree",
            // which is a near-miss: the closer the content, the smaller the
            // prediction error, exactly as for an update.
            Self::Contradiction { similarity, .. } => 1.0 - similarity,
            Self::Merge { avg_similarity, .. } => 1.0 - avg_similarity,
            // A rejected candidate is never compared with anything — the reject
            // is decided from the content alone, before the similarity probe. A
            // number here would be read as a measurement, so this is NaN, which
            // is "no value" and stays loud if a caller ever aggregates it.
            Self::Reject { .. } => f32::NAN,
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

    /// Check if this decision refuses to write anything.
    pub fn is_reject(&self) -> bool {
        matches!(self, Self::Reject { .. })
    }

    /// Check if this decision flags a possible contradiction without acting on it.
    pub fn is_contradiction(&self) -> bool {
        matches!(self, Self::Contradiction { .. })
    }

    /// Get target ID if updating or superseding
    pub fn target_id(&self) -> Option<&str> {
        match self {
            Self::Update { target_id, .. } => Some(target_id),
            Self::Supersede { old_memory_id, .. } => Some(old_memory_id),
            Self::Contradiction { existing_id, .. } => Some(existing_id),
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

//! [`EncodingContext`] aggregates the four context dimensions captured at
//! the moment a memory is stored.

use serde::{Deserialize, Serialize};

use super::emotional::EmotionalContext;
use super::session::SessionContext;
use super::temporal::TemporalContext;
use super::topical::TopicalContext;

// ============================================================================
// ENCODING CONTEXT (COMBINED)
// ============================================================================

/// Complete encoding context capturing all dimensions
///
/// This is the full context snapshot taken when a memory is encoded.
/// It combines temporal, topical, session, and emotional contexts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodingContext {
    /// When the memory was encoded
    pub temporal: TemporalContext,
    /// What topics were active
    pub topical: TopicalContext,
    /// What session/activity was occurring
    pub session: SessionContext,
    /// Emotional state during encoding
    pub emotional: EmotionalContext,
}

impl EncodingContext {
    /// Create a new encoding context with current temporal context
    pub fn new() -> Self {
        Self {
            temporal: TemporalContext::now(),
            topical: TopicalContext::default(),
            session: SessionContext::default(),
            emotional: EmotionalContext::neutral(),
        }
    }

    /// Capture the current context (minimal version)
    pub fn capture_current() -> Self {
        Self::new()
    }

    /// Create with all components
    pub fn with_all(
        temporal: TemporalContext,
        topical: TopicalContext,
        session: SessionContext,
        emotional: EmotionalContext,
    ) -> Self {
        Self {
            temporal,
            topical,
            session,
            emotional,
        }
    }

    /// Builder: set temporal context
    pub fn with_temporal(mut self, temporal: TemporalContext) -> Self {
        self.temporal = temporal;
        self
    }

    /// Builder: set topical context
    pub fn with_topical(mut self, topical: TopicalContext) -> Self {
        self.topical = topical;
        self
    }

    /// Builder: set session context
    pub fn with_session(mut self, session: SessionContext) -> Self {
        self.session = session;
        self
    }

    /// Builder: set emotional context
    pub fn with_emotional(mut self, emotional: EmotionalContext) -> Self {
        self.emotional = emotional;
        self
    }

    /// Add a topic to the topical context
    pub fn add_topic(&mut self, topic: impl Into<String>) {
        self.topical.add_topic(topic);
    }

    /// Set the project in session context
    pub fn set_project(&mut self, project: impl Into<String>) {
        self.session.set_project(project);
    }

    /// Refresh dynamic fields (like recency)
    pub fn refresh(&mut self) {
        self.temporal.refresh_recency();
    }
}

impl Default for EncodingContext {
    fn default() -> Self {
        Self::new()
    }
}

//! Retrieval-time helpers: weighting strategies, the [`ContextMatcher`]
//! itself, and the score-bearing wrapper used to boost candidates.

use serde::{Deserialize, Serialize};

use super::emotional::EmotionalContext;
use super::encoding::EncodingContext;
use super::session::SessionContext;
use super::temporal::TemporalContext;
use super::topical::TopicalContext;

// ============================================================================
// CONTEXT WEIGHTS
// ============================================================================

/// Weights for different context dimensions in matching
///
/// These can be tuned based on the application domain or user preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextWeights {
    /// Weight for temporal context match (0.0 to 1.0)
    pub temporal: f64,
    /// Weight for topical context match (0.0 to 1.0)
    pub topical: f64,
    /// Weight for session context match (0.0 to 1.0)
    pub session: f64,
    /// Weight for emotional context match (0.0 to 1.0)
    pub emotional: f64,
}

impl Default for ContextWeights {
    fn default() -> Self {
        Self {
            temporal: 0.2,   // Moderate weight for time
            topical: 0.4,    // Highest weight for topic match
            session: 0.25,   // Good weight for same session/project
            emotional: 0.15, // Lower weight for emotional match
        }
    }
}

impl ContextWeights {
    /// Create weights emphasizing topical match
    pub fn topic_focused() -> Self {
        Self {
            temporal: 0.1,
            topical: 0.6,
            session: 0.2,
            emotional: 0.1,
        }
    }

    /// Create weights emphasizing temporal match
    pub fn recency_focused() -> Self {
        Self {
            temporal: 0.4,
            topical: 0.3,
            session: 0.2,
            emotional: 0.1,
        }
    }

    /// Create weights emphasizing session match
    pub fn session_focused() -> Self {
        Self {
            temporal: 0.15,
            topical: 0.3,
            session: 0.45,
            emotional: 0.1,
        }
    }

    /// Normalize weights to sum to 1.0
    pub fn normalize(&mut self) {
        let sum = self.temporal + self.topical + self.session + self.emotional;
        if sum > 0.0 {
            self.temporal /= sum;
            self.topical /= sum;
            self.session /= sum;
            self.emotional /= sum;
        }
    }
}

// ============================================================================
// CONTEXT REINSTATEMENT
// ============================================================================

/// Hints for context reinstatement during retrieval
///
/// When a memory is retrieved, these hints help the user remember
/// the original context ("You were discussing X when this came up").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextReinstatement {
    /// Memory ID this reinstatement is for
    pub memory_id: String,
    /// Temporal hint ("This was from last Tuesday morning")
    pub temporal_hint: Option<String>,
    /// Topical hint ("You were discussing authentication")
    pub topical_hint: Option<String>,
    /// Session hint ("This was during your work on the API refactor")
    pub session_hint: Option<String>,
    /// Related memories from the same context
    pub related_memories: Vec<String>,
}

impl ContextReinstatement {
    /// Create an empty reinstatement
    pub fn new(memory_id: impl Into<String>) -> Self {
        Self {
            memory_id: memory_id.into(),
            temporal_hint: None,
            topical_hint: None,
            session_hint: None,
            related_memories: vec![],
        }
    }

    /// Generate reinstatement hints from an encoding context
    pub fn from_context(memory_id: impl Into<String>, context: &EncodingContext) -> Self {
        let mut reinstatement = Self::new(memory_id);

        // Generate temporal hint
        let recency = context.temporal.recency_bucket.as_str();
        let time_of_day = context.temporal.time_of_day.as_str();
        let day = format!("{:?}", context.temporal.day_of_week);
        reinstatement.temporal_hint = Some(format!(
            "This memory is from {} ({} on {})",
            recency, time_of_day, day
        ));

        // Generate topical hint
        if !context.topical.active_topics.is_empty() {
            let topics = context.topical.active_topics.join(", ");
            reinstatement.topical_hint = Some(format!("You were discussing: {}", topics));
        }

        // Generate session hint
        if let Some(ref project) = context.session.project {
            reinstatement.session_hint = Some(format!("This was during work on '{}'", project));
        } else if let Some(ref activity) = context.session.activity_type {
            reinstatement.session_hint = Some(format!("This was during {}", activity));
        }

        reinstatement
    }

    /// Check if any hints are available
    pub fn has_hints(&self) -> bool {
        self.temporal_hint.is_some() || self.topical_hint.is_some() || self.session_hint.is_some()
    }

    /// Get a combined hint string
    pub fn combined_hint(&self) -> Option<String> {
        let mut hints = Vec::new();

        if let Some(ref hint) = self.topical_hint {
            hints.push(hint.clone());
        }
        if let Some(ref hint) = self.session_hint {
            hints.push(hint.clone());
        }
        if let Some(ref hint) = self.temporal_hint {
            hints.push(hint.clone());
        }

        if hints.is_empty() {
            None
        } else {
            Some(hints.join(". "))
        }
    }
}

// ============================================================================
// SCORED MEMORY
// ============================================================================

/// A memory with its context match score
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredMemory<T> {
    /// The memory item
    pub memory: T,
    /// Original relevance score (from search)
    pub relevance_score: f64,
    /// Context match score (0.0 to 1.0)
    pub context_score: f64,
    /// Final combined score
    pub combined_score: f64,
    /// Context reinstatement hints
    pub reinstatement: Option<ContextReinstatement>,
}

impl<T> ScoredMemory<T> {
    /// Create a new scored memory
    pub fn new(memory: T, relevance_score: f64, context_score: f64) -> Self {
        let combined_score = Self::compute_combined(relevance_score, context_score);
        Self {
            memory,
            relevance_score,
            context_score,
            combined_score,
            reinstatement: None,
        }
    }

    /// Compute combined score (can be customized)
    fn compute_combined(relevance: f64, context: f64) -> f64 {
        // Context provides up to 30% boost to relevance
        relevance * (1.0 + 0.3 * context)
    }

    /// Add reinstatement hints
    pub fn with_reinstatement(mut self, reinstatement: ContextReinstatement) -> Self {
        self.reinstatement = Some(reinstatement);
        self
    }
}

// ============================================================================
// CONTEXT MATCHER
// ============================================================================

/// Matches encoding and retrieval contexts to compute similarity
///
/// This is the core component that implements the Encoding Specificity Principle.
#[derive(Debug, Clone)]
pub struct ContextMatcher {
    /// Weights for different context dimensions
    pub weights: ContextWeights,
}

impl Default for ContextMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextMatcher {
    /// Create a new context matcher with default weights
    pub fn new() -> Self {
        Self {
            weights: ContextWeights::default(),
        }
    }

    /// Create with custom weights
    pub fn with_weights(weights: ContextWeights) -> Self {
        Self { weights }
    }

    /// Compute similarity between encoding and retrieval contexts
    ///
    /// Returns a score from 0.0 (no match) to 1.0 (perfect match).
    pub fn match_contexts(&self, encoding: &EncodingContext, retrieval: &EncodingContext) -> f64 {
        let temporal_match = self.match_temporal(&encoding.temporal, &retrieval.temporal);
        let topical_match = self.match_topical(&encoding.topical, &retrieval.topical);
        let session_match = self.match_session(&encoding.session, &retrieval.session);
        let emotional_match = self.match_emotional(&encoding.emotional, &retrieval.emotional);

        // Weighted combination
        temporal_match * self.weights.temporal
            + topical_match * self.weights.topical
            + session_match * self.weights.session
            + emotional_match * self.weights.emotional
    }

    /// Match temporal contexts
    fn match_temporal(&self, encoding: &TemporalContext, retrieval: &TemporalContext) -> f64 {
        let mut score = 0.0;

        // Time of day match (0.3 weight)
        if encoding.time_of_day == retrieval.time_of_day {
            score += 0.3;
        } else if encoding.time_of_day.is_adjacent(&retrieval.time_of_day) {
            score += 0.15;
        }

        // Day of week match (0.2 weight)
        if encoding.day_of_week == retrieval.day_of_week {
            score += 0.2;
        } else if encoding.is_weekday() == retrieval.is_weekday() {
            score += 0.1;
        }

        // Recency match (0.5 weight) - most important temporal factor
        if encoding.recency_bucket == retrieval.recency_bucket {
            score += 0.5;
        } else if encoding
            .recency_bucket
            .is_adjacent(&retrieval.recency_bucket)
        {
            score += 0.25;
        }

        score
    }

    /// Match topical contexts
    fn match_topical(&self, encoding: &TopicalContext, retrieval: &TopicalContext) -> f64 {
        let encoding_terms = encoding.all_terms();
        let retrieval_terms = retrieval.all_terms();

        // If both are empty, they're identical (perfect match)
        if encoding_terms.is_empty() && retrieval_terms.is_empty() {
            return 1.0;
        }

        // If only one is empty, no match
        if encoding_terms.is_empty() || retrieval_terms.is_empty() {
            return 0.0;
        }

        // Jaccard similarity
        let intersection = encoding_terms.intersection(&retrieval_terms).count();
        let union = encoding_terms.union(&retrieval_terms).count();

        if union == 0 {
            0.0
        } else {
            (intersection as f64 / union as f64).min(1.0)
        }
    }

    /// Match session contexts
    fn match_session(&self, encoding: &SessionContext, retrieval: &SessionContext) -> f64 {
        let mut score = 0.0;

        // Same session is a very strong match
        if let (Some(e_id), Some(r_id)) = (&encoding.session_id, &retrieval.session_id)
            && e_id == r_id
        {
            return 1.0;
        }

        // Project match (0.4 weight)
        if let (Some(e_proj), Some(r_proj)) = (&encoding.project, &retrieval.project)
            && e_proj == r_proj
        {
            score += 0.4;
        }

        // Activity type match (0.3 weight)
        if let (Some(e_act), Some(r_act)) = (&encoding.activity_type, &retrieval.activity_type)
            && e_act == r_act
        {
            score += 0.3;
        }

        // Git branch match (0.2 weight)
        if let (Some(e_br), Some(r_br)) = (&encoding.git_branch, &retrieval.git_branch)
            && e_br == r_br
        {
            score += 0.2;
        }

        // Active file match (0.1 weight)
        if let (Some(e_file), Some(r_file)) = (&encoding.active_file, &retrieval.active_file)
            && e_file == r_file
        {
            score += 0.1;
        }

        score
    }

    /// Match emotional contexts
    fn match_emotional(&self, encoding: &EmotionalContext, retrieval: &EmotionalContext) -> f64 {
        // Emotional match based on VAD (Valence-Arousal-Dominance) distance
        let valence_diff = (encoding.valence - retrieval.valence).abs();
        let arousal_diff = (encoding.arousal - retrieval.arousal).abs();
        let dominance_diff = (encoding.dominance - retrieval.dominance).abs();

        // Convert distances to similarity (max distance is 2.0 per dimension)
        let valence_sim = 1.0 - (valence_diff / 2.0);
        let arousal_sim = 1.0 - arousal_diff;
        let dominance_sim = 1.0 - dominance_diff;

        // Weighted average (valence is most important for mood-congruence)
        valence_sim * 0.5 + arousal_sim * 0.3 + dominance_sim * 0.2
    }

    /// Boost retrieval results based on context match
    ///
    /// Takes a vector of memories with their encoding contexts and the current
    /// retrieval context, returns memories with boosted scores.
    pub fn boost_retrieval<T, F>(
        &self,
        memories: Vec<T>,
        current_context: &EncodingContext,
        get_context: F,
        get_relevance: impl Fn(&T) -> f64,
    ) -> Vec<ScoredMemory<T>>
    where
        F: Fn(&T) -> Option<&EncodingContext>,
    {
        let mut scored: Vec<ScoredMemory<T>> = memories
            .into_iter()
            .map(|memory| {
                let relevance = get_relevance(&memory);
                let context_score = get_context(&memory)
                    .map(|ctx| self.match_contexts(ctx, current_context))
                    .unwrap_or(0.0);

                ScoredMemory::new(memory, relevance, context_score)
            })
            .collect();

        // Sort by combined score (descending)
        scored.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        scored
    }

    /// Generate context reinstatement hints for a memory
    pub fn reinstate_context(
        &self,
        memory_id: &str,
        context: &EncodingContext,
    ) -> ContextReinstatement {
        ContextReinstatement::from_context(memory_id, context)
    }

    /// Disambiguate same content in different contexts
    ///
    /// When the same content appears in multiple memories with different contexts,
    /// this function helps identify which one is most relevant to the current context.
    pub fn disambiguate<T, F>(
        &self,
        memories: &[T],
        current_context: &EncodingContext,
        get_context: F,
    ) -> Vec<(usize, f64)>
    where
        F: Fn(&T) -> Option<&EncodingContext>,
    {
        let mut scores: Vec<(usize, f64)> = memories
            .iter()
            .enumerate()
            .map(|(idx, memory)| {
                let score = get_context(memory)
                    .map(|ctx| self.match_contexts(ctx, current_context))
                    .unwrap_or(0.0);
                (idx, score)
            })
            .collect();

        // Sort by context match (descending)
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scores
    }
}

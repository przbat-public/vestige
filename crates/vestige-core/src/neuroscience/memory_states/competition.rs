//! Retrieval-induced forgetting: pairwise competition between similar
//! memories, with suppression applied to the losers.

use std::collections::VecDeque;

use chrono::Duration;
use serde::{Deserialize, Serialize};

use super::states::CompetitionEvent;
use super::{COMPETITION_SIMILARITY_THRESHOLD, MAX_COMPETITION_HISTORY_SIZE};

// ============================================================================
// RETRIEVAL COMPETITION MANAGER
// ============================================================================

/// Manages retrieval-induced forgetting (RIF) and memory competition.
///
/// When multiple similar memories compete during retrieval:
/// 1. The winner gets strengthened (moved to Active)
/// 2. The losers get suppressed (moved to Unavailable)
/// 3. This implements the neuroscience concept of retrieval-induced forgetting
///
/// When [`CompetitionCandidate::embedding`] vectors are provided, the manager
/// uses actual pairwise cosine similarity instead of the query-similarity
/// average approximation.
///
/// # Example
///
/// ```rust
/// use vestige_core::neuroscience::{CompetitionManager, CompetitionCandidate};
///
/// let mut manager = CompetitionManager::new();
///
/// let candidates = vec![
///     CompetitionCandidate {
///         memory_id: "mem1".to_string(),
///         relevance_score: 0.95,
///         similarity_to_query: 0.9,
///         embedding: None,
///     },
///     CompetitionCandidate {
///         memory_id: "mem2".to_string(),
///         relevance_score: 0.80,
///         similarity_to_query: 0.85,
///         embedding: None,
///     },
/// ];
///
/// // Winner: mem1, Loser: mem2 (if similar enough)
/// if let Some(result) = manager.run_competition(&candidates, 0.6) {
///     println!("Winner: {}", result.winner_id);
///     println!("Suppressed: {:?}", result.suppressed_ids);
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionManager {
    /// Configuration for competition behavior
    pub config: CompetitionConfig,
    /// History of competition events
    pub history: VecDeque<CompetitionEvent>,
}

impl Default for CompetitionManager {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut norm_a, mut norm_b) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b.iter()) {
        let (x, y) = (*x as f64, *y as f64);
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        return 0.0;
    }
    (dot / denom).clamp(-1.0, 1.0)
}

impl CompetitionManager {
    /// Create a new competition manager with default config.
    pub fn new() -> Self {
        Self {
            config: CompetitionConfig::default(),
            history: VecDeque::with_capacity(MAX_COMPETITION_HISTORY_SIZE),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: CompetitionConfig) -> Self {
        Self {
            config,
            history: VecDeque::with_capacity(MAX_COMPETITION_HISTORY_SIZE),
        }
    }

    /// Run a competition among candidate memories.
    ///
    /// # Arguments
    ///
    /// * `candidates` - Memories competing for retrieval
    /// * `similarity_threshold` - Minimum similarity for memories to compete
    ///
    /// # Returns
    ///
    /// Competition result if competition occurred, None if not enough similar candidates.
    pub fn run_competition(
        &mut self,
        candidates: &[CompetitionCandidate],
        similarity_threshold: f64,
    ) -> Option<CompetitionResult> {
        if candidates.len() < 2 {
            return None;
        }

        // Sort by relevance score (highest first = winner)
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let winner = &sorted[0];
        let mut suppressed_ids = Vec::new();
        let mut suppressed_similarities = Vec::new();

        // Check each other candidate for competition
        for loser in sorted.iter().skip(1) {
            let similarity = match (&winner.embedding, &loser.embedding) {
                // Both embeddings available — use actual cosine similarity
                (Some(w_emb), Some(l_emb)) => cosine_similarity(w_emb, l_emb),
                // Fallback: average query-similarity as a rough proxy
                _ => (winner.similarity_to_query + loser.similarity_to_query) / 2.0,
            };

            if similarity >= similarity_threshold {
                suppressed_ids.push(loser.memory_id.clone());
                suppressed_similarities.push(similarity);
            }
        }

        if suppressed_ids.is_empty() {
            return None; // No competition occurred
        }

        // Record the event
        let event = CompetitionEvent::new(
            winner.memory_id.clone(),
            suppressed_ids.clone(),
            suppressed_similarities.clone(),
            self.config.suppression_duration,
        );
        self.record_event(event);

        Some(CompetitionResult {
            winner_id: winner.memory_id.clone(),
            winner_boost: self.config.winner_boost,
            suppressed_ids,
            suppressed_similarities,
            suppression_duration: self.config.suppression_duration,
        })
    }

    /// Record a competition event.
    fn record_event(&mut self, event: CompetitionEvent) {
        self.history.push_back(event);
        while self.history.len() > MAX_COMPETITION_HISTORY_SIZE {
            self.history.pop_front();
        }
    }

    /// Get memories that have been suppressed by a specific winner.
    pub fn get_suppressed_by(&self, winner_id: &str) -> Vec<&CompetitionEvent> {
        self.history
            .iter()
            .filter(|e| e.winner_id == winner_id)
            .collect()
    }

    /// Get how many times a memory has been suppressed.
    pub fn suppression_count(&self, memory_id: &str) -> usize {
        self.history
            .iter()
            .filter(|e| e.loser_ids.contains(&memory_id.to_string()))
            .count()
    }

    /// Get how many times a memory has won competitions.
    pub fn win_count(&self, memory_id: &str) -> usize {
        self.history
            .iter()
            .filter(|e| e.winner_id == memory_id)
            .count()
    }

    /// Clear competition history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}

// ============================================================================
// COMPETITION TYPES
// ============================================================================

/// Configuration for retrieval competition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionConfig {
    /// Minimum similarity for memories to compete
    pub similarity_threshold: f64,
    /// How much to boost the winner's strength
    pub winner_boost: f64,
    /// How long losers are suppressed
    pub suppression_duration: Duration,
    /// Whether to track competition history
    pub track_history: bool,
}

impl Default for CompetitionConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: COMPETITION_SIMILARITY_THRESHOLD,
            winner_boost: 0.1,
            suppression_duration: Duration::hours(24),
            track_history: true,
        }
    }
}

/// A candidate in a retrieval competition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionCandidate {
    /// Unique ID of the memory
    pub memory_id: String,
    /// How relevant this memory is to the query (higher = more likely to win)
    pub relevance_score: f64,
    /// How similar this memory is to the query
    pub similarity_to_query: f64,
    /// Optional embedding vector for pairwise cosine similarity.
    /// When present, `run_competition` uses actual cosine distance between
    /// candidates instead of the query-similarity average approximation.
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

/// Result of a retrieval competition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionResult {
    /// ID of the winning memory
    pub winner_id: String,
    /// How much to boost the winner
    pub winner_boost: f64,
    /// IDs of suppressed (losing) memories
    pub suppressed_ids: Vec<String>,
    /// Similarity of each suppressed memory to winner
    pub suppressed_similarities: Vec<f64>,
    /// How long suppression lasts
    pub suppression_duration: Duration,
}

//! Novelty signal — dopamine-like prediction-error detector.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use super::{
    Context, DEFAULT_NOVELTY_THRESHOLD, MAX_IMPORTANCE, MAX_PREDICTION_PATTERNS, MIN_IMPORTANCE,
    PATTERN_DECAY_RATE,
};

// ============================================================================
// NOVELTY SIGNAL (Dopamine-like: Prediction Error)
// ============================================================================

/// Novelty signal inspired by dopamine's role in signaling prediction errors.
///
/// In the brain, dopamine neurons fire when outcomes differ from predictions.
/// This "prediction error" signal drives learning and memory formation.
///
/// ## How It Works
///
/// 1. Maintains a simple n-gram based prediction model of content patterns
/// 2. Computes how much new content deviates from learned patterns
/// 3. High deviation = high novelty = stronger encoding signal
///
/// ## Adaptation
///
/// The model continuously learns from content it sees, so the same content
/// becomes less novel over time - just like habituation in biological systems.
#[derive(Debug)]
pub struct NoveltySignal {
    /// The prediction model that learns content patterns
    prediction_model: PredictionModel,
    /// Threshold below which content is considered "expected"
    novelty_threshold: f64,
}

impl Default for NoveltySignal {
    fn default() -> Self {
        Self::new()
    }
}

impl NoveltySignal {
    /// Create a new novelty signal detector
    pub fn new() -> Self {
        Self {
            prediction_model: PredictionModel::new(),
            novelty_threshold: DEFAULT_NOVELTY_THRESHOLD,
        }
    }

    /// Create with custom novelty threshold
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.novelty_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Compute novelty score for content
    ///
    /// Returns a score from 0.0 (completely expected) to 1.0 (completely novel).
    pub fn compute(&self, content: &str, context: &Context) -> f64 {
        let prediction_error = self.prediction_model.compute_prediction_error(content);

        // Context-based adjustment
        let context_modifier = self.compute_context_modifier(content, context);

        // Combine prediction error with context
        let raw_novelty = (prediction_error * 0.7) + (context_modifier * 0.3);

        // Apply threshold - content below threshold gets reduced novelty
        if raw_novelty < self.novelty_threshold {
            raw_novelty * 0.5
        } else {
            raw_novelty
        }
        .clamp(MIN_IMPORTANCE, MAX_IMPORTANCE)
    }

    /// Update the prediction model with new content (learning)
    pub fn update_model(&mut self, content: &str) {
        self.prediction_model.learn(content);
    }

    /// Check if content is considered novel (above threshold)
    pub fn is_novel(&self, content: &str, context: &Context) -> bool {
        self.compute(content, context) > self.novelty_threshold
    }

    /// Get explanation for novelty score
    pub fn explain(&self, content: &str, context: &Context) -> NoveltyExplanation {
        let score = self.compute(content, context);
        let novel_patterns = self.prediction_model.find_novel_patterns(content);
        let familiar_patterns = self.prediction_model.find_familiar_patterns(content);

        NoveltyExplanation {
            score,
            novel_patterns,
            familiar_patterns,
            prediction_confidence: self.prediction_model.pattern_coverage(content),
        }
    }

    fn compute_context_modifier(&self, content: &str, context: &Context) -> f64 {
        let mut modifier: f64 = 0.5; // Neutral starting point

        // New topics are more novel
        if !context.recent_queries.is_empty() {
            let content_lower = content.to_lowercase();
            let query_overlap = context
                .recent_queries
                .iter()
                .filter(|q| content_lower.contains(&q.to_lowercase()))
                .count();

            if query_overlap == 0 {
                modifier += 0.3; // Content unrelated to recent queries = more novel
            }
        }

        // Content in new project context is more novel
        if context.project.is_some() {
            modifier += 0.1;
        }

        modifier.clamp(0.0, 1.0)
    }
}

/// Explanation of novelty score for transparency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoveltyExplanation {
    /// The computed novelty score
    pub score: f64,
    /// Patterns in content that were novel (not seen before)
    pub novel_patterns: Vec<String>,
    /// Patterns in content that were familiar (seen before)
    pub familiar_patterns: Vec<String>,
    /// How much of the content the model can predict
    pub prediction_confidence: f64,
}

impl NoveltyExplanation {
    /// Generate human-readable explanation
    pub fn explain(&self) -> String {
        if self.score > 0.7 {
            format!(
                "Highly novel content ({:.0}% new). Novel patterns: {}",
                self.score * 100.0,
                self.novel_patterns
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if self.score > 0.4 {
            format!(
                "Moderately novel ({:.0}% new). Mix of familiar and new patterns.",
                self.score * 100.0
            )
        } else {
            format!(
                "Familiar content ({:.0}% expected). Matches known patterns.",
                (1.0 - self.score) * 100.0
            )
        }
    }
}

/// Simple n-gram based prediction model
#[derive(Debug)]
struct PredictionModel {
    /// N-gram frequencies (pattern -> count)
    patterns: Arc<RwLock<HashMap<String, u32>>>,
    /// Total patterns seen
    total_count: Arc<RwLock<u64>>,
    /// N-gram size
    ngram_size: usize,
}

impl PredictionModel {
    fn new() -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
            total_count: Arc::new(RwLock::new(0)),
            ngram_size: 3,
        }
    }

    fn learn(&self, content: &str) {
        let ngrams = self.extract_ngrams(content);

        if let Ok(mut patterns) = self.patterns.write()
            && let Ok(mut total) = self.total_count.write()
        {
            for ngram in ngrams {
                *patterns.entry(ngram).or_insert(0) += 1;
                *total += 1;
            }

            // Prune if too large
            if patterns.len() > MAX_PREDICTION_PATTERNS {
                self.apply_decay(&mut patterns);
            }
        }
    }

    fn compute_prediction_error(&self, content: &str) -> f64 {
        let ngrams = self.extract_ngrams(content);
        if ngrams.is_empty() {
            return 0.5; // Unknown content = moderate novelty
        }

        let patterns = match self.patterns.read() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(
                    "PredictionModel: patterns RwLock poisoned, returning neutral score"
                );
                return 0.5;
            }
        };

        let total = match self.total_count.read() {
            Ok(t) => *t,
            Err(_) => {
                tracing::warn!(
                    "PredictionModel: total_count RwLock poisoned, returning neutral score"
                );
                return 0.5;
            }
        };

        if total == 0 || patterns.is_empty() {
            return 1.0; // No training data = everything is novel
        }

        // Calculate what fraction of ngrams are "unexpected"
        let mut unexpected_count = 0;
        let mut total_surprise = 0.0;

        for ngram in &ngrams {
            match patterns.get(ngram) {
                Some(&count) => {
                    // Lower frequency = more surprising
                    let probability = count as f64 / total as f64;
                    total_surprise += 1.0 - probability.sqrt();
                }
                None => {
                    // Never seen = maximum surprise
                    unexpected_count += 1;
                    total_surprise += 1.0;
                }
            }
        }

        // Combine unexpected ratio with average surprise
        let unexpected_ratio = unexpected_count as f64 / ngrams.len() as f64;
        let avg_surprise = total_surprise / ngrams.len() as f64;

        (unexpected_ratio * 0.6 + avg_surprise * 0.4).clamp(0.0, 1.0)
    }

    fn pattern_coverage(&self, content: &str) -> f64 {
        let ngrams = self.extract_ngrams(content);
        if ngrams.is_empty() {
            return 0.0;
        }

        let patterns = match self.patterns.read() {
            Ok(p) => p,
            Err(_) => return 0.0,
        };

        let known_count = ngrams
            .iter()
            .filter(|ng| patterns.contains_key(*ng))
            .count();

        known_count as f64 / ngrams.len() as f64
    }

    fn find_novel_patterns(&self, content: &str) -> Vec<String> {
        let ngrams = self.extract_ngrams(content);
        let patterns = match self.patterns.read() {
            Ok(p) => p,
            Err(_) => return vec![],
        };

        ngrams
            .into_iter()
            .filter(|ng| !patterns.contains_key(ng))
            .take(5)
            .collect()
    }

    fn find_familiar_patterns(&self, content: &str) -> Vec<String> {
        let ngrams = self.extract_ngrams(content);
        let patterns = match self.patterns.read() {
            Ok(p) => p,
            Err(_) => return vec![],
        };

        let mut familiar: Vec<_> = ngrams
            .into_iter()
            .filter_map(|ng| patterns.get(&ng).map(|&count| (ng, count)))
            .collect();

        familiar.sort_by(|a, b| b.1.cmp(&a.1));
        familiar.into_iter().take(5).map(|(ng, _)| ng).collect()
    }

    fn extract_ngrams(&self, content: &str) -> Vec<String> {
        let lowercased = content.to_lowercase();
        let words: Vec<&str> = lowercased
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .collect();

        if words.len() < self.ngram_size {
            return words.iter().map(|s| s.to_string()).collect();
        }

        words
            .windows(self.ngram_size)
            .map(|w| w.join(" "))
            .collect()
    }

    fn apply_decay(&self, patterns: &mut HashMap<String, u32>) {
        // Remove lowest frequency patterns
        let mut entries: Vec<_> = patterns.iter().map(|(k, v)| (k.clone(), *v)).collect();
        entries.sort_by(|a, b| a.1.cmp(&b.1));

        // Remove bottom 20%
        let remove_count = patterns.len() / 5;
        for (key, _) in entries.into_iter().take(remove_count) {
            patterns.remove(&key);
        }

        // Apply decay to remaining
        for count in patterns.values_mut() {
            *count = ((*count as f64) * PATTERN_DECAY_RATE) as u32;
        }
    }
}

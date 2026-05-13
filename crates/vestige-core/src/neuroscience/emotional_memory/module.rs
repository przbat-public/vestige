//! `EmotionalMemory` — module #29 of the cognitive engine.

use std::collections::HashMap;

use chrono::{Duration, Utc};

use super::constants::{
    CAPTURE_BOOST, CAPTURE_WINDOW_MINUTES, EMOTIONAL_DECAY_FACTOR,
    FLASHBULB_AROUSAL_THRESHOLD, FLASHBULB_NOVELTY_THRESHOLD, MOOD_CONGRUENCE_BOOST,
    MOOD_CONGRUENCE_THRESHOLD, MOOD_HISTORY_CAPACITY,
};
use super::evaluation::{EmotionCategory, EmotionalEvaluation};
use super::record::EmotionalRecord;

/// Emotional Memory module — CognitiveEngine field #29.
///
/// Manages emotion-cognition interaction for memory encoding, consolidation,
/// and retrieval. Implements flashbulb encoding, mood-congruent retrieval,
/// emotional decay modulation, and tag-and-capture.
#[derive(Debug)]
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Fields are read and mutated by sibling submodules (`evaluation`, `record`) of the cohesive `emotional_memory` component (split-by-responsibility refactor); siblings have the same trust level as the parent module, and accessor wrappers would add boilerplate without protecting any invariant."
)]
pub struct EmotionalMemory {
    /// Current mood state (running average of recent emotional evaluations)
    pub(super) current_mood_valence: f64,
    pub(super) current_mood_arousal: f64,

    /// History of recent emotional evaluations for mood tracking
    pub(super) mood_history: Vec<(f64, f64)>, // (valence, arousal)

    /// Recent emotional records for tag-and-capture
    pub(super) recent_records: Vec<EmotionalRecord>,

    /// Emotion lexicon: word -> (valence, arousal)
    pub(super) lexicon: HashMap<String, (f64, f64)>,

    /// Urgency markers that trigger high arousal
    pub(super) urgency_markers: Vec<String>,

    /// Total evaluations performed
    pub(super) evaluations_count: u64,

    /// Total flashbulbs detected
    pub(super) flashbulbs_detected: u64,
}

impl Default for EmotionalMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl EmotionalMemory {
    /// Create a new EmotionalMemory module with default lexicon
    pub fn new() -> Self {
        Self {
            current_mood_valence: 0.0,
            current_mood_arousal: 0.3,
            mood_history: Vec::new(),
            recent_records: Vec::new(),
            lexicon: Self::build_lexicon(),
            urgency_markers: Self::build_urgency_markers(),
            evaluations_count: 0,
            flashbulbs_detected: 0,
        }
    }

    /// Evaluate the emotional content of text.
    ///
    /// Returns valence, arousal, flashbulb flag, and emotion category.
    /// This is the primary entry point for the emotional memory system.
    pub fn evaluate_content(&mut self, content: &str) -> EmotionalEvaluation {
        let words: Vec<String> = content
            .to_lowercase()
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| !w.is_empty())
            .collect();

        let mut total_valence = 0.0;
        let mut total_arousal = 0.0;
        let mut contributing = Vec::new();
        let mut hit_count = 0;

        // Check negation context (simple window-based)
        let negation_words: Vec<&str> = vec![
            "not",
            "no",
            "never",
            "don't",
            "doesn't",
            "didn't",
            "won't",
            "can't",
            "couldn't",
            "shouldn't",
            "without",
            "hardly",
        ];

        for (i, word) in words.iter().enumerate() {
            if let Some(&(valence, arousal)) = self.lexicon.get(word.as_str()) {
                // Check for negation in 3-word window before
                let negated =
                    (i.saturating_sub(3)..i).any(|j| negation_words.contains(&words[j].as_str()));

                let effective_valence = if negated { -valence * 0.7 } else { valence };

                total_valence += effective_valence;
                total_arousal += arousal;
                contributing.push(word.clone());
                hit_count += 1;
            }
        }

        // Check urgency markers (case-insensitive full phrases)
        let content_lower = content.to_lowercase();
        let mut urgency_boost = 0.0;
        for marker in &self.urgency_markers {
            if content_lower.contains(marker) {
                urgency_boost += 0.3;
                if !contributing.contains(marker) {
                    contributing.push(marker.clone());
                }
            }
        }

        // Normalize scores
        let (valence, arousal) = if hit_count > 0 {
            let v = (total_valence / hit_count as f64).clamp(-1.0, 1.0);
            let a = (total_arousal / hit_count as f64 + urgency_boost).clamp(0.0, 1.0);
            (v, a)
        } else {
            (0.0, urgency_boost.clamp(0.0, 1.0))
        };

        // Determine category
        let category = self.categorize(valence, arousal, &content_lower);

        // Confidence based on lexicon coverage
        let confidence = if words.is_empty() {
            0.0
        } else {
            (hit_count as f64 / words.len() as f64).min(1.0) * 0.5
                + if urgency_boost > 0.0 { 0.3 } else { 0.0 }
                + if hit_count > 3 { 0.2 } else { 0.0 }
        };

        // Flashbulb detection: high novelty proxy (urgency/surprise markers) + high arousal
        let novelty_proxy = urgency_boost
            + if category == EmotionCategory::Surprise {
                0.4
            } else {
                0.0
            };
        let is_flashbulb =
            novelty_proxy >= FLASHBULB_NOVELTY_THRESHOLD && arousal >= FLASHBULB_AROUSAL_THRESHOLD;

        if is_flashbulb {
            self.flashbulbs_detected += 1;
        }

        // Update mood state
        self.update_mood(valence, arousal);
        self.evaluations_count += 1;

        EmotionalEvaluation {
            valence,
            arousal,
            is_flashbulb,
            category,
            contributing_words: contributing,
            confidence,
        }
    }

    /// Evaluate content with external importance scores (from ImportanceSignals).
    ///
    /// Uses the actual novelty and arousal scores from the 4-channel importance
    /// system for more accurate flashbulb detection.
    pub fn evaluate_with_importance(
        &mut self,
        content: &str,
        novelty_score: f64,
        arousal_score: f64,
    ) -> EmotionalEvaluation {
        let mut eval = self.evaluate_content(content);

        // Override flashbulb detection with real importance scores
        eval.is_flashbulb = novelty_score >= FLASHBULB_NOVELTY_THRESHOLD
            && arousal_score >= FLASHBULB_AROUSAL_THRESHOLD;

        // Blend arousal from lexicon with importance arousal
        eval.arousal = (eval.arousal * 0.4 + arousal_score * 0.6).clamp(0.0, 1.0);

        if eval.is_flashbulb && self.flashbulbs_detected == 0 {
            self.flashbulbs_detected += 1;
        }

        eval
    }

    /// Record a memory's emotional state for tag-and-capture.
    ///
    /// Call this after ingesting a memory so that subsequent high-emotion
    /// events can retroactively boost temporally adjacent memories.
    pub fn record_encoding(&mut self, memory_id: &str, valence: f64, arousal: f64) {
        self.recent_records.push(EmotionalRecord {
            memory_id: memory_id.to_string(),
            valence,
            arousal,
            encoded_at: Utc::now(),
        });

        // Keep only records within the capture window
        let cutoff = Utc::now() - Duration::minutes(CAPTURE_WINDOW_MINUTES * 2);
        self.recent_records.retain(|r| r.encoded_at > cutoff);
    }

    /// Get memory IDs that should be boosted via tag-and-capture.
    ///
    /// When a high-arousal event occurs, memories encoded within ±30 minutes
    /// get a retroactive boost. Returns (memory_id, boost_amount) pairs.
    pub fn get_capture_targets(&self, trigger_arousal: f64) -> Vec<(String, f64)> {
        if trigger_arousal < FLASHBULB_AROUSAL_THRESHOLD {
            return Vec::new();
        }

        let now = Utc::now();
        let window = Duration::minutes(CAPTURE_WINDOW_MINUTES);

        self.recent_records
            .iter()
            .filter(|r| {
                let age = now - r.encoded_at;
                age < window && age >= Duration::zero()
            })
            .map(|r| {
                // Boost scales with trigger arousal and proximity
                let age_minutes = (now - r.encoded_at).num_minutes() as f64;
                let proximity = 1.0 - (age_minutes / CAPTURE_WINDOW_MINUTES as f64);
                let boost = CAPTURE_BOOST * trigger_arousal * proximity;
                (r.memory_id.clone(), boost)
            })
            .collect()
    }

    /// Compute FSRS stability multiplier for emotional content.
    ///
    /// Emotional memories decay more slowly. Multiplier > 1.0 means slower decay.
    /// Formula: 1.0 + EMOTIONAL_DECAY_FACTOR * arousal
    pub fn stability_multiplier(&self, arousal: f64) -> f64 {
        1.0 + EMOTIONAL_DECAY_FACTOR * arousal
    }

    /// Compute mood-congruent retrieval boost for a memory.
    ///
    /// If the memory's emotional valence matches the current mood,
    /// it gets a retrieval score boost.
    pub fn mood_congruence_boost(&self, memory_valence: f64) -> f64 {
        let valence_match = 1.0 - (self.current_mood_valence - memory_valence).abs();
        if valence_match > MOOD_CONGRUENCE_THRESHOLD {
            MOOD_CONGRUENCE_BOOST * valence_match
        } else {
            0.0
        }
    }

    /// Get the current mood state
    pub fn current_mood(&self) -> (f64, f64) {
        (self.current_mood_valence, self.current_mood_arousal)
    }

    /// Get module statistics
    pub fn stats(&self) -> EmotionalMemoryStats {
        EmotionalMemoryStats {
            evaluations_count: self.evaluations_count,
            flashbulbs_detected: self.flashbulbs_detected,
            current_mood_valence: self.current_mood_valence,
            current_mood_arousal: self.current_mood_arousal,
            recent_records_count: self.recent_records.len(),
            lexicon_size: self.lexicon.len(),
        }
    }

    // ========================================================================
    // PRIVATE METHODS
    // ========================================================================

    /// Update running mood average
    fn update_mood(&mut self, valence: f64, arousal: f64) {
        self.mood_history.push((valence, arousal));
        if self.mood_history.len() > MOOD_HISTORY_CAPACITY {
            self.mood_history.remove(0);
        }

        if !self.mood_history.is_empty() {
            let len = self.mood_history.len() as f64;
            self.current_mood_valence = self.mood_history.iter().map(|(v, _)| v).sum::<f64>() / len;
            self.current_mood_arousal = self.mood_history.iter().map(|(_, a)| a).sum::<f64>() / len;
        }
    }

    /// Categorize emotion based on valence and arousal
    fn categorize(&self, valence: f64, arousal: f64, content: &str) -> EmotionCategory {
        // Check for urgency first (high priority)
        if arousal > 0.7 && self.urgency_markers.iter().any(|m| content.contains(m)) {
            return EmotionCategory::Urgency;
        }

        // Use valence-arousal space (Russell's circumplex model)
        if arousal < 0.2 && valence.abs() < 0.2 {
            EmotionCategory::Neutral
        } else if valence > 0.3 && arousal > 0.4 {
            EmotionCategory::Joy
        } else if valence < -0.3 && arousal > 0.5 {
            EmotionCategory::Frustration
        } else if arousal > 0.6 && valence.abs() < 0.4 {
            EmotionCategory::Surprise
        } else if valence < -0.1 && arousal < 0.4 {
            EmotionCategory::Confusion
        } else {
            EmotionCategory::Neutral
        }
    }

    /// Build the emotion lexicon (word -> (valence, arousal))
    fn build_lexicon() -> HashMap<String, (f64, f64)> {
        let mut lex = HashMap::new();

        // Positive / Low arousal
        for (word, v, a) in [
            ("good", 0.6, 0.3),
            ("nice", 0.5, 0.2),
            ("clean", 0.4, 0.2),
            ("simple", 0.3, 0.1),
            ("smooth", 0.4, 0.2),
            ("stable", 0.4, 0.1),
            ("helpful", 0.5, 0.3),
            ("elegant", 0.6, 0.3),
            ("solid", 0.4, 0.2),
        ] {
            lex.insert(word.to_string(), (v, a));
        }

        // Positive / High arousal
        for (word, v, a) in [
            ("amazing", 0.9, 0.8),
            ("excellent", 0.8, 0.6),
            ("perfect", 0.9, 0.7),
            ("awesome", 0.8, 0.7),
            ("great", 0.7, 0.5),
            ("fantastic", 0.9, 0.8),
            ("brilliant", 0.8, 0.7),
            ("incredible", 0.9, 0.8),
            ("love", 0.8, 0.7),
            ("success", 0.7, 0.6),
            ("solved", 0.7, 0.6),
            ("fixed", 0.6, 0.5),
            ("working", 0.5, 0.4),
            ("breakthrough", 0.9, 0.9),
            ("discovered", 0.7, 0.7),
        ] {
            lex.insert(word.to_string(), (v, a));
        }

        // Negative / Low arousal
        for (word, v, a) in [
            ("bad", -0.5, 0.3),
            ("wrong", -0.4, 0.3),
            ("slow", -0.3, 0.2),
            ("confusing", -0.4, 0.3),
            ("unclear", -0.3, 0.2),
            ("messy", -0.4, 0.3),
            ("annoying", -0.5, 0.4),
            ("boring", -0.3, 0.1),
            ("ugly", -0.5, 0.3),
            ("deprecated", -0.3, 0.2),
            ("stale", -0.3, 0.1),
        ] {
            lex.insert(word.to_string(), (v, a));
        }

        // Negative / High arousal (bugs, errors, failures)
        for (word, v, a) in [
            ("error", -0.6, 0.7),
            ("bug", -0.6, 0.6),
            ("crash", -0.8, 0.9),
            ("fail", -0.7, 0.7),
            ("failed", -0.7, 0.7),
            ("failure", -0.7, 0.7),
            ("broken", -0.7, 0.7),
            ("panic", -0.9, 0.9),
            ("fatal", -0.9, 0.9),
            ("critical", -0.5, 0.9),
            ("severe", -0.6, 0.8),
            ("urgent", -0.3, 0.9),
            ("emergency", -0.5, 0.9),
            ("vulnerability", -0.7, 0.8),
            ("exploit", -0.7, 0.8),
            ("leaked", -0.8, 0.9),
            ("compromised", -0.8, 0.9),
            ("timeout", -0.5, 0.6),
            ("deadlock", -0.7, 0.8),
            ("overflow", -0.6, 0.7),
            ("corruption", -0.8, 0.8),
            ("regression", -0.6, 0.7),
            ("blocker", -0.6, 0.8),
            ("outage", -0.8, 0.9),
            ("incident", -0.5, 0.7),
        ] {
            lex.insert(word.to_string(), (v, a));
        }

        // Surprise / Discovery
        for (word, v, a) in [
            ("unexpected", 0.0, 0.7),
            ("surprising", 0.1, 0.7),
            ("strange", -0.1, 0.6),
            ("weird", -0.2, 0.5),
            ("interesting", 0.4, 0.6),
            ("curious", 0.3, 0.5),
            ("insight", 0.6, 0.7),
            ("realized", 0.4, 0.6),
            ("found", 0.3, 0.5),
            ("noticed", 0.2, 0.4),
        ] {
            lex.insert(word.to_string(), (v, a));
        }

        // Technical intensity markers
        for (word, v, a) in [
            ("production", -0.1, 0.7),
            ("deploy", 0.1, 0.6),
            ("migration", -0.1, 0.5),
            ("refactor", 0.1, 0.4),
            ("security", -0.1, 0.6),
            ("performance", 0.1, 0.4),
            ("important", 0.2, 0.6),
            ("remember", 0.1, 0.5),
        ] {
            lex.insert(word.to_string(), (v, a));
        }

        lex
    }

    /// Build urgency markers (phrases that indicate high-urgency situations)
    fn build_urgency_markers() -> Vec<String> {
        vec![
            "production down".to_string(),
            "server down".to_string(),
            "data loss".to_string(),
            "security breach".to_string(),
            "critical bug".to_string(),
            "urgent fix".to_string(),
            "asap".to_string(),
            "p0".to_string(),
            "hotfix".to_string(),
            "rollback".to_string(),
            "incident".to_string(),
        ]
    }
}

/// Statistics for the EmotionalMemory module
#[derive(Debug, Clone)]
pub struct EmotionalMemoryStats {
    pub evaluations_count: u64,
    pub flashbulbs_detected: u64,
    pub current_mood_valence: f64,
    pub current_mood_arousal: f64,
    pub recent_records_count: usize,
    pub lexicon_size: usize,
}

//! Emotional context: sentiment polarity and magnitude.

use serde::{Deserialize, Serialize};

// ============================================================================
// EMOTIONAL CONTEXT
// ============================================================================

/// Emotional context captures the emotional state during encoding
///
/// Based on mood-congruent memory research, emotional context
/// significantly affects memory encoding and retrieval.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionalContext {
    /// Emotional valence (-1.0 = negative, 0.0 = neutral, 1.0 = positive)
    pub valence: f64,
    /// Arousal level (0.0 = calm, 1.0 = excited/agitated)
    pub arousal: f64,
    /// Dominance (0.0 = submissive, 1.0 = dominant/in control)
    pub dominance: f64,
    /// Primary emotion label (optional)
    pub primary_emotion: Option<String>,
    /// Confidence in the emotional assessment (0.0 to 1.0)
    pub confidence: f64,
}

impl EmotionalContext {
    /// Create a neutral emotional context
    pub fn neutral() -> Self {
        Self {
            valence: 0.0,
            arousal: 0.5,
            dominance: 0.5,
            primary_emotion: None,
            confidence: 0.5,
        }
    }

    /// Create from sentiment scores (maps to valence)
    pub fn from_sentiment(score: f64, magnitude: f64) -> Self {
        Self {
            valence: score,
            arousal: magnitude,
            dominance: 0.5,
            primary_emotion: Self::infer_emotion(score, magnitude),
            confidence: magnitude.min(1.0),
        }
    }

    /// Infer primary emotion from valence and arousal
    fn infer_emotion(valence: f64, arousal: f64) -> Option<String> {
        let emotion = match (valence > 0.3, valence < -0.3, arousal > 0.6) {
            (true, false, true) => "excited",
            (true, false, false) => "content",
            (false, true, true) => "angry",
            (false, true, false) => "sad",
            (false, false, true) => "anxious",
            (false, false, false) => "neutral",
            // Edge case: both conditions true (shouldn't happen with proper thresholds)
            (true, true, _) => "conflicted",
        };
        Some(emotion.to_string())
    }

    /// Check if this is a positive emotional state
    pub fn is_positive(&self) -> bool {
        self.valence > 0.2
    }

    /// Check if this is a negative emotional state
    pub fn is_negative(&self) -> bool {
        self.valence < -0.2
    }

    /// Check if this is a high-arousal state
    pub fn is_high_arousal(&self) -> bool {
        self.arousal > 0.6
    }
}

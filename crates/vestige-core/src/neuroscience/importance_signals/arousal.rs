//! Arousal signal — norepinephrine-like emotional-intensity detector.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::sentiment::SentimentAnalyzer;
use super::{MAX_IMPORTANCE, MIN_IMPORTANCE};

// ============================================================================
// AROUSAL SIGNAL (Norepinephrine-like: Emotional Intensity)
// ============================================================================

/// Arousal signal inspired by norepinephrine's role in emotional processing.
///
/// In the brain, the locus coeruleus releases norepinephrine during emotionally
/// charged events, which strengthens amygdala-mediated memory encoding.
/// This creates "flashbulb memories" - vivid memories of emotionally intense events.
///
/// ## Detection Methods
///
/// 1. **Sentiment Analysis**: Detects emotional polarity and intensity
/// 2. **Intensity Keywords**: Domain-specific vocabulary indicating urgency/importance
/// 3. **Punctuation Patterns**: !!! and ??? indicate emotional emphasis
/// 4. **Capitalization**: ALL CAPS suggests heightened emotional state
#[derive(Debug)]
pub struct ArousalSignal {
    /// Sentiment analyzer for emotional content detection
    sentiment_analyzer: SentimentAnalyzer,
    /// Domain-specific keywords indicating high intensity
    intensity_keywords: HashSet<String>,
}

impl Default for ArousalSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ArousalSignal {
    /// Create a new arousal signal detector
    pub fn new() -> Self {
        Self {
            sentiment_analyzer: SentimentAnalyzer::new(),
            intensity_keywords: Self::default_intensity_keywords(),
        }
    }

    /// Add custom intensity keywords
    pub fn with_keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for kw in keywords {
            self.intensity_keywords.insert(kw.into().to_lowercase());
        }
        self
    }

    /// Compute arousal score for content
    ///
    /// Returns a score from 0.0 (emotionally neutral) to 1.0 (highly arousing).
    pub fn compute(&self, content: &str) -> f64 {
        let sentiment = self.sentiment_analyzer.analyze(content);
        let keyword_score = self.compute_keyword_intensity(content);
        let punctuation_score = self.compute_punctuation_intensity(content);
        let capitalization_score = self.compute_capitalization_intensity(content);

        // Weighted combination
        let raw_arousal = sentiment.magnitude * 0.35
            + keyword_score * 0.30
            + punctuation_score * 0.20
            + capitalization_score * 0.15;

        raw_arousal.clamp(MIN_IMPORTANCE, MAX_IMPORTANCE)
    }

    /// Detect emotional markers in content
    pub fn detect_emotional_markers(&self, content: &str) -> Vec<EmotionalMarker> {
        let mut markers = Vec::new();
        let content_lower = content.to_lowercase();

        // Check for intensity keywords
        for keyword in &self.intensity_keywords {
            if content_lower.contains(keyword) {
                markers.push(EmotionalMarker {
                    marker_type: MarkerType::IntensityKeyword,
                    text: keyword.clone(),
                    intensity: 0.8,
                });
            }
        }

        // Check sentiment words
        let sentiment = self.sentiment_analyzer.analyze(content);
        for word in sentiment.contributing_words {
            markers.push(EmotionalMarker {
                marker_type: if sentiment.polarity >= 0.0 {
                    MarkerType::PositiveSentiment
                } else {
                    MarkerType::NegativeSentiment
                },
                text: word,
                intensity: sentiment.magnitude.abs(),
            });
        }

        // Check punctuation patterns
        if content.contains("!!!") || content.contains("???") {
            markers.push(EmotionalMarker {
                marker_type: MarkerType::PunctuationEmphasis,
                text: "Multiple punctuation".to_string(),
                intensity: 0.7,
            });
        }

        // Check capitalization
        let caps_ratio = self.compute_capitalization_intensity(content);
        if caps_ratio > 0.3 {
            markers.push(EmotionalMarker {
                marker_type: MarkerType::Capitalization,
                text: "Excessive capitalization".to_string(),
                intensity: caps_ratio,
            });
        }

        markers
    }

    /// Get explanation for arousal score
    pub fn explain(&self, content: &str) -> ArousalExplanation {
        let score = self.compute(content);
        let markers = self.detect_emotional_markers(content);
        let sentiment = self.sentiment_analyzer.analyze(content);

        ArousalExplanation {
            score,
            emotional_markers: markers,
            sentiment_polarity: sentiment.polarity,
            sentiment_magnitude: sentiment.magnitude,
        }
    }

    fn default_intensity_keywords() -> HashSet<String> {
        [
            // Urgency
            "urgent",
            "critical",
            "emergency",
            "immediately",
            "asap",
            "now",
            "deadline",
            "priority",
            "important",
            "crucial",
            "vital",
            // Negative intensity
            "error",
            "failed",
            "failure",
            "crash",
            "broken",
            "bug",
            "issue",
            "problem",
            "wrong",
            "bad",
            "terrible",
            "disaster",
            "catastrophe",
            "panic",
            "crisis",
            "alert",
            "warning",
            "danger",
            "risk",
            // Positive intensity
            "amazing",
            "incredible",
            "awesome",
            "excellent",
            "perfect",
            "brilliant",
            "breakthrough",
            "success",
            "solved",
            "fixed",
            "working",
            "victory",
            "achievement",
            "milestone",
            "celebration",
            // Technical urgency
            "production",
            "outage",
            "downtime",
            "security",
            "vulnerability",
            "exploit",
            "breach",
            "data loss",
            "corruption",
            "rollback",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn compute_keyword_intensity(&self, content: &str) -> f64 {
        let content_lower = content.to_lowercase();
        let word_count = content.split_whitespace().count().max(1) as f64;

        let keyword_count = self
            .intensity_keywords
            .iter()
            .filter(|kw| content_lower.contains(kw.as_str()))
            .count() as f64;

        // Normalize by content length but cap the intensity
        (keyword_count / word_count * 10.0).min(1.0)
    }

    fn compute_punctuation_intensity(&self, content: &str) -> f64 {
        let char_count = content.chars().count().max(1) as f64;

        let exclamation_count = content.matches('!').count() as f64;
        let question_count = content.matches('?').count() as f64;

        // Multiple consecutive punctuation is more intense
        let multi_punct =
            content.matches("!!").count() as f64 * 2.0 + content.matches("??").count() as f64 * 2.0;

        ((exclamation_count + question_count + multi_punct) / char_count * 20.0).min(1.0)
    }

    fn compute_capitalization_intensity(&self, content: &str) -> f64 {
        let letters: Vec<char> = content.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.is_empty() {
            return 0.0;
        }

        let uppercase_count = letters.iter().filter(|c| c.is_uppercase()).count();
        let ratio = uppercase_count as f64 / letters.len() as f64;

        // Normal text has ~5-10% capitals (sentence starts, names)
        // Anything above 30% suggests emphasis
        if ratio > 0.3 {
            ((ratio - 0.3) * 2.0).min(1.0)
        } else {
            0.0
        }
    }
}

/// An emotional marker detected in content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalMarker {
    /// Type of emotional marker
    pub marker_type: MarkerType,
    /// The text that triggered this marker
    pub text: String,
    /// Intensity of this marker (0.0 to 1.0)
    pub intensity: f64,
}

/// Types of emotional markers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarkerType {
    /// Positive sentiment word
    PositiveSentiment,
    /// Negative sentiment word
    NegativeSentiment,
    /// Intensity/urgency keyword
    IntensityKeyword,
    /// Emphatic punctuation (!!! ???)
    PunctuationEmphasis,
    /// Excessive capitalization
    Capitalization,
}

/// Explanation of arousal score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArousalExplanation {
    /// The computed arousal score
    pub score: f64,
    /// Emotional markers detected
    pub emotional_markers: Vec<EmotionalMarker>,
    /// Overall sentiment polarity (-1.0 to 1.0)
    pub sentiment_polarity: f64,
    /// Sentiment intensity (0.0 to 1.0)
    pub sentiment_magnitude: f64,
}

impl ArousalExplanation {
    /// Generate human-readable explanation
    pub fn explain(&self) -> String {
        let intensity_level = if self.score > 0.7 {
            "Highly emotional"
        } else if self.score > 0.4 {
            "Moderately emotional"
        } else {
            "Emotionally neutral"
        };

        let sentiment_desc = if self.sentiment_polarity > 0.3 {
            "positive"
        } else if self.sentiment_polarity < -0.3 {
            "negative"
        } else {
            "neutral"
        };

        format!(
            "{} content ({:.0}% arousal) with {} sentiment. {} markers detected.",
            intensity_level,
            self.score * 100.0,
            sentiment_desc,
            self.emotional_markers.len()
        )
    }
}

//! Lightweight keyword-based sentiment analyzer used by the arousal signal.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Simple keyword-based sentiment analyzer
#[derive(Debug)]
pub struct SentimentAnalyzer {
    /// Positive sentiment words with weights
    positive_words: HashMap<String, f64>,
    /// Negative sentiment words with weights
    negative_words: HashMap<String, f64>,
    /// Negation words that flip sentiment
    negation_words: HashSet<String>,
}

impl Default for SentimentAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SentimentAnalyzer {
    /// Create a new sentiment analyzer with default vocabulary
    pub fn new() -> Self {
        Self {
            positive_words: Self::default_positive_words(),
            negative_words: Self::default_negative_words(),
            negation_words: Self::default_negation_words(),
        }
    }

    /// Analyze sentiment of content
    pub fn analyze(&self, content: &str) -> SentimentResult {
        let lowercased = content.to_lowercase();
        let words: Vec<&str> = lowercased
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();

        let mut positive_score = 0.0;
        let mut negative_score = 0.0;
        let mut contributing_words = Vec::new();
        let mut negated = false;

        for (i, word) in words.iter().enumerate() {
            // Check for negation
            if self.negation_words.contains(*word) {
                negated = true;
                continue;
            }

            // Check positive words
            if let Some(&weight) = self.positive_words.get(*word) {
                if negated {
                    negative_score += weight;
                    negated = false;
                } else {
                    positive_score += weight;
                }
                contributing_words.push(word.to_string());
            }

            // Check negative words
            if let Some(&weight) = self.negative_words.get(*word) {
                if negated {
                    positive_score += weight;
                    negated = false;
                } else {
                    negative_score += weight;
                }
                contributing_words.push(word.to_string());
            }

            // Reset negation after a few words
            if i > 0 && negated {
                negated = false;
            }
        }

        let word_count = words.len().max(1) as f64;
        let total = positive_score + negative_score;

        SentimentResult {
            polarity: if total > 0.0 {
                (positive_score - negative_score) / total
            } else {
                0.0
            },
            magnitude: ((positive_score + negative_score) / word_count * 5.0).min(1.0),
            contributing_words,
        }
    }

    fn default_positive_words() -> HashMap<String, f64> {
        [
            ("good", 0.5),
            ("great", 0.7),
            ("excellent", 0.9),
            ("amazing", 0.9),
            ("wonderful", 0.8),
            ("fantastic", 0.8),
            ("awesome", 0.8),
            ("brilliant", 0.8),
            ("perfect", 0.9),
            ("love", 0.8),
            ("happy", 0.6),
            ("pleased", 0.5),
            ("successful", 0.7),
            ("success", 0.7),
            ("solved", 0.6),
            ("fixed", 0.5),
            ("working", 0.4),
            ("works", 0.4),
            ("better", 0.5),
            ("best", 0.7),
            ("helpful", 0.5),
            ("useful", 0.5),
            ("efficient", 0.5),
            ("effective", 0.5),
            ("impressive", 0.7),
            ("outstanding", 0.8),
            ("superb", 0.8),
            ("remarkable", 0.7),
            ("thanks", 0.5),
            ("thank", 0.5),
            ("appreciate", 0.6),
            ("grateful", 0.6),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect()
    }

    fn default_negative_words() -> HashMap<String, f64> {
        [
            ("bad", 0.5),
            ("terrible", 0.9),
            ("horrible", 0.9),
            ("awful", 0.8),
            ("poor", 0.5),
            ("wrong", 0.5),
            ("error", 0.6),
            ("fail", 0.7),
            ("failed", 0.7),
            ("failure", 0.8),
            ("broken", 0.7),
            ("bug", 0.5),
            ("crash", 0.8),
            ("crashed", 0.8),
            ("problem", 0.5),
            ("issue", 0.4),
            ("hate", 0.8),
            ("angry", 0.7),
            ("frustrated", 0.6),
            ("annoyed", 0.5),
            ("disappointed", 0.6),
            ("confusing", 0.5),
            ("confused", 0.5),
            ("difficult", 0.4),
            ("hard", 0.3),
            ("impossible", 0.7),
            ("slow", 0.4),
            ("ugly", 0.5),
            ("useless", 0.7),
            ("waste", 0.6),
            ("pain", 0.5),
            ("painful", 0.6),
            ("nightmare", 0.8),
            ("disaster", 0.9),
            ("catastrophe", 0.9),
            ("crisis", 0.7),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect()
    }

    fn default_negation_words() -> HashSet<String> {
        [
            "not",
            "no",
            "never",
            "neither",
            "nobody",
            "nothing",
            "nowhere",
            "dont",
            "doesn't",
            "didn't",
            "won't",
            "wouldn't",
            "couldn't",
            "shouldn't",
            "isn't",
            "aren't",
            "wasn't",
            "weren't",
            "cannot",
            "can't",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }
}

/// Result of sentiment analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentResult {
    /// Polarity from -1.0 (negative) to 1.0 (positive)
    pub polarity: f64,
    /// Intensity/magnitude of sentiment (0.0 to 1.0)
    pub magnitude: f64,
    /// Words that contributed to the sentiment
    pub contributing_words: Vec<String>,
}

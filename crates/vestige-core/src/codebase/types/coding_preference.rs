//! `CodingPreference` and `PreferenceSource`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Records a user's coding preferences for consistent suggestions.
///
/// Examples:
/// - "For error handling, prefer Result over panic"
/// - "For naming, use snake_case for functions"
/// - "For async, prefer tokio over async-std"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodingPreference {
    pub id: String,
    /// Context where this preference applies (e.g., "error handling", "naming")
    pub context: String,
    /// The preferred approach
    pub preference: String,
    /// What NOT to do (optional)
    pub counter_preference: Option<String>,
    /// Examples showing the preference in action
    pub examples: Vec<String>,
    /// Confidence in this preference (0.0 - 1.0)
    /// Higher confidence = more consistently applied
    pub confidence: f64,
    /// When this preference was recorded
    pub created_at: DateTime<Utc>,
    /// Language this applies to (None = all languages)
    pub language: Option<String>,
    /// How this preference was learned
    pub source: PreferenceSource,
    /// Number of times this preference has been observed
    pub observation_count: u32,
}

/// How a preference was learned
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceSource {
    /// Explicitly stated by user
    UserStated,
    /// Inferred from code review feedback
    CodeReview,
    /// Detected from coding patterns in history
    PatternDetection,
    /// From project configuration (e.g., rustfmt.toml)
    ProjectConfig,
}

impl CodingPreference {
    pub fn new(id: String, context: String, preference: String) -> Self {
        Self {
            id,
            context,
            preference,
            counter_preference: None,
            examples: vec![],
            confidence: 0.5,
            created_at: Utc::now(),
            language: None,
            source: PreferenceSource::UserStated,
            observation_count: 1,
        }
    }

    pub fn with_counter(mut self, counter: String) -> Self {
        self.counter_preference = Some(counter);
        self
    }

    pub fn with_examples(mut self, examples: Vec<String>) -> Self {
        self.examples = examples;
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

//! Output of an emotional evaluation and the `EmotionCategory` enum.

/// Result of emotional evaluation of content
#[derive(Debug, Clone)]
pub struct EmotionalEvaluation {
    /// Emotional valence: -1.0 (very negative) to 1.0 (very positive)
    pub valence: f64,
    /// Emotional arousal: 0.0 (calm) to 1.0 (extremely arousing)
    pub arousal: f64,
    /// Whether this triggers flashbulb encoding
    pub is_flashbulb: bool,
    /// Dominant emotion category
    pub category: EmotionCategory,
    /// Words that contributed to the evaluation
    pub contributing_words: Vec<String>,
    /// Confidence in the evaluation (0.0 to 1.0)
    pub confidence: f64,
}

/// Emotion categories for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmotionCategory {
    /// Joy, success, accomplishment
    Joy,
    /// Frustration, bugs, failures
    Frustration,
    /// Urgency, deadlines, critical issues
    Urgency,
    /// Discovery, learning, insight
    Surprise,
    /// Confusion, uncertainty
    Confusion,
    /// Neutral / no strong emotion
    Neutral,
}

impl EmotionCategory {
    /// Get the base arousal level for this category
    #[allow(dead_code)]
    fn base_arousal(&self) -> f64 {
        match self {
            Self::Joy => 0.6,
            Self::Frustration => 0.7,
            Self::Urgency => 0.9,
            Self::Surprise => 0.8,
            Self::Confusion => 0.4,
            Self::Neutral => 0.1,
        }
    }
}

impl std::fmt::Display for EmotionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Joy => write!(f, "joy"),
            Self::Frustration => write!(f, "frustration"),
            Self::Urgency => write!(f, "urgency"),
            Self::Surprise => write!(f, "surprise"),
            Self::Confusion => write!(f, "confusion"),
            Self::Neutral => write!(f, "neutral"),
        }
    }
}

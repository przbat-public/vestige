//! Detection output (`IntentDetectionResult`) and memory-query payload.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::intent_kinds::DetectedIntent;

/// Result of intent detection with confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentDetectionResult {
    /// Primary detected intent
    pub primary_intent: DetectedIntent,
    /// Confidence in primary intent (0.0 to 1.0)
    pub confidence: f64,
    /// Alternative intents with lower confidence
    pub alternatives: Vec<(DetectedIntent, f64)>,
    /// Evidence supporting the detection
    pub evidence: Vec<String>,
    /// When this detection was made
    pub detected_at: DateTime<Utc>,
}

/// Query parameters for finding memories relevant to an intent
#[derive(Debug, Clone)]
pub struct IntentMemoryQuery {
    /// Tags to search for
    pub tags: Vec<String>,
    /// Keywords to search for
    pub keywords: Vec<String>,
    /// Whether to boost recent memories
    pub recency_boost: bool,
}

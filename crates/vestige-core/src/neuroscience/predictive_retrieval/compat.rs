//! Backward-compatibility aliases and tiny adapters for older callers.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::engine::{PredictiveMemory, PredictiveMemoryConfig};
use super::types::{PredictedMemory, TemporalPatterns};

// ============================================================================
// BACKWARD COMPATIBILITY ALIASES
// ============================================================================

/// Alias for backward compatibility with existing code
pub type PredictiveRetriever = PredictiveMemory;

/// Alias for backward compatibility with existing code
pub type Prediction = PredictedMemory;

/// Prediction confidence level for backward compatibility
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PredictionConfidence {
    /// Very low confidence (< 0.2)
    VeryLow,
    /// Low confidence (0.2 - 0.4)
    Low,
    /// Medium confidence (0.4 - 0.6)
    Medium,
    /// High confidence (0.6 - 0.8)
    High,
    /// Very high confidence (> 0.8)
    VeryHigh,
}

impl PredictionConfidence {
    /// Create from a confidence score
    pub fn from_score(score: f64) -> Self {
        if score < 0.2 {
            Self::VeryLow
        } else if score < 0.4 {
            Self::Low
        } else if score < 0.6 {
            Self::Medium
        } else if score < 0.8 {
            Self::High
        } else {
            Self::VeryHigh
        }
    }

    /// Get the numeric range for this confidence level
    pub fn range(&self) -> (f64, f64) {
        match self {
            Self::VeryLow => (0.0, 0.2),
            Self::Low => (0.2, 0.4),
            Self::Medium => (0.4, 0.6),
            Self::High => (0.6, 0.8),
            Self::VeryHigh => (0.8, 1.0),
        }
    }
}

/// Sequence-based predictor for temporal access patterns
#[derive(Debug, Default)]
pub struct SequencePredictor {
    /// Recent access sequences
    sequences: Vec<Vec<String>>,
    /// Maximum sequence length
    max_length: usize,
}

impl SequencePredictor {
    /// Create a new sequence predictor
    pub fn new(max_length: usize) -> Self {
        Self {
            sequences: Vec::new(),
            max_length,
        }
    }

    /// Add an access to the sequence
    pub fn add_access(&mut self, memory_id: String) {
        if self.sequences.is_empty() {
            self.sequences.push(Vec::new());
        }

        if let Some(last) = self.sequences.last_mut() {
            last.push(memory_id);
            if last.len() > self.max_length {
                last.remove(0);
            }
        }
    }

    /// Predict next likely accesses
    pub fn predict_next(&self, _current_id: &str) -> Vec<(String, f64)> {
        // Simple implementation - return empty for now
        // Full implementation would use sequence matching
        Vec::new()
    }
}

/// Temporal predictor for time-based patterns
#[derive(Debug, Default)]
pub struct TemporalPredictor {
    /// Patterns by hour of day
    hourly_patterns: TemporalPatterns,
}

impl TemporalPredictor {
    /// Create a new temporal predictor
    pub fn new() -> Self {
        Self {
            hourly_patterns: TemporalPatterns::new(),
        }
    }

    /// Record an access at the current time
    pub fn record_access(&mut self, _memory_id: &str, topics: &[String]) {
        let now = Utc::now();
        for topic in topics {
            self.hourly_patterns.record_activity(now, topic, 0.5);
        }
    }

    /// Predict memories likely to be accessed now
    pub fn predict_for_time(&self, time: DateTime<Utc>) -> Vec<(String, f64)> {
        self.hourly_patterns.topics_for_time(time)
    }
}

/// Contextual predictor for context-based patterns
#[derive(Debug, Default)]
pub struct ContextualPredictor {
    /// Context-memory associations
    context_memories: HashMap<String, Vec<String>>,
}

impl ContextualPredictor {
    /// Create a new contextual predictor
    pub fn new() -> Self {
        Self::default()
    }

    /// Associate a memory with a context
    pub fn add_association(&mut self, context: &str, memory_id: String) {
        self.context_memories
            .entry(context.to_string())
            .or_default()
            .push(memory_id);
    }

    /// Predict memories for a given context
    pub fn predict_for_context(&self, context: &str) -> Vec<String> {
        self.context_memories
            .get(context)
            .cloned()
            .unwrap_or_default()
    }
}

/// Configuration for predictive retrieval (backward compatibility alias)
pub type PredictiveConfig = PredictiveMemoryConfig;

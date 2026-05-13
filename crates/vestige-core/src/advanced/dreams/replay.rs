//! Memory replay sequence and pattern types discovered during replay.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Result of memory replay during consolidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReplay {
    /// Memory IDs in replay order (chronological)
    pub sequence: Vec<String>,
    /// Synthetic combinations tested for connections
    pub synthetic_combinations: Vec<(String, String)>,
    /// Patterns discovered during replay
    pub discovered_patterns: Vec<Pattern>,
    /// When replay occurred
    pub replayed_at: DateTime<Utc>,
}

/// A discovered pattern from memory analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Unique pattern ID
    pub id: String,
    /// Type of pattern
    pub pattern_type: PatternType,
    /// Human-readable description
    pub description: String,
    /// Memory IDs that form this pattern
    pub memory_ids: Vec<String>,
    /// Confidence in this pattern (0.0 to 1.0)
    pub confidence: f64,
    /// When this pattern was discovered
    pub discovered_at: DateTime<Utc>,
}

/// Types of patterns that can be discovered
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatternType {
    /// Recurring theme across memories
    Recurring,
    /// Sequential pattern (A followed by B)
    Sequential,
    /// Co-occurrence pattern
    CoOccurrence,
    /// Temporal pattern (time-based)
    Temporal,
    /// Causal pattern
    Causal,
}

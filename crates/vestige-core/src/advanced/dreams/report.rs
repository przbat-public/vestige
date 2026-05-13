//! Per-cycle consolidation report aggregating results from all stages.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::replay::MemoryReplay;
use super::types::DreamResult;

/// Report from a consolidation cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    /// Stage 1: Memory replay results
    pub stage1_replay: Option<MemoryReplay>,
    /// Stage 2: Number of cross-references found
    pub stage2_connections: usize,
    /// Stage 3: Number of connections strengthened
    pub stage3_strengthened: usize,
    /// Stage 4: Number of connections pruned
    pub stage4_pruned: usize,
    /// Stage 5: Memory IDs transferred to semantic storage
    pub stage5_transferred: Vec<String>,
    /// Dream cycle results
    pub dream_result: Option<DreamResult>,
    /// Total duration in milliseconds
    pub duration_ms: u64,
    /// When consolidation completed
    pub completed_at: DateTime<Utc>,
}

impl ConsolidationReport {
    /// Create a new empty report
    pub fn new() -> Self {
        Self {
            stage1_replay: None,
            stage2_connections: 0,
            stage3_strengthened: 0,
            stage4_pruned: 0,
            stage5_transferred: Vec::new(),
            dream_result: None,
            duration_ms: 0,
            completed_at: Utc::now(),
        }
    }

    /// Get total insights generated
    pub fn total_insights(&self) -> usize {
        self.dream_result
            .as_ref()
            .map(|r| r.insights_generated.len())
            .unwrap_or(0)
    }

    /// Get total new connections discovered
    pub fn total_new_connections(&self) -> usize {
        self.stage2_connections
            + self
                .dream_result
                .as_ref()
                .map(|r| r.new_connections_found)
                .unwrap_or(0)
    }
}

impl Default for ConsolidationReport {
    fn default() -> Self {
        Self::new()
    }
}

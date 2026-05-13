//! Aggregated gate statistics.

use serde::{Deserialize, Serialize};

/// Statistics about gate decisions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GateStats {
    /// Total evaluations performed
    pub total_evaluations: usize,
    /// Decisions to create new
    pub creates: usize,
    /// Decisions to update existing
    pub updates: usize,
    /// Decisions to supersede
    pub supersedes: usize,
    /// Decisions to merge
    pub merges: usize,
}

impl GateStats {
    /// Get create rate
    pub fn create_rate(&self) -> f64 {
        if self.total_evaluations > 0 {
            self.creates as f64 / self.total_evaluations as f64
        } else {
            0.0
        }
    }

    /// Get update rate
    pub fn update_rate(&self) -> f64 {
        if self.total_evaluations > 0 {
            self.updates as f64 / self.total_evaluations as f64
        } else {
            0.0
        }
    }

    /// Get supersede rate
    pub fn supersede_rate(&self) -> f64 {
        if self.total_evaluations > 0 {
            self.supersedes as f64 / self.total_evaluations as f64
        } else {
            0.0
        }
    }
}

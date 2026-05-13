//! Aggregated reconsolidation statistics.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconsolidationStats {
    /// Total memories marked labile
    pub total_marked_labile: usize,
    /// Total memories reconsolidated
    pub total_reconsolidated: usize,
    /// Total memories modified during labile window
    pub total_modified: usize,
    /// Total modifications applied
    pub total_modifications: usize,
}

impl ReconsolidationStats {
    /// Get modification rate (modifications per labile memory)
    pub fn modification_rate(&self) -> f64 {
        if self.total_marked_labile > 0 {
            self.total_modifications as f64 / self.total_marked_labile as f64
        } else {
            0.0
        }
    }

    /// Get modified rate (% of labile memories that were modified)
    pub fn modified_rate(&self) -> f64 {
        if self.total_reconsolidated > 0 {
            self.total_modified as f64 / self.total_reconsolidated as f64
        } else {
            0.0
        }
    }
}

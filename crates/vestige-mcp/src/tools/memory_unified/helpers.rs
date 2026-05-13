//! Accessibility constants and the four-tier state classifier.

use vestige_core::MemoryState;

pub const ACCESSIBILITY_ACTIVE: f64 = 0.7;
pub const ACCESSIBILITY_DORMANT: f64 = 0.4;
pub const ACCESSIBILITY_SILENT: f64 = 0.1;

/// Weighted accessibility: retention 50%, retrieval 30%, storage 20%.
pub fn compute_accessibility(retention: f64, retrieval: f64, storage: f64) -> f64 {
    retention * 0.5 + retrieval * 0.3 + storage * 0.2
}

pub fn state_from_accessibility(accessibility: f64) -> MemoryState {
    if accessibility >= ACCESSIBILITY_ACTIVE {
        MemoryState::Active
    } else if accessibility >= ACCESSIBILITY_DORMANT {
        MemoryState::Dormant
    } else if accessibility >= ACCESSIBILITY_SILENT {
        MemoryState::Silent
    } else {
        MemoryState::Unavailable
    }
}

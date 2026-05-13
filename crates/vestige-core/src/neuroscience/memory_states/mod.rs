//! # Memory States System
//!
//! Implements the neuroscience concept that memories exist in different accessibility states.
//!
//! ## Background
//!
//! Modern memory science recognizes that memories don't simply "exist" or "not exist" -
//! they exist on a continuum of accessibility. A memory might be:
//!
//! - **Active**: Currently in working memory, immediately accessible
//! - **Dormant**: Easily retrievable with partial cues (like remembering a friend's name)
//! - **Silent**: Exists but requires strong/specific cues (like childhood memories)
//! - **Unavailable**: Temporarily blocked due to interference or suppression
//!
//! ## Key Phenomena Modeled
//!
//! 1. **State Decay**: Active memories naturally decay to Dormant, then Silent over time
//! 2. **Reactivation**: Strong cue matches can reactivate Silent memories
//! 3. **Retrieval-Induced Forgetting (RIF)**: Retrieving one memory can suppress related competitors
//! 4. **Interference**: Similar memories compete, with winners strengthening and losers weakening
//!
//! ## References
//!
//! - Bjork, R. A., & Bjork, E. L. (1992). A new theory of disuse and an old theory of stimulus fluctuation.
//! - Anderson, M. C., Bjork, R. A., & Bjork, E. L. (1994). Remembering can cause forgetting.
//! - Tulving, E. (1974). Cue-dependent forgetting. American Scientist.

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default time (in hours) before Active memories decay to Dormant
pub const DEFAULT_ACTIVE_DECAY_HOURS: i64 = 4;

/// Default time (in days) before Dormant memories decay to Silent
pub const DEFAULT_DORMANT_DECAY_DAYS: i64 = 30;

/// Base accessibility multiplier for Active state
pub const ACCESSIBILITY_ACTIVE: f64 = 1.0;

/// Base accessibility multiplier for Dormant state
pub const ACCESSIBILITY_DORMANT: f64 = 0.7;

/// Base accessibility multiplier for Silent state
pub const ACCESSIBILITY_SILENT: f64 = 0.3;

/// Base accessibility multiplier for Unavailable state
pub const ACCESSIBILITY_UNAVAILABLE: f64 = 0.05;

/// Minimum similarity threshold for competition to occur
pub const COMPETITION_SIMILARITY_THRESHOLD: f64 = 0.6;

/// Suppression strength applied to losers in retrieval competition
pub const COMPETITION_SUPPRESSION_FACTOR: f64 = 0.15;

/// Maximum number of state transitions to keep in history
pub const MAX_STATE_HISTORY_SIZE: usize = 50;

/// Maximum number of competition events to track
pub const MAX_COMPETITION_HISTORY_SIZE: usize = 100;

mod accessibility;
mod competition;
mod lifecycle;
mod service;
mod states;

#[cfg(test)]
mod tests;

pub use accessibility::{AccessibilityCalculator, MemoryStateInfo};
pub use competition::{
    CompetitionCandidate, CompetitionConfig, CompetitionManager, CompetitionResult,
};
pub use lifecycle::{
    LifecycleSummary, MemoryLifecycle, StateDecayConfig, StatePercentages, StateTimeAccumulator,
};
pub use service::{BatchUpdateResult, StateUpdateService};
pub use states::{CompetitionEvent, MemoryState, StateTransition, StateTransitionReason};

//! # Synaptic Tagging and Capture (STC)
//!
//! Implements the neuroscience finding that memories can become important RETROACTIVELY
//! based on subsequent events. This is a fundamental capability that distinguishes
//! biological memory from traditional AI memory systems.
//!
//! ## The Neuroscience
//!
//! Synaptic Tagging and Capture (STC) explains how memories can be consolidated
//! hours after initial encoding:
//!
//! 1. **Weak stimulation** creates a "synaptic tag" - a temporary molecular marker
//! 2. **Strong stimulation** (important event) triggers production of
//!    Plasticity-Related Products (PRPs)
//! 3. **PRPs can be captured** by tagged synapses within a temporal window
//! 4. **Captured memories** are consolidated to long-term storage
//!
//! > "Successful STC is observed even with a 9-hour interval between weak and strong
//! > stimulation, suggesting a broader temporal flexibility for tag-PRP interactions."
//! > - Redondo & Morris (2011)
//!
//! ## Why This Matters for AI
//!
//! Traditional AI memory systems determine importance at encoding time. But in reality:
//! - A conversation about a coworker's vacation might seem trivial
//! - Hours later, you learn that coworker is leaving the company
//! - Suddenly, that vacation conversation becomes important context
//!
//! STC enables this retroactive importance assignment - something no other AI memory
//! system does.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use vestige_core::neuroscience::{SynapticTaggingSystem, ImportanceEvent, ImportanceEventType};
//! use chrono::Utc;
//!
//! let mut stc = SynapticTaggingSystem::new();
//!
//! // Memory is encoded (automatically tagged)
//! stc.tag_memory("mem-123");
//!
//! // Hours later, user explicitly flags something as important
//! let captured = stc.trigger_prp(ImportanceEvent {
//!     event_type: ImportanceEventType::UserFlag,
//!     memory_id: Some("mem-456".to_string()),
//!     timestamp: Utc::now(),
//!     strength: 1.0,
//!     context: Some("User said 'remember this'".to_string()),
//! });
//!
//! // PRPs sweep backward through time, capturing tagged memories
//! for memory in captured {
//!     println!("Retroactively consolidated: {}", memory.memory_id);
//! }
//! ```
//!
//! ## Configuration
//!
//! The system is highly configurable to match different use cases:
//!
//! - **Capture Window**: How far back/forward to look for tagged memories
//! - **Decay Function**: How tag strength decays over time
//! - **PRP Threshold**: Minimum importance to trigger PRP production
//! - **Cluster Settings**: How to group related important memories

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default backward capture window (hours) - based on neuroscience research
/// showing successful STC even with 9-hour intervals
pub(super) const DEFAULT_BACKWARD_HOURS: f64 = 9.0;

/// Default forward capture window (hours) - smaller since we're looking ahead
pub(super) const DEFAULT_FORWARD_HOURS: f64 = 2.0;

/// Default tag lifetime before complete decay (hours)
pub(super) const DEFAULT_TAG_LIFETIME_HOURS: f64 = 12.0;

/// Default PRP threshold - minimum importance to trigger PRP production
pub(super) const DEFAULT_PRP_THRESHOLD: f64 = 0.7;

/// Default minimum tag strength for capture
pub(super) const DEFAULT_MIN_TAG_STRENGTH: f64 = 0.3;

/// Maximum importance cluster size
pub(super) const DEFAULT_MAX_CLUSTER_SIZE: usize = 50;

mod decay;
mod engine;
mod events;
mod tag;

#[cfg(test)]
mod tests;

pub use decay::DecayFunction;
pub use engine::{SynapticTaggingConfig, SynapticTaggingSystem, TaggingStats};
pub use events::{
    CaptureResult, CapturedMemory, ImportanceCluster, ImportanceEvent, ImportanceEventType,
};
pub use tag::{CaptureWindow, SynapticTag};

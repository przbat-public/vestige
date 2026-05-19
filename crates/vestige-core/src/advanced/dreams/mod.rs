//! # Memory Dreams (Enhanced Consolidation)
//!
//! Enhanced sleep-inspired consolidation that creates NEW insights from
//! existing memories. Like how the brain consolidates and generates novel
//! connections during sleep, Memory Dreams finds hidden patterns and
//! synthesizes new knowledge.
//!
//! ## Dream Cycle (Sleep Stages)
//!
//! 1. **Stage 1 - Replay**: Replay recent memories in sequence
//! 2. **Stage 2 - Cross-reference**: Find connections with existing knowledge
//! 3. **Stage 3 - Strengthen**: Reinforce connections that fire together
//! 4. **Stage 4 - Prune**: Remove weak connections not reactivated
//! 5. **Stage 5 - Transfer**: Move consolidated from episodic to semantic
//!
//! ## Module Layout
//!
//! - `activity` — `ActivityTracker` and `ActivityStats`
//! - `scheduler` — `ConsolidationScheduler` (5-stage pipeline)
//! - `replay` — `MemoryReplay`, `Pattern`, `PatternType`
//! - `connection_graph` — `ConnectionGraph`, `MemoryConnection`, `ConnectionReason`, `ConnectionStats`
//! - `report` — `ConsolidationReport`
//! - `similarity` — Cosine, Jaccard, word-overlap and word-boundary helpers
//! - `types` — `DreamResult`, `DreamConfig`, `SynthesizedInsight`, `DreamMemory` and friends
//! - `dreamer` — `MemoryDreamer` (insight generation, contradiction detection)

pub(crate) mod constants;

mod activity;
mod connection_graph;
mod dreamer;
mod dreamer_clustering;
mod dreamer_connections;
mod dreamer_contradictions;
mod dreamer_hubs;
mod dreamer_insights;
mod dreamer_lifecycle;
mod replay;
mod report;
mod scheduler;
pub(crate) mod similarity;
mod types;

#[cfg(test)]
mod tests;

pub use activity::{ActivityStats, ActivityTracker};
pub use connection_graph::{ConnectionGraph, ConnectionReason, ConnectionStats, MemoryConnection};
pub use dreamer::MemoryDreamer;
pub use replay::{MemoryReplay, Pattern, PatternType};
pub use report::ConsolidationReport;
pub use scheduler::ConsolidationScheduler;
pub use types::{
    ContradictionPair, DiscoveredConnection, DiscoveredConnectionType, DreamConfig, DreamMemory,
    DreamResult, DreamStats, HubCandidate, InsightType, SynthesizedInsight,
};

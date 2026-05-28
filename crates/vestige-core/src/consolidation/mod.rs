//! Memory Consolidation Module
//!
//! Implements sleep-inspired memory consolidation:
//! - Decay weak memories
//! - Promote emotional/important memories
//! - Generate embeddings
//! - Prune very weak memories (optional)
//! - 4-Phase biologically-accurate dream cycle (v2.0)

pub mod phases;
mod sleep;

pub use phases::{
    CreativeConnection, CreativeConnectionType, DEFAULT_NREM3_DOWNSCALE_FACTOR, DreamEngine,
    DreamInsight, DreamPhase, FourPhaseDreamResult, PhaseResult, TriageCategory, TriagedMemory,
    default_nrem3_downscale_factor,
};
pub use sleep::SleepConsolidation;

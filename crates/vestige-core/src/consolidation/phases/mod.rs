//! 4-Phase Biologically-Accurate Dream Cycle
//!
//! Implements a neuroscience-grounded sleep cycle based on:
//! - **NREM1 (Light Sleep / Triage)**: Score & categorize memories, build replay queue
//! - **NREM3 (Deep Sleep / Consolidation)**: SO-spindle-ripple coupling, FSRS decay, synaptic downscaling
//! - **REM (Dreaming / Creative)**: Cross-domain pairing, pattern extraction, emotional processing
//! - **Integration (Pre-Wake)**: Validate insights, store new nodes, generate report
//!
//! References:
//! - Diekelmann & Born (2010): Active system consolidation during NREM
//! - Stickgold & Walker (2013): REM creativity and abstraction
//! - Tononi & Cirelli (2006): Synaptic homeostasis (downscaling)
//! - Frey & Morris (1997): Synaptic tag-and-capture

mod engine;
mod integration;
mod nrem1;
mod nrem3;
mod rem;
mod types;

#[cfg(test)]
mod tests;

pub use engine::DreamEngine;
pub use types::{
    CreativeConnection, CreativeConnectionType, DreamInsight, DreamPhase, FourPhaseDreamResult,
    PhaseResult, TriageCategory, TriagedMemory,
};

//! # Prediction Error Gating
//!
//! Implements neuroscience-inspired prediction error gating for intelligent
//! memory updates.
//!
//! Based on research showing that prediction error (PE) determines whether
//! memories are:
//! - **Updated** (small PE): New info is similar enough to existing memory
//! - **Created** (large PE): New info is different enough to warrant new memory
//!
//! ## Module Layout
//!
//! - `constants` — tunable thresholds (similarity, correction, candidate cap)
//! - `decision` — `GateDecision`, `CreateReason`, `UpdateType`,
//!   `SupersedeReason`, `MergeStrategy`, `GateFinding`
//! - `candidate` — `CandidateMemory`, `SimilarityResult`
//! - `stats` — `GateStats`
//! - `similarity` — `cosine_similarity` helper
//! - `gate` — `PredictionErrorGate`, `PredictionErrorConfig`,
//!   `EvaluationIntent` (the public API)
//!
//! ## Scientific Background
//!
//! Based on:
//! - Sinclair & Bhavnani (2020): "The Reconsolidation Dilemma"
//! - Lee et al. (2017): Prediction error and memory updating
//! - Google Titans (2025): Surprise-based storage

mod candidate;
mod constants;
mod decision;
mod gate;
mod similarity;
mod stats;

#[cfg(test)]
mod tests;

pub use candidate::{CandidateMemory, SimilarityResult};
pub use decision::{
    CreateReason, GateDecision, GateFinding, MergeStrategy, SupersedeReason, UpdateType,
};
pub use gate::{EvaluationIntent, PredictionErrorConfig, PredictionErrorGate};
pub use similarity::cosine_similarity;
pub use stats::GateStats;

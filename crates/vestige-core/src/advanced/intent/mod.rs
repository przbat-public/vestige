//! # Intent Detection
//!
//! Understand WHY the user is doing something, not just WHAT they're doing.
//! This allows Vestige to provide proactively relevant memories based on the
//! underlying goal.
//!
//! ## Module Layout
//!
//! - `constants` — detection thresholds (history size, time window, min confidence)
//! - `intent_kinds` — `DetectedIntent` and the auxiliary enums
//!   (`MaintenanceType`, `LearningLevel`, `ReviewDepth`, `OptimizationType`)
//! - `actions` — `UserAction` (single observation) and the `ActionType` enum
//! - `result` — `IntentDetectionResult`, `IntentMemoryQuery`
//! - `detector` — `IntentDetector` (the public scoring engine)
//!
//! ## How It Works
//!
//! 1. Analyzes recent user actions (file opens, searches, edits)
//! 2. Identifies patterns that suggest intent
//! 3. Returns intent with confidence and supporting evidence
//! 4. Retrieves memories relevant to detected intent

mod actions;
mod constants;
mod detector;
mod intent_kinds;
mod result;

#[cfg(test)]
mod tests;

pub use actions::{ActionType, UserAction};
pub use detector::IntentDetector;
pub use intent_kinds::{
    DetectedIntent, LearningLevel, MaintenanceType, OptimizationType, ReviewDepth,
};
pub use result::{IntentDetectionResult, IntentMemoryQuery};

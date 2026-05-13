//! Tunable thresholds for the prediction-error gate.

/// Default similarity threshold for considering memories as "similar".
/// Above this = potential update candidate.
pub(super) const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.75;

/// Threshold for considering content as "nearly identical".
/// Above this = definitely update, not create.
pub(super) const NEAR_IDENTICAL_THRESHOLD: f32 = 0.92;

/// Threshold for "correction" detection: new content contradicts existing
/// with high similarity.
pub(super) const CORRECTION_THRESHOLD: f32 = 0.70;

/// Maximum candidates to consider for update.
pub(super) const MAX_UPDATE_CANDIDATES: usize = 5;

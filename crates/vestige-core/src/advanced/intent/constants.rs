//! Tunable thresholds for intent detection.

/// Maximum actions to keep in history
pub(super) const MAX_ACTION_HISTORY: usize = 100;

/// Time window for intent detection (minutes)
pub(super) const INTENT_WINDOW_MINUTES: i64 = 30;

/// Minimum confidence for intent detection
pub(super) const MIN_INTENT_CONFIDENCE: f64 = 0.4;

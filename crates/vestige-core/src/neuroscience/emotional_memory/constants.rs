//! Tunable thresholds for the emotional-memory module.

/// Flashbulb detection thresholds (Brown & Kulik 1977)
pub(super) const FLASHBULB_NOVELTY_THRESHOLD: f64 = 0.7;
pub(super) const FLASHBULB_AROUSAL_THRESHOLD: f64 = 0.6;

/// Tag-and-capture window (Frey & Morris 1997)
pub(super) const CAPTURE_WINDOW_MINUTES: i64 = 30;
pub(super) const CAPTURE_BOOST: f64 = 0.05;

/// Emotional decay modulation (LaBar & Cabeza 2006)
/// FSRS stability multiplier: stability * (1.0 + EMOTIONAL_DECAY_FACTOR * arousal)
pub(super) const EMOTIONAL_DECAY_FACTOR: f64 = 0.3;

/// Mood-congruent retrieval boost
pub(super) const MOOD_CONGRUENCE_BOOST: f64 = 0.15;
pub(super) const MOOD_CONGRUENCE_THRESHOLD: f64 = 0.3;

/// Maximum number of recent emotions to track for mood state
pub(super) const MOOD_HISTORY_CAPACITY: usize = 20;

//! Tunable constants used by every stage of the dream consolidation pipeline.

/// Minimum similarity for connection discovery
pub(crate) const MIN_SIMILARITY_FOR_CONNECTION: f64 = 0.5;

/// Maximum insights to generate per dream cycle
pub(crate) const MAX_INSIGHTS_PER_DREAM: usize = 10;

/// Minimum novelty score for insights
pub(crate) const MIN_NOVELTY_SCORE: f64 = 0.3;

/// Minimum memories needed for insight generation
pub(crate) const MIN_MEMORIES_FOR_INSIGHT: usize = 2;

/// Default consolidation interval (6 hours)
pub(crate) const DEFAULT_CONSOLIDATION_INTERVAL_HOURS: i64 = 6;

/// Default activity window for tracking (5 minutes)
pub(crate) const DEFAULT_ACTIVITY_WINDOW_SECS: i64 = 300;

/// Minimum idle time before consolidation can run (30 minutes)
pub(crate) const MIN_IDLE_TIME_FOR_CONSOLIDATION_MINS: i64 = 30;

/// Minimum brief idle time for force/mini consolidation triggers (5 minutes)
pub(crate) const MIN_BRIEF_IDLE_MINS: i64 = 5;

/// Connection strength decay factor
pub(crate) const CONNECTION_DECAY_FACTOR: f64 = 0.95;

/// Minimum connection strength to keep
pub(crate) const MIN_CONNECTION_STRENGTH: f64 = 0.1;

/// Maximum memories to replay per cycle
pub(crate) const MAX_REPLAY_MEMORIES: usize = 100;

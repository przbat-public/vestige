//! Tunable constants used across the reconsolidation pipeline.

/// Default labile window duration (5 minutes)
pub(super) const DEFAULT_LABILE_WINDOW_SECS: i64 = 300;

/// Maximum modifications per memory during labile window
pub(super) const MAX_MODIFICATIONS_PER_WINDOW: usize = 10;

/// How long to keep retrieval history
pub(super) const RETRIEVAL_HISTORY_DAYS: i64 = 30;

//! Per-memory emotional encoding record (used by tag-and-capture).

use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub(super) struct EmotionalRecord {
    pub memory_id: String,
    #[allow(dead_code)]
    pub valence: f64,
    #[allow(dead_code)]
    pub arousal: f64,
    pub encoded_at: DateTime<Utc>,
}

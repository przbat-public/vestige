//! Temporal context: bucketed time-of-day and recency tags plus the
//! [`TemporalContext`] capture used at encoding time.

use chrono::{DateTime, Datelike, Duration, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};

// ============================================================================
// TIME OF DAY
// ============================================================================

/// Time of day categories for temporal context matching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    /// 5:00 AM - 11:59 AM
    Morning,
    /// 12:00 PM - 4:59 PM
    Afternoon,
    /// 5:00 PM - 8:59 PM
    Evening,
    /// 9:00 PM - 4:59 AM
    Night,
}

impl TimeOfDay {
    /// Determine time of day from a timestamp
    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        let hour = dt.hour();
        match hour {
            5..=11 => Self::Morning,
            12..=16 => Self::Afternoon,
            17..=20 => Self::Evening,
            _ => Self::Night,
        }
    }

    /// Get the current time of day
    pub fn now() -> Self {
        Self::from_datetime(Utc::now())
    }

    /// Check if two time-of-day values are adjacent (within one period)
    pub fn is_adjacent(&self, other: &Self) -> bool {
        use TimeOfDay::*;
        matches!(
            (self, other),
            (Morning, Afternoon)
                | (Afternoon, Morning)
                | (Afternoon, Evening)
                | (Evening, Afternoon)
                | (Evening, Night)
                | (Night, Evening)
                | (Night, Morning)
                | (Morning, Night)
        )
    }

    /// Human-readable name
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
        }
    }
}

// ============================================================================
// RECENCY BUCKET
// ============================================================================

/// Recency categories for temporal context matching
///
/// Based on memory research showing that temporal context decays over time
/// but in discrete "chunks" rather than continuously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RecencyBucket {
    /// Within the last hour
    VeryRecent,
    /// Within the last day (1-24 hours)
    Today,
    /// Within the last week (1-7 days)
    ThisWeek,
    /// Within the last month (1-4 weeks)
    ThisMonth,
    /// Within the last quarter (1-3 months)
    ThisQuarter,
    /// Within the last year (3-12 months)
    ThisYear,
    /// Older than a year
    Older,
}

impl RecencyBucket {
    /// Determine recency bucket from a timestamp
    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        let now = Utc::now();
        let age = now.signed_duration_since(dt);

        if age < Duration::hours(1) {
            Self::VeryRecent
        } else if age < Duration::hours(24) {
            Self::Today
        } else if age < Duration::days(7) {
            Self::ThisWeek
        } else if age < Duration::days(30) {
            Self::ThisMonth
        } else if age < Duration::days(90) {
            Self::ThisQuarter
        } else if age < Duration::days(365) {
            Self::ThisYear
        } else {
            Self::Older
        }
    }

    /// Check if two recency buckets are within one step of each other
    pub fn is_adjacent(&self, other: &Self) -> bool {
        let self_ord = *self as i32;
        let other_ord = *other as i32;
        (self_ord - other_ord).abs() <= 1
    }

    /// Human-readable description
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VeryRecent => "very recent (< 1 hour)",
            Self::Today => "today",
            Self::ThisWeek => "this week",
            Self::ThisMonth => "this month",
            Self::ThisQuarter => "this quarter",
            Self::ThisYear => "this year",
            Self::Older => "older than a year",
        }
    }
}

// ============================================================================
// TEMPORAL CONTEXT
// ============================================================================

/// Temporal context captures WHEN a memory was encoded
///
/// Research shows that temporal context is a powerful retrieval cue.
/// Memories encoded at the same time of day, day of week, or in the
/// same temporal "neighborhood" are more likely to be recalled together.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporalContext {
    /// Exact timestamp of encoding
    pub timestamp: DateTime<Utc>,
    /// Categorized time of day
    pub time_of_day: TimeOfDay,
    /// Day of the week
    pub day_of_week: Weekday,
    /// Recency bucket (computed dynamically at retrieval)
    pub recency_bucket: RecencyBucket,
}

impl TemporalContext {
    /// Create a new temporal context from a timestamp
    pub fn new(timestamp: DateTime<Utc>) -> Self {
        Self {
            timestamp,
            time_of_day: TimeOfDay::from_datetime(timestamp),
            day_of_week: timestamp.weekday(),
            recency_bucket: RecencyBucket::from_datetime(timestamp),
        }
    }

    /// Capture the current temporal context
    pub fn now() -> Self {
        Self::new(Utc::now())
    }

    /// Update the recency bucket (should be called at retrieval time)
    pub fn refresh_recency(&mut self) {
        self.recency_bucket = RecencyBucket::from_datetime(self.timestamp);
    }

    /// Check if this is a weekday
    pub fn is_weekday(&self) -> bool {
        !matches!(self.day_of_week, Weekday::Sat | Weekday::Sun)
    }

    /// Check if this is a weekend
    pub fn is_weekend(&self) -> bool {
        matches!(self.day_of_week, Weekday::Sat | Weekday::Sun)
    }
}

impl Default for TemporalContext {
    fn default() -> Self {
        Self::now()
    }
}

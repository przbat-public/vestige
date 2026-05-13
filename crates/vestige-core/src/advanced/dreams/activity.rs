//! User-activity tracking and idle detection.

use std::collections::VecDeque;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::constants::{DEFAULT_ACTIVITY_WINDOW_SECS, MIN_IDLE_TIME_FOR_CONSOLIDATION_MINS};

/// Tracks user activity to detect low-activity periods
#[derive(Debug, Clone)]
pub struct ActivityTracker {
    /// Recent activity timestamps
    activity_log: VecDeque<DateTime<Utc>>,
    /// Maximum activity log size
    max_log_size: usize,
    /// Activity window duration for rate calculation
    activity_window: Duration,
}

impl Default for ActivityTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityTracker {
    /// Create a new activity tracker
    pub fn new() -> Self {
        Self {
            activity_log: VecDeque::with_capacity(1000),
            max_log_size: 1000,
            activity_window: Duration::seconds(DEFAULT_ACTIVITY_WINDOW_SECS),
        }
    }

    /// Record an activity event
    pub fn record_activity(&mut self) {
        let now = Utc::now();
        self.activity_log.push_back(now);

        // Trim old entries
        while self.activity_log.len() > self.max_log_size {
            self.activity_log.pop_front();
        }
    }

    /// Get activity rate (events per minute) in the recent window
    pub fn activity_rate(&self) -> f64 {
        let now = Utc::now();
        let window_start = now - self.activity_window;

        let recent_count = self
            .activity_log
            .iter()
            .filter(|&&t| t >= window_start)
            .count();

        let window_minutes = self.activity_window.num_seconds() as f64 / 60.0;
        if window_minutes > 0.0 {
            recent_count as f64 / window_minutes
        } else {
            0.0
        }
    }

    /// Get time since last activity
    pub fn time_since_last_activity(&self) -> Option<Duration> {
        self.activity_log.back().map(|&last| Utc::now() - last)
    }

    /// Check if system is idle (no recent activity)
    pub fn is_idle(&self) -> bool {
        self.time_since_last_activity()
            .map(|d| d >= Duration::minutes(MIN_IDLE_TIME_FOR_CONSOLIDATION_MINS))
            .unwrap_or(true) // No activity ever = idle
    }

    /// Get activity statistics
    pub fn get_stats(&self) -> ActivityStats {
        ActivityStats {
            total_events: self.activity_log.len(),
            events_per_minute: self.activity_rate(),
            last_activity: self.activity_log.back().copied(),
            is_idle: self.is_idle(),
        }
    }
}

/// Activity statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityStats {
    /// Total activity events tracked
    pub total_events: usize,
    /// Current activity rate (events per minute)
    pub events_per_minute: f64,
    /// Timestamp of last activity
    pub last_activity: Option<DateTime<Utc>>,
    /// Whether system is currently idle
    pub is_idle: bool,
}

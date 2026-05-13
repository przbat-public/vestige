//! Attention signal — acetylcholine-like focus / learning-mode detector.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::{LEARNING_MODE_TIMEOUT_MINUTES, MAX_IMPORTANCE, MIN_IMPORTANCE};

// ============================================================================
// ATTENTION SIGNAL (Acetylcholine-like: Focus & Learning Mode)
// ============================================================================

/// Attention signal inspired by acetylcholine's role in attention and learning.
///
/// In the brain, the basal forebrain releases acetylcholine during focused
/// attention and active learning, which gates plasticity in cortical circuits.
/// This signal detects when the user is in an active learning/focused state.
///
/// ## Detection Methods
///
/// 1. **Access Patterns**: Frequent, focused access suggests active learning
/// 2. **Session Analysis**: Sustained engagement indicates focused state
/// 3. **Query Patterns**: Exploratory queries suggest learning mode
#[derive(Debug)]
pub struct AttentionSignal {
    /// Focus detector for analyzing access patterns
    focus_detector: FocusDetector,
    /// Whether learning mode is currently active
    learning_mode_active: Arc<RwLock<bool>>,
    /// Recent sessions for learning mode detection
    sessions: Arc<RwLock<VecDeque<Session>>>,
}

impl Default for AttentionSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl AttentionSignal {
    /// Create a new attention signal detector
    pub fn new() -> Self {
        Self {
            focus_detector: FocusDetector::new(),
            learning_mode_active: Arc::new(RwLock::new(false)),
            sessions: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
        }
    }

    /// Compute attention score from access pattern
    pub fn compute(&self, access_pattern: &AccessPattern) -> f64 {
        let focus_score = self.focus_detector.compute_focus(access_pattern);
        let learning_mode = self.is_learning_mode();

        // Boost score if in learning mode
        let base_score = focus_score;
        let learning_boost = if learning_mode { 0.2 } else { 0.0 };

        (base_score + learning_boost).clamp(MIN_IMPORTANCE, MAX_IMPORTANCE)
    }

    /// Record a session activity
    pub fn record_session_activity(&self, session: Session) {
        if let Ok(mut sessions) = self.sessions.write() {
            sessions.push_back(session);

            // Keep only recent sessions
            while sessions.len() > 100 {
                sessions.pop_front();
            }
        }

        // Update learning mode based on sessions
        self.update_learning_mode();
    }

    /// Check if user is in learning mode
    pub fn is_learning_mode(&self) -> bool {
        self.learning_mode_active
            .read()
            .map(|m| *m)
            .unwrap_or_else(|_| {
                tracing::warn!("AttentionSignal: learning_mode RwLock poisoned");
                false
            })
    }

    /// Detect learning mode from a session
    pub fn detect_learning_mode(&self, session: &Session) -> bool {
        // High query frequency suggests active exploration
        let high_query_rate = session.query_count as f64 / session.duration_minutes.max(1.0) > 2.0;

        // Diverse access patterns suggest learning
        let diverse_access = session.unique_memories_accessed > 5;

        // Low edit ratio (more reading than writing) suggests learning
        let reading_mode = (session.edit_count as f64 / session.query_count.max(1) as f64) < 0.3;

        // Long session duration suggests engagement
        let sustained = session.duration_minutes > 15.0;

        (high_query_rate as u8 + diverse_access as u8 + reading_mode as u8 + sustained as u8) >= 2
    }

    /// Get explanation for attention score
    pub fn explain(&self, access_pattern: &AccessPattern) -> AttentionExplanation {
        let score = self.compute(access_pattern);
        let learning_mode = self.is_learning_mode();
        let focus_metrics = self.focus_detector.get_focus_metrics(access_pattern);

        AttentionExplanation {
            score,
            learning_mode_active: learning_mode,
            access_frequency: focus_metrics.access_frequency,
            session_depth: focus_metrics.session_depth,
            query_diversity: focus_metrics.query_diversity,
        }
    }

    /// Set learning mode manually (for external triggers)
    pub fn set_learning_mode(&self, active: bool) {
        if let Ok(mut mode) = self.learning_mode_active.write() {
            *mode = active;
        }
    }

    fn update_learning_mode(&self) {
        let sessions = match self.sessions.read() {
            Ok(s) => s,
            Err(_) => return,
        };

        let now = Utc::now();
        let cutoff = now - Duration::minutes(LEARNING_MODE_TIMEOUT_MINUTES);

        // Check recent sessions for learning indicators
        let recent_sessions: Vec<_> = sessions.iter().filter(|s| s.start_time > cutoff).collect();

        let learning_sessions = recent_sessions
            .iter()
            .filter(|s| self.detect_learning_mode(s))
            .count();

        let is_learning = !recent_sessions.is_empty()
            && learning_sessions as f64 / recent_sessions.len() as f64 > 0.5;

        if let Ok(mut mode) = self.learning_mode_active.write() {
            *mode = is_learning;
        }
    }
}

/// Access pattern data for attention analysis
#[derive(Debug, Clone, Default)]
pub struct AccessPattern {
    /// Memory IDs accessed in this pattern
    pub memory_ids: Vec<String>,
    /// Time between accesses (seconds)
    pub inter_access_times: Vec<f64>,
    /// Queries made
    pub queries: Vec<String>,
    /// Total duration of this access pattern (seconds)
    pub duration_seconds: f64,
    /// Whether accesses were sequential (related) or random
    pub sequential_access: bool,
    /// Number of repeat accesses to same memories
    pub repeat_access_count: u32,
}

impl AccessPattern {
    /// Create a new access pattern
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an access event
    pub fn add_access(&mut self, memory_id: impl Into<String>, time_since_last: f64) {
        self.memory_ids.push(memory_id.into());
        if time_since_last > 0.0 {
            self.inter_access_times.push(time_since_last);
        }
    }

    /// Add a query
    pub fn add_query(&mut self, query: impl Into<String>) {
        self.queries.push(query.into());
    }

    /// Get unique memory count
    pub fn unique_memories(&self) -> usize {
        let set: HashSet<_> = self.memory_ids.iter().collect();
        set.len()
    }

    /// Get average inter-access time
    pub fn avg_inter_access_time(&self) -> f64 {
        if self.inter_access_times.is_empty() {
            return 0.0;
        }
        self.inter_access_times.iter().sum::<f64>() / self.inter_access_times.len() as f64
    }
}

/// Session data for learning mode detection
#[derive(Debug, Clone)]
pub struct Session {
    /// Session ID
    pub session_id: String,
    /// When session started
    pub start_time: DateTime<Utc>,
    /// Duration in minutes
    pub duration_minutes: f64,
    /// Number of queries made
    pub query_count: u32,
    /// Number of edits/actions made
    pub edit_count: u32,
    /// Number of unique memories accessed
    pub unique_memories_accessed: u32,
    /// Whether session includes documentation viewing
    pub viewed_docs: bool,
    /// Query topics (for diversity analysis)
    pub query_topics: Vec<String>,
}

impl Session {
    /// Create a new session
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            start_time: Utc::now(),
            duration_minutes: 0.0,
            query_count: 0,
            edit_count: 0,
            unique_memories_accessed: 0,
            viewed_docs: false,
            query_topics: Vec::new(),
        }
    }
}

/// Focus detector for analyzing attention patterns
#[derive(Debug)]
struct FocusDetector {
    /// Baseline for "normal" inter-access time (seconds)
    baseline_inter_access: f64,
    /// Baseline for session depth (for future depth-weighted focus scoring)
    #[allow(dead_code)]
    baseline_session_depth: f64,
}

impl Default for FocusDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusDetector {
    fn new() -> Self {
        Self {
            baseline_inter_access: 60.0, // 1 minute
            baseline_session_depth: 5.0,
        }
    }

    fn compute_focus(&self, pattern: &AccessPattern) -> f64 {
        let metrics = self.get_focus_metrics(pattern);

        // Combine metrics with weights
        let frequency_score = (metrics.access_frequency * 0.5).min(1.0);
        let depth_score = (metrics.session_depth / 10.0).min(1.0);
        let diversity_score = metrics.query_diversity;

        (frequency_score * 0.4 + depth_score * 0.35 + diversity_score * 0.25)
            .clamp(MIN_IMPORTANCE, MAX_IMPORTANCE)
    }

    fn get_focus_metrics(&self, pattern: &AccessPattern) -> FocusMetrics {
        let avg_time = pattern.avg_inter_access_time();

        // Access frequency: faster access = more focused
        let access_frequency = if avg_time > 0.0 {
            (self.baseline_inter_access / avg_time).min(2.0)
        } else {
            1.0
        };

        // Session depth: more unique memories = deeper exploration
        let session_depth = pattern.unique_memories() as f64;

        // Query diversity: varied queries suggest active exploration
        let unique_query_words: HashSet<String> = pattern
            .queries
            .iter()
            .flat_map(|q| {
                q.to_lowercase()
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|w| w.len() > 2)
            .collect();

        let query_diversity = (unique_query_words.len() as f64 / 20.0).min(1.0);

        FocusMetrics {
            access_frequency,
            session_depth,
            query_diversity,
        }
    }
}

/// Focus metrics for transparency
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FocusMetrics {
    access_frequency: f64,
    session_depth: f64,
    query_diversity: f64,
}

/// Explanation of attention score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionExplanation {
    /// The computed attention score
    pub score: f64,
    /// Whether learning mode is active
    pub learning_mode_active: bool,
    /// Normalized access frequency
    pub access_frequency: f64,
    /// Depth of session exploration
    pub session_depth: f64,
    /// Diversity of queries
    pub query_diversity: f64,
}

impl AttentionExplanation {
    /// Generate human-readable explanation
    pub fn explain(&self) -> String {
        let focus_level = if self.score > 0.7 {
            "Highly focused"
        } else if self.score > 0.4 {
            "Moderately focused"
        } else {
            "Low focus"
        };

        let learning_str = if self.learning_mode_active {
            " (Learning mode active)"
        } else {
            ""
        };

        format!(
            "{}{} - Score: {:.2}. Access freq: {:.1}x, Depth: {:.0}, Diversity: {:.0}%",
            focus_level,
            learning_str,
            self.score,
            self.access_frequency,
            self.session_depth,
            self.query_diversity * 100.0
        )
    }
}

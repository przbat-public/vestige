//! Effective retrieval ranking score combining state, recency and frequency,
//! plus the convenience [`MemoryStateInfo`] view bundling lifecycle metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::lifecycle::MemoryLifecycle;
use super::states::{MemoryState, StateTransition};

// ============================================================================
// ACCESSIBILITY CALCULATOR
// ============================================================================

/// Calculates effective accessibility scores for retrieval ranking.
///
/// Combines:
/// - State-based accessibility (Active: 1.0, Dormant: 0.7, Silent: 0.3, Unavailable: 0.05)
/// - Recency boost (recently accessed memories get a boost)
/// - Access frequency boost (frequently accessed memories get a boost)
#[derive(Debug, Clone)]
pub struct AccessibilityCalculator {
    /// Weight for recency in the final score (0.0-1.0)
    pub recency_weight: f64,
    /// Weight for access frequency in the final score (0.0-1.0)
    pub frequency_weight: f64,
    /// Half-life for recency decay in hours
    pub recency_half_life_hours: f64,
    /// Access count at which frequency bonus maxes out
    pub frequency_saturation_count: u32,
}

impl Default for AccessibilityCalculator {
    fn default() -> Self {
        Self {
            recency_weight: 0.15,
            frequency_weight: 0.1,
            recency_half_life_hours: 24.0,
            frequency_saturation_count: 10,
        }
    }
}

impl AccessibilityCalculator {
    /// Calculate the effective accessibility score for a memory.
    ///
    /// # Arguments
    ///
    /// * `lifecycle` - The memory's lifecycle state
    /// * `base_score` - The base relevance score from search (0.0-1.0)
    ///
    /// # Returns
    ///
    /// Adjusted score factoring in accessibility (0.0-1.0).
    pub fn calculate(&self, lifecycle: &MemoryLifecycle, base_score: f64) -> f64 {
        let state_multiplier = lifecycle.state.accessibility_multiplier();

        // Recency boost: exponential decay based on time since last access
        let hours_since_access = Utc::now()
            .signed_duration_since(lifecycle.last_access)
            .num_minutes() as f64
            / 60.0;
        let recency_factor = 0.5_f64.powf(hours_since_access / self.recency_half_life_hours);
        let recency_boost = recency_factor * self.recency_weight;

        // Frequency boost: logarithmic saturation
        let frequency_factor = (lifecycle.access_count as f64)
            .min(self.frequency_saturation_count as f64)
            / self.frequency_saturation_count as f64;
        let frequency_boost = frequency_factor * self.frequency_weight;

        // Combine: base * state_multiplier + boosts
        let raw_score = base_score * state_multiplier + recency_boost + frequency_boost;

        // Clamp to valid range
        raw_score.clamp(0.0, 1.0)
    }

    /// Calculate minimum similarity threshold for a given state.
    ///
    /// Silent memories require higher similarity to be retrieved.
    pub fn minimum_similarity_for_state(&self, state: MemoryState, base_threshold: f64) -> f64 {
        match state {
            MemoryState::Active => base_threshold * 0.8, // Lower threshold
            MemoryState::Dormant => base_threshold,
            MemoryState::Silent => base_threshold * 1.5, // Higher threshold
            MemoryState::Unavailable => 1.1,             // Effectively unreachable
        }
    }
}

// ============================================================================
// MEMORY STATE QUERY RESULT
// ============================================================================

/// Extended information about a memory's state for user queries.
///
/// This provides transparency about why a memory might be harder to access.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStateInfo {
    /// Current state
    pub state: MemoryState,
    /// Human-readable explanation of current state
    pub explanation: String,
    /// Current accessibility (0.0-1.0)
    pub accessibility: f64,
    /// How many times accessed
    pub access_count: u32,
    /// Last access time
    pub last_access: DateTime<Utc>,
    /// Time since last access in human-readable format
    pub time_since_access: String,
    /// If applicable, why the memory is in this state
    pub state_reason: Option<String>,
    /// If suppressed, when it will be accessible again
    pub accessible_after: Option<DateTime<Utc>>,
    /// Recent state transitions
    pub recent_transitions: Vec<StateTransition>,
    /// Recommendations for improving accessibility
    pub recommendations: Vec<String>,
}

impl MemoryStateInfo {
    /// Create state info from a lifecycle.
    pub fn from_lifecycle(lifecycle: &MemoryLifecycle) -> Self {
        let now = Utc::now();
        let duration_since_access = now.signed_duration_since(lifecycle.last_access);

        // Format time since access
        let time_since_access = if duration_since_access.num_days() > 0 {
            format!("{} days ago", duration_since_access.num_days())
        } else if duration_since_access.num_hours() > 0 {
            format!("{} hours ago", duration_since_access.num_hours())
        } else if duration_since_access.num_minutes() > 0 {
            format!("{} minutes ago", duration_since_access.num_minutes())
        } else {
            "just now".to_string()
        };

        // Get state reason from most recent transition
        let state_reason = lifecycle
            .state_history
            .back()
            .map(|t| t.reason.description());

        // Generate recommendations
        let mut recommendations = Vec::new();
        match lifecycle.state {
            MemoryState::Silent => {
                recommendations.push(
                    "This memory needs a strong, specific cue to be retrieved. \
                     Try using more detailed search terms."
                        .to_string(),
                );
            }
            MemoryState::Unavailable => {
                if let Some(until) = lifecycle.suppression_until
                    && until > now
                {
                    recommendations.push(format!(
                        "This memory is temporarily suppressed. \
                             It will become accessible again after {}.",
                        until.format("%Y-%m-%d %H:%M UTC")
                    ));
                }
            }
            MemoryState::Dormant => {
                if duration_since_access.num_days() > 20 {
                    recommendations.push(
                        "Consider accessing this memory soon to prevent it from \
                         becoming harder to retrieve."
                            .to_string(),
                    );
                }
            }
            _ => {}
        }

        // Get recent transitions (last 5)
        let recent_transitions: Vec<_> = lifecycle
            .state_history
            .iter()
            .rev()
            .take(5)
            .cloned()
            .collect();

        Self {
            state: lifecycle.state,
            explanation: lifecycle.state.description().to_string(),
            accessibility: lifecycle.accessibility(),
            access_count: lifecycle.access_count,
            last_access: lifecycle.last_access,
            time_since_access,
            state_reason,
            accessible_after: if lifecycle.state == MemoryState::Unavailable {
                lifecycle.suppression_until
            } else {
                None
            },
            recent_transitions,
            recommendations,
        }
    }
}

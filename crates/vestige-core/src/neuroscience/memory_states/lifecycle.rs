//! [`MemoryLifecycle`] tracks each memory through its accessibility states
//! over time, plus the supporting analytics helpers and decay configuration.

use std::collections::VecDeque;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::states::{MemoryState, StateTransition, StateTransitionReason};
use super::{DEFAULT_ACTIVE_DECAY_HOURS, DEFAULT_DORMANT_DECAY_DAYS, MAX_STATE_HISTORY_SIZE};

// ============================================================================
// MEMORY LIFECYCLE
// ============================================================================

/// Tracks the complete lifecycle and state of a memory.
///
/// This struct should be embedded in or associated with each Memory
/// to track its accessibility state over time.
///
/// # Example
///
/// ```rust
/// use vestige_core::neuroscience::{MemoryLifecycle, MemoryState};
///
/// // Create a new lifecycle (starts Active)
/// let mut lifecycle = MemoryLifecycle::new();
/// assert_eq!(lifecycle.state, MemoryState::Active);
///
/// // Record an access
/// lifecycle.record_access();
///
/// // Check if memory should decay
/// let config = lifecycle.decay_config();
/// if lifecycle.should_decay_to_dormant(&config) {
///     lifecycle.transition_to(MemoryState::Dormant,
///         vestige_core::neuroscience::StateTransitionReason::TimeDecay);
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLifecycle {
    /// Current accessibility state
    pub state: MemoryState,
    /// When the memory was last accessed
    pub last_access: DateTime<Utc>,
    /// Total number of times this memory has been accessed
    pub access_count: u32,
    /// History of state transitions (most recent last)
    pub state_history: VecDeque<StateTransition>,
    /// If Unavailable due to suppression, when it expires
    pub suppression_until: Option<DateTime<Utc>>,
    /// IDs of memories that have suppressed this one
    pub suppressed_by: Vec<String>,
    /// When the current state was entered
    pub state_entered_at: DateTime<Utc>,
    /// Total time spent in each state (for analytics)
    pub time_in_states: StateTimeAccumulator,
}

impl Default for MemoryLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryLifecycle {
    /// Create a new lifecycle in the Active state.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            state: MemoryState::Active,
            last_access: now,
            access_count: 1,
            state_history: VecDeque::with_capacity(MAX_STATE_HISTORY_SIZE),
            suppression_until: None,
            suppressed_by: Vec::new(),
            state_entered_at: now,
            time_in_states: StateTimeAccumulator::default(),
        }
    }

    /// Create a lifecycle with a specific initial state.
    pub fn with_state(state: MemoryState) -> Self {
        let mut lifecycle = Self::new();
        lifecycle.state = state;
        lifecycle.state_history.push_back(StateTransition::new(
            MemoryState::Active,
            state,
            StateTransitionReason::SystemInit,
        ));
        lifecycle
    }

    /// Record an access to this memory.
    ///
    /// Accessing a memory:
    /// 1. Resets it to Active state (if not suppressed)
    /// 2. Updates last_access timestamp
    /// 3. Increments access count
    ///
    /// # Returns
    ///
    /// Whether the state changed (i.e., memory was reactivated).
    pub fn record_access(&mut self) -> bool {
        self.last_access = Utc::now();
        self.access_count = self.access_count.saturating_add(1);

        // Can't reactivate if suppressed
        if self.state == MemoryState::Unavailable && !self.is_suppression_expired() {
            return false;
        }

        if self.state != MemoryState::Active {
            self.transition_to(MemoryState::Active, StateTransitionReason::Access);
            true
        } else {
            false
        }
    }

    /// Transition to a new state with a reason.
    pub fn transition_to(&mut self, new_state: MemoryState, reason: StateTransitionReason) {
        if self.state == new_state {
            return; // No change
        }

        // Update time accumulator
        let now = Utc::now();
        let time_in_current = now
            .signed_duration_since(self.state_entered_at)
            .num_seconds()
            .max(0) as u64;
        self.time_in_states.add(self.state, time_in_current);

        // Record transition
        let transition = StateTransition::new(self.state, new_state, reason);
        self.state_history.push_back(transition);

        // Trim history if needed
        while self.state_history.len() > MAX_STATE_HISTORY_SIZE {
            self.state_history.pop_front();
        }

        // Update state
        self.state = new_state;
        self.state_entered_at = now;

        // Clear suppression if leaving Unavailable
        if new_state != MemoryState::Unavailable {
            self.suppression_until = None;
            self.suppressed_by.clear();
        }
    }

    /// Suppress this memory due to competition loss.
    ///
    /// # Arguments
    ///
    /// * `winner_id` - ID of the memory that won the competition
    /// * `similarity` - How similar this memory was to the winner
    /// * `duration` - How long the suppression lasts
    pub fn suppress_from_competition(
        &mut self,
        winner_id: String,
        similarity: f64,
        duration: Duration,
    ) {
        let reason = StateTransitionReason::CompetitionLoss {
            winner_id: winner_id.clone(),
            similarity,
        };

        self.transition_to(MemoryState::Unavailable, reason);
        self.suppression_until = Some(Utc::now() + duration);
        self.suppressed_by.push(winner_id);
    }

    /// Suppress this memory due to user action.
    ///
    /// # Arguments
    ///
    /// * `duration` - How long the suppression lasts
    /// * `reason` - Optional reason from the user
    pub fn suppress_by_user(&mut self, duration: Duration, reason: Option<String>) {
        self.transition_to(
            MemoryState::Unavailable,
            StateTransitionReason::UserSuppression { reason },
        );
        self.suppression_until = Some(Utc::now() + duration);
    }

    /// Check if suppression has expired.
    pub fn is_suppression_expired(&self) -> bool {
        self.suppression_until
            .map(|until| Utc::now() >= until)
            .unwrap_or(true)
    }

    /// Get the default decay configuration.
    pub fn decay_config(&self) -> StateDecayConfig {
        StateDecayConfig::default()
    }

    /// Check if this memory should decay from Active to Dormant.
    pub fn should_decay_to_dormant(&self, config: &StateDecayConfig) -> bool {
        if self.state != MemoryState::Active {
            return false;
        }

        let hours_since_access = Utc::now()
            .signed_duration_since(self.last_access)
            .num_hours();
        hours_since_access >= config.active_decay_hours
    }

    /// Check if this memory should decay from Dormant to Silent.
    pub fn should_decay_to_silent(&self, config: &StateDecayConfig) -> bool {
        if self.state != MemoryState::Dormant {
            return false;
        }

        let days_since_access = Utc::now()
            .signed_duration_since(self.last_access)
            .num_days();
        days_since_access >= config.dormant_decay_days
    }

    /// Try to reactivate from Silent with a strong cue.
    ///
    /// # Arguments
    ///
    /// * `cue_similarity` - Similarity score of the retrieval cue
    /// * `threshold` - Minimum similarity required for reactivation
    ///
    /// # Returns
    ///
    /// Whether reactivation succeeded.
    pub fn try_reactivate_with_cue(&mut self, cue_similarity: f64, threshold: f64) -> bool {
        if self.state != MemoryState::Silent {
            return false;
        }

        if cue_similarity >= threshold {
            self.transition_to(
                MemoryState::Dormant,
                StateTransitionReason::CueReactivation { cue_similarity },
            );
            true
        } else {
            false
        }
    }

    /// Get the current accessibility multiplier.
    pub fn accessibility(&self) -> f64 {
        self.state.accessibility_multiplier()
    }

    /// Get a summary of this lifecycle for debugging/display.
    pub fn summary(&self) -> LifecycleSummary {
        LifecycleSummary {
            state: self.state,
            state_description: self.state.description().to_string(),
            accessibility: self.accessibility(),
            access_count: self.access_count,
            last_access: self.last_access,
            time_in_current_state: Utc::now()
                .signed_duration_since(self.state_entered_at)
                .num_seconds()
                .max(0) as u64,
            total_transitions: self.state_history.len(),
            is_suppressed: self.state == MemoryState::Unavailable && !self.is_suppression_expired(),
            suppression_expires: self.suppression_until,
        }
    }
}

// ============================================================================
// STATE TIME ACCUMULATOR
// ============================================================================

/// Accumulates time spent in each state.
///
/// Useful for analytics and understanding memory behavior over time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateTimeAccumulator {
    /// Seconds spent in Active state
    pub active_seconds: u64,
    /// Seconds spent in Dormant state
    pub dormant_seconds: u64,
    /// Seconds spent in Silent state
    pub silent_seconds: u64,
    /// Seconds spent in Unavailable state
    pub unavailable_seconds: u64,
}

impl StateTimeAccumulator {
    /// Add time to the appropriate state counter.
    pub fn add(&mut self, state: MemoryState, seconds: u64) {
        match state {
            MemoryState::Active => self.active_seconds += seconds,
            MemoryState::Dormant => self.dormant_seconds += seconds,
            MemoryState::Silent => self.silent_seconds += seconds,
            MemoryState::Unavailable => self.unavailable_seconds += seconds,
        }
    }

    /// Get total tracked time across all states.
    pub fn total_seconds(&self) -> u64 {
        self.active_seconds + self.dormant_seconds + self.silent_seconds + self.unavailable_seconds
    }

    /// Get percentage of time spent in each state.
    pub fn percentages(&self) -> StatePercentages {
        let total = self.total_seconds().max(1) as f64;
        StatePercentages {
            active: (self.active_seconds as f64 / total) * 100.0,
            dormant: (self.dormant_seconds as f64 / total) * 100.0,
            silent: (self.silent_seconds as f64 / total) * 100.0,
            unavailable: (self.unavailable_seconds as f64 / total) * 100.0,
        }
    }
}

/// Percentage breakdown of time spent in each state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatePercentages {
    pub active: f64,
    pub dormant: f64,
    pub silent: f64,
    pub unavailable: f64,
}

// ============================================================================
// LIFECYCLE SUMMARY
// ============================================================================

/// A summary of a memory's lifecycle for display/debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleSummary {
    pub state: MemoryState,
    pub state_description: String,
    pub accessibility: f64,
    pub access_count: u32,
    pub last_access: DateTime<Utc>,
    pub time_in_current_state: u64,
    pub total_transitions: usize,
    pub is_suppressed: bool,
    pub suppression_expires: Option<DateTime<Utc>>,
}

// ============================================================================
// STATE DECAY CONFIGURATION
// ============================================================================

/// Configuration for automatic state decay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDecayConfig {
    /// Hours before Active decays to Dormant
    pub active_decay_hours: i64,
    /// Days before Dormant decays to Silent
    pub dormant_decay_days: i64,
    /// Similarity threshold for cue reactivation of Silent memories
    pub reactivation_threshold: f64,
    /// Default suppression duration for competition losses
    pub competition_suppression_duration: Duration,
    /// Whether to automatically resolve expired suppressions
    pub auto_resolve_suppression: bool,
}

impl Default for StateDecayConfig {
    fn default() -> Self {
        Self {
            active_decay_hours: DEFAULT_ACTIVE_DECAY_HOURS,
            dormant_decay_days: DEFAULT_DORMANT_DECAY_DAYS,
            reactivation_threshold: 0.8,
            competition_suppression_duration: Duration::hours(24),
            auto_resolve_suppression: true,
        }
    }
}

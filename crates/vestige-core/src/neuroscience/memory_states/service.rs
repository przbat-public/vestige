//! Background service that walks lifecycles forward in time — applying
//! decay transitions, resolving suppressions, and aggregating batch stats.

use serde::{Deserialize, Serialize};

use super::lifecycle::{MemoryLifecycle, StateDecayConfig};
use super::states::{MemoryState, StateTransition, StateTransitionReason};

// ============================================================================
// STATE UPDATE SERVICE
// ============================================================================

/// Service for updating memory states based on time and access patterns.
///
/// This should be run periodically (e.g., as a background task) to:
/// 1. Decay Active memories to Dormant
/// 2. Decay Dormant memories to Silent
/// 3. Resolve expired suppressions
///
/// # Example
///
/// ```rust
/// use vestige_core::neuroscience::{StateUpdateService, MemoryLifecycle, MemoryState};
///
/// let service = StateUpdateService::new();
/// let mut lifecycle = MemoryLifecycle::new();
///
/// // Check and apply any needed transitions
/// let transitions = service.update_lifecycle(&mut lifecycle);
/// println!("Applied {} transitions", transitions.len());
/// ```
#[derive(Debug, Clone)]
pub struct StateUpdateService {
    config: StateDecayConfig,
}

impl Default for StateUpdateService {
    fn default() -> Self {
        Self::new()
    }
}

impl StateUpdateService {
    /// Create a new update service with default config.
    pub fn new() -> Self {
        Self {
            config: StateDecayConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: StateDecayConfig) -> Self {
        Self { config }
    }

    /// Get the configuration.
    pub fn config(&self) -> &StateDecayConfig {
        &self.config
    }

    /// Update a single lifecycle, applying any needed transitions.
    ///
    /// # Returns
    ///
    /// List of transitions that were applied.
    pub fn update_lifecycle(&self, lifecycle: &mut MemoryLifecycle) -> Vec<StateTransition> {
        let mut transitions = Vec::new();

        // Check for suppression expiry first
        if lifecycle.state == MemoryState::Unavailable
            && lifecycle.is_suppression_expired()
            && self.config.auto_resolve_suppression
        {
            let from = lifecycle.state;
            lifecycle.transition_to(
                MemoryState::Dormant,
                StateTransitionReason::SuppressionExpired,
            );
            transitions.push(StateTransition::new(
                from,
                MemoryState::Dormant,
                StateTransitionReason::SuppressionExpired,
            ));
        }

        // Check for Active -> Dormant decay
        if lifecycle.should_decay_to_dormant(&self.config) {
            let from = lifecycle.state;
            lifecycle.transition_to(MemoryState::Dormant, StateTransitionReason::TimeDecay);
            transitions.push(StateTransition::new(
                from,
                MemoryState::Dormant,
                StateTransitionReason::TimeDecay,
            ));
        }

        // Check for Dormant -> Silent decay
        if lifecycle.should_decay_to_silent(&self.config) {
            let from = lifecycle.state;
            lifecycle.transition_to(MemoryState::Silent, StateTransitionReason::TimeDecay);
            transitions.push(StateTransition::new(
                from,
                MemoryState::Silent,
                StateTransitionReason::TimeDecay,
            ));
        }

        transitions
    }

    /// Batch update multiple lifecycles.
    ///
    /// # Returns
    ///
    /// Total number of transitions applied.
    pub fn batch_update(&self, lifecycles: &mut [MemoryLifecycle]) -> BatchUpdateResult {
        let mut result = BatchUpdateResult::default();

        for lifecycle in lifecycles {
            let transitions = self.update_lifecycle(lifecycle);
            for t in transitions {
                match t.to_state {
                    MemoryState::Dormant => {
                        if matches!(t.reason, StateTransitionReason::SuppressionExpired) {
                            result.suppressions_resolved += 1;
                        } else {
                            result.active_to_dormant += 1;
                        }
                    }
                    MemoryState::Silent => result.dormant_to_silent += 1,
                    _ => {}
                }
                result.total_transitions += 1;
            }
        }

        result
    }
}

/// Result of a batch update operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchUpdateResult {
    /// Total transitions applied
    pub total_transitions: usize,
    /// Active -> Dormant transitions
    pub active_to_dormant: usize,
    /// Dormant -> Silent transitions
    pub dormant_to_silent: usize,
    /// Suppressions that were resolved
    pub suppressions_resolved: usize,
}

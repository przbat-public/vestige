//! [`SynapticTag`] (the per-memory marker) and [`CaptureWindow`] (its
//! retroactive/proactive temporal envelope).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::decay::DecayFunction;
use super::{DEFAULT_BACKWARD_HOURS, DEFAULT_FORWARD_HOURS};

// ============================================================================
// SYNAPTIC TAG
// ============================================================================

/// A synaptic tag marking a memory for potential consolidation
///
/// In neuroscience, a synaptic tag is a temporary molecular marker created at
/// a synapse after weak stimulation. It marks the synapse as "eligible" for
/// consolidation if plasticity-related products (PRPs) arrive within the
/// capture window.
///
/// ## Lifecycle
///
/// 1. Created when a memory is encoded
/// 2. Strength decays over time
/// 3. If PRP arrives while strength > threshold, memory is captured
/// 4. Captured memories are promoted to long-term storage
/// 5. Uncaptured tags eventually decay to zero and are cleaned up
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynapticTag {
    /// The memory this tag is attached to
    pub memory_id: String,
    /// When the tag was created (memory encoding time)
    pub created_at: DateTime<Utc>,
    /// Current tag strength (decays over time)
    pub tag_strength: f64,
    /// Initial tag strength at creation
    pub initial_strength: f64,
    /// Whether this memory has been captured (consolidated)
    pub captured: bool,
    /// The event that captured this memory (if any)
    pub capture_event: Option<String>,
    /// When the memory was captured
    pub captured_at: Option<DateTime<Utc>>,
    /// Context about why this tag was created
    pub encoding_context: Option<String>,
}

impl SynapticTag {
    /// Create a new synaptic tag for a memory
    pub fn new(memory_id: &str) -> Self {
        Self {
            memory_id: memory_id.to_string(),
            created_at: Utc::now(),
            tag_strength: 1.0,
            initial_strength: 1.0,
            captured: false,
            capture_event: None,
            captured_at: None,
            encoding_context: None,
        }
    }

    /// Create with custom initial strength
    pub fn with_strength(memory_id: &str, strength: f64) -> Self {
        Self {
            memory_id: memory_id.to_string(),
            created_at: Utc::now(),
            tag_strength: strength.clamp(0.0, 1.0),
            initial_strength: strength.clamp(0.0, 1.0),
            captured: false,
            capture_event: None,
            captured_at: None,
            encoding_context: None,
        }
    }

    /// Create with encoding context
    pub fn with_context(memory_id: &str, context: &str) -> Self {
        Self {
            memory_id: memory_id.to_string(),
            created_at: Utc::now(),
            tag_strength: 1.0,
            initial_strength: 1.0,
            captured: false,
            capture_event: None,
            captured_at: None,
            encoding_context: Some(context.to_string()),
        }
    }

    /// Calculate current tag strength with decay
    pub fn current_strength(&self, decay_fn: DecayFunction, lifetime_hours: f64) -> f64 {
        // Use milliseconds for precise timing (important for tests with short lifetimes)
        let hours_elapsed = (Utc::now() - self.created_at).num_milliseconds() as f64 / 3_600_000.0;
        decay_fn.apply(self.initial_strength, hours_elapsed, lifetime_hours)
    }

    /// Check if the tag is still active (above minimum threshold)
    pub fn is_active(
        &self,
        decay_fn: DecayFunction,
        lifetime_hours: f64,
        min_strength: f64,
    ) -> bool {
        !self.captured && self.current_strength(decay_fn, lifetime_hours) >= min_strength
    }

    /// Mark this tag as captured
    pub fn capture(&mut self, event_id: &str) {
        self.captured = true;
        self.capture_event = Some(event_id.to_string());
        self.captured_at = Some(Utc::now());
    }

    /// Get the age of this tag in hours
    pub fn age_hours(&self) -> f64 {
        (Utc::now() - self.created_at).num_milliseconds() as f64 / 3_600_000.0
    }
}

// ============================================================================
// CAPTURE WINDOW
// ============================================================================

/// Temporal window for PRP capture
///
/// When an important event occurs, PRPs are produced and can be captured by
/// tagged memories within this temporal window. The window extends both
/// backward (already encoded memories) and forward (memories about to be
/// encoded).
///
/// ## Biological Basis
///
/// Research shows that STC can occur with intervals up to 9 hours between
/// weak and strong stimulation. This suggests a broader temporal flexibility
/// for tag-PRP interactions than previously thought.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureWindow {
    /// How far back to look for tagged memories (hours)
    pub backward_hours: f64,
    /// How far forward to look for tagged memories (hours)
    pub forward_hours: f64,
    /// Decay function for capture probability
    pub decay_function: DecayFunction,
}

impl Default for CaptureWindow {
    fn default() -> Self {
        Self {
            backward_hours: DEFAULT_BACKWARD_HOURS,
            forward_hours: DEFAULT_FORWARD_HOURS,
            decay_function: DecayFunction::Exponential,
        }
    }
}

impl CaptureWindow {
    /// Create a new capture window
    pub fn new(backward_hours: f64, forward_hours: f64) -> Self {
        Self {
            backward_hours,
            forward_hours,
            decay_function: DecayFunction::Exponential,
        }
    }

    /// Create with custom decay function
    pub fn with_decay(backward_hours: f64, forward_hours: f64, decay_fn: DecayFunction) -> Self {
        Self {
            backward_hours,
            forward_hours,
            decay_function: decay_fn,
        }
    }

    /// Calculate capture probability based on temporal distance
    ///
    /// Memories closer to the importance event have higher capture probability.
    /// The probability decays with distance according to the configured decay function.
    ///
    /// # Arguments
    /// * `memory_time` - When the memory was encoded
    /// * `event_time` - When the importance event occurred
    ///
    /// # Returns
    /// Capture probability (0.0 to 1.0), or None if outside window
    pub fn capture_probability(
        &self,
        memory_time: DateTime<Utc>,
        event_time: DateTime<Utc>,
    ) -> Option<f64> {
        let diff_hours = (event_time - memory_time).num_minutes() as f64 / 60.0;

        if diff_hours > 0.0 {
            // Memory was encoded before event (backward capture)
            if diff_hours > self.backward_hours {
                return None;
            }
            Some(
                self.decay_function
                    .apply(1.0, diff_hours, self.backward_hours),
            )
        } else {
            // Memory was encoded after event (forward capture)
            let abs_diff = diff_hours.abs();
            if abs_diff > self.forward_hours {
                return None;
            }
            Some(self.decay_function.apply(1.0, abs_diff, self.forward_hours))
        }
    }

    /// Get the start of the capture window
    pub fn window_start(&self, event_time: DateTime<Utc>) -> DateTime<Utc> {
        event_time - Duration::minutes((self.backward_hours * 60.0) as i64)
    }

    /// Get the end of the capture window
    pub fn window_end(&self, event_time: DateTime<Utc>) -> DateTime<Utc> {
        event_time + Duration::minutes((self.forward_hours * 60.0) as i64)
    }

    /// Check if a time is within the capture window
    pub fn is_in_window(&self, memory_time: DateTime<Utc>, event_time: DateTime<Utc>) -> bool {
        memory_time >= self.window_start(event_time) && memory_time <= self.window_end(event_time)
    }
}

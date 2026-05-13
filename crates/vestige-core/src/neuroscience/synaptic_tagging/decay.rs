//! Decay-function vocabulary used to age out synaptic tags over time.

use serde::{Deserialize, Serialize};

// ============================================================================
// DECAY FUNCTIONS
// ============================================================================

/// Decay function for synaptic tag strength
///
/// Different decay functions model different memory characteristics:
/// - Exponential: Rapid initial decay, slow tail (default for short-term)
/// - Linear: Constant decay rate
/// - Power: Slow initial decay, accelerating over time
/// - Logarithmic: Very slow decay, good for important memories
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DecayFunction {
    /// Exponential decay: strength = initial * e^(-lambda * t)
    /// Best for modeling biological tag decay
    #[default]
    Exponential,
    /// Linear decay: strength = initial * (1 - t/lifetime)
    /// Simple, predictable decay
    Linear,
    /// Power law decay: strength = initial * (1 + t)^(-alpha)
    /// Matches FSRS-6 forgetting curve
    Power,
    /// Logarithmic decay: strength = initial * (1 - ln(1+t)/ln(1+lifetime))
    /// Very slow decay for persistent tags
    Logarithmic,
}

impl DecayFunction {
    /// Calculate decayed strength
    ///
    /// # Arguments
    /// * `initial_strength` - Initial tag strength (0.0 to 1.0)
    /// * `hours_elapsed` - Time since tag creation
    /// * `lifetime_hours` - Total lifetime before complete decay
    ///
    /// # Returns
    /// Decayed strength (0.0 to 1.0)
    pub fn apply(&self, initial_strength: f64, hours_elapsed: f64, lifetime_hours: f64) -> f64 {
        if hours_elapsed <= 0.0 {
            return initial_strength;
        }
        if hours_elapsed >= lifetime_hours {
            return 0.0;
        }

        let t = hours_elapsed;
        let l = lifetime_hours;

        let decayed = match self {
            DecayFunction::Exponential => {
                // lambda = -ln(0.01) / lifetime for 99% decay at lifetime
                let lambda = 4.605 / l;
                initial_strength * (-lambda * t).exp()
            }
            DecayFunction::Linear => initial_strength * (1.0 - t / l),
            DecayFunction::Power => {
                // alpha = 0.5 matches FSRS-6
                let alpha = 0.5;
                initial_strength * (1.0 + t / l).powf(-alpha)
            }
            DecayFunction::Logarithmic => {
                initial_strength * (1.0 - (1.0 + t).ln() / (1.0 + l).ln())
            }
        };

        decayed.clamp(0.0, 1.0)
    }
}

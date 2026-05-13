//! # Predictive Memory Retrieval
//!
//! Implementation of Friston's Free Energy Principle for memory systems.
//! The brain predicts rather than passively stores - this module anticipates
//! what the user needs BEFORE they ask.
//!
//! ## Theoretical Foundation
//!
//! Based on the Active Inference framework (Friston, 2010):
//! - The brain is fundamentally a prediction machine
//! - Memory recall is a predictive process, not passive retrieval
//! - Prediction errors signal novelty and drive enhanced encoding
//! - Free energy minimization guides memory optimization
//!
//! ## How It Works
//!
//! 1. **User Modeling**: Build probabilistic model of user interests, patterns, and context
//! 2. **Predictive Caching**: Pre-fetch likely-needed memories into fast cache
//! 3. **Reinforcement Learning**: Learn from prediction accuracy to improve future predictions
//! 4. **Proactive Surfacing**: Show predictions ("You might also need...")
//! 5. **Novelty Detection**: Prediction errors signal important new information
//!
//! ## Example
//!
//! ```rust,ignore
//! use vestige_core::neuroscience::{PredictiveMemory, UserModel};
//!
//! let mut predictor = PredictiveMemory::new();
//!
//! // Update user model based on activity
//! predictor.record_query("authentication", &["jwt", "oauth"]);
//! predictor.record_interest("security", 0.8);
//!
//! // Get predictions for current context
//! let predictions = predictor.predict_needed_memories(&session_context);
//!
//! // Proactively surface relevant memories
//! for prediction in predictions.iter().filter(|p| p.confidence > 0.7) {
//!     println!("You might need: {} ({}% confidence)",
//!         prediction.memory_id,
//!         (prediction.confidence * 100.0) as u32
//!     );
//! }
//! ```

// ============================================================================
// CONFIGURATION CONSTANTS
// ============================================================================

/// Maximum number of cached predictions
pub(super) const MAX_CACHE_SIZE: usize = 100;

/// Maximum recent queries to track
pub(super) const MAX_QUERY_HISTORY: usize = 500;

/// Maximum predictions per query
pub(super) const MAX_PREDICTIONS: usize = 20;

/// Minimum confidence threshold for predictions
pub(super) const DEFAULT_MIN_CONFIDENCE: f64 = 0.3;

/// Learning rate for interest updates
pub(super) const INTEREST_LEARNING_RATE: f64 = 0.1;

/// Decay rate for outdated interests (per day)
pub(super) const INTEREST_DECAY_RATE: f64 = 0.98;

/// Time window for considering memories as "session-related" (in minutes)
pub(super) const SESSION_WINDOW_MINUTES: i64 = 60;

/// Number of recent queries to consider for pattern matching
pub(super) const RECENT_QUERY_WINDOW: usize = 10;

mod cache;
mod compat;
mod engine;
mod error;
mod types;
mod user_model;

#[cfg(test)]
mod tests;

pub use compat::{
    ContextualPredictor, Prediction, PredictionConfidence, PredictiveConfig, PredictiveRetriever,
    SequencePredictor, TemporalPredictor,
};
pub use engine::{PredictiveMemory, PredictiveMemoryConfig};
pub use error::{PredictiveMemoryError, Result};
pub use types::{
    PredictedMemory, PredictionOutcome, PredictionReason, ProjectContext, QueryPattern,
    SessionContext, TemporalPatterns,
};
pub use user_model::UserModel;

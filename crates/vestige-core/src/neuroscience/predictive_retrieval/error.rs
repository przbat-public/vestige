//! Error type and result alias for predictive retrieval.

use thiserror::Error;

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Errors that can occur during predictive retrieval operations
#[derive(Debug, Error)]
pub enum PredictiveMemoryError {
    /// Failed to access prediction cache
    #[error("Cache access error: {0}")]
    CacheAccess(String),

    /// Failed to update user model
    #[error("User model update error: {0}")]
    UserModelUpdate(String),

    /// Failed to generate predictions
    #[error("Prediction generation error: {0}")]
    PredictionGeneration(String),

    /// Lock poisoned during concurrent access
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Result type for predictive memory operations
pub type Result<T> = std::result::Result<T, PredictiveMemoryError>;

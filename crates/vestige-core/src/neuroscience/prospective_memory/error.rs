//! Error type and result alias for the prospective-memory module.

use thiserror::Error;

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Errors that can occur in prospective memory operations
#[derive(Debug, Error)]
pub enum ProspectiveMemoryError {
    /// Failed to create intention
    #[error("Failed to create intention: {0}")]
    IntentionCreation(String),

    /// Intention not found
    #[error("Intention not found: {0}")]
    NotFound(String),

    /// Invalid trigger configuration
    #[error("Invalid trigger: {0}")]
    InvalidTrigger(String),

    /// Failed to parse natural language intention
    #[error("Failed to parse intention: {0}")]
    ParseError(String),

    /// Lock poisoned during concurrent access
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),

    /// Maximum intentions reached
    #[error("Maximum intentions reached ({0})")]
    MaxIntentionsReached(usize),
}

/// Result type for prospective memory operations
pub type Result<T> = std::result::Result<T, ProspectiveMemoryError>;

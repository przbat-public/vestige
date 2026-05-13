//! Storage error type and `SmartIngestResult` payload.

use crate::memory::KnowledgeNode;

/// Storage error type
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// Node not found
    #[error("Node not found: {0}")]
    NotFound(String),
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Invalid timestamp
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),
    /// Initialization error
    #[error("Initialization error: {0}")]
    Init(String),
}

/// Storage result type
pub type Result<T> = std::result::Result<T, StorageError>;

/// Result of smart ingest with prediction error gating
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartIngestResult {
    /// Decision made: "create", "update", "supersede", "merge", "reinforce", etc.
    pub decision: String,
    /// The resulting node (new or updated)
    pub node: KnowledgeNode,
    /// ID of superseded memory (if any)
    pub superseded_id: Option<String>,
    /// Similarity to closest existing memory (0.0 - 1.0)
    pub similarity: Option<f32>,
    /// Prediction error (1.0 - similarity)
    pub prediction_error: Option<f32>,
    /// Human-readable explanation of the decision
    pub reason: String,
}

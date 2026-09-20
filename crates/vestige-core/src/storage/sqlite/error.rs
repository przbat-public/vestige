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
    /// A write the gate refused to perform.
    ///
    /// Separate from the variants above because it is not a defect in the
    /// store: the caller asked to remember something that must not be
    /// remembered, and the answer is "no, and here is why". Reporting it as
    /// `Init` or `Database` would make an audited refusal look like a bug.
    #[error("Write refused: {0}")]
    Rejected(String),
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
    /// A possible contradiction with an existing memory, reported rather than
    /// acted on. Present only when the evidence cleared the gate's confidence
    /// floor; the existing memory's text and validity are untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contradiction: Option<ContradictionReport>,
    /// Nearest neighbours considered by the prediction-error gate, ordered by
    /// similarity (highest first). Exposed so callers — typically the MCP
    /// layer that wires the new node into the cognitive engine — can avoid
    /// re-running `semantic_search_raw` (and re-embedding the same content)
    /// just to learn what was nearby.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neighbor_ids: Vec<String>,
}

/// A contradiction the write path noticed and did not act on.
///
/// It exists so the writer can judge the signal: an id to look at, how similar
/// the two memories are, how strong the evidence was, and what fired it. The
/// decision to retire the older memory stays with the caller, because only the
/// caller knows whether the new text replaces the old claim or merely mentions
/// the same subject.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionReport {
    /// The memory the new content may deny.
    pub existing_id: String,
    /// Similarity between the two memories (0.0 - 1.0).
    pub similarity: f32,
    /// Confidence of the contradiction evidence (0.0 - 1.0).
    pub confidence: f32,
    /// The detector's evidence, each finding naming the rule and the text.
    pub evidence: Vec<crate::advanced::prediction_error::GateFinding>,
    /// What to do about it, worded for the agent that wrote the memory.
    pub hint: String,
}

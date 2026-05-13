//! Candidate input (`CandidateMemory`) and per-pair similarity output.

/// A candidate memory for update consideration
#[derive(Debug, Clone)]
pub struct CandidateMemory {
    /// Memory ID
    pub id: String,
    /// Memory content
    pub content: String,
    /// Embedding vector
    pub embedding: Vec<f32>,
    /// Current retrieval strength
    pub retrieval_strength: f64,
    /// Current retention strength
    pub retention_strength: f64,
    /// Tags on the memory
    pub tags: Vec<String>,
    /// Source of the memory
    pub source: Option<String>,
    /// Whether this memory was previously demoted
    pub was_demoted: bool,
    /// Whether this memory was previously promoted
    pub was_promoted: bool,
}

/// Result of similarity comparison
#[derive(Debug, Clone)]
pub struct SimilarityResult {
    /// Memory ID
    pub memory_id: String,
    /// Cosine similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// Prediction error (1.0 - similarity)
    pub prediction_error: f32,
    /// Semantic overlap (estimated shared concepts)
    pub semantic_overlap: f32,
    /// Whether contents appear contradictory
    pub appears_contradictory: bool,
}

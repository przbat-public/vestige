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
    /// How strong the contradiction evidence is (0.0 - 1.0), fused over the
    /// detector's signals.
    ///
    /// Carried separately from [`Self::appears_contradictory`] because the two
    /// are held to different bars: flagging a memory as possibly contradicted is
    /// cheap, while the caller that reports it to the writer wants to know how
    /// much the report is worth.
    pub contradiction_confidence: f32,
    /// What fired the detector, in the shape a response can carry.
    ///
    /// A bare "these disagree" is not actionable: the writer has to see which
    /// rule fired and on what text to judge whether the claim really is denied.
    pub contradiction_evidence: Vec<super::decision::GateFinding>,
}

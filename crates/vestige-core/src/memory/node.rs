//! Knowledge Node - The fundamental unit of memory
//!
//! Each node represents a discrete piece of knowledge with:
//! - Content and metadata
//! - FSRS-6 scheduling state
//! - Dual-strength retention model
//! - Temporal validity (bi-temporal)
//! - Embedding metadata

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// NODE TYPES
// ============================================================================

/// Types of knowledge nodes
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    /// A discrete fact or piece of information
    #[default]
    Fact,
    /// A concept or abstract idea
    Concept,
    /// A procedure or how-to knowledge
    Procedure,
    /// An event or experience
    Event,
    /// A relationship between entities
    Relationship,
    /// A quote or verbatim text
    Quote,
    /// Code or technical snippet
    Code,
    /// A question to be answered
    Question,
    /// User insight or reflection
    Insight,
}

impl NodeType {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::Fact => "fact",
            NodeType::Concept => "concept",
            NodeType::Procedure => "procedure",
            NodeType::Event => "event",
            NodeType::Relationship => "relationship",
            NodeType::Quote => "quote",
            NodeType::Code => "code",
            NodeType::Question => "question",
            NodeType::Insight => "insight",
        }
    }

    /// Parse from string name
    pub fn parse_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fact" => NodeType::Fact,
            "concept" => NodeType::Concept,
            "procedure" => NodeType::Procedure,
            "event" => NodeType::Event,
            "relationship" => NodeType::Relationship,
            "quote" => NodeType::Quote,
            "code" => NodeType::Code,
            "question" => NodeType::Question,
            "insight" => NodeType::Insight,
            _ => NodeType::Fact,
        }
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// MEMORY KIND (ENGRAM-style typed memory)
// ============================================================================

/// Memory kind classifies *how* the content is shaped and retrieved.
///
/// Orthogonal to [`NodeType`]: a NodeType says what the content is *about*
/// (a fact, a procedure, an event), while a MemoryKind says how the content
/// was *recorded* — was it raw input, or was it distilled by an extractor
/// into an atomic, self-contained statement?
///
/// Per-kind retrieval (route temporal questions to `Episodic`, attribute
/// questions to `Semantic`, etc.) is opt-in and lives in the search layer;
/// this enum just carries the metadata so the search layer can filter on
/// it without reparsing content.
///
/// LoCoMo PoC results (full N=1540): replacing raw chunks with extracted
/// facts alone loses −6.88 pp. Augmenting raw chunks with extracted facts
/// side-by-side wins +3.96 pp. See `docs/TYPED-MEMORY-DESIGN.md`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MemoryKind {
    /// Raw input — dialogue, chunks, anything not classified by an
    /// extractor. Default for back-compat: existing memories migrate here.
    #[default]
    Raw,
    /// A stable attribute or relationship ("Caroline lives in Berlin").
    /// Carries `subject`, `predicate`, `object`.
    Semantic,
    /// A specific event tied to a date/time ("Caroline attended X on Y").
    /// Carries `subject` and `episodic_at`.
    Episodic,
    /// A habit, frequency, or recurring pattern ("X goes to Y every Tuesday").
    /// Carries `subject` and `procedural_frequency`.
    Procedural,
    /// A multi-item enumeration captured as one statement. Solves
    /// list-style questions ("What activities does X do?"). Carries
    /// `subject` only; items live in `content`.
    Aggregate,
}

impl MemoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryKind::Raw => "raw",
            MemoryKind::Semantic => "semantic",
            MemoryKind::Episodic => "episodic",
            MemoryKind::Procedural => "procedural",
            MemoryKind::Aggregate => "aggregate",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "semantic" => MemoryKind::Semantic,
            "episodic" => MemoryKind::Episodic,
            "procedural" => MemoryKind::Procedural,
            "aggregate" => MemoryKind::Aggregate,
            _ => MemoryKind::Raw,
        }
    }
}

impl std::fmt::Display for MemoryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// KNOWLEDGE NODE
// ============================================================================

/// A knowledge node in the memory graph
///
/// Combines multiple memory science models:
/// - FSRS-6 for optimal review scheduling
/// - Bjork dual-strength for realistic forgetting
/// - Temporal validity for time-sensitive knowledge
///
/// **API stability:** This struct is exhaustively constructible while
/// `vestige-core` is pre-1.0. When stabilising we may reintroduce
/// `#[non_exhaustive]`; until then tests, fixtures and downstream crates may
/// build instances with `KnowledgeNode { ..Default::default() }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeNode {
    /// Unique identifier (UUID v4)
    pub id: String,
    /// The actual content/knowledge
    pub content: String,
    /// Type of knowledge (fact, concept, procedure, etc.)
    pub node_type: String,
    /// When the node was created
    pub created_at: DateTime<Utc>,
    /// When the node was last modified
    pub updated_at: DateTime<Utc>,
    /// When the node was last accessed/reviewed
    pub last_accessed: DateTime<Utc>,

    // ========== FSRS-6 State (21 parameters) ==========
    /// Memory stability (days until 90% forgetting probability)
    pub stability: f64,
    /// Inherent difficulty (1.0 = easy, 10.0 = hard)
    pub difficulty: f64,
    /// Number of successful reviews
    pub reps: i32,
    /// Number of lapses (forgotten after learning)
    pub lapses: i32,

    // ========== Dual-Strength Model (Bjork & Bjork 1992) ==========
    /// Storage strength - accumulated with practice, never decays
    pub storage_strength: f64,
    /// Retrieval strength - current accessibility, decays over time
    pub retrieval_strength: f64,
    /// Combined retention score (0.0 - 1.0)
    pub retention_strength: f64,

    // ========== Emotional Memory ==========
    /// Sentiment polarity (-1.0 to 1.0)
    pub sentiment_score: f64,
    /// Sentiment intensity (0.0 to 1.0) - affects stability
    pub sentiment_magnitude: f64,

    // ========== Scheduling ==========
    /// Next scheduled review date
    pub next_review: Option<DateTime<Utc>>,

    // ========== Provenance ==========
    /// Source of the knowledge (URL, file, conversation, etc.)
    pub source: Option<String>,
    /// Tags for categorization
    pub tags: Vec<String>,

    // ========== Temporal Memory (Bi-temporal) ==========
    /// When this knowledge became valid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    /// When this knowledge stops being valid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,

    // ========== Utility Tracking (MemRL v1.9.0) ==========
    /// Utility score = times_useful / times_retrieved (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utility_score: Option<f64>,
    /// Number of times this memory was retrieved in search
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times_retrieved: Option<i32>,
    /// Number of times this memory was subsequently useful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times_useful: Option<i32>,

    // ========== Emotional Memory (v2.0.0) ==========
    /// Emotional valence: -1.0 (negative) to 1.0 (positive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotional_valence: Option<f64>,
    /// Flashbulb memory flag: ultra-high-fidelity encoding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flashbulb: Option<bool>,

    // ========== Temporal Hierarchy (v2.0.0) ==========
    /// Temporal level for summary nodes: None=leaf, "daily"/"weekly"/"monthly"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal_level: Option<String>,

    // ========== Semantic Embedding ==========
    /// Whether this node has an embedding vector
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_embedding: Option<bool>,
    /// Which model generated the embedding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,

    // ========== Provenance (v3.1.0 Content Intelligence) ==========
    /// Structured provenance metadata: session, agent, derivation chain, preprocessing artifacts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<serde_json::Value>,

    // ========== Typed Memory (v3.3.0 — ENGRAM-style) ==========
    /// Memory kind classifies *how* the content is shaped and retrieved.
    /// Defaults to `Raw` for back-compat; extractors set this to a more
    /// specific kind when they distill content into atomic facts.
    #[serde(default)]
    pub memory_kind: MemoryKind,
    /// Subject the fact is about (proper noun like "Caroline"). Only set
    /// for non-Raw kinds, used by subject-aware retrieval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Predicate for semantic facts ("lives in", "has", "is"). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    /// Object for semantic facts ("Berlin", "a dog named Max"). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// Absolute timestamp for `Episodic` memories. Anchors "when did X happen"
    /// queries even when the content uses relative phrasing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodic_at: Option<DateTime<Utc>>,
    /// Frequency descriptor for `Procedural` memories ("weekly",
    /// "every Tuesday", "monthly"). Free-form string for now; later
    /// versions may parse into a structured Frequency enum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedural_frequency: Option<String>,
}

impl Default for KnowledgeNode {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            content: String::new(),
            node_type: "fact".to_string(),
            created_at: now,
            updated_at: now,
            last_accessed: now,
            stability: 2.5,
            difficulty: 5.0,
            reps: 0,
            lapses: 0,
            storage_strength: 1.0,
            retrieval_strength: 1.0,
            retention_strength: 1.0,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            next_review: None,
            source: None,
            tags: vec![],
            valid_from: None,
            valid_until: None,
            utility_score: None,
            times_retrieved: None,
            times_useful: None,
            emotional_valence: None,
            flashbulb: None,
            temporal_level: None,
            has_embedding: None,
            embedding_model: None,
            provenance: None,
            memory_kind: MemoryKind::Raw,
            subject: None,
            predicate: None,
            object: None,
            episodic_at: None,
            procedural_frequency: None,
        }
    }
}

impl KnowledgeNode {
    /// Create a new knowledge node with the given content
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Default::default()
        }
    }

    /// Check if this node is currently valid (within temporal bounds)
    pub fn is_valid_at(&self, time: DateTime<Utc>) -> bool {
        let after_start = self.valid_from.map(|t| time >= t).unwrap_or(true);
        let before_end = self.valid_until.map(|t| time <= t).unwrap_or(true);
        after_start && before_end
    }

    /// Check if this node is currently valid (now)
    pub fn is_currently_valid(&self) -> bool {
        self.is_valid_at(Utc::now())
    }

    /// Check if this node is due for review
    pub fn is_due(&self) -> bool {
        self.next_review.map(|t| t <= Utc::now()).unwrap_or(true)
    }

    /// Get the parsed node type
    pub fn get_node_type(&self) -> NodeType {
        NodeType::parse_name(&self.node_type)
    }

    /// Bayesian confidence estimate from Beta(1 + useful, 1 + retrieved - useful).
    /// Returns 95% credible interval and posterior mean.
    pub fn confidence(&self) -> super::ConfidenceEstimate {
        super::ConfidenceEstimate::from_counts(
            self.times_retrieved.unwrap_or(0),
            self.times_useful.unwrap_or(0),
        )
    }

    /// Classify this memory into Tulving's memory systems (episodic/semantic/procedural).
    /// Inferred from `node_type`.
    pub fn memory_system(&self) -> super::MemorySystem {
        match self.get_node_type() {
            NodeType::Event | NodeType::Quote => super::MemorySystem::Episodic,
            NodeType::Procedure | NodeType::Code => super::MemorySystem::Procedural,
            _ => super::MemorySystem::Semantic,
        }
    }

    /// Infer epistemic status from node type, tags, and content signals.
    ///
    /// Hindsight's World/Experience/Observation/Opinion classification helps
    /// the agent calibrate trust in retrieved memories.
    pub fn epistemic_status(&self) -> super::EpistemicStatus {
        let lower_content = self.content.to_lowercase();
        let tags_lower: Vec<String> = self.tags.iter().map(|t| t.to_lowercase()).collect();

        if tags_lower
            .iter()
            .any(|t| t == "preference" || t == "opinion")
        {
            return super::EpistemicStatus::Opinion;
        }
        if tags_lower
            .iter()
            .any(|t| t == "pattern" || t == "observation")
        {
            return super::EpistemicStatus::Observation;
        }

        if lower_content.starts_with("i think")
            || lower_content.starts_with("i prefer")
            || lower_content.starts_with("i believe")
            || lower_content.starts_with("imo ")
            || lower_content.contains("in my opinion")
        {
            return super::EpistemicStatus::Opinion;
        }

        if lower_content.starts_with("i noticed")
            || lower_content.starts_with("it seems")
            || lower_content.starts_with("it appears")
            || lower_content.contains("pattern:")
        {
            return super::EpistemicStatus::Observation;
        }

        match self.get_node_type() {
            NodeType::Event | NodeType::Quote => super::EpistemicStatus::Experience,
            NodeType::Insight => super::EpistemicStatus::Observation,
            _ => super::EpistemicStatus::World,
        }
    }
}

// ============================================================================
// INPUT TYPES
// ============================================================================

/// Input for creating a new memory
///
/// Uses `deny_unknown_fields` to prevent field injection attacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IngestInput {
    /// The content to memorize
    pub content: String,
    /// Type of knowledge (fact, concept, procedure, etc.)
    pub node_type: String,
    /// Source of the knowledge
    pub source: Option<String>,
    /// Sentiment polarity (-1.0 to 1.0)
    #[serde(default)]
    pub sentiment_score: f64,
    /// Sentiment intensity (0.0 to 1.0)
    #[serde(default)]
    pub sentiment_magnitude: f64,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// When this knowledge becomes valid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    /// When this knowledge stops being valid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    /// Structured provenance metadata (JSON): session_id, agent, derived_from, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<serde_json::Value>,

    // ========== Typed Memory ==========
    /// Memory kind. Defaults to `Raw`; extractors set a specific kind.
    #[serde(default)]
    pub memory_kind: MemoryKind,
    /// Subject for non-Raw kinds (proper noun like "Caroline"). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Predicate for semantic facts ("lives in"). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    /// Object for semantic facts ("Berlin"). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// Absolute event timestamp for episodic memories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodic_at: Option<DateTime<Utc>>,
    /// Frequency descriptor for procedural memories ("weekly", "every Tuesday").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedural_frequency: Option<String>,
}

impl Default for IngestInput {
    fn default() -> Self {
        Self {
            content: String::new(),
            node_type: "fact".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec![],
            valid_from: None,
            valid_until: None,
            provenance: None,
            memory_kind: MemoryKind::Raw,
            subject: None,
            predicate: None,
            object: None,
            episodic_at: None,
            procedural_frequency: None,
        }
    }
}

/// Search mode for recall queries
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    /// Keyword search only (FTS5/BM25)
    Keyword,
    /// Semantic search only (embeddings)
    Semantic,
    /// Hybrid search with RRF fusion (default, best results)
    #[default]
    Hybrid,
}

/// Input for recalling memories
///
/// Uses `deny_unknown_fields` to prevent field injection attacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecallInput {
    /// Search query
    pub query: String,
    /// Maximum results to return
    pub limit: i32,
    /// Minimum retention strength (0.0 to 1.0)
    #[serde(default)]
    pub min_retention: f64,
    /// Search mode (keyword, semantic, or hybrid)
    #[serde(default)]
    pub search_mode: SearchMode,
    /// Only return results valid at this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_at: Option<DateTime<Utc>>,
}

impl Default for RecallInput {
    fn default() -> Self {
        Self {
            query: String::new(),
            limit: 10,
            min_retention: 0.0,
            search_mode: SearchMode::Hybrid,
            valid_at: None,
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_type_roundtrip() {
        for node_type in [
            NodeType::Fact,
            NodeType::Concept,
            NodeType::Procedure,
            NodeType::Event,
            NodeType::Code,
        ] {
            assert_eq!(NodeType::parse_name(node_type.as_str()), node_type);
        }
    }

    #[test]
    fn test_memory_kind_roundtrip() {
        for kind in [
            MemoryKind::Raw,
            MemoryKind::Semantic,
            MemoryKind::Episodic,
            MemoryKind::Procedural,
            MemoryKind::Aggregate,
        ] {
            assert_eq!(MemoryKind::parse(kind.as_str()), kind);
        }
        // Unknown / malformed strings fall back to Raw for back-compat.
        assert_eq!(MemoryKind::parse(""), MemoryKind::Raw);
        assert_eq!(MemoryKind::parse("garbage"), MemoryKind::Raw);
        assert_eq!(MemoryKind::parse("SEMANTIC"), MemoryKind::Semantic);
    }

    #[test]
    fn test_memory_kind_default_is_raw() {
        let node = KnowledgeNode::default();
        assert_eq!(node.memory_kind, MemoryKind::Raw);
        assert!(node.subject.is_none());
        assert!(node.episodic_at.is_none());
        assert!(node.procedural_frequency.is_none());

        let input = IngestInput::default();
        assert_eq!(input.memory_kind, MemoryKind::Raw);
        assert!(input.subject.is_none());
    }

    #[test]
    fn test_memory_kind_serde() {
        use serde_json;
        let json = serde_json::to_string(&MemoryKind::Episodic).unwrap();
        assert_eq!(json, "\"episodic\"");
        let parsed: MemoryKind = serde_json::from_str("\"semantic\"").unwrap();
        assert_eq!(parsed, MemoryKind::Semantic);
    }

    #[test]
    fn test_knowledge_node_default() {
        let node = KnowledgeNode::default();
        assert!(node.id.is_empty());
        assert_eq!(node.node_type, "fact");
        assert!(node.is_due());
        assert!(node.is_currently_valid());
    }

    #[test]
    fn test_temporal_validity() {
        let mut node = KnowledgeNode::default();
        let now = Utc::now();

        // No bounds = always valid
        assert!(node.is_valid_at(now));

        // Set future valid_from = not valid now
        node.valid_from = Some(now + chrono::Duration::days(1));
        assert!(!node.is_valid_at(now));

        // Set past valid_from = valid now
        node.valid_from = Some(now - chrono::Duration::days(1));
        assert!(node.is_valid_at(now));

        // Set past valid_until = not valid now
        node.valid_until = Some(now - chrono::Duration::hours(1));
        assert!(!node.is_valid_at(now));
    }

    #[test]
    fn test_ingest_input_deny_unknown_fields() {
        // Valid input should parse
        let json = r#"{"content": "test", "nodeType": "fact", "tags": []}"#;
        let result: Result<IngestInput, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        // Unknown field should fail (security feature)
        let json_with_unknown =
            r#"{"content": "test", "nodeType": "fact", "tags": [], "malicious_field": "attack"}"#;
        let result: Result<IngestInput, _> = serde_json::from_str(json_with_unknown);
        assert!(result.is_err());
    }
}

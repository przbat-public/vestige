//! Dream-cycle data types: result, config, insights, contradictions, memories, connections.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::constants::{MAX_INSIGHTS_PER_DREAM, MIN_NOVELTY_SCORE, MIN_SIMILARITY_FOR_CONNECTION};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamResult {
    /// Number of new connections discovered
    pub new_connections_found: usize,
    /// Number of memories that were strengthened
    pub memories_strengthened: usize,
    /// Number of memories that were compressed
    pub memories_compressed: usize,
    /// Insights generated during the dream
    pub insights_generated: Vec<SynthesizedInsight>,
    /// Contradictions detected (pairs of memory IDs with conflicting info)
    pub contradictions_found: Vec<ContradictionPair>,
    /// Memory IDs demoted by active forgetting (low retention + superseded)
    pub memories_demoted: Vec<String>,
    /// Dream cycle duration in milliseconds
    pub duration_ms: u64,
    /// Timestamp of the dream
    pub dreamed_at: DateTime<Utc>,
    /// Statistics about the dream
    pub stats: DreamStats,
}

/// A detected contradiction between two memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionPair {
    /// The newer / more-accessed memory (likely correct)
    pub survivor_id: String,
    /// The older / less-accessed memory (candidate for demotion)
    pub demoted_id: String,
    /// Similarity between the two (high sim + different content = contradiction)
    pub similarity: f64,
    /// Short explanation
    pub reason: String,
}

/// Statistics from a dream cycle
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DreamStats {
    /// Memories analyzed
    pub memories_analyzed: usize,
    /// Potential connections evaluated
    pub connections_evaluated: usize,
    /// Pattern clusters found
    pub clusters_found: usize,
    /// Candidate insights considered
    pub candidates_considered: usize,
}

/// A synthesized insight from memory combination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizedInsight {
    /// Unique ID for this insight
    pub id: String,
    /// The insight itself
    pub insight: String,
    /// Memory IDs that contributed to this insight
    pub source_memories: Vec<String>,
    /// Confidence in this insight (0.0 to 1.0)
    pub confidence: f64,
    /// Novelty score - how "new" is this insight (0.0 to 1.0)
    pub novelty_score: f64,
    /// Category/type of insight
    pub insight_type: InsightType,
    /// When this insight was generated
    pub generated_at: DateTime<Utc>,
    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Types of insights that can be generated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InsightType {
    /// Connection between seemingly unrelated concepts
    HiddenConnection,
    /// Recurring pattern across memories
    RecurringPattern,
    /// Generalization from specific examples
    Generalization,
    /// Contradiction or tension between memories
    Contradiction,
    /// Gap in knowledge that should be filled
    KnowledgeGap,
    /// Trend or evolution over time
    TemporalTrend,
    /// Synthesis of multiple sources
    Synthesis,
}

impl InsightType {
    /// Get description of insight type
    pub fn description(&self) -> &str {
        match self {
            Self::HiddenConnection => "Hidden connection discovered between concepts",
            Self::RecurringPattern => "Recurring pattern identified across memories",
            Self::Generalization => "General principle derived from specific cases",
            Self::Contradiction => "Potential contradiction detected",
            Self::KnowledgeGap => "Gap in knowledge identified",
            Self::TemporalTrend => "Trend or evolution observed over time",
            Self::Synthesis => "New understanding from combining sources",
        }
    }
}

/// Configuration for dream cycles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamConfig {
    /// Maximum memories to analyze per dream
    pub max_memories_per_dream: usize,
    /// Minimum similarity for connection discovery
    pub min_similarity: f64,
    /// Maximum insights to generate
    pub max_insights: usize,
    /// Minimum novelty required for insights
    pub min_novelty: f64,
    /// Enable compression during dreams
    pub enable_compression: bool,
    /// Enable strengthening during dreams
    pub enable_strengthening: bool,
    /// Focus on specific tags (empty = all)
    pub focus_tags: Vec<String>,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            max_memories_per_dream: 1000,
            min_similarity: MIN_SIMILARITY_FOR_CONNECTION,
            max_insights: MAX_INSIGHTS_PER_DREAM,
            min_novelty: MIN_NOVELTY_SCORE,
            enable_compression: true,
            enable_strengthening: true,
            focus_tags: vec![],
        }
    }
}

/// Memory input for dreaming
#[derive(Debug, Clone)]
pub struct DreamMemory {
    /// Memory ID
    pub id: String,
    /// Memory content
    pub content: String,
    /// Embedding vector
    pub embedding: Option<Vec<f32>>,
    /// Tags
    pub tags: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Access count
    pub access_count: u32,
}

/// A discovered connection between memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredConnection {
    /// Source memory ID
    pub from_id: String,
    /// Target memory ID
    pub to_id: String,
    /// Similarity score
    pub similarity: f64,
    /// Type of connection discovered
    pub connection_type: DiscoveredConnectionType,
    /// Reasoning for this connection
    pub reasoning: String,
}

/// Types of connections discovered during dreaming
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DiscoveredConnectionType {
    /// Semantic similarity
    Semantic,
    /// Shared concepts/entities
    SharedConcept,
    /// Temporal correlation
    Temporal,
    /// Complementary information
    Complementary,
    /// Cause-effect relationship
    CausalChain,
    /// Contradictory information (active forgetting candidate)
    Contradiction,
}

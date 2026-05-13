//! Compact in-memory index entries plus the queries and matches used to walk
//! them. Mirrors the "hippocampal" half of the indexing theory.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{ContentPointer, ImportanceFlags, IndexLink, MemoryBarcode, TemporalMarker};

// ============================================================================
// MEMORY INDEX (The "Hippocampal" Entry)
// ============================================================================

/// Compressed index dimension for semantic summary
/// (Smaller than full embedding for efficiency)
pub const INDEX_EMBEDDING_DIM: usize = 128;

/// Compact index entry - what the "hippocampus" stores
///
/// This is the core data structure that enables fast search.
/// It contains only enough information to:
/// 1. Identify the memory (barcode)
/// 2. Match semantic queries (compressed embedding)
/// 3. Filter by time and importance
/// 4. Find associated memories
/// 5. Locate the full content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryIndex {
    /// Unique identifier (barcode)
    pub barcode: MemoryBarcode,
    /// Original memory ID (e.g., UUID from KnowledgeNode)
    pub memory_id: String,
    /// Compressed semantic embedding (smaller dimension)
    pub semantic_summary: Vec<f32>,
    /// Temporal information
    pub temporal_marker: TemporalMarker,
    /// Pointers to actual content
    pub content_pointers: Vec<ContentPointer>,
    /// Links to associated memories
    pub association_links: Vec<IndexLink>,
    /// Importance flags
    pub importance_flags: ImportanceFlags,
    /// Node type (fact, concept, etc.)
    pub node_type: String,
    /// Brief content preview (first ~100 chars)
    pub preview: String,
}

impl MemoryIndex {
    /// Create a new memory index
    pub fn new(
        barcode: MemoryBarcode,
        memory_id: String,
        node_type: String,
        created_at: DateTime<Utc>,
        preview: String,
    ) -> Self {
        Self {
            barcode,
            memory_id,
            semantic_summary: Vec::new(),
            temporal_marker: TemporalMarker::new(created_at),
            content_pointers: Vec::new(),
            association_links: Vec::new(),
            importance_flags: ImportanceFlags::empty(),
            node_type,
            preview: preview.chars().take(100).collect(),
        }
    }

    /// Set semantic summary (compressed embedding)
    pub fn with_semantic_summary(mut self, summary: Vec<f32>) -> Self {
        self.semantic_summary = summary;
        self
    }

    /// Add a content pointer
    pub fn add_content_pointer(&mut self, pointer: ContentPointer) {
        self.content_pointers.push(pointer);
    }

    /// Add an association link
    pub fn add_link(&mut self, link: IndexLink) {
        // Check for existing link to same target
        if let Some(existing) = self
            .association_links
            .iter_mut()
            .find(|l| l.target_barcode == link.target_barcode)
        {
            // Strengthen existing link
            existing.strengthen(link.strength * 0.5);
        } else {
            self.association_links.push(link);
        }
    }

    /// Remove weak links (below threshold)
    pub fn prune_weak_links(&mut self, threshold: f32) {
        self.association_links.retain(|l| l.strength >= threshold);
    }

    /// Record an access event
    pub fn record_access(&mut self) {
        self.temporal_marker.record_access();

        // Update importance flags based on access patterns
        if self.temporal_marker.access_count > 10 {
            self.importance_flags.set_frequently_accessed(true);
        }
    }

    /// Get total size of all content (for memory estimation)
    pub fn estimated_content_size(&self) -> usize {
        self.content_pointers
            .iter()
            .filter_map(|p| p.size_bytes)
            .sum()
    }

    /// Check if this index matches importance criteria
    pub fn matches_importance(&self, min_flags: u32) -> bool {
        self.importance_flags.to_bits() & min_flags == min_flags
    }
}

// ============================================================================
// INDEX QUERY
// ============================================================================

/// Query for searching the index
#[derive(Debug, Clone)]
pub struct IndexQuery {
    /// Semantic query embedding (optional)
    pub semantic_embedding: Option<Vec<f32>>,
    /// Text query (for preview matching)
    pub text_query: Option<String>,
    /// Time range filter
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Required importance flags
    pub required_flags: Option<ImportanceFlags>,
    /// Node type filter
    pub node_types: Option<Vec<String>>,
    /// Minimum semantic similarity threshold
    pub min_similarity: f32,
    /// Maximum results
    pub limit: usize,
}

impl IndexQuery {
    /// Create query from text
    pub fn from_text(query: &str) -> Self {
        Self {
            semantic_embedding: None,
            text_query: Some(query.to_string()),
            time_range: None,
            required_flags: None,
            node_types: None,
            min_similarity: 0.3,
            limit: 10,
        }
    }

    /// Create query from embedding
    pub fn from_embedding(embedding: Vec<f32>) -> Self {
        Self {
            semantic_embedding: Some(embedding),
            text_query: None,
            time_range: None,
            required_flags: None,
            node_types: None,
            min_similarity: 0.3,
            limit: 10,
        }
    }

    /// Set time range filter
    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.time_range = Some((start, end));
        self
    }

    /// Set required importance flags
    pub fn with_required_flags(mut self, flags: ImportanceFlags) -> Self {
        self.required_flags = Some(flags);
        self
    }

    /// Set node type filter
    pub fn with_node_types(mut self, types: Vec<String>) -> Self {
        self.node_types = Some(types);
        self
    }

    /// Set minimum similarity
    pub fn with_min_similarity(mut self, threshold: f32) -> Self {
        self.min_similarity = threshold;
        self
    }

    /// Set result limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

impl Default for IndexQuery {
    fn default() -> Self {
        Self {
            semantic_embedding: None,
            text_query: None,
            time_range: None,
            required_flags: None,
            node_types: None,
            min_similarity: 0.3,
            limit: 10,
        }
    }
}

// ============================================================================
// INDEX MATCH
// ============================================================================

/// Result of an index search
#[derive(Debug, Clone)]
pub struct IndexMatch {
    /// The matched index entry
    pub index: MemoryIndex,
    /// Semantic similarity score (0.0 to 1.0)
    pub semantic_score: f32,
    /// Text match score (0.0 to 1.0)
    pub text_score: f32,
    /// Temporal relevance score (0.0 to 1.0)
    pub temporal_score: f32,
    /// Importance score (0.0 to 1.0)
    pub importance_score: f32,
    /// Combined relevance score
    pub combined_score: f32,
}

impl IndexMatch {
    /// Create a new index match
    pub fn new(index: MemoryIndex) -> Self {
        Self {
            index,
            semantic_score: 0.0,
            text_score: 0.0,
            temporal_score: 0.0,
            importance_score: 0.0,
            combined_score: 0.0,
        }
    }

    /// Calculate combined score with weights
    pub fn calculate_combined(
        &mut self,
        semantic_weight: f32,
        text_weight: f32,
        temporal_weight: f32,
        importance_weight: f32,
    ) {
        self.combined_score = self.semantic_score * semantic_weight
            + self.text_score * text_weight
            + self.temporal_score * temporal_weight
            + self.importance_score * importance_weight;
    }
}

// ============================================================================
// FULL MEMORY (Retrieved Content)
// ============================================================================

/// Complete memory with all content retrieved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullMemory {
    /// The index entry
    pub barcode: MemoryBarcode,
    /// Original memory ID
    pub memory_id: String,
    /// Full text content
    pub content: String,
    /// Node type
    pub node_type: String,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last accessed time
    pub last_accessed: DateTime<Utc>,
    /// Full embedding (if available)
    pub embedding: Option<Vec<f32>>,
    /// All tags
    pub tags: Vec<String>,
    /// Source information
    pub source: Option<String>,
    /// FSRS scheduling state
    pub stability: f64,
    pub difficulty: f64,
    pub next_review: Option<DateTime<Utc>>,
    /// Retention strength
    pub retention_strength: f64,
}

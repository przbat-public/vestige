//! Labile state, snapshot, and modification types for memory reconsolidation.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::constants::MAX_MODIFICATIONS_PER_WINDOW;
use super::context::AccessContext;
use super::helpers::truncate;

/// State of a memory that has become labile (modifiable) after access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabileState {
    /// Memory ID
    pub memory_id: String,
    /// When the memory was accessed (became labile)
    pub accessed_at: DateTime<Utc>,
    /// Snapshot of the original memory state
    pub original_state: MemorySnapshot,
    /// Modifications applied during labile window
    pub modifications: Vec<Modification>,
    /// Access context (what triggered the retrieval)
    pub access_context: Option<AccessContext>,
    /// Whether this memory has been reconsolidated
    pub reconsolidated: bool,
}

impl LabileState {
    /// Create a new labile state for a memory
    pub fn new(memory_id: String, original: MemorySnapshot) -> Self {
        Self {
            memory_id,
            accessed_at: Utc::now(),
            original_state: original,
            modifications: Vec::new(),
            access_context: None,
            reconsolidated: false,
        }
    }

    /// Check if still within labile window
    pub fn is_within_window(&self, window: Duration) -> bool {
        Utc::now() - self.accessed_at < window
    }

    /// Add a modification
    pub fn add_modification(&mut self, modification: Modification) -> bool {
        if self.modifications.len() < MAX_MODIFICATIONS_PER_WINDOW {
            self.modifications.push(modification);
            true
        } else {
            false
        }
    }

    /// Set access context
    pub fn with_context(mut self, context: AccessContext) -> Self {
        self.access_context = Some(context);
        self
    }
}

/// Snapshot of a memory's state before modification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Memory content at time of access
    pub content: String,
    /// Tags at time of access
    pub tags: Vec<String>,
    /// Retention strength at time of access
    pub retention_strength: f64,
    /// Storage strength at time of access
    pub storage_strength: f64,
    /// Retrieval strength at time of access
    pub retrieval_strength: f64,
    /// Connection IDs at time of access
    pub connection_ids: Vec<String>,
    /// Snapshot timestamp
    pub captured_at: DateTime<Utc>,
}

impl MemorySnapshot {
    /// Create a snapshot from memory data
    pub fn capture(
        content: String,
        tags: Vec<String>,
        retention_strength: f64,
        storage_strength: f64,
        retrieval_strength: f64,
        connection_ids: Vec<String>,
    ) -> Self {
        Self {
            content,
            tags,
            retention_strength,
            storage_strength,
            retrieval_strength,
            connection_ids,
            captured_at: Utc::now(),
        }
    }
}

// ============================================================================
// MODIFICATIONS
// ============================================================================

/// Types of modifications that can be applied during the labile window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Modification {
    /// Add contextual information
    AddContext {
        /// New context to add
        context: String,
    },
    /// Strengthen connection to another memory
    StrengthenConnection {
        /// Connected memory ID
        target_memory_id: String,
        /// Strength boost (0.0 to 1.0)
        boost: f64,
    },
    /// Add a new tag
    AddTag {
        /// Tag to add
        tag: String,
    },
    /// Remove a tag
    RemoveTag {
        /// Tag to remove
        tag: String,
    },
    /// Update emotional association
    UpdateEmotion {
        /// New sentiment score (-1.0 to 1.0)
        sentiment_score: Option<f64>,
        /// New sentiment magnitude (0.0 to 1.0)
        sentiment_magnitude: Option<f64>,
    },
    /// Link to related memory
    LinkMemory {
        /// Memory to link to
        related_memory_id: String,
        /// Type of relationship
        relationship: RelationshipType,
    },
    /// Correct or update content
    UpdateContent {
        /// Updated content (or None to keep original)
        new_content: Option<String>,
        /// Whether this is a correction
        is_correction: bool,
    },
    /// Add source/provenance information
    AddSource {
        /// Source information
        source: String,
    },
    /// Boost retrieval strength (successful recall)
    BoostRetrieval {
        /// Boost amount
        boost: f64,
    },
}

/// Types of relationships between memories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelationshipType {
    /// Memory A supports/reinforces Memory B
    Supports,
    /// Memory A contradicts Memory B
    Contradicts,
    /// Memory A is an elaboration of Memory B
    Elaborates,
    /// Memory A is a generalization of Memory B
    Generalizes,
    /// Memory A is a specific example of Memory B
    Exemplifies,
    /// Memory A is temporally related to Memory B
    TemporallyRelated,
    /// Memory A caused Memory B
    Causes,
    /// General semantic similarity
    SimilarTo,
}

impl Modification {
    /// Get a description of this modification
    pub fn description(&self) -> String {
        match self {
            Self::AddContext { context } => format!("Add context: {}", truncate(context, 50)),
            Self::StrengthenConnection {
                target_memory_id,
                boost,
            } => format!(
                "Strengthen connection to {} by {:.2}",
                target_memory_id, boost
            ),
            Self::AddTag { tag } => format!("Add tag: {}", tag),
            Self::RemoveTag { tag } => format!("Remove tag: {}", tag),
            Self::UpdateEmotion {
                sentiment_score,
                sentiment_magnitude,
            } => format!(
                "Update emotion: score={:?}, magnitude={:?}",
                sentiment_score, sentiment_magnitude
            ),
            Self::LinkMemory {
                related_memory_id,
                relationship,
            } => format!("Link to {} ({:?})", related_memory_id, relationship),
            Self::UpdateContent { is_correction, .. } => {
                format!("Update content (correction={})", is_correction)
            }
            Self::AddSource { source } => format!("Add source: {}", truncate(source, 50)),
            Self::BoostRetrieval { boost } => format!("Boost retrieval by {:.2}", boost),
        }
    }
}

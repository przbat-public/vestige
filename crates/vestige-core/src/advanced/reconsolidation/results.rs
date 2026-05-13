//! Result types: applied modifications, change summary, retrieval history record.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::context::AccessContext;
use super::labile::Modification;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconsolidatedMemory {
    /// Memory ID
    pub memory_id: String,
    /// When reconsolidation occurred
    pub reconsolidated_at: DateTime<Utc>,
    /// Duration of labile window
    pub labile_duration: Duration,
    /// Modifications that were applied
    pub applied_modifications: Vec<AppliedModification>,
    /// Whether any modifications were made
    pub was_modified: bool,
    /// Summary of changes
    pub change_summary: ChangeSummary,
    /// New retrieval count
    pub retrieval_count: u32,
}

/// A modification that was successfully applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedModification {
    /// The modification
    pub modification: Modification,
    /// When it was applied
    pub applied_at: DateTime<Utc>,
    /// Whether it succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Summary of changes made during reconsolidation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// Number of tags added
    pub tags_added: usize,
    /// Number of tags removed
    pub tags_removed: usize,
    /// Number of connections strengthened
    pub connections_strengthened: usize,
    /// Number of new links created
    pub links_created: usize,
    /// Whether content was updated
    pub content_updated: bool,
    /// Whether emotion was updated
    pub emotion_updated: bool,
    /// Total retrieval boost applied
    pub retrieval_boost: f64,
}

impl ChangeSummary {
    /// Check if any changes were made
    pub fn has_changes(&self) -> bool {
        self.tags_added > 0
            || self.tags_removed > 0
            || self.connections_strengthened > 0
            || self.links_created > 0
            || self.content_updated
            || self.emotion_updated
            || self.retrieval_boost > 0.0
    }
}

// ============================================================================
// RETRIEVAL HISTORY
// ============================================================================

/// Record of a memory retrieval event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalRecord {
    /// Memory ID
    pub memory_id: String,
    /// When retrieval occurred
    pub retrieved_at: DateTime<Utc>,
    /// Access context
    pub context: Option<AccessContext>,
    /// Whether memory was modified during labile window
    pub was_modified: bool,
    /// Retrieval strength at time of access
    pub retrieval_strength_at_access: f64,
}

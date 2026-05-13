//! Access context describing how/why a memory was retrieved.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessContext {
    /// What triggered the retrieval
    pub trigger: AccessTrigger,
    /// Search query if applicable
    pub query: Option<String>,
    /// Other memories retrieved in same session
    pub co_retrieved: Vec<String>,
    /// Session or task identifier
    pub session_id: Option<String>,
}

/// What triggered memory retrieval
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccessTrigger {
    /// Direct search by user
    Search,
    /// Automatic retrieval (speculative, context-based)
    Automatic,
    /// Consolidation replay
    ConsolidationReplay,
    /// Linked from another memory
    LinkedRetrieval,
    /// User explicitly accessed
    DirectAccess,
    /// Review/study session
    Review,
}

//! `ArchitecturalDecision` and `DecisionStatus`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Records an architectural decision with its rationale.
///
/// Example:
/// - Decision: "Use Event Sourcing for order management"
/// - Rationale: "Need complete audit trail and ability to replay state"
/// - Files: ["src/orders/events.rs", "src/orders/aggregate.rs"]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitecturalDecision {
    pub id: String,
    /// The decision that was made
    pub decision: String,
    /// Why this decision was made
    pub rationale: String,
    /// Files affected by this decision
    pub files_affected: Vec<PathBuf>,
    /// Git commit SHA where this was implemented (if applicable)
    pub commit_sha: Option<String>,
    /// When this decision was recorded
    pub created_at: DateTime<Utc>,
    /// When this decision was last updated
    pub updated_at: Option<DateTime<Utc>>,
    /// Additional context or notes
    pub context: Option<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Status of the decision
    pub status: DecisionStatus,
    /// Alternatives that were considered
    pub alternatives_considered: Vec<String>,
}

/// Status of an architectural decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum DecisionStatus {
    /// Decision is proposed but not yet implemented
    Proposed,
    /// Decision is accepted and being implemented
    #[default]
    Accepted,
    /// Decision has been superseded by another
    Superseded,
    /// Decision was rejected
    Deprecated,
}

impl ArchitecturalDecision {
    pub fn new(id: String, decision: String, rationale: String) -> Self {
        Self {
            id,
            decision,
            rationale,
            files_affected: vec![],
            commit_sha: None,
            created_at: Utc::now(),
            updated_at: None,
            context: None,
            tags: vec![],
            status: DecisionStatus::default(),
            alternatives_considered: vec![],
        }
    }

    pub fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.files_affected = files;
        self
    }

    pub fn with_commit(mut self, sha: String) -> Self {
        self.commit_sha = Some(sha);
        self
    }

    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

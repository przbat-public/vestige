//! `WorkContext` and `WorkStatus`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Tracks the current work context for continuity across sessions.
///
/// This allows Vestige to remember:
/// - What task the user was working on
/// - What files were being edited
/// - What the next steps were
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkContext {
    pub id: String,
    /// Description of the current task
    pub task_description: String,
    /// Files currently being worked on
    pub active_files: Vec<PathBuf>,
    /// Current git branch
    pub branch: Option<String>,
    /// Status of the work
    pub status: WorkStatus,
    /// Next steps that were planned
    pub next_steps: Vec<String>,
    /// Blockers or issues encountered
    pub blockers: Vec<String>,
    /// When this context was created
    pub created_at: DateTime<Utc>,
    /// When this context was last updated
    pub updated_at: DateTime<Utc>,
    /// Related issue/ticket IDs
    pub related_issues: Vec<String>,
    /// Notes about the work
    pub notes: Option<String>,
}

/// Status of work in progress
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkStatus {
    /// Actively being worked on
    InProgress,
    /// Paused, will resume later
    Paused,
    /// Completed
    Completed,
    /// Blocked by something
    Blocked,
    /// Abandoned
    Abandoned,
}

impl WorkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Abandoned => "abandoned",
        }
    }
}

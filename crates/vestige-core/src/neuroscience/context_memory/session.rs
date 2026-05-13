//! Session-scoped context: id, activity, project.

use serde::{Deserialize, Serialize};

// ============================================================================
// SESSION CONTEXT
// ============================================================================

/// Session context captures the SESSION in which encoding occurred
///
/// This helps distinguish memories from different work sessions,
/// even if they occurred on the same day or with similar topics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    /// Unique session identifier
    pub session_id: Option<String>,
    /// Type of activity (coding, research, debugging, etc.)
    pub activity_type: Option<String>,
    /// Current project or workspace
    pub project: Option<String>,
    /// Current file or document being worked on
    pub active_file: Option<String>,
    /// Git branch (for code-related sessions)
    pub git_branch: Option<String>,
    /// Duration of the session so far (in minutes)
    pub session_duration_minutes: Option<u32>,
}

impl SessionContext {
    /// Create a new session context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a session ID
    pub fn with_id(id: impl Into<String>) -> Self {
        Self {
            session_id: Some(id.into()),
            ..Default::default()
        }
    }

    /// Set the activity type
    pub fn set_activity(&mut self, activity: impl Into<String>) {
        self.activity_type = Some(activity.into());
    }

    /// Set the project
    pub fn set_project(&mut self, project: impl Into<String>) {
        self.project = Some(project.into());
    }

    /// Set the active file
    pub fn set_active_file(&mut self, file: impl Into<String>) {
        self.active_file = Some(file.into());
    }

    /// Set the git branch
    pub fn set_branch(&mut self, branch: impl Into<String>) {
        self.git_branch = Some(branch.into());
    }
}

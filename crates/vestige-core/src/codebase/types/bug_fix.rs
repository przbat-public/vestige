//! `BugFix` and `BugSeverity`.

use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Records a bug fix with root cause analysis.
///
/// This is invaluable for:
/// - Preventing regressions
/// - Understanding why certain code exists
/// - Training junior developers on common pitfalls
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BugFix {
    pub id: String,
    /// What symptoms was the bug causing?
    pub symptom: String,
    /// What was the actual root cause?
    pub root_cause: String,
    /// How was it fixed?
    pub solution: String,
    /// Files that were changed to fix the bug
    pub files_changed: Vec<PathBuf>,
    /// Git commit SHA of the fix
    pub commit_sha: String,
    /// When the fix was recorded
    pub created_at: DateTime<Utc>,
    /// Link to issue tracker (if applicable)
    pub issue_link: Option<String>,
    /// Severity of the bug
    pub severity: BugSeverity,
    /// How the bug was discovered
    pub discovered_by: Option<String>,
    /// Prevention measures (what would have caught this earlier)
    pub prevention_notes: Option<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Severity level of a bug
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum BugSeverity {
    Critical,
    High,
    #[default]
    Medium,
    Low,
    Trivial,
}

impl BugFix {
    pub fn new(
        id: String,
        symptom: String,
        root_cause: String,
        solution: String,
        commit_sha: String,
    ) -> Self {
        Self {
            id,
            symptom,
            root_cause,
            solution,
            files_changed: vec![],
            commit_sha,
            created_at: Utc::now(),
            issue_link: None,
            severity: BugSeverity::default(),
            discovered_by: None,
            prevention_notes: None,
            tags: vec![],
        }
    }

    pub fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.files_changed = files;
        self
    }

    pub fn with_severity(mut self, severity: BugSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_issue(mut self, link: String) -> Self {
        self.issue_link = Some(link);
        self
    }
}

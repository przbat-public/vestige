//! Snapshot of the working tree (`GitContext`) and `CommitInfo`.

use std::path::PathBuf;

use chrono::{DateTime, Utc};

/// Current git context for a repository
#[derive(Debug, Clone)]
pub struct GitContext {
    /// Root path of the repository
    pub repo_root: PathBuf,
    /// Current branch name
    pub current_branch: String,
    /// HEAD commit SHA
    pub head_commit: String,
    /// Files with uncommitted changes (unstaged)
    pub uncommitted_changes: Vec<PathBuf>,
    /// Files staged for commit
    pub staged_changes: Vec<PathBuf>,
    /// Recent commits
    pub recent_commits: Vec<CommitInfo>,
    /// Whether the repository has any commits
    pub has_commits: bool,
    /// Whether there are untracked files
    pub has_untracked: bool,
}

/// Information about a git commit
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// Commit SHA (short)
    pub sha: String,
    /// Full commit SHA
    pub full_sha: String,
    /// Commit message (first line)
    pub message: String,
    /// Full commit message
    pub full_message: String,
    /// Author name
    pub author: String,
    /// Author email
    pub author_email: String,
    /// Commit timestamp
    pub timestamp: DateTime<Utc>,
    /// Files changed in this commit
    pub files_changed: Vec<PathBuf>,
    /// Is this a merge commit?
    pub is_merge: bool,
}

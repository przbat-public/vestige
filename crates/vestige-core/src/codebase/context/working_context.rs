//! Aggregated `WorkingContext` and the git slice (`GitContextInfo`).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::codebase::git::GitContext;

use super::framework::Framework;
use super::project_type::ProjectType;

/// Complete working context for memory storage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingContext {
    /// Git context (branch, commits, changes)
    pub git: Option<GitContextInfo>,
    /// Currently active file (e.g., file being edited)
    pub active_file: Option<PathBuf>,
    /// Project type (Rust, TypeScript, etc.)
    pub project_type: ProjectType,
    /// Detected frameworks
    pub frameworks: Vec<Framework>,
    /// Project name (from cargo.toml, package.json, etc.)
    pub project_name: Option<String>,
    /// Project root directory
    pub project_root: PathBuf,
    /// When this context was captured
    pub captured_at: DateTime<Utc>,
    /// Recent files (for context)
    pub recent_files: Vec<PathBuf>,
    /// Key configuration files found
    pub config_files: Vec<PathBuf>,
}

/// Serializable git context info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitContextInfo {
    pub current_branch: String,
    pub head_commit: String,
    pub uncommitted_changes: Vec<PathBuf>,
    pub staged_changes: Vec<PathBuf>,
    pub has_uncommitted: bool,
    pub is_clean: bool,
}

impl From<GitContext> for GitContextInfo {
    fn from(ctx: GitContext) -> Self {
        let has_uncommitted = !ctx.uncommitted_changes.is_empty();
        let is_clean = ctx.uncommitted_changes.is_empty() && ctx.staged_changes.is_empty();

        Self {
            current_branch: ctx.current_branch,
            head_commit: ctx.head_commit,
            uncommitted_changes: ctx.uncommitted_changes,
            staged_changes: ctx.staged_changes,
            has_uncommitted,
            is_clean,
        }
    }
}

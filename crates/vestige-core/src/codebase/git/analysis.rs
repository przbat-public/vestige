//! Result of `GitAnalyzer::analyze_history`.

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::codebase::types::{BugFix, FileRelationship};

/// Result of analyzing git history
#[derive(Debug)]
pub struct HistoryAnalysis {
    /// Bug fixes extracted from commits
    pub bug_fixes: Vec<BugFix>,
    /// File relationships discovered from co-change patterns
    pub file_relationships: Vec<FileRelationship>,
    /// Total commits analyzed
    pub commit_count: usize,
    /// Top contributors (author, commit count)
    pub top_contributors: Vec<(String, u32)>,
    /// Most frequently changed files (path, change count)
    pub hot_files: Vec<(PathBuf, u32)>,
    /// Time period analyzed from
    pub analyzed_since: Option<DateTime<Utc>>,
}

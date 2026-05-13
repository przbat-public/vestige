//! Git history analysis for extracting codebase knowledge.
//!
//! Analyzes git history to automatically extract:
//! - File co-change patterns (files that frequently change together)
//! - Bug fix patterns (from commit messages matching conventional formats)
//! - Current git context (branch, uncommitted changes, recent history)
//!
//! This is a key differentiator for Vestige — learning from the codebase's
//! history without requiring explicit user input.
//!
//! ## Module Layout
//!
//! - `errors` — `GitError`, `Result`
//! - `context` — `GitContext`, `CommitInfo` (working-tree snapshot)
//! - `analysis` — `HistoryAnalysis` (output of `analyze_history`)
//! - `analyzer` — `GitAnalyzer` (the public entry point)

mod analysis;
mod analyzer;
mod context;
mod errors;

#[cfg(test)]
mod tests;

pub use analysis::HistoryAnalysis;
pub use analyzer::GitAnalyzer;
pub use context::{CommitInfo, GitContext};
pub use errors::{GitError, Result};

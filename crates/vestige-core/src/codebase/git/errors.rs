//! Errors raised by the git-analysis pipeline.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git repository error: {0}")]
    Repository(#[from] git2::Error),
    #[error("Repository not found at: {0}")]
    NotFound(PathBuf),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("No commits found")]
    NoCommits,
}

pub type Result<T> = std::result::Result<T, GitError>;

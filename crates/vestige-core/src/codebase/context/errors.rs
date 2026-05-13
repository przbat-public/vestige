//! Errors raised by the context-capture pipeline.

use std::path::PathBuf;

use crate::codebase::git::GitError;

/// Errors that can occur during context capture
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("Git error: {0}")]
    Git(#[from] GitError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),
}

pub type Result<T> = std::result::Result<T, ContextError>;

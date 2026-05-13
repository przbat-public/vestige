//! Per-file context (language, related files, dirty status).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Context specific to a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContext {
    /// Path to the file
    pub path: PathBuf,
    /// Detected language
    pub language: Option<String>,
    /// File extension
    pub extension: Option<String>,
    /// Parent directory
    pub directory: PathBuf,
    /// Related files (imports, tests, etc.)
    pub related_files: Vec<PathBuf>,
    /// Whether the file has uncommitted changes
    pub has_changes: bool,
    /// Last modified time
    pub last_modified: Option<DateTime<Utc>>,
    /// Whether it's a test file
    pub is_test_file: bool,
    /// Module/package this file belongs to
    pub module: Option<String>,
}

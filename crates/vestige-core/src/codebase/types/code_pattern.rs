//! `CodePattern`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Records a reusable code pattern with examples and guidance.
///
/// Patterns can be:
/// - Discovered automatically from git history
/// - Taught explicitly by the user
/// - Extracted from documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodePattern {
    pub id: String,
    /// Name of the pattern (e.g., "Repository Pattern", "Error Handling")
    pub name: String,
    /// Detailed description of the pattern
    pub description: String,
    /// Example code showing the pattern
    pub example_code: String,
    /// Files containing examples of this pattern
    pub example_files: Vec<PathBuf>,
    /// When should this pattern be used?
    pub when_to_use: String,
    /// When should this pattern NOT be used?
    pub when_not_to_use: Option<String>,
    /// Language this pattern applies to
    pub language: Option<String>,
    /// When this pattern was recorded
    pub created_at: DateTime<Utc>,
    /// How many times this pattern has been applied
    pub usage_count: u32,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Related patterns
    pub related_patterns: Vec<String>,
}

impl CodePattern {
    pub fn new(id: String, name: String, description: String, when_to_use: String) -> Self {
        Self {
            id,
            name,
            description,
            example_code: String::new(),
            example_files: vec![],
            when_to_use,
            when_not_to_use: None,
            language: None,
            created_at: Utc::now(),
            usage_count: 0,
            tags: vec![],
            related_patterns: vec![],
        }
    }

    pub fn with_example(mut self, code: String, files: Vec<PathBuf>) -> Self {
        self.example_code = code;
        self.example_files = files;
        self
    }

    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }
}

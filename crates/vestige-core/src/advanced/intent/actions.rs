//! `UserAction` (single observation) and the `ActionType` enum.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A user action that can indicate intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAction {
    /// Type of action
    pub action_type: ActionType,
    /// Associated file (if any)
    pub file: Option<PathBuf>,
    /// Content/query (if any)
    pub content: Option<String>,
    /// When this action occurred
    pub timestamp: DateTime<Utc>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl UserAction {
    /// Create action for file opened
    pub fn file_opened(path: &str) -> Self {
        Self {
            action_type: ActionType::FileOpened,
            file: Some(PathBuf::from(path)),
            content: None,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create action for file edited
    pub fn file_edited(path: &str) -> Self {
        Self {
            action_type: ActionType::FileEdited,
            file: Some(PathBuf::from(path)),
            content: None,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create action for search query
    pub fn search(query: &str) -> Self {
        Self {
            action_type: ActionType::Search,
            file: None,
            content: Some(query.to_string()),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create action for error encountered
    pub fn error(message: &str) -> Self {
        Self {
            action_type: ActionType::ErrorEncountered,
            file: None,
            content: Some(message.to_string()),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create action for command executed
    pub fn command(cmd: &str) -> Self {
        Self {
            action_type: ActionType::CommandExecuted,
            file: None,
            content: Some(cmd.to_string()),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create action for documentation viewed
    pub fn docs_viewed(topic: &str) -> Self {
        Self {
            action_type: ActionType::DocumentationViewed,
            file: None,
            content: Some(topic.to_string()),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Types of user actions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionType {
    /// Opened a file
    FileOpened,
    /// Edited a file
    FileEdited,
    /// Created a new file
    FileCreated,
    /// Deleted a file
    FileDeleted,
    /// Searched for something
    Search,
    /// Executed a command
    CommandExecuted,
    /// Encountered an error
    ErrorEncountered,
    /// Viewed documentation
    DocumentationViewed,
    /// Ran tests
    TestsRun,
    /// Started debug session
    DebugStarted,
    /// Made a git commit
    GitCommit,
    /// Viewed a diff
    DiffViewed,
}

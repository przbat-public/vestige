//! Snapshot of the situation we evaluate triggers against, plus a small
//! monitor that holds the active context.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// CONTEXT
// ============================================================================

/// Current context for trigger matching
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Context {
    /// Current time
    pub timestamp: DateTime<Utc>,
    /// Current project name
    pub project_name: Option<String>,
    /// Current project path
    pub project_path: Option<String>,
    /// Active files being worked on
    pub active_files: Vec<String>,
    /// Active topics/tags
    pub active_topics: Vec<String>,
    /// Current user mode (debugging, reviewing, etc.)
    pub user_mode: Option<String>,
    /// Recent events (for event-based triggers)
    pub recent_events: Vec<String>,
    /// People/entities mentioned recently
    pub mentioned_entities: Vec<String>,
    /// Current conversation context
    pub conversation_context: Option<String>,
}

impl Context {
    /// Create a new context
    pub fn new() -> Self {
        Self {
            timestamp: Utc::now(),
            ..Default::default()
        }
    }

    /// Set project
    pub fn with_project(mut self, name: impl Into<String>, path: impl Into<String>) -> Self {
        self.project_name = Some(name.into());
        self.project_path = Some(path.into());
        self
    }

    /// Add active file
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.active_files.push(file.into());
        self
    }

    /// Add topic
    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.active_topics.push(topic.into());
        self
    }

    /// Set user mode
    pub fn with_mode(mut self, mode: impl Into<String>) -> Self {
        self.user_mode = Some(mode.into());
        self
    }

    /// Add event
    pub fn with_event(mut self, event: impl Into<String>) -> Self {
        self.recent_events.push(event.into());
        self
    }

    /// Add mentioned entity
    pub fn with_entity(mut self, entity: impl Into<String>) -> Self {
        self.mentioned_entities.push(entity.into());
        self
    }
}

/// Context monitor for checking triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMonitor {
    /// IDs of intentions currently being monitored
    pub active_intentions: Vec<String>,
    /// Current context snapshot
    pub current_context: Context,
    /// Last check time
    pub last_check: DateTime<Utc>,
}

impl Default for ContextMonitor {
    fn default() -> Self {
        Self {
            active_intentions: Vec::new(),
            current_context: Context::new(),
            last_check: Utc::now(),
        }
    }
}

impl ContextMonitor {
    /// Create a new context monitor
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the current context
    pub fn update_context(&mut self, context: Context) {
        self.current_context = context;
        self.last_check = Utc::now();
    }
}

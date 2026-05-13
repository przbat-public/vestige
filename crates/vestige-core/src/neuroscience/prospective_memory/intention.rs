//! The [`Intention`] struct itself plus how it was created.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::triggers::{IntentionStatus, IntentionTrigger, Priority};
use super::{
    DEFAULT_ESCALATION_THRESHOLD_HOURS, MAX_REMINDERS_PER_INTENTION, MIN_REMINDER_INTERVAL_MINUTES,
};

/// A future intention to be remembered
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intention {
    /// Unique identifier
    pub id: String,
    /// What to remember to do
    pub content: String,
    /// When/how to trigger
    pub trigger: IntentionTrigger,
    /// Priority level
    pub priority: Priority,
    /// Current status
    pub status: IntentionStatus,
    /// When the intention was created
    pub created_at: DateTime<Utc>,
    /// Optional deadline
    pub deadline: Option<DateTime<Utc>>,
    /// When the intention was fulfilled (if fulfilled)
    pub fulfilled_at: Option<DateTime<Utc>>,
    /// Number of times this has been reminded
    pub reminder_count: u32,
    /// Last reminder time
    pub last_reminded_at: Option<DateTime<Utc>>,
    /// Optional notes/context
    pub notes: Option<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Related memory IDs
    pub related_memories: Vec<String>,
    /// Snoozed until (if snoozed)
    pub snoozed_until: Option<DateTime<Utc>>,
    /// Source of the intention (natural language, API, etc.)
    pub source: IntentionSource,
}

impl Intention {
    /// Create a new intention
    pub fn new(content: impl Into<String>, trigger: IntentionTrigger) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content: content.into(),
            trigger,
            priority: Priority::Normal,
            status: IntentionStatus::Active,
            created_at: Utc::now(),
            deadline: None,
            fulfilled_at: None,
            reminder_count: 0,
            last_reminded_at: None,
            notes: None,
            tags: Vec::new(),
            related_memories: Vec::new(),
            snoozed_until: None,
            source: IntentionSource::Api,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set deadline
    pub fn with_deadline(mut self, deadline: DateTime<Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Add notes
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Add related memory
    pub fn with_related_memory(mut self, memory_id: String) -> Self {
        self.related_memories.push(memory_id);
        self
    }

    /// Check if the intention is overdue
    pub fn is_overdue(&self) -> bool {
        self.deadline.map(|d| Utc::now() > d).unwrap_or(false)
    }

    /// Check if deadline is approaching
    pub fn is_deadline_approaching(&self, threshold: Duration) -> bool {
        self.deadline
            .map(|d| {
                let now = Utc::now();
                now < d && (d - now) < threshold
            })
            .unwrap_or(false)
    }

    /// Check if should remind again
    pub fn should_remind(&self) -> bool {
        if self.status != IntentionStatus::Active && self.status != IntentionStatus::Triggered {
            return false;
        }

        if self.reminder_count >= MAX_REMINDERS_PER_INTENTION {
            return false;
        }

        // Check snoozed
        if let Some(snoozed_until) = self.snoozed_until
            && Utc::now() < snoozed_until
        {
            return false;
        }

        // Check minimum interval
        if let Some(last) = self.last_reminded_at
            && (Utc::now() - last) < Duration::minutes(MIN_REMINDER_INTERVAL_MINUTES)
        {
            return false;
        }

        true
    }

    /// Mark as triggered
    pub fn mark_triggered(&mut self) {
        self.status = IntentionStatus::Triggered;
        self.reminder_count += 1;
        self.last_reminded_at = Some(Utc::now());
    }

    /// Mark as fulfilled
    pub fn mark_fulfilled(&mut self) {
        self.status = IntentionStatus::Fulfilled;
        self.fulfilled_at = Some(Utc::now());
    }

    /// Snooze for a duration
    pub fn snooze(&mut self, duration: Duration) {
        self.status = IntentionStatus::Snoozed;
        self.snoozed_until = Some(Utc::now() + duration);
    }

    /// Wake from snooze
    pub fn wake(&mut self) {
        if self.status == IntentionStatus::Snoozed {
            self.status = IntentionStatus::Active;
            self.snoozed_until = None;
        }
    }

    /// Get effective priority (accounting for deadline proximity)
    pub fn effective_priority(&self) -> Priority {
        let mut priority = self.priority;

        // Escalate if deadline is approaching
        if self.is_deadline_approaching(Duration::hours(1)) {
            priority = priority.escalate().escalate();
        } else if self.is_deadline_approaching(Duration::hours(DEFAULT_ESCALATION_THRESHOLD_HOURS))
        {
            priority = priority.escalate();
        }

        // Escalate if overdue
        if self.is_overdue() {
            priority = Priority::Critical;
        }

        priority
    }
}

/// Source of an intention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentionSource {
    /// Created via API
    Api,
    /// Parsed from natural language
    NaturalLanguage {
        /// Original text
        original_text: String,
        /// Confidence in parsing
        confidence: f64,
    },
    /// Inferred from user behavior
    Inferred {
        /// What triggered inference
        trigger: String,
        /// Confidence in inference
        confidence: f64,
    },
    /// Imported from external system
    Imported {
        /// Source system
        source: String,
    },
}

//! Trigger vocabulary: priorities, status, pattern matchers, recurrence rules
//! and the [`IntentionTrigger`] enum that ties them together.

use chrono::{DateTime, Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::context::Context;

// ============================================================================
// CORE TYPES
// ============================================================================

/// Priority levels for intentions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Priority {
    /// Low priority - nice to remember
    Low = 1,
    /// Normal priority - should remember
    #[default]
    Normal = 2,
    /// High priority - important to remember
    High = 3,
    /// Critical priority - must not forget
    Critical = 4,
}

impl Priority {
    /// Get numeric value for comparison
    pub fn value(&self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Normal => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }

    /// Create from numeric value
    pub fn from_value(value: u8) -> Self {
        match value {
            1 => Self::Low,
            2 => Self::Normal,
            3 => Self::High,
            _ => Self::Critical,
        }
    }

    /// Escalate to next level
    pub fn escalate(&self) -> Self {
        match self {
            Self::Low => Self::Normal,
            Self::Normal => Self::High,
            Self::High => Self::Critical,
            Self::Critical => Self::Critical,
        }
    }
}

/// Status of an intention
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IntentionStatus {
    /// Intention is active and being monitored
    #[default]
    Active,
    /// Intention has been triggered but not yet fulfilled
    Triggered,
    /// Intention has been fulfilled
    Fulfilled,
    /// Intention was cancelled
    Cancelled,
    /// Intention expired (deadline passed without fulfillment)
    Expired,
    /// Intention is snoozed until a specific time
    Snoozed,
}

/// Pattern for matching trigger conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerPattern {
    /// Exact string match
    Exact(String),
    /// Contains substring (case-insensitive)
    Contains(String),
    /// Matches regex pattern
    Regex(String),
    /// Matches any of the given patterns
    AnyOf(Vec<TriggerPattern>),
    /// Matches all of the given patterns
    AllOf(Vec<TriggerPattern>),
}

impl TriggerPattern {
    /// Check if input matches this pattern
    pub fn matches(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();

        match self {
            Self::Exact(s) => input_lower == s.to_lowercase(),
            Self::Contains(s) => input_lower.contains(&s.to_lowercase()),
            Self::Regex(pattern) => {
                // Simple regex matching (in production, use the regex crate)
                input_lower.contains(&pattern.to_lowercase())
            }
            Self::AnyOf(patterns) => patterns.iter().any(|p| p.matches(input)),
            Self::AllOf(patterns) => patterns.iter().all(|p| p.matches(input)),
        }
    }

    /// Create a contains pattern
    pub fn contains(s: impl Into<String>) -> Self {
        Self::Contains(s.into())
    }

    /// Create an exact match pattern
    pub fn exact(s: impl Into<String>) -> Self {
        Self::Exact(s.into())
    }
}

/// Pattern for matching context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextPattern {
    /// Working in a specific codebase/project
    InCodebase(String),
    /// Working with a specific file pattern
    FilePattern(String),
    /// Specific topic/tag is active
    TopicActive(String),
    /// User is in a specific mode (debugging, reviewing, etc.)
    UserMode(String),
    /// Multiple conditions
    Composite {
        /// All conditions that must match
        all: Vec<ContextPattern>,
        /// Any conditions (at least one must match)
        any: Vec<ContextPattern>,
    },
}

impl ContextPattern {
    /// Check if context matches this pattern
    pub fn matches(&self, context: &Context) -> bool {
        match self {
            Self::InCodebase(name) => context
                .project_name
                .as_ref()
                .map(|p| p.to_lowercase().contains(&name.to_lowercase()))
                .unwrap_or(false),
            Self::FilePattern(pattern) => context
                .active_files
                .iter()
                .any(|f| f.to_lowercase().contains(&pattern.to_lowercase())),
            Self::TopicActive(topic) => context
                .active_topics
                .iter()
                .any(|t| t.to_lowercase().contains(&topic.to_lowercase())),
            Self::UserMode(mode) => context
                .user_mode
                .as_ref()
                .map(|m| m.to_lowercase() == mode.to_lowercase())
                .unwrap_or(false),
            Self::Composite { all, any } => {
                let all_match = all.is_empty() || all.iter().all(|p| p.matches(context));
                let any_match = any.is_empty() || any.iter().any(|p| p.matches(context));
                all_match && any_match
            }
        }
    }

    /// Create a codebase pattern
    pub fn in_codebase(name: impl Into<String>) -> Self {
        Self::InCodebase(name.into())
    }

    /// Create a file pattern
    pub fn file_pattern(pattern: impl Into<String>) -> Self {
        Self::FilePattern(pattern.into())
    }

    /// Create a topic pattern
    pub fn topic_active(topic: impl Into<String>) -> Self {
        Self::TopicActive(topic.into())
    }
}

/// Trigger types for intentions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentionTrigger {
    /// Trigger at a specific time
    TimeBased {
        /// The time to trigger
        at: DateTime<Utc>,
    },

    /// Trigger after a duration from creation
    DurationBased {
        /// Duration to wait before triggering
        after: Duration,
        /// Calculated trigger time (set on creation)
        trigger_at: Option<DateTime<Utc>>,
    },

    /// Trigger based on an event/condition
    EventBased {
        /// Description of the condition
        condition: String,
        /// Pattern to match
        pattern: TriggerPattern,
    },

    /// Trigger based on context
    ContextBased {
        /// Context pattern to match
        context_match: ContextPattern,
    },

    /// Trigger when an activity is completed
    ActivityBased {
        /// Activity that must complete
        activity: String,
        /// Pattern to match completion
        completion_pattern: TriggerPattern,
    },

    /// Recurring trigger (repeats)
    Recurring {
        /// Base trigger type
        base: Box<IntentionTrigger>,
        /// Recurrence pattern
        recurrence: RecurrencePattern,
        /// Next occurrence
        next_occurrence: Option<DateTime<Utc>>,
    },

    /// Compound trigger (multiple conditions)
    Compound {
        /// All triggers that must fire
        all_of: Vec<IntentionTrigger>,
        /// Any triggers (at least one must fire)
        any_of: Vec<IntentionTrigger>,
    },
}

impl IntentionTrigger {
    /// Create a time-based trigger
    pub fn at_time(time: DateTime<Utc>) -> Self {
        Self::TimeBased { at: time }
    }

    /// Create a duration-based trigger
    pub fn after_duration(duration: Duration) -> Self {
        Self::DurationBased {
            after: duration,
            trigger_at: Some(Utc::now() + duration),
        }
    }

    /// Create an event-based trigger
    pub fn on_event(condition: impl Into<String>, pattern: TriggerPattern) -> Self {
        Self::EventBased {
            condition: condition.into(),
            pattern,
        }
    }

    /// Create a context-based trigger
    pub fn on_context(context_match: ContextPattern) -> Self {
        Self::ContextBased { context_match }
    }

    /// Check if this trigger matches the current state
    pub fn is_triggered(&self, context: &Context, events: &[String]) -> bool {
        let now = Utc::now();

        match self {
            Self::TimeBased { at } => now >= *at,
            Self::DurationBased { trigger_at, .. } => trigger_at.map(|t| now >= t).unwrap_or(false),
            Self::EventBased { pattern, .. } => events.iter().any(|e| pattern.matches(e)),
            Self::ContextBased { context_match } => context_match.matches(context),
            Self::ActivityBased {
                completion_pattern, ..
            } => events.iter().any(|e| completion_pattern.matches(e)),
            Self::Recurring {
                next_occurrence, ..
            } => next_occurrence.map(|t| now >= t).unwrap_or(false),
            Self::Compound { all_of, any_of } => {
                let all_match =
                    all_of.is_empty() || all_of.iter().all(|t| t.is_triggered(context, events));
                let any_match =
                    any_of.is_empty() || any_of.iter().any(|t| t.is_triggered(context, events));
                all_match && any_match
            }
        }
    }

    /// Get a human-readable description of the trigger
    pub fn description(&self) -> String {
        match self {
            Self::TimeBased { at } => format!("At {}", at.format("%Y-%m-%d %H:%M")),
            Self::DurationBased { after, .. } => {
                let hours = after.num_hours();
                let minutes = after.num_minutes() % 60;
                if hours > 0 {
                    format!("In {} hours {} minutes", hours, minutes)
                } else {
                    format!("In {} minutes", minutes)
                }
            }
            Self::EventBased { condition, .. } => format!("When: {}", condition),
            Self::ContextBased { context_match } => match context_match {
                ContextPattern::InCodebase(name) => format!("In {} codebase", name),
                ContextPattern::FilePattern(pattern) => format!("Working on {}", pattern),
                ContextPattern::TopicActive(topic) => format!("Discussing {}", topic),
                ContextPattern::UserMode(mode) => format!("In {} mode", mode),
                ContextPattern::Composite { .. } => "Complex context".to_string(),
            },
            Self::ActivityBased { activity, .. } => format!("After completing: {}", activity),
            Self::Recurring {
                base, recurrence, ..
            } => {
                format!("{} ({})", base.description(), recurrence.description())
            }
            Self::Compound { all_of, any_of } => {
                let parts: Vec<String> = all_of
                    .iter()
                    .chain(any_of.iter())
                    .map(|t| t.description())
                    .collect();
                parts.join(" and ")
            }
        }
    }
}

/// Recurrence patterns for recurring intentions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecurrencePattern {
    /// Every N minutes
    EveryMinutes(i64),
    /// Every N hours
    EveryHours(i64),
    /// Daily at specific time
    Daily { hour: u32, minute: u32 },
    /// Weekly on specific days
    Weekly {
        days: Vec<chrono::Weekday>,
        hour: u32,
        minute: u32,
    },
    /// Monthly on specific day
    Monthly { day: u32, hour: u32, minute: u32 },
    /// Custom interval
    Custom { interval: Duration },
}

impl RecurrencePattern {
    /// Get the next occurrence from a given time
    pub fn next_occurrence(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::EveryMinutes(mins) => from + Duration::minutes(*mins),
            Self::EveryHours(hours) => from + Duration::hours(*hours),
            Self::Daily { hour, minute } => {
                let today = from.date_naive();
                // Default to midnight if invalid time (00:00:00 is always valid)
                let time = chrono::NaiveTime::from_hms_opt(*hour, *minute, 0)
                    .unwrap_or(chrono::NaiveTime::MIN);
                let datetime = today.and_time(time);
                let result = DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc);

                if result <= from {
                    result + Duration::days(1)
                } else {
                    result
                }
            }
            Self::Weekly { days, hour, minute } => {
                // Find next matching day
                let mut candidate = from + Duration::days(1);
                for _ in 0..7 {
                    if days.contains(&candidate.weekday()) {
                        let date = candidate.date_naive();
                        // Default to midnight if invalid time
                        let time = chrono::NaiveTime::from_hms_opt(*hour, *minute, 0)
                            .unwrap_or(chrono::NaiveTime::MIN);
                        return DateTime::<Utc>::from_naive_utc_and_offset(
                            date.and_time(time),
                            Utc,
                        );
                    }
                    candidate += Duration::days(1);
                }
                from + Duration::days(7) // Fallback
            }
            Self::Monthly { day, hour, minute } => {
                let current_month = from.month();
                let current_year = from.year();

                let target_date = chrono::NaiveDate::from_ymd_opt(
                    current_year,
                    current_month,
                    (*day).min(28), // Safe day
                )
                .unwrap_or_else(|| from.date_naive());

                // Default to midnight if invalid time
                let time = chrono::NaiveTime::from_hms_opt(*hour, *minute, 0)
                    .unwrap_or(chrono::NaiveTime::MIN);

                let result =
                    DateTime::<Utc>::from_naive_utc_and_offset(target_date.and_time(time), Utc);

                if result <= from {
                    // Go to next month
                    let next_month = if current_month == 12 {
                        1
                    } else {
                        current_month + 1
                    };
                    let next_year = if current_month == 12 {
                        current_year + 1
                    } else {
                        current_year
                    };

                    let next_date =
                        chrono::NaiveDate::from_ymd_opt(next_year, next_month, (*day).min(28))
                            .unwrap_or_else(|| from.date_naive());

                    DateTime::<Utc>::from_naive_utc_and_offset(next_date.and_time(time), Utc)
                } else {
                    result
                }
            }
            Self::Custom { interval } => from + *interval,
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            Self::EveryMinutes(mins) => format!("every {} minutes", mins),
            Self::EveryHours(hours) => format!("every {} hours", hours),
            Self::Daily { hour, minute } => format!("daily at {:02}:{:02}", hour, minute),
            Self::Weekly { days, hour, minute } => {
                let day_names: Vec<_> = days.iter().map(|d| format!("{:?}", d)).collect();
                format!(
                    "every {} at {:02}:{:02}",
                    day_names.join(", "),
                    hour,
                    minute
                )
            }
            Self::Monthly { day, hour, minute } => {
                format!("monthly on day {} at {:02}:{:02}", day, hour, minute)
            }
            Self::Custom { interval } => format!("every {} minutes", interval.num_minutes()),
        }
    }
}

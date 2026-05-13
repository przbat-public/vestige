//! # Prospective Memory
//!
//! Implementation of prospective memory - "remember to do X when Y happens."
//! This is a distinct cognitive system for future intentions, separate from
//! retrospective memory (remembering past events).
//!
//! ## Theoretical Foundation
//!
//! Based on neuroscience research on prospective memory (Einstein & McDaniel, 1990):
//! - **Event-based**: Triggered by external cues (seeing someone, entering a location)
//! - **Time-based**: Triggered by time passing (in 2 hours, at 3pm)
//! - **Activity-based**: Triggered by completing an activity
//!
//! Key cognitive processes:
//! - **Intention formation**: Creating the future intention
//! - **Retention**: Maintaining the intention during delay
//! - **Intention retrieval**: Recognizing the trigger and recalling the intention
//! - **Execution**: Carrying out the intended action
//!
//! ## How It Works
//!
//! 1. **Parse intentions** from natural language or explicit API
//! 2. **Monitor context** continuously for trigger matches
//! 3. **Escalate priority** as deadlines approach
//! 4. **Surface proactively** when triggers are detected
//! 5. **Track fulfillment** and learn from patterns
//!
//! ## Example
//!
//! ```rust,ignore
//! use vestige_core::neuroscience::{ProspectiveMemory, Intention, IntentionTrigger};
//!
//! let mut pm = ProspectiveMemory::new();
//!
//! // Time-based intention
//! pm.create_intention(Intention::new(
//!     "Send weekly report to team",
//!     IntentionTrigger::TimeBased {
//!         at: next_friday_at_3pm,
//!     },
//! ));
//!
//! // Event-based intention
//! pm.create_intention(Intention::new(
//!     "Ask John about the API design",
//!     IntentionTrigger::EventBased {
//!         condition: "meeting with John".to_string(),
//!         pattern: TriggerPattern::Contains("john".to_string()),
//!     },
//! ));
//!
//! // Context-based intention
//! pm.create_intention(Intention::new(
//!     "Review the error handling in payments module",
//!     IntentionTrigger::ContextBased {
//!         context_match: ContextPattern::InCodebase("payments".to_string()),
//!     },
//! ));
//!
//! // Check for triggered intentions
//! let triggered = pm.check_triggers(&current_context);
//! for intention in triggered {
//!     notify_user(&intention);
//! }
//! ```

// ============================================================================
// CONFIGURATION CONSTANTS
// ============================================================================

/// Maximum active intentions to track
pub(super) const MAX_INTENTIONS: usize = 1000;

/// Default priority escalation threshold (hours before deadline)
pub(super) const DEFAULT_ESCALATION_THRESHOLD_HOURS: i64 = 24;

/// Maximum times to remind for a single intention
pub(super) const MAX_REMINDERS_PER_INTENTION: u32 = 5;

/// Minimum interval between reminders (minutes)
pub(super) const MIN_REMINDER_INTERVAL_MINUTES: i64 = 30;

/// Maximum age for completed intentions in history (days)
pub(super) const COMPLETED_INTENTION_RETENTION_DAYS: i64 = 30;

mod context;
mod engine;
mod error;
mod intention;
mod parser;
mod triggers;

#[cfg(test)]
mod tests;

pub use context::{Context, ContextMonitor};
pub use engine::{IntentionStats, ProspectiveMemory, ProspectiveMemoryConfig};
pub use error::{ProspectiveMemoryError, Result};
pub use intention::{Intention, IntentionSource};
pub use parser::IntentionParser;
pub use triggers::{
    ContextPattern, IntentionStatus, IntentionTrigger, Priority, RecurrencePattern, TriggerPattern,
};

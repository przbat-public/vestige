//! [`ProspectiveMemory`] engine: configuration, intention storage, trigger
//! checking and aggregate statistics.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use super::context::{Context, ContextMonitor};
use super::error::{ProspectiveMemoryError, Result};
use super::intention::Intention;
use super::parser::IntentionParser;
use super::triggers::{IntentionStatus, Priority};
use super::{
    COMPLETED_INTENTION_RETENTION_DAYS, DEFAULT_ESCALATION_THRESHOLD_HOURS, MAX_INTENTIONS,
    MAX_REMINDERS_PER_INTENTION, MIN_REMINDER_INTERVAL_MINUTES,
};

// ============================================================================
// PROSPECTIVE MEMORY ENGINE
// ============================================================================

/// Configuration for the prospective memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProspectiveMemoryConfig {
    /// Maximum active intentions
    pub max_intentions: usize,
    /// Enable priority escalation
    pub enable_escalation: bool,
    /// Hours before deadline to start escalation
    pub escalation_threshold_hours: i64,
    /// Maximum reminders per intention
    pub max_reminders: u32,
    /// Minimum minutes between reminders
    pub min_reminder_interval_minutes: i64,
    /// Auto-expire intentions after deadline
    pub auto_expire: bool,
    /// Days to retain completed intentions
    pub completed_retention_days: i64,
}

impl Default for ProspectiveMemoryConfig {
    fn default() -> Self {
        Self {
            max_intentions: MAX_INTENTIONS,
            enable_escalation: true,
            escalation_threshold_hours: DEFAULT_ESCALATION_THRESHOLD_HOURS,
            max_reminders: MAX_REMINDERS_PER_INTENTION,
            min_reminder_interval_minutes: MIN_REMINDER_INTERVAL_MINUTES,
            auto_expire: true,
            completed_retention_days: COMPLETED_INTENTION_RETENTION_DAYS,
        }
    }
}

/// The main prospective memory engine
pub struct ProspectiveMemory {
    /// Active intentions
    intentions: Arc<RwLock<HashMap<String, Intention>>>,
    /// Context monitors
    monitors: Arc<RwLock<Vec<ContextMonitor>>>,
    /// Natural language parser
    parser: IntentionParser,
    /// Configuration
    config: ProspectiveMemoryConfig,
    /// History of fulfilled intentions (for learning)
    history: Arc<RwLock<VecDeque<Intention>>>,
}

impl ProspectiveMemory {
    /// Create a new prospective memory engine
    pub fn new() -> Self {
        Self::with_config(ProspectiveMemoryConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: ProspectiveMemoryConfig) -> Self {
        Self {
            intentions: Arc::new(RwLock::new(HashMap::new())),
            monitors: Arc::new(RwLock::new(vec![ContextMonitor::new()])),
            parser: IntentionParser::new(),
            config,
            history: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// Get configuration
    pub fn config(&self) -> &ProspectiveMemoryConfig {
        &self.config
    }

    /// Create a new intention
    pub fn create_intention(&self, intention: Intention) -> Result<String> {
        let mut intentions = self
            .intentions
            .write()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        // Check capacity
        if intentions.len() >= self.config.max_intentions {
            return Err(ProspectiveMemoryError::MaxIntentionsReached(
                self.config.max_intentions,
            ));
        }

        let id = intention.id.clone();
        intentions.insert(id.clone(), intention);

        Ok(id)
    }

    /// Create intention from natural language
    pub fn create_from_text(&self, text: &str) -> Result<String> {
        let intention = self.parser.parse(text)?;
        self.create_intention(intention)
    }

    /// Get an intention by ID
    pub fn get_intention(&self, id: &str) -> Result<Option<Intention>> {
        let intentions = self
            .intentions
            .read()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        Ok(intentions.get(id).cloned())
    }

    /// Get all active intentions
    pub fn get_active_intentions(&self) -> Result<Vec<Intention>> {
        let intentions = self
            .intentions
            .read()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        Ok(intentions
            .values()
            .filter(|i| {
                i.status == IntentionStatus::Active || i.status == IntentionStatus::Triggered
            })
            .cloned()
            .collect())
    }

    /// Get intentions by priority
    pub fn get_by_priority(&self, min_priority: Priority) -> Result<Vec<Intention>> {
        let intentions = self
            .intentions
            .read()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        let mut result: Vec<_> = intentions
            .values()
            .filter(|i| i.effective_priority() >= min_priority)
            .filter(|i| {
                i.status == IntentionStatus::Active || i.status == IntentionStatus::Triggered
            })
            .cloned()
            .collect();

        // Sort by effective priority (highest first)
        result.sort_by_key(|i| std::cmp::Reverse(i.effective_priority()));

        Ok(result)
    }

    /// Get overdue intentions
    pub fn get_overdue(&self) -> Result<Vec<Intention>> {
        let intentions = self
            .intentions
            .read()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        Ok(intentions
            .values()
            .filter(|i| i.is_overdue())
            .filter(|i| {
                i.status == IntentionStatus::Active || i.status == IntentionStatus::Triggered
            })
            .cloned()
            .collect())
    }

    /// Check triggers against current context
    pub fn check_triggers(&self, context: &Context) -> Result<Vec<Intention>> {
        let mut intentions = self
            .intentions
            .write()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        let mut triggered = Vec::new();

        for intention in intentions.values_mut() {
            // Skip non-active intentions
            if intention.status != IntentionStatus::Active {
                // Check if snoozed intention should wake
                if intention.status == IntentionStatus::Snoozed
                    && let Some(until) = intention.snoozed_until
                    && Utc::now() >= until
                {
                    intention.wake();
                }
                continue;
            }

            // Check if triggered
            if intention
                .trigger
                .is_triggered(context, &context.recent_events)
                && intention.should_remind()
            {
                intention.mark_triggered();
                triggered.push(intention.clone());
            }

            // Check for deadline escalation
            if self.config.enable_escalation {
                let threshold = Duration::hours(self.config.escalation_threshold_hours);
                if intention.is_deadline_approaching(threshold) {
                    // Priority will be automatically escalated via effective_priority()
                }
            }

            // Auto-expire overdue intentions
            if self.config.auto_expire && intention.is_overdue() {
                intention.status = IntentionStatus::Expired;
            }
        }

        // Sort triggered by effective priority
        triggered.sort_by_key(|i| std::cmp::Reverse(i.effective_priority()));

        Ok(triggered)
    }

    /// Update context and check for triggers
    pub fn update_context(&self, context: Context) -> Result<Vec<Intention>> {
        // Update monitors
        {
            let mut monitors = self
                .monitors
                .write()
                .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

            if let Some(monitor) = monitors.first_mut() {
                monitor.update_context(context.clone());
            }
        }

        // Check triggers
        self.check_triggers(&context)
    }

    /// Mark intention as fulfilled
    pub fn fulfill(&self, id: &str) -> Result<()> {
        let mut intentions = self
            .intentions
            .write()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        if let Some(intention) = intentions.get_mut(id) {
            intention.mark_fulfilled();

            // Add to history
            let fulfilled_intention = intention.clone();

            let mut history = self
                .history
                .write()
                .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

            history.push_back(fulfilled_intention);

            // Maintain history size
            let retention_cutoff =
                Utc::now() - Duration::days(self.config.completed_retention_days);
            while history
                .front()
                .map(|i| i.fulfilled_at.unwrap_or(i.created_at) < retention_cutoff)
                .unwrap_or(false)
            {
                history.pop_front();
            }

            Ok(())
        } else {
            Err(ProspectiveMemoryError::NotFound(id.to_string()))
        }
    }

    /// Snooze an intention
    pub fn snooze(&self, id: &str, duration: Duration) -> Result<()> {
        let mut intentions = self
            .intentions
            .write()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        if let Some(intention) = intentions.get_mut(id) {
            intention.snooze(duration);
            Ok(())
        } else {
            Err(ProspectiveMemoryError::NotFound(id.to_string()))
        }
    }

    /// Cancel an intention
    pub fn cancel(&self, id: &str) -> Result<()> {
        let mut intentions = self
            .intentions
            .write()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        if let Some(intention) = intentions.get_mut(id) {
            intention.status = IntentionStatus::Cancelled;
            Ok(())
        } else {
            Err(ProspectiveMemoryError::NotFound(id.to_string()))
        }
    }

    /// Update intention priority
    pub fn set_priority(&self, id: &str, priority: Priority) -> Result<()> {
        let mut intentions = self
            .intentions
            .write()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        if let Some(intention) = intentions.get_mut(id) {
            intention.priority = priority;
            Ok(())
        } else {
            Err(ProspectiveMemoryError::NotFound(id.to_string()))
        }
    }

    /// Get intention statistics
    pub fn stats(&self) -> Result<IntentionStats> {
        let intentions = self
            .intentions
            .read()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        let history = self
            .history
            .read()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        let active = intentions
            .values()
            .filter(|i| i.status == IntentionStatus::Active)
            .count();

        let triggered = intentions
            .values()
            .filter(|i| i.status == IntentionStatus::Triggered)
            .count();

        let overdue = intentions.values().filter(|i| i.is_overdue()).count();

        let fulfilled = history.len();

        let high_priority = intentions
            .values()
            .filter(|i| i.effective_priority() >= Priority::High)
            .filter(|i| {
                i.status == IntentionStatus::Active || i.status == IntentionStatus::Triggered
            })
            .count();

        Ok(IntentionStats {
            total_active: active,
            triggered,
            overdue,
            fulfilled_lifetime: fulfilled,
            high_priority,
        })
    }

    /// Clean up old/completed intentions
    pub fn cleanup(&self) -> Result<usize> {
        let mut intentions = self
            .intentions
            .write()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        let before = intentions.len();

        // Remove fulfilled, cancelled, and expired intentions
        intentions.retain(|_, i| {
            matches!(
                i.status,
                IntentionStatus::Active | IntentionStatus::Triggered | IntentionStatus::Snoozed
            )
        });

        Ok(before - intentions.len())
    }

    /// Get fulfillment history
    pub fn get_history(&self, limit: usize) -> Result<Vec<Intention>> {
        let history = self
            .history
            .read()
            .map_err(|e| ProspectiveMemoryError::LockPoisoned(e.to_string()))?;

        Ok(history.iter().rev().take(limit).cloned().collect())
    }
}

impl Default for ProspectiveMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about intentions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentionStats {
    /// Number of active intentions
    pub total_active: usize,
    /// Number of triggered (pending action) intentions
    pub triggered: usize,
    /// Number of overdue intentions
    pub overdue: usize,
    /// Total fulfilled in history
    pub fulfilled_lifetime: usize,
    /// High priority intentions needing attention
    pub high_priority: usize,
}

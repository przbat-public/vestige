//! `ReconsolidationManager` — tracks labile memories and applies modifications.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{Duration, Utc};

use super::constants::{DEFAULT_LABILE_WINDOW_SECS, RETRIEVAL_HISTORY_DAYS};
use super::context::AccessContext;
use super::labile::{LabileState, MemorySnapshot, Modification};
use super::results::{AppliedModification, ChangeSummary, ReconsolidatedMemory, RetrievalRecord};
use super::stats::ReconsolidationStats;

/// Manages memory reconsolidation
///
/// Tracks labile memories and applies modifications during the labile window.
/// Inspired by Nader's research on memory reconsolidation.
#[derive(Debug)]
pub struct ReconsolidationManager {
    /// Currently labile memories
    labile_memories: HashMap<String, LabileState>,
    /// Duration of labile window
    labile_window: Duration,
    /// Retrieval history
    retrieval_history: Arc<RwLock<Vec<RetrievalRecord>>>,
    /// Reconsolidation statistics
    stats: ReconsolidationStats,
    /// Whether reconsolidation is enabled
    enabled: bool,
}

impl Default for ReconsolidationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ReconsolidationManager {
    /// Create a new reconsolidation manager
    pub fn new() -> Self {
        Self {
            labile_memories: HashMap::new(),
            labile_window: Duration::seconds(DEFAULT_LABILE_WINDOW_SECS),
            retrieval_history: Arc::new(RwLock::new(Vec::new())),
            stats: ReconsolidationStats::default(),
            enabled: true,
        }
    }

    /// Create with custom labile window
    pub fn with_window(window_seconds: i64) -> Self {
        let mut manager = Self::new();
        manager.labile_window = Duration::seconds(window_seconds);
        manager
    }

    /// Enable or disable reconsolidation
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if reconsolidation is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Mark a memory as labile (accessed)
    ///
    /// Call this when a memory is retrieved. The memory will be modifiable
    /// during the labile window.
    pub fn mark_labile(&mut self, memory_id: &str, snapshot: MemorySnapshot) {
        if !self.enabled {
            return;
        }

        let state = LabileState::new(memory_id.to_string(), snapshot);
        self.labile_memories.insert(memory_id.to_string(), state);
        self.stats.total_marked_labile += 1;
    }

    /// Mark a memory as labile with context
    pub fn mark_labile_with_context(
        &mut self,
        memory_id: &str,
        snapshot: MemorySnapshot,
        context: AccessContext,
    ) {
        if !self.enabled {
            return;
        }

        let state = LabileState::new(memory_id.to_string(), snapshot).with_context(context);
        self.labile_memories.insert(memory_id.to_string(), state);
        self.stats.total_marked_labile += 1;
    }

    /// Check if a memory is currently labile (modifiable)
    pub fn is_labile(&self, memory_id: &str) -> bool {
        self.labile_memories
            .get(memory_id)
            .map(|state| state.is_within_window(self.labile_window))
            .unwrap_or(false)
    }

    /// Get the labile state for a memory
    pub fn get_labile_state(&self, memory_id: &str) -> Option<&LabileState> {
        self.labile_memories
            .get(memory_id)
            .filter(|state| state.is_within_window(self.labile_window))
    }

    /// Get remaining labile window time
    pub fn remaining_labile_time(&self, memory_id: &str) -> Option<Duration> {
        self.labile_memories.get(memory_id).and_then(|state| {
            let elapsed = Utc::now() - state.accessed_at;
            if elapsed < self.labile_window {
                Some(self.labile_window - elapsed)
            } else {
                None
            }
        })
    }

    /// Apply a modification to a labile memory
    ///
    /// Returns true if the modification was applied, false if the memory
    /// is not labile or the modification limit was reached.
    pub fn apply_modification(&mut self, memory_id: &str, modification: Modification) -> bool {
        if !self.enabled {
            return false;
        }

        if let Some(state) = self.labile_memories.get_mut(memory_id)
            && state.is_within_window(self.labile_window)
        {
            let success = state.add_modification(modification);
            if success {
                self.stats.total_modifications += 1;
            }
            return success;
        }
        false
    }

    /// Apply multiple modifications at once
    pub fn apply_modifications(
        &mut self,
        memory_id: &str,
        modifications: Vec<Modification>,
    ) -> usize {
        let mut applied = 0;
        for modification in modifications {
            if self.apply_modification(memory_id, modification) {
                applied += 1;
            }
        }
        applied
    }

    /// Reconsolidate a memory (finalize modifications)
    ///
    /// This should be called when:
    /// - The labile window expires
    /// - Explicitly by the system when appropriate
    ///
    /// Returns the reconsolidation result with all applied modifications.
    pub fn reconsolidate(&mut self, memory_id: &str) -> Option<ReconsolidatedMemory> {
        let state = self.labile_memories.remove(memory_id)?;

        if state.reconsolidated {
            return None;
        }

        let labile_duration = Utc::now() - state.accessed_at;

        // Build change summary
        let mut change_summary = ChangeSummary::default();
        let mut applied_modifications = Vec::new();

        for modification in &state.modifications {
            let applied = AppliedModification {
                modification: modification.clone(),
                applied_at: Utc::now(),
                success: true,
                error: None,
            };

            // Update summary based on modification type
            match modification {
                Modification::AddTag { .. } => change_summary.tags_added += 1,
                Modification::RemoveTag { .. } => change_summary.tags_removed += 1,
                Modification::StrengthenConnection { .. } => {
                    change_summary.connections_strengthened += 1
                }
                Modification::LinkMemory { .. } => change_summary.links_created += 1,
                Modification::UpdateContent { .. } => change_summary.content_updated = true,
                Modification::UpdateEmotion { .. } => change_summary.emotion_updated = true,
                Modification::BoostRetrieval { boost } => change_summary.retrieval_boost += boost,
                _ => {}
            }

            applied_modifications.push(applied);
        }

        let was_modified = change_summary.has_changes();

        // Record retrieval in history
        self.record_retrieval(RetrievalRecord {
            memory_id: memory_id.to_string(),
            retrieved_at: state.accessed_at,
            context: state.access_context,
            was_modified,
            retrieval_strength_at_access: state.original_state.retrieval_strength,
        });

        self.stats.total_reconsolidated += 1;
        if was_modified {
            self.stats.total_modified += 1;
        }

        Some(ReconsolidatedMemory {
            memory_id: memory_id.to_string(),
            reconsolidated_at: Utc::now(),
            labile_duration,
            applied_modifications,
            was_modified,
            change_summary,
            retrieval_count: self.get_retrieval_count(memory_id),
        })
    }

    /// Force reconsolidation of all expired labile memories
    pub fn reconsolidate_expired(&mut self) -> Vec<ReconsolidatedMemory> {
        let expired_ids: Vec<_> = self
            .labile_memories
            .iter()
            .filter(|(_, state)| !state.is_within_window(self.labile_window))
            .map(|(id, _)| id.clone())
            .collect();

        expired_ids
            .into_iter()
            .filter_map(|id| self.reconsolidate(&id))
            .collect()
    }

    /// Get all currently labile memory IDs
    pub fn get_labile_memory_ids(&self) -> Vec<String> {
        self.labile_memories
            .iter()
            .filter(|(_, state)| state.is_within_window(self.labile_window))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Record a retrieval event
    fn record_retrieval(&self, record: RetrievalRecord) {
        if let Ok(mut history) = self.retrieval_history.write() {
            history.push(record);

            // Trim old records
            let cutoff = Utc::now() - Duration::days(RETRIEVAL_HISTORY_DAYS);
            history.retain(|r| r.retrieved_at >= cutoff);
        }
    }

    /// Get retrieval count for a memory
    pub fn get_retrieval_count(&self, memory_id: &str) -> u32 {
        self.retrieval_history
            .read()
            .map(|history| history.iter().filter(|r| r.memory_id == memory_id).count() as u32)
            .unwrap_or(0)
    }

    /// Get retrieval history for a memory
    pub fn get_retrieval_history(&self, memory_id: &str) -> Vec<RetrievalRecord> {
        self.retrieval_history
            .read()
            .map(|history| {
                history
                    .iter()
                    .filter(|r| r.memory_id == memory_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get most recently retrieved memories
    pub fn get_recent_retrievals(&self, limit: usize) -> Vec<RetrievalRecord> {
        self.retrieval_history
            .read()
            .map(|history| {
                let mut recent: Vec<_> = history.iter().cloned().collect();
                recent.sort_by(|a, b| b.retrieved_at.cmp(&a.retrieved_at));
                recent.into_iter().take(limit).collect()
            })
            .unwrap_or_default()
    }

    /// Get memories frequently retrieved together
    pub fn get_co_retrieved_memories(&self, memory_id: &str) -> HashMap<String, usize> {
        let mut co_retrieved = HashMap::new();

        if let Ok(history) = self.retrieval_history.read() {
            for record in history.iter() {
                if record.memory_id == memory_id
                    && let Some(context) = &record.context
                {
                    for co_id in &context.co_retrieved {
                        if co_id != memory_id {
                            *co_retrieved.entry(co_id.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        co_retrieved
    }

    /// Get reconsolidation statistics
    pub fn get_stats(&self) -> &ReconsolidationStats {
        &self.stats
    }

    /// Get current labile window duration
    pub fn get_labile_window(&self) -> Duration {
        self.labile_window
    }

    /// Set labile window duration
    pub fn set_labile_window(&mut self, window: Duration) {
        self.labile_window = window;
    }

    /// Clear all labile states (for cleanup)
    pub fn clear_labile_states(&mut self) {
        self.labile_memories.clear();
    }
}

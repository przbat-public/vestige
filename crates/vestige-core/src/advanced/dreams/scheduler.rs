//! Schedules and runs the 5-stage sleep consolidation pipeline.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::activity::{ActivityStats, ActivityTracker};
use super::connection_graph::{ConnectionGraph, ConnectionReason, ConnectionStats};
use super::constants::{
    CONNECTION_DECAY_FACTOR, DEFAULT_CONSOLIDATION_INTERVAL_HOURS, MAX_REPLAY_MEMORIES,
    MIN_BRIEF_IDLE_MINS, MIN_CONNECTION_STRENGTH, MIN_SIMILARITY_FOR_CONNECTION,
};
use super::dreamer::MemoryDreamer;
use super::replay::{MemoryReplay, Pattern, PatternType};
use super::report::ConsolidationReport;
use super::similarity::calculate_memory_similarity;
use super::types::DreamMemory;

#[derive(Debug)]
pub struct ConsolidationScheduler {
    /// Timestamp of last consolidation
    last_consolidation: DateTime<Utc>,
    /// Minimum interval between consolidations
    consolidation_interval: Duration,
    /// Activity tracker for detecting idle periods
    activity_tracker: ActivityTracker,
    /// Consolidation history
    consolidation_history: Vec<ConsolidationReport>,
    /// Whether automatic consolidation is enabled
    auto_enabled: bool,
    /// Memory dreamer for insight generation
    dreamer: MemoryDreamer,
    /// Connection manager for tracking memory connections
    connections: Arc<RwLock<ConnectionGraph>>,
}

impl Default for ConsolidationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsolidationScheduler {
    /// Create a new consolidation scheduler
    pub fn new() -> Self {
        Self {
            last_consolidation: Utc::now() - Duration::hours(DEFAULT_CONSOLIDATION_INTERVAL_HOURS),
            consolidation_interval: Duration::hours(DEFAULT_CONSOLIDATION_INTERVAL_HOURS),
            activity_tracker: ActivityTracker::new(),
            consolidation_history: Vec::new(),
            auto_enabled: true,
            dreamer: MemoryDreamer::new(),
            connections: Arc::new(RwLock::new(ConnectionGraph::new())),
        }
    }

    /// Create with custom consolidation interval
    pub fn with_interval(interval_hours: i64) -> Self {
        let mut scheduler = Self::new();
        scheduler.consolidation_interval = Duration::hours(interval_hours);
        scheduler
    }

    /// Record user activity (call this on memory operations)
    pub fn record_activity(&mut self) {
        self.activity_tracker.record_activity();
    }

    /// Check if consolidation should run
    ///
    /// v1.9.0: Improved scheduler with multiple trigger conditions:
    /// - Full consolidation: >6h stale AND >10 new memories since last
    /// - Mini-consolidation (decay only): >2h if active
    /// - System idle AND interval passed
    pub fn should_consolidate(&self) -> bool {
        if !self.auto_enabled {
            return false;
        }

        let time_since_last = Utc::now() - self.last_consolidation;

        // Trigger 1: Standard interval + idle check
        let interval_passed = time_since_last >= self.consolidation_interval;
        let is_idle = self.activity_tracker.is_idle();
        if interval_passed && is_idle {
            return true;
        }

        // Brief idle: no activity in the last 5 minutes (shorter than full idle)
        let briefly_idle = self
            .activity_tracker
            .time_since_last_activity()
            .map(|d| d >= Duration::minutes(MIN_BRIEF_IDLE_MINS))
            .unwrap_or(true); // No activity ever = idle

        // Trigger 2: >6h stale — force during any idle period (even brief)
        if time_since_last >= Duration::hours(6) && briefly_idle {
            return true;
        }

        // Trigger 3: Mini-consolidation every 2h during brief lulls (5-30 min idle)
        if time_since_last >= Duration::hours(2) && briefly_idle && !is_idle {
            return true;
        }

        false
    }

    /// Force check if consolidation should run (ignoring idle check)
    pub fn should_consolidate_force(&self) -> bool {
        let time_since_last = Utc::now() - self.last_consolidation;
        time_since_last >= self.consolidation_interval
    }

    /// Run a complete consolidation cycle
    ///
    /// This implements the 5-stage sleep consolidation model:
    /// 1. Replay recent memories
    /// 2. Cross-reference with existing knowledge
    /// 3. Strengthen co-activated connections
    /// 4. Prune weak connections
    /// 5. Transfer consolidated memories
    pub async fn run_consolidation_cycle(
        &mut self,
        memories: &[DreamMemory],
    ) -> ConsolidationReport {
        let start = Instant::now();
        let mut report = ConsolidationReport::new();

        // Stage 1: Memory Replay
        let replay = self.stage1_replay(memories);
        report.stage1_replay = Some(replay.clone());

        // Stage 2: Cross-reference
        let cross_refs = self.stage2_cross_reference(memories, &replay);
        report.stage2_connections = cross_refs;

        // Stage 3: Strengthen connections
        let strengthened = self.stage3_strengthen(&replay);
        report.stage3_strengthened = strengthened;

        // Stage 4: Prune weak connections
        let pruned = self.stage4_prune();
        report.stage4_pruned = pruned;

        // Stage 5: Transfer (identify memories for semantic storage)
        let transferred = self.stage5_transfer(memories);
        report.stage5_transferred = transferred;

        // Run dream cycle for insights
        let dream_result = self.dreamer.dream(memories).await;
        report.dream_result = Some(dream_result);

        // Update state
        self.last_consolidation = Utc::now();
        report.duration_ms = start.elapsed().as_millis() as u64;
        report.completed_at = Utc::now();

        // Store in history
        self.consolidation_history.push(report.clone());
        if self.consolidation_history.len() > 100 {
            self.consolidation_history.remove(0);
        }

        report
    }

    /// Stage 1: Replay recent memories in sequence
    fn stage1_replay(&self, memories: &[DreamMemory]) -> MemoryReplay {
        // Sort by creation time for sequential replay
        let mut sorted: Vec<_> = memories.iter().take(MAX_REPLAY_MEMORIES).collect();
        sorted.sort_by_key(|m| m.created_at);

        let sequence: Vec<String> = sorted.iter().map(|m| m.id.clone()).collect();

        // Generate synthetic combinations (test pairs that might have hidden connections)
        let mut synthetic_combinations = Vec::new();
        for i in 0..sorted.len().saturating_sub(1) {
            for j in (i + 1)..sorted.len().min(i + 5) {
                // Only combine memories within a close window
                synthetic_combinations.push((sorted[i].id.clone(), sorted[j].id.clone()));
            }
        }

        // Discover patterns from replay
        let discovered_patterns = self.discover_replay_patterns(&sorted);

        MemoryReplay {
            sequence,
            synthetic_combinations,
            discovered_patterns,
            replayed_at: Utc::now(),
        }
    }

    /// Discover patterns during replay
    fn discover_replay_patterns(&self, memories: &[&DreamMemory]) -> Vec<Pattern> {
        let mut patterns = Vec::new();
        let mut tag_sequences: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        // Track tag occurrence patterns
        for memory in memories {
            for tag in &memory.tags {
                tag_sequences
                    .entry(tag.clone())
                    .or_default()
                    .push(memory.created_at);
            }
        }

        // Identify recurring patterns
        for (tag, timestamps) in tag_sequences {
            if timestamps.len() >= 3 {
                patterns.push(Pattern {
                    id: format!("pattern-{}", Uuid::new_v4()),
                    pattern_type: PatternType::Recurring,
                    description: format!(
                        "Recurring theme '{}' across {} memories",
                        tag,
                        timestamps.len()
                    ),
                    memory_ids: memories
                        .iter()
                        .filter(|m| m.tags.contains(&tag))
                        .map(|m| m.id.clone())
                        .collect(),
                    confidence: (timestamps.len() as f64 / memories.len() as f64).min(1.0),
                    discovered_at: Utc::now(),
                });
            }
        }

        patterns
    }

    /// Stage 2: Cross-reference with existing knowledge
    fn stage2_cross_reference(&self, memories: &[DreamMemory], replay: &MemoryReplay) -> usize {
        let memory_map: HashMap<_, _> = memories.iter().map(|m| (m.id.clone(), m)).collect();

        let mut connections_found = 0;

        if let Ok(mut graph) = self.connections.write() {
            for (id_a, id_b) in &replay.synthetic_combinations {
                if let (Some(mem_a), Some(mem_b)) = (memory_map.get(id_a), memory_map.get(id_b)) {
                    // Check for connection potential
                    let similarity = calculate_memory_similarity(mem_a, mem_b);
                    if similarity >= MIN_SIMILARITY_FOR_CONNECTION {
                        graph.add_connection(
                            id_a,
                            id_b,
                            similarity,
                            ConnectionReason::CrossReference,
                        );
                        connections_found += 1;
                    }
                }
            }
        }

        connections_found
    }

    /// Stage 3: Strengthen connections that fired together
    fn stage3_strengthen(&self, replay: &MemoryReplay) -> usize {
        let mut strengthened = 0;

        if let Ok(mut graph) = self.connections.write() {
            // Strengthen connections between sequentially replayed memories
            for window in replay.sequence.windows(2) {
                if let [id_a, id_b] = window
                    && graph.strengthen_connection(id_a, id_b, 0.1)
                {
                    strengthened += 1;
                }
            }

            // Also strengthen based on discovered patterns
            for pattern in &replay.discovered_patterns {
                for i in 0..pattern.memory_ids.len() {
                    for j in (i + 1)..pattern.memory_ids.len() {
                        if graph.strengthen_connection(
                            &pattern.memory_ids[i],
                            &pattern.memory_ids[j],
                            0.05 * pattern.confidence,
                        ) {
                            strengthened += 1;
                        }
                    }
                }
            }
        }

        strengthened
    }

    /// Stage 4: Prune weak connections not reactivated
    fn stage4_prune(&self) -> usize {
        let mut pruned = 0;

        if let Ok(mut graph) = self.connections.write() {
            // Apply decay to all connections
            graph.apply_decay(CONNECTION_DECAY_FACTOR);

            // Remove connections below threshold
            pruned = graph.prune_weak(MIN_CONNECTION_STRENGTH);
        }

        pruned
    }

    /// Stage 5: Identify memories ready for semantic storage transfer
    fn stage5_transfer(&self, memories: &[DreamMemory]) -> Vec<String> {
        // Memories with high access count and strong connections are candidates
        // for transfer from episodic to semantic storage
        let mut candidates = Vec::new();

        if let Ok(graph) = self.connections.read() {
            for memory in memories {
                let connection_count = graph.connection_count(&memory.id);
                let total_strength = graph.total_connection_strength(&memory.id);

                // Criteria for semantic transfer:
                // - Accessed multiple times
                // - Has multiple strong connections
                // - Is part of discovered patterns
                if memory.access_count >= 3 && connection_count >= 2 && total_strength >= 1.0 {
                    candidates.push(memory.id.clone());
                }
            }
        }

        candidates
    }

    /// Enable or disable automatic consolidation
    pub fn set_auto_enabled(&mut self, enabled: bool) {
        self.auto_enabled = enabled;
    }

    /// Get consolidation history
    pub fn get_history(&self) -> &[ConsolidationReport] {
        &self.consolidation_history
    }

    /// Get activity statistics
    pub fn get_activity_stats(&self) -> ActivityStats {
        self.activity_tracker.get_stats()
    }

    /// Get time until next scheduled consolidation
    pub fn time_until_next(&self) -> Duration {
        let elapsed = Utc::now() - self.last_consolidation;
        if elapsed >= self.consolidation_interval {
            Duration::zero()
        } else {
            self.consolidation_interval - elapsed
        }
    }

    /// Get the memory dreamer for direct access
    pub fn dreamer(&self) -> &MemoryDreamer {
        &self.dreamer
    }

    /// Get connection graph statistics
    pub fn get_connection_stats(&self) -> Option<ConnectionStats> {
        self.connections.read().ok().map(|g| g.get_stats())
    }
}

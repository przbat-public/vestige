//! `DreamEngine` orchestrator — runs all four phases and shares the categorize helper.

use std::time::Instant;

use crate::memory::KnowledgeNode;
use crate::neuroscience::emotional_memory::{EmotionCategory, EmotionalMemory};
use crate::neuroscience::importance_signals::ImportanceSignals;
use crate::neuroscience::synaptic_tagging::SynapticTaggingSystem;

use super::types::{FourPhaseDreamResult, TriageCategory};

/// Orchestrates the 4-phase biologically-accurate dream cycle
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Tunable thresholds are read by sibling submodules `nrem1`, `nrem3`, `rem`, `integration` of the cohesive `phases` component (split-by-responsibility refactor); siblings have the same trust level as the parent module."
)]
pub struct DreamEngine {
    /// NREM1: 70% high-value, 30% random noise floor
    pub(super) high_value_ratio: f64,
    /// NREM3: batch size for oscillation waves
    pub(super) wave_batch_size: usize,
    /// NREM3: synaptic downscaling factor for unreplayed low-importance memories
    pub(super) downscale_factor: f64,
    /// REM: minimum confidence for cross-domain insights
    pub(super) min_insight_confidence: f64,
    /// Integration: minimum confidence to keep an insight
    pub(super) validation_threshold: f64,
}

impl Default for DreamEngine {
    fn default() -> Self {
        Self {
            high_value_ratio: 0.7,
            wave_batch_size: 15,
            downscale_factor: 0.95,
            min_insight_confidence: 0.3,
            validation_threshold: 0.4,
        }
    }
}

impl DreamEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the complete 4-phase dream cycle
    pub fn run(
        &self,
        memories: &[KnowledgeNode],
        emotional_memory: &mut EmotionalMemory,
        importance_signals: &ImportanceSignals,
        synaptic_tagging: &mut SynapticTaggingSystem,
    ) -> FourPhaseDreamResult {
        let total_start = Instant::now();
        let mut phases = Vec::with_capacity(4);

        // ==================== PHASE 1: NREM1 (Triage) ====================
        let (triaged, replay_queue, phase1) =
            self.phase_nrem1(memories, emotional_memory, importance_signals);
        phases.push(phase1);

        // ==================== PHASE 2: NREM3 (Consolidation) ====================
        let (strengthened_ids, downscaled_ids, phase2) =
            self.phase_nrem3(&replay_queue, &triaged, synaptic_tagging);
        phases.push(phase2);

        // ==================== PHASE 3: REM (Creative) ====================
        let (connections, emotional_processed, phase3) = self.phase_rem(&triaged, emotional_memory);
        phases.push(phase3);

        // ==================== PHASE 4: Integration ====================
        let (insights, phase4) = self.phase_integration(&connections, &triaged);
        phases.push(phase4);

        FourPhaseDreamResult {
            total_duration_ms: total_start.elapsed().as_millis() as u64,
            memories_replayed: replay_queue.len(),
            replay_queue_size: replay_queue.len(),
            insights,
            creative_connections: connections,
            memories_strengthened: strengthened_ids.len(),
            memories_downscaled: downscaled_ids.len(),
            emotional_processed,
            phases,
            strengthened_ids,
            downscaled_ids,
        }
    }

    // Shared with phase_nrem1 (cross-module).
    pub(super) fn categorize_memory(
        &self,
        node: &KnowledgeNode,
        importance: f64,
        emotion: &EmotionCategory,
    ) -> TriageCategory {
        // High emotional content
        if matches!(
            emotion,
            EmotionCategory::Frustration
                | EmotionCategory::Urgency
                | EmotionCategory::Joy
                | EmotionCategory::Surprise
        ) && node.sentiment_magnitude > 0.4
        {
            return TriageCategory::Emotional;
        }

        // Future-relevant (intentions, TODOs)
        let content_lower = node.content.to_lowercase();
        if content_lower.contains("todo")
            || content_lower.contains("remind")
            || content_lower.contains("intention")
            || content_lower.contains("next time")
            || content_lower.contains("plan to")
        {
            return TriageCategory::FutureRelevant;
        }

        // Rewarded (promoted or high utility)
        if node.utility_score.unwrap_or(0.0) > 0.5 || node.reps >= 5 {
            return TriageCategory::Rewarded;
        }

        // Novel (high importance score)
        if importance > 0.6 {
            return TriageCategory::Novel;
        }

        TriageCategory::Standard
    }
}

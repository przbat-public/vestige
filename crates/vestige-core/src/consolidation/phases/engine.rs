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

/// Default NREM3 synaptic downscaling factor — a multiplicative retention
/// loss applied per dream cycle to unreplayed low-importance memories.
///
/// Pre-2026-05-20 this was `0.95` (5 % loss per cycle). The synaptic
/// homeostasis hypothesis (Tononi & Cirelli, *Sleep & Brain Plasticity*,
/// 2014; Diekelmann & Born 2010) reports 10-25 % weakening of weak,
/// unreplayed synapses per slow-wave-rich sleep cycle. Five percent sat
/// at the very low end of that range and effectively turned downscaling
/// into a slow, almost imperceptible nudge; the consolidation pass
/// happens at most a couple of times a day in our deployment, so 5 %
/// would take ~14 cycles to halve retention.
///
/// `0.90` sits at the conservative mid-band of biological estimates and
/// halves retention in ~7 cycles — closer to the observable "if I never
/// touch this for a week, the system stops surfacing it" behaviour we
/// actually want for the long tail. Override at runtime with
/// `VESTIGE_NREM3_DOWNSCALE_FACTOR` if you want to tighten / loosen the
/// curve without recompiling. Out-of-range values (≤0, >1, NaN) fall
/// back to this default.
pub const DEFAULT_NREM3_DOWNSCALE_FACTOR: f64 = 0.90;

/// Resolve the NREM3 downscale factor honoured by this build.
///
/// Reads `VESTIGE_NREM3_DOWNSCALE_FACTOR` from the environment with the
/// usual `(0.0, 1.0]` clamp; falls back to
/// [`DEFAULT_NREM3_DOWNSCALE_FACTOR`] otherwise.
pub fn default_nrem3_downscale_factor() -> f64 {
    match std::env::var("VESTIGE_NREM3_DOWNSCALE_FACTOR") {
        Ok(raw) => raw
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0 && *v <= 1.0)
            .unwrap_or(DEFAULT_NREM3_DOWNSCALE_FACTOR),
        Err(_) => DEFAULT_NREM3_DOWNSCALE_FACTOR,
    }
}

impl Default for DreamEngine {
    fn default() -> Self {
        Self {
            high_value_ratio: 0.7,
            wave_batch_size: 15,
            downscale_factor: default_nrem3_downscale_factor(),
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
            downscale_factor: self.downscale_factor,
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

        // Future-relevant (intentions, TODOs).
        //
        // Routed through [`crate::nlp::default_future_relevance_detector`]
        // so this call site stays untouched if/when we swap in an ONNX
        // classifier behind a feature flag.
        if crate::nlp::default_future_relevance_detector()
            .detect(&node.content)
            .positive
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

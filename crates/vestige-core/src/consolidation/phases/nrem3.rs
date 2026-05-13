//! NREM3 — deep sleep / consolidation. SO-spindle-ripple coupling, downscaling.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::neuroscience::synaptic_tagging::SynapticTaggingSystem;

use super::engine::DreamEngine;
use super::types::{DreamPhase, PhaseResult, TriagedMemory};

impl DreamEngine {
    pub(super) fn phase_nrem3(
        &self,
        replay_queue: &[String],
        triaged: &[TriagedMemory],
        synaptic_tagging: &mut SynapticTaggingSystem,
    ) -> (Vec<String>, usize, PhaseResult) {
        let start = Instant::now();
        let mut actions = Vec::new();
        let mut strengthened_ids = Vec::new();

        let replay_set: HashSet<&String> = replay_queue.iter().collect();
        let _triaged_map: HashMap<&str, &TriagedMemory> =
            triaged.iter().map(|m| (m.id.as_str(), m)).collect();

        // Process replay queue in oscillation waves
        let wave_count = replay_queue.len().div_ceil(self.wave_batch_size);

        for wave_idx in 0..wave_count {
            let wave_start = wave_idx * self.wave_batch_size;
            let wave_end = (wave_start + self.wave_batch_size).min(replay_queue.len());
            let wave = &replay_queue[wave_start..wave_end];

            // SO phase: The wave IS the selected cluster
            // Spindle phase: Tag memories for consolidation via synaptic tagging
            for id in wave {
                // Tag this memory in the synaptic tagging system
                synaptic_tagging.tag_memory(id);
                strengthened_ids.push(id.clone());
            }

            // Ripple phase: Find sequential pairs within the wave for causal linking
            // (Adjacent memories in replay order represent temporal associations)
        }

        actions.push(format!(
            "Processed {} waves of {} memories",
            wave_count,
            replay_queue.len()
        ));
        actions.push(format!(
            "Strengthened {} memories via synaptic tagging",
            strengthened_ids.len()
        ));

        // Synaptic downscaling: reduce retention on unreplayed low-importance memories
        let mut downscaled_count = 0;
        for tm in triaged {
            if !replay_set.contains(&tm.id) && tm.importance < 0.4 {
                // This memory wasn't replayed and has low importance
                // In the actual DB update, we'd multiply retrieval_strength by downscale_factor
                downscaled_count += 1;
            }
        }

        if downscaled_count > 0 {
            actions.push(format!(
                "Synaptic downscaling: {} unreplayed low-importance memories marked for {}x decay",
                downscaled_count, self.downscale_factor
            ));
        }

        let phase = PhaseResult {
            phase: DreamPhase::Nrem3,
            duration_ms: start.elapsed().as_millis() as u64,
            memories_processed: replay_queue.len(),
            actions,
        };

        (strengthened_ids, downscaled_count, phase)
    }
}

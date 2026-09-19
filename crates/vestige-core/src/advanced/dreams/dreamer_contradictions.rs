//! Phase 3 of the dream cycle: detect contradictions among connected memories
//! and pick the demotion candidate when one is found.
//!
//! Extracted from `dreamer.rs`. Inherent methods on [`MemoryDreamer`].

use std::collections::HashMap;

use super::dreamer::MemoryDreamer;
use super::types::{
    ContradictionPair, DiscoveredConnection, DiscoveredConnectionType, DreamMemory,
};

impl MemoryDreamer {
    /// Detect contradictions and pick which memory to demote.
    ///
    /// The survivor is the current version by
    /// [`FreshnessKey`](crate::memory::FreshnessKey) — newer wins, and an exact
    /// tie falls to the id tiebreak. Neither `access_count` nor the direction
    /// the connection was discovered in takes part: ranking a stale memory up
    /// for being retrieved often is how the dream cycle used to demote the
    /// correction and keep the fact it replaced.
    pub(super) fn detect_contradictions(
        &self,
        memories: &[&DreamMemory],
        connections: &[DiscoveredConnection],
    ) -> Vec<ContradictionPair> {
        let mem_map: HashMap<&str, &DreamMemory> =
            memories.iter().map(|m| (m.id.as_str(), *m)).collect();

        connections
            .iter()
            .filter(|c| c.connection_type == DiscoveredConnectionType::Contradiction)
            .filter_map(|c| {
                let a = mem_map.get(c.from_id.as_str())?;
                let b = mem_map.get(c.to_id.as_str())?;
                // Both sides are reduced to the same key before comparing, so
                // swapping `from_id`/`to_id` swaps the verdict, never changes
                // it. Distinct ids can never compare `Equal`.
                let (survivor, demoted) = if b.freshness_key().is_fresher_than(&a.freshness_key()) {
                    (b, a)
                } else {
                    (a, b)
                };
                Some(ContradictionPair {
                    survivor_id: survivor.id.clone(),
                    demoted_id: demoted.id.clone(),
                    similarity: c.similarity,
                    reason: c.reasoning.clone(),
                })
            })
            .collect()
    }
}

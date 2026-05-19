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
    /// Heuristic: when two memories share high similarity (same topic)
    /// but diverge in negation markers, the newer or more-accessed one
    /// is the "survivor" and the other becomes the demotion candidate.
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
                let (survivor, demoted) = if a.access_count > b.access_count {
                    (a, b)
                } else if b.access_count > a.access_count {
                    (b, a)
                } else if a.created_at > b.created_at {
                    (a, b)
                } else {
                    (b, a)
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

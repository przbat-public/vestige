//! Phase 5 & 6 of the dream cycle plus result persistence helpers: identify
//! which memories to strengthen, which are compression candidates, and write
//! the cycle's outputs into the in-memory caches.
//!
//! Extracted from `dreamer.rs`. Inherent methods on [`MemoryDreamer`].

use std::collections::HashMap;

use chrono::{Duration, Utc};

use super::dreamer::MemoryDreamer;
use super::types::{DiscoveredConnection, DreamMemory, SynthesizedInsight};

impl MemoryDreamer {
    pub(super) fn identify_memories_to_strengthen(
        &self,
        _memories: &[&DreamMemory],
        connections: &[DiscoveredConnection],
    ) -> usize {
        // Memories that participate in more than the average number of
        // connections are considered "hubs" worth strengthening.
        let mut connection_counts: HashMap<&str, usize> = HashMap::new();

        for conn in connections {
            *connection_counts.entry(&conn.from_id).or_insert(0) += 1;
            *connection_counts.entry(&conn.to_id).or_insert(0) += 1;
        }

        let avg_connections = if connection_counts.is_empty() {
            0.0
        } else {
            connection_counts.values().sum::<usize>() as f64 / connection_counts.len() as f64
        };

        connection_counts
            .values()
            .filter(|&&count| count as f64 > avg_connections)
            .count()
    }

    pub(super) fn identify_compression_candidates(&self, memories: &[&DreamMemory]) -> usize {
        // Old, rarely-accessed memories are candidates; conservative third
        // of those that match the criterion is returned.
        let now = Utc::now();
        let old_threshold = now - Duration::days(60);

        memories
            .iter()
            .filter(|m| m.created_at < old_threshold && m.access_count < 3)
            .count()
            / 3
    }

    pub(super) fn store_connections(&self, connections: &[DiscoveredConnection]) {
        if let Ok(mut stored) = self.connections.write() {
            stored.extend(connections.iter().cloned());
            // Bounded cache; oldest connections fall off the front.
            let len = stored.len();
            if len > 1000 {
                stored.drain(0..(len - 1000));
            }
        }
    }

    pub(super) fn store_insights(&self, insights: &[SynthesizedInsight]) {
        if let Ok(mut stored) = self.insights.write() {
            stored.extend(insights.iter().cloned());
            let len = stored.len();
            if len > 500 {
                stored.drain(0..(len - 500));
            }
        }
    }
}

//! Phase 2 of the dream cycle: cluster discovered connections into groups of
//! related memories. Used as the basis for insight generation downstream.
//!
//! Extracted from `dreamer.rs`. Inherent methods on [`MemoryDreamer`].

use std::collections::HashSet;

use super::constants::MIN_MEMORIES_FOR_INSIGHT;
use super::dreamer::MemoryDreamer;
use super::types::{DiscoveredConnection, DreamMemory};

impl MemoryDreamer {
    /// Union-find style clustering over the connection graph. Each pair of
    /// connected ids ends up in the same cluster; overlapping clusters are
    /// merged until none touch.
    pub(super) fn find_clusters(
        &self,
        _memories: &[&DreamMemory],
        connections: &[DiscoveredConnection],
    ) -> Vec<Vec<String>> {
        let mut clusters: Vec<HashSet<String>> = Vec::new();

        for conn in connections {
            // Find existing cluster containing either endpoint.
            let mut found_cluster = None;
            for (i, cluster) in clusters.iter().enumerate() {
                if cluster.contains(&conn.from_id) || cluster.contains(&conn.to_id) {
                    found_cluster = Some(i);
                    break;
                }
            }

            match found_cluster {
                Some(i) => {
                    clusters[i].insert(conn.from_id.clone());
                    clusters[i].insert(conn.to_id.clone());
                }
                None => {
                    let mut new_cluster = HashSet::new();
                    new_cluster.insert(conn.from_id.clone());
                    new_cluster.insert(conn.to_id.clone());
                    clusters.push(new_cluster);
                }
            }
        }

        // Merge overlapping clusters until stable.
        let mut merged = true;
        while merged {
            merged = false;
            for i in 0..clusters.len() {
                for j in (i + 1)..clusters.len() {
                    if !clusters[i].is_disjoint(&clusters[j]) {
                        let to_merge: HashSet<_> = clusters[j].drain().collect();
                        clusters[i].extend(to_merge);
                        merged = true;
                        break;
                    }
                }
                if merged {
                    clusters.retain(|c| !c.is_empty());
                    break;
                }
            }
        }

        clusters
            .into_iter()
            .filter(|c| c.len() >= MIN_MEMORIES_FOR_INSIGHT)
            .map(|c| c.into_iter().collect())
            .collect()
    }
}

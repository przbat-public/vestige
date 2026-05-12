//! Graph traversal repository for [`super::Storage`].
//!
//! Two queries that walk the `memory_connections` graph for the dashboard
//! visualizer:
//!
//! - [`Storage::get_most_connected_memory`] — degree-rank hub query,
//!   returns the single ID with the highest combined in+out degree.
//!   Used as the default center for the subgraph view.
//! - [`Storage::get_memory_subgraph`] — bounded BFS from a center node up
//!   to `depth` hops, capped by `max_nodes`. Returns both the visited
//!   nodes and the induced edge set (edges where both endpoints are in
//!   the visited set), so the frontend never has to filter.
//!
//! BFS is intentional: it ensures the closest neighborhood is rendered
//! first if `max_nodes` cuts the walk short. DFS would bias the result
//! toward one chain.

use rusqlite::OptionalExtension;

use crate::memory::KnowledgeNode;

use super::{ConnectionRecord, Result, Storage, StorageError};

impl Storage {
    /// Get the memory with the most connections (best center node for graph visualization)
    pub fn get_most_connected_memory(&self) -> Result<Option<String>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT id, COUNT(*) as cnt FROM (
                SELECT source_id as id FROM memory_connections
                UNION ALL
                SELECT target_id as id FROM memory_connections
            ) GROUP BY id ORDER BY cnt DESC LIMIT 1"
        )?;
        let result = stmt.query_row([], |row| row.get::<_, String>(0)).optional()?;
        Ok(result)
    }

    /// Get memories with their connection data for graph visualization
    pub fn get_memory_subgraph(&self, center_id: &str, depth: u32, max_nodes: usize) -> Result<(Vec<KnowledgeNode>, Vec<ConnectionRecord>)> {
        let mut visited_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut frontier = vec![center_id.to_string()];
        visited_ids.insert(center_id.to_string());

        // BFS to discover connected nodes up to depth
        for _ in 0..depth {
            let mut next_frontier = Vec::new();
            for id in &frontier {
                let connections = self.get_connections_for_memory(id)?;
                for conn in &connections {
                    let other_id = if conn.source_id == *id { &conn.target_id } else { &conn.source_id };
                    if visited_ids.insert(other_id.clone()) {
                        next_frontier.push(other_id.clone());
                        if visited_ids.len() >= max_nodes {
                            break;
                        }
                    }
                }
                if visited_ids.len() >= max_nodes {
                    break;
                }
            }
            frontier = next_frontier;
            if frontier.is_empty() || visited_ids.len() >= max_nodes {
                break;
            }
        }

        // Fetch nodes
        let mut nodes = Vec::new();
        for id in &visited_ids {
            if let Some(node) = self.get_node(id)? {
                nodes.push(node);
            }
        }

        // Fetch edges between visited nodes
        let all_connections = self.get_all_connections()?;
        let edges: Vec<ConnectionRecord> = all_connections
            .into_iter()
            .filter(|c| visited_ids.contains(&c.source_id) && visited_ids.contains(&c.target_id))
            .collect();

        Ok((nodes, edges))
    }
}

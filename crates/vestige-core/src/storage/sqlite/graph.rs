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
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT id, COUNT(*) as cnt FROM (
                SELECT source_id as id FROM memory_connections
                UNION ALL
                SELECT target_id as id FROM memory_connections
            ) GROUP BY id ORDER BY cnt DESC LIMIT 1",
        )?;
        let result = stmt
            .query_row([], |row| row.get::<_, String>(0))
            .optional()?;
        Ok(result)
    }

    /// Get memories with their connection data for graph visualization
    pub fn get_memory_subgraph(
        &self,
        center_id: &str,
        depth: u32,
        max_nodes: usize,
    ) -> Result<(Vec<KnowledgeNode>, Vec<ConnectionRecord>)> {
        let mut visited_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut frontier = vec![center_id.to_string()];
        visited_ids.insert(center_id.to_string());

        // BFS to discover connected nodes up to depth
        for _ in 0..depth {
            let mut next_frontier = Vec::new();
            for id in &frontier {
                let connections = self.get_connections_for_memory(id)?;
                for conn in &connections {
                    let other_id = if conn.source_id == *id {
                        &conn.target_id
                    } else {
                        &conn.source_id
                    };
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

    /// Find the shortest connection chain between two memories, treating
    /// edges as undirected (mirrors how `get_connections_for_memory`
    /// returns both directions). Returns the ordered ID sequence from
    /// `from` to `to`, or `None` when no chain of length ≤ `max_depth`
    /// exists.
    ///
    /// This is the storage primitive behind the dashboard's `chains` and
    /// `bridges` Explore modes. The old code called
    /// [`Self::get_memory_subgraph`] from `from` and silently dropped
    /// `to`, so the UI displayed "everything reachable from A" instead
    /// of "the path from A to B" — a semantic bug that made bridge
    /// detection meaningless. BFS guarantees the shortest path, which
    /// is what `bridges` needs (intermediates are exactly
    /// `path[1..path.len()-1]`).
    pub fn find_path_between(
        &self,
        from: &str,
        to: &str,
        max_depth: u32,
    ) -> Result<Option<Vec<String>>> {
        if from == to {
            return Ok(Some(vec![from.to_string()]));
        }
        if max_depth == 0 {
            return Ok(None);
        }

        // parent[child] = node we came from. Reconstruct the path by
        // walking parents from `to` back to `from` once BFS finds it.
        let mut parent: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        visited.insert(from.to_string());

        let mut frontier = vec![from.to_string()];

        for _ in 0..max_depth {
            let mut next_frontier = Vec::new();
            for current in &frontier {
                let connections = self.get_connections_for_memory(current)?;
                for conn in &connections {
                    let neighbor = if conn.source_id == *current {
                        &conn.target_id
                    } else {
                        &conn.source_id
                    };
                    if visited.insert(neighbor.clone()) {
                        parent.insert(neighbor.clone(), current.clone());
                        if neighbor == to {
                            // Reconstruct path in reverse, then flip.
                            let mut path = vec![to.to_string()];
                            let mut cursor = to.to_string();
                            while let Some(prev) = parent.get(&cursor) {
                                path.push(prev.clone());
                                cursor = prev.clone();
                            }
                            path.reverse();
                            return Ok(Some(path));
                        }
                        next_frontier.push(neighbor.clone());
                    }
                }
            }
            if next_frontier.is_empty() {
                break;
            }
            frontier = next_frontier;
        }

        Ok(None)
    }
}

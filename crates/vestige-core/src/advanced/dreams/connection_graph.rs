//! Bidirectional graph of memory-to-memory connections.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Graph of connections between memories
#[derive(Debug, Clone)]
pub struct ConnectionGraph {
    /// Adjacency list: memory_id -> [(connected_id, strength, reason)]
    connections: HashMap<String, Vec<MemoryConnection>>,
    /// Total connections ever created
    total_created: usize,
    /// Total connections pruned
    total_pruned: usize,
}

/// A connection between two memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConnection {
    /// Connected memory ID
    pub target_id: String,
    /// Connection strength (0.0 to 1.0+)
    pub strength: f64,
    /// Why this connection exists
    pub reason: ConnectionReason,
    /// When this connection was created
    pub created_at: DateTime<Utc>,
    /// When this connection was last strengthened
    pub last_strengthened: DateTime<Utc>,
}

/// Reason for a memory connection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionReason {
    /// Semantic similarity
    Semantic,
    /// Cross-reference during consolidation
    CrossReference,
    /// Sequential access pattern
    Sequential,
    /// Shared tags/concepts
    SharedConcepts,
    /// User-defined link
    UserDefined,
    /// Discovered pattern
    Pattern,
}

impl Default for ConnectionGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionGraph {
    /// Create a new connection graph
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            total_created: 0,
            total_pruned: 0,
        }
    }

    /// Add a connection between two memories
    pub fn add_connection(
        &mut self,
        from_id: &str,
        to_id: &str,
        strength: f64,
        reason: ConnectionReason,
    ) {
        let now = Utc::now();

        // Add bidirectional connection
        for (a, b) in [(from_id, to_id), (to_id, from_id)] {
            let connections = self.connections.entry(a.to_string()).or_default();

            // Check if connection already exists
            if let Some(existing) = connections.iter_mut().find(|c| c.target_id == b) {
                existing.strength = (existing.strength + strength).min(2.0);
                existing.last_strengthened = now;
            } else {
                connections.push(MemoryConnection {
                    target_id: b.to_string(),
                    strength,
                    reason: reason.clone(),
                    created_at: now,
                    last_strengthened: now,
                });
                self.total_created += 1;
            }
        }
    }

    /// Strengthen an existing connection
    pub fn strengthen_connection(&mut self, from_id: &str, to_id: &str, boost: f64) -> bool {
        let now = Utc::now();
        let mut strengthened = false;

        for (a, b) in [(from_id, to_id), (to_id, from_id)] {
            if let Some(connections) = self.connections.get_mut(a)
                && let Some(conn) = connections.iter_mut().find(|c| c.target_id == b)
            {
                conn.strength = (conn.strength + boost).min(2.0);
                conn.last_strengthened = now;
                strengthened = true;
            }
        }

        strengthened
    }

    /// Apply decay to all connections
    pub fn apply_decay(&mut self, decay_factor: f64) {
        for connections in self.connections.values_mut() {
            for conn in connections.iter_mut() {
                conn.strength *= decay_factor;
            }
        }
    }

    /// Prune connections below threshold
    pub fn prune_weak(&mut self, min_strength: f64) -> usize {
        let mut pruned = 0;

        for connections in self.connections.values_mut() {
            let before = connections.len();
            connections.retain(|c| c.strength >= min_strength);
            pruned += before - connections.len();
        }

        self.total_pruned += pruned;
        pruned
    }

    /// Get number of connections for a memory
    pub fn connection_count(&self, memory_id: &str) -> usize {
        self.connections
            .get(memory_id)
            .map(|c| c.len())
            .unwrap_or(0)
    }

    /// Get total connection strength for a memory
    pub fn total_connection_strength(&self, memory_id: &str) -> f64 {
        self.connections
            .get(memory_id)
            .map(|connections| connections.iter().map(|c| c.strength).sum())
            .unwrap_or(0.0)
    }

    /// Get all connections for a memory
    pub fn get_connections(&self, memory_id: &str) -> Vec<&MemoryConnection> {
        self.connections
            .get(memory_id)
            .map(|c| c.iter().collect())
            .unwrap_or_default()
    }

    /// Get statistics about the connection graph
    pub fn get_stats(&self) -> ConnectionStats {
        let total_connections: usize = self.connections.values().map(|c| c.len()).sum();
        let total_strength: f64 = self
            .connections
            .values()
            .flat_map(|c| c.iter())
            .map(|c| c.strength)
            .sum();

        ConnectionStats {
            total_memories: self.connections.len(),
            total_connections: total_connections / 2, // Bidirectional, so divide by 2
            average_strength: if total_connections > 0 {
                total_strength / total_connections as f64
            } else {
                0.0
            },
            total_created: self.total_created / 2,
            total_pruned: self.total_pruned / 2,
        }
    }
}

/// Statistics about the connection graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    /// Number of memories with connections
    pub total_memories: usize,
    /// Total number of connections
    pub total_connections: usize,
    /// Average connection strength
    pub average_strength: f64,
    /// Total connections ever created
    pub total_created: usize,
    /// Total connections pruned
    pub total_pruned: usize,
}

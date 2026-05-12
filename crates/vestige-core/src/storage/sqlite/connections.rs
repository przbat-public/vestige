//! Memory connections persistence — the spreading-activation network.
//!
//! Connections form a weighted directed graph over `knowledge_nodes`. They
//! power semantic priming, dream-cycle replay edges, and graph-style "show
//! me what's related" queries. The graph stays small enough to fit in
//! SQLite — that's deliberate, because every consolidation cycle decays
//! and prunes edges (see `apply_connection_decay`, `prune_weak_connections`).

use chrono::Utc;
use rusqlite::params;

use super::{ConnectionRecord, Result, Storage, StorageError};

impl Storage {
    /// Save a memory connection
    pub fn save_connection(&self, connection: &ConnectionRecord) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT OR REPLACE INTO memory_connections (
                source_id, target_id, strength, link_type, created_at, last_activated, activation_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                connection.source_id,
                connection.target_id,
                connection.strength,
                connection.link_type,
                connection.created_at.to_rfc3339(),
                connection.last_activated.to_rfc3339(),
                connection.activation_count,
            ],
        )?;
        Ok(())
    }

    /// Get connections for a memory
    pub fn get_connections_for_memory(&self, memory_id: &str) -> Result<Vec<ConnectionRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM memory_connections WHERE source_id = ?1 OR target_id = ?1 ORDER BY strength DESC"
        )?;

        let rows = stmt.query_map(params![memory_id], Self::row_to_connection)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get all connections (for building activation network)
    pub fn get_all_connections(&self) -> Result<Vec<ConnectionRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare("SELECT * FROM memory_connections ORDER BY strength DESC")?;

        let rows = stmt.query_map([], Self::row_to_connection)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Strengthen a connection
    pub fn strengthen_connection(
        &self,
        source_id: &str,
        target_id: &str,
        boost: f64,
    ) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute(
            "UPDATE memory_connections SET
                strength = MIN(strength + ?1, 1.0),
                last_activated = ?2,
                activation_count = activation_count + 1
             WHERE source_id = ?3 AND target_id = ?4",
            params![boost, now, source_id, target_id],
        )?;
        Ok(rows > 0)
    }

    /// Apply decay to all connections
    pub fn apply_connection_decay(&self, decay_factor: f64) -> Result<i32> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute(
            "UPDATE memory_connections SET strength = strength * ?1",
            params![decay_factor],
        )?;
        Ok(rows as i32)
    }

    /// Prune weak connections below threshold
    pub fn prune_weak_connections(&self, min_strength: f64) -> Result<i32> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute(
            "DELETE FROM memory_connections WHERE strength < ?1",
            params![min_strength],
        )?;
        Ok(rows as i32)
    }
}

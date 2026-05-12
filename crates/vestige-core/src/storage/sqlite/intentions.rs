//! Intentions persistence — CRUD over the `intentions` table.
//!
//! Prospective memory: things the agent committed to do later. These rows are
//! independent of `knowledge_nodes` and never participate in FSRS, embeddings,
//! or the cognitive engine — they only get checked by the proactive-trigger
//! scheduler. Splitting them out of the SQLite monolith reduces cognitive
//! load when reading the storage layer.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

use super::{IntentionRecord, Result, Storage, StorageError};

impl Storage {
    /// Save an intention to the database
    pub fn save_intention(&self, intention: &IntentionRecord) -> Result<()> {
        let tags_json = serde_json::to_string(&intention.tags).unwrap_or_else(|_| "[]".to_string());
        let related_json =
            serde_json::to_string(&intention.related_memories).unwrap_or_else(|_| "[]".to_string());

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT OR REPLACE INTO intentions (
                id, content, trigger_type, trigger_data, priority, status,
                created_at, deadline, fulfilled_at, reminder_count, last_reminded_at,
                notes, tags, related_memories, snoozed_until, source_type, source_data
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                intention.id,
                intention.content,
                intention.trigger_type,
                intention.trigger_data,
                intention.priority,
                intention.status,
                intention.created_at.to_rfc3339(),
                intention.deadline.map(|dt| dt.to_rfc3339()),
                intention.fulfilled_at.map(|dt| dt.to_rfc3339()),
                intention.reminder_count,
                intention.last_reminded_at.map(|dt| dt.to_rfc3339()),
                intention.notes,
                tags_json,
                related_json,
                intention.snoozed_until.map(|dt| dt.to_rfc3339()),
                intention.source_type,
                intention.source_data,
            ],
        )?;
        Ok(())
    }

    /// Get an intention by ID
    pub fn get_intention(&self, id: &str) -> Result<Option<IntentionRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare("SELECT * FROM intentions WHERE id = ?1")?;

        stmt.query_row(params![id], Self::row_to_intention)
            .optional()
            .map_err(StorageError::from)
    }

    /// Get all active intentions
    pub fn get_active_intentions(&self) -> Result<Vec<IntentionRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM intentions WHERE status = 'active' ORDER BY priority DESC, created_at ASC"
        )?;

        let rows = stmt.query_map([], Self::row_to_intention)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get intentions by status
    pub fn get_intentions_by_status(&self, status: &str) -> Result<Vec<IntentionRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM intentions WHERE status = ?1 ORDER BY priority DESC, created_at ASC",
        )?;

        let rows = stmt.query_map(params![status], Self::row_to_intention)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Update intention status
    pub fn update_intention_status(&self, id: &str, status: &str) -> Result<bool> {
        let now = Utc::now();
        let fulfilled_at = if status == "fulfilled" {
            Some(now.to_rfc3339())
        } else {
            None
        };

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute(
            "UPDATE intentions SET status = ?1, fulfilled_at = ?2 WHERE id = ?3",
            params![status, fulfilled_at, id],
        )?;
        Ok(rows > 0)
    }

    /// Delete an intention
    pub fn delete_intention(&self, id: &str) -> Result<bool> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute("DELETE FROM intentions WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    /// Get overdue intentions
    pub fn get_overdue_intentions(&self) -> Result<Vec<IntentionRecord>> {
        let now = Utc::now().to_rfc3339();
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM intentions WHERE status = 'active' AND deadline IS NOT NULL AND deadline < ?1 ORDER BY deadline ASC"
        )?;

        let rows = stmt.query_map(params![now], Self::row_to_intention)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Snooze an intention
    pub fn snooze_intention(&self, id: &str, until: DateTime<Utc>) -> Result<bool> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute(
            "UPDATE intentions SET status = 'snoozed', snoozed_until = ?1 WHERE id = ?2",
            params![until.to_rfc3339(), id],
        )?;
        Ok(rows > 0)
    }
}

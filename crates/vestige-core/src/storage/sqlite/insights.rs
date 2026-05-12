//! Insights persistence — CRUD over the `insights` table.
//!
//! Insights are emergent observations produced by the cognitive engine
//! (dream cycles, meta-cognition, contradiction detection). They reference
//! source memories by id but live in their own table to keep the
//! `knowledge_nodes` schema focused on raw memories.

use rusqlite::params;

use super::{InsightRecord, Result, Storage, StorageError};

impl Storage {
    /// Save an insight to the database
    pub fn save_insight(&self, insight: &InsightRecord) -> Result<()> {
        let source_json = serde_json::to_string(&insight.source_memories).unwrap_or_else(|_| "[]".to_string());
        let tags_json = serde_json::to_string(&insight.tags).unwrap_or_else(|_| "[]".to_string());

        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT OR REPLACE INTO insights (
                id, insight, source_memories, confidence, novelty_score, insight_type,
                generated_at, tags, feedback, applied_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                insight.id,
                insight.insight,
                source_json,
                insight.confidence,
                insight.novelty_score,
                insight.insight_type,
                insight.generated_at.to_rfc3339(),
                tags_json,
                insight.feedback,
                insight.applied_count,
            ],
        )?;
        Ok(())
    }

    /// Get insights with optional limit
    pub fn get_insights(&self, limit: i32) -> Result<Vec<InsightRecord>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM insights ORDER BY generated_at DESC LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![limit], Self::row_to_insight)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get insights without feedback (pending review)
    pub fn get_pending_insights(&self) -> Result<Vec<InsightRecord>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM insights WHERE feedback IS NULL ORDER BY novelty_score DESC"
        )?;

        let rows = stmt.query_map([], Self::row_to_insight)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Mark insight feedback
    pub fn mark_insight_feedback(&self, id: &str, feedback: &str) -> Result<bool> {
        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute(
            "UPDATE insights SET feedback = ?1 WHERE id = ?2",
            params![feedback, id],
        )?;
        Ok(rows > 0)
    }

    /// Clear all insights
    pub fn clear_insights(&self) -> Result<i32> {
        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let count: i32 = writer.query_row("SELECT COUNT(*) FROM insights", [], |row| row.get(0))?;
        writer.execute("DELETE FROM insights", [])?;
        Ok(count)
    }
}

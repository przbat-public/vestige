//! Time-series history repository for [`super::Storage`].
//!
//! Backs the dashboard's "system health" view and the dream/consolidation
//! schedulers. Three append-only tables plus two derived metrics:
//!
//! - `consolidation_history` — one row per full 17-step consolidation pass
//!   (counts of replayed memories, strengthened/pruned connections,
//!   insights generated). Used to throttle re-runs and to render the
//!   retrospective heatmap.
//! - `dream_history` — one row per 4-phase dream cycle, with per-phase
//!   wall-clock budgets (NREM1 / NREM3 / REM / integration) plus emergent
//!   creative-connection and emotional-replay counts.
//! - `retention_snapshots` — periodic captures of average retention,
//!   total memory count, count below the safe-floor, and whether GC fired.
//!   Read by [`Storage::get_retention_trend`].
//!
//! [`Storage::get_last_backup_timestamp`] is filesystem-only — it scans
//! the per-OS data dir's `backups/` for `vestige-YYYYMMDD-HHMMSS.db`
//! filenames and returns the most recent timestamp. No SQL, no instance.

use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{
    ConsolidationHistoryRecord, DreamHistoryRecord, Result, Storage, StorageError,
};

impl Storage {
    // ========================================================================
    // CONSOLIDATION HISTORY
    // ========================================================================

    /// Save consolidation history record
    pub fn save_consolidation_history(&self, record: &ConsolidationHistoryRecord) -> Result<i64> {
        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT INTO consolidation_history (
                completed_at, duration_ms, memories_replayed, connections_found,
                connections_strengthened, connections_pruned, insights_generated
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.completed_at.to_rfc3339(),
                record.duration_ms,
                record.memories_replayed,
                record.connections_found,
                record.connections_strengthened,
                record.connections_pruned,
                record.insights_generated,
            ],
        )?;
        Ok(writer.last_insert_rowid())
    }

    /// Get last consolidation timestamp
    pub fn get_last_consolidation(&self) -> Result<Option<DateTime<Utc>>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let result: Option<String> = reader.query_row(
            "SELECT MAX(completed_at) FROM consolidation_history",
            [],
            |row| row.get(0),
        ).ok().flatten();

        Ok(result.and_then(|s| {
            DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
        }))
    }

    /// Get consolidation history
    pub fn get_consolidation_history(&self, limit: i32) -> Result<Vec<ConsolidationHistoryRecord>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM consolidation_history ORDER BY completed_at DESC LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            Ok(ConsolidationHistoryRecord {
                id: row.get("id")?,
                completed_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("completed_at")?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                duration_ms: row.get("duration_ms")?,
                memories_replayed: row.get("memories_replayed").unwrap_or(0),
                connections_found: row.get("connections_found").unwrap_or(0),
                connections_strengthened: row.get("connections_strengthened").unwrap_or(0),
                connections_pruned: row.get("connections_pruned").unwrap_or(0),
                insights_generated: row.get("insights_generated").unwrap_or(0),
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ========================================================================
    // DREAM HISTORY PERSISTENCE
    // ========================================================================

    /// Save a dream history record
    pub fn save_dream_history(&self, record: &DreamHistoryRecord) -> Result<i64> {
        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT INTO dream_history (
                dreamed_at, duration_ms, memories_replayed, connections_found,
                insights_generated, memories_strengthened, memories_compressed,
                phase_nrem1_ms, phase_nrem3_ms, phase_rem_ms, phase_integration_ms,
                summaries_generated, emotional_memories_processed, creative_connections_found
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                record.dreamed_at.to_rfc3339(),
                record.duration_ms,
                record.memories_replayed,
                record.connections_found,
                record.insights_generated,
                record.memories_strengthened,
                record.memories_compressed,
                record.phase_nrem1_ms,
                record.phase_nrem3_ms,
                record.phase_rem_ms,
                record.phase_integration_ms,
                record.summaries_generated,
                record.emotional_memories_processed,
                record.creative_connections_found,
            ],
        )?;
        Ok(writer.last_insert_rowid())
    }

    /// Get last dream timestamp
    pub fn get_last_dream(&self) -> Result<Option<DateTime<Utc>>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let result: Option<String> = reader.query_row(
            "SELECT MAX(dreamed_at) FROM dream_history",
            [],
            |row| row.get(0),
        ).ok().flatten();

        Ok(result.and_then(|s| {
            DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
        }))
    }

    /// Get recent dream history records
    pub fn get_dream_history(&self, limit: i32) -> Result<Vec<DreamHistoryRecord>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT dreamed_at, duration_ms, memories_replayed, connections_found,
                    insights_generated, memories_strengthened, memories_compressed,
                    phase_nrem1_ms, phase_nrem3_ms, phase_rem_ms, phase_integration_ms,
                    summaries_generated, emotional_memories_processed, creative_connections_found
             FROM dream_history ORDER BY dreamed_at DESC LIMIT ?1"
        )?;
        let records = stmt.query_map(params![limit], |row| {
            let dreamed_str: String = row.get(0)?;
            let dreamed_at = DateTime::parse_from_rfc3339(&dreamed_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok(DreamHistoryRecord {
                dreamed_at,
                duration_ms: row.get(1)?,
                memories_replayed: row.get(2)?,
                connections_found: row.get(3)?,
                insights_generated: row.get(4)?,
                memories_strengthened: row.get(5)?,
                memories_compressed: row.get(6)?,
                phase_nrem1_ms: row.get(7)?,
                phase_nrem3_ms: row.get(8)?,
                phase_rem_ms: row.get(9)?,
                phase_integration_ms: row.get(10)?,
                summaries_generated: row.get(11)?,
                emotional_memories_processed: row.get(12)?,
                creative_connections_found: row.get(13)?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(records)
    }

    // ========================================================================
    // RETENTION SNAPSHOTS & METRICS
    // ========================================================================

    /// Count memories created since a given timestamp
    pub fn count_memories_since(&self, since: DateTime<Utc>) -> Result<i64> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let count: i64 = reader.query_row(
            "SELECT COUNT(*) FROM knowledge_nodes WHERE created_at >= ?1",
            params![since.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get last backup timestamp by scanning the backups directory.
    /// Parses `vestige-YYYYMMDD-HHMMSS.db` filenames.
    pub fn get_last_backup_timestamp() -> Option<DateTime<Utc>> {
        let proj_dirs = directories::ProjectDirs::from("com", "vestige", "core")?;
        let backup_dir = proj_dirs.data_dir().parent()?.join("backups");

        if !backup_dir.exists() {
            return None;
        }

        let mut latest: Option<DateTime<Utc>> = None;

        if let Ok(entries) = std::fs::read_dir(&backup_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Parse vestige-YYYYMMDD-HHMMSS.db
                if let Some(ts_part) = name_str.strip_prefix("vestige-").and_then(|s| s.strip_suffix(".db"))
                    && let Ok(naive) = chrono::NaiveDateTime::parse_from_str(ts_part, "%Y%m%d-%H%M%S") {
                        let dt = naive.and_utc();
                        if latest.as_ref().is_none_or(|l| dt > *l) {
                            latest = Some(dt);
                        }
                    }
            }
        }

        latest
    }

    /// Get average retention across all memories
    pub fn get_avg_retention(&self) -> Result<f64> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let avg: f64 = reader.query_row(
            "SELECT COALESCE(AVG(retention_strength), 0.0) FROM knowledge_nodes",
            [],
            |row| row.get(0),
        )?;
        Ok(avg)
    }

    /// Get retention distribution in buckets (0-20%, 20-40%, 40-60%, 60-80%, 80-100%)
    pub fn get_retention_distribution(&self) -> Result<Vec<(String, i64)>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT
                CASE
                    WHEN retention_strength < 0.2 THEN '0-20%'
                    WHEN retention_strength < 0.4 THEN '20-40%'
                    WHEN retention_strength < 0.6 THEN '40-60%'
                    WHEN retention_strength < 0.8 THEN '60-80%'
                    ELSE '80-100%'
                END as bucket,
                COUNT(*) as count
            FROM knowledge_nodes
            GROUP BY bucket
            ORDER BY bucket"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get retention trend (improving/declining/stable) from retention snapshots
    pub fn get_retention_trend(&self) -> Result<String> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;

        let snapshots: Vec<f64> = reader.prepare(
            "SELECT avg_retention FROM retention_snapshots ORDER BY snapshot_at DESC LIMIT 5"
        )?.query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        if snapshots.len() < 3 {
            return Ok("insufficient_data".to_string());
        }

        // Compare recent vs older snapshots
        let recent_avg = snapshots.iter().take(2).sum::<f64>() / 2.0;
        let older_avg = snapshots.iter().skip(2).sum::<f64>() / (snapshots.len() - 2) as f64;

        let diff = recent_avg - older_avg;
        Ok(if diff > 0.02 {
            "improving".to_string()
        } else if diff < -0.02 {
            "declining".to_string()
        } else {
            "stable".to_string()
        })
    }

    /// Save a retention snapshot (called during consolidation)
    pub fn save_retention_snapshot(&self, avg_retention: f64, total: i64, below_target: i64, gc_triggered: bool) -> Result<()> {
        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT INTO retention_snapshots (snapshot_at, avg_retention, total_memories, memories_below_target, gc_triggered)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Utc::now().to_rfc3339(), avg_retention, total, below_target, gc_triggered],
        )?;
        Ok(())
    }

    /// Count memories below a given retention threshold
    pub fn count_memories_below_retention(&self, threshold: f64) -> Result<i64> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let count: i64 = reader.query_row(
            "SELECT COUNT(*) FROM knowledge_nodes WHERE retention_strength < ?1",
            params![threshold],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

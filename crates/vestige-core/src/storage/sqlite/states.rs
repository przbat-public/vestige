//! Memory state machine repository for [`super::Storage`].
//!
//! Persists the lifecycle of every memory in two tables:
//!
//! - `memory_states` — current state per memory (`active`,
//!   `passive`, `suppressed`, ...) plus access bookkeeping
//!   (`last_access`, `access_count`, `state_entered_at`,
//!   `suppression_until`, `suppressed_by`).
//! - `state_transitions` — append-only audit trail of every state
//!   change, used by the dashboard changelog and by metacognitive
//!   replay.
//!
//! Public API maps onto a small CRUD surface plus one mutating helper
//! ([`Storage::record_memory_access`]) that doubles as "create-if-missing".
//! The transition row is written inside the same writer lock as the
//! `UPDATE` so the audit log can never lag the state.
//!
//! # Which state a memory is in
//!
//! The four retention bands (`active`/`dormant`/`silent`/`unavailable`) are a
//! pure function of the reconciled `retention_strength` value
//! (`crate::fsrs::memory_state_for`), never of the fact that some writer just
//! touched the row. Before this rule existed, `record_memory_access` stamped
//! `state = 'active'` on every search hit, so a memory that had already decayed
//! to Silent looked Active until the next consolidation overwrote the retention
//! column, and stayed that way for as long as the pipeline kept returning it.
//! Any state outside the four bands (`suppressed`, manual pins) is an explicit
//! override owned by its caller and is never rewritten from a retention band.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

use crate::fsrs::memory_state_for;
use crate::neuroscience::memory_states::MemoryState;

use super::{MemoryStateRecord, Result, StateTransitionRecord, Storage, StorageError};

/// `memory_states.state` values that mirror the reconciled retention value.
const RETENTION_BANDS: [&str; 4] = ["active", "dormant", "silent", "unavailable"];

/// Reason recorded when a lifecycle row crosses a retention band on its own.
const RETENTION_DECAY_REASON: &str = "retention_decay";

impl Storage {
    /// Save or update memory state
    pub fn save_memory_state(&self, state: &MemoryStateRecord) -> Result<()> {
        let suppressed_json =
            serde_json::to_string(&state.suppressed_by).unwrap_or_else(|_| "[]".to_string());

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT OR REPLACE INTO memory_states (
                memory_id, state, last_access, access_count, state_entered_at,
                suppression_until, suppressed_by
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                state.memory_id,
                state.state,
                state.last_access.to_rfc3339(),
                state.access_count,
                state.state_entered_at.to_rfc3339(),
                state.suppression_until.map(|dt| dt.to_rfc3339()),
                suppressed_json,
            ],
        )?;
        Ok(())
    }

    /// Get memory state
    pub fn get_memory_state(&self, memory_id: &str) -> Result<Option<MemoryStateRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare("SELECT * FROM memory_states WHERE memory_id = ?1")?;

        stmt.query_row(params![memory_id], Self::row_to_memory_state)
            .optional()
            .map_err(StorageError::from)
    }

    /// Get memories by state
    pub fn get_memories_by_state(&self, state: &str) -> Result<Vec<String>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare("SELECT memory_id FROM memory_states WHERE state = ?1")?;

        let rows = stmt.query_map(params![state], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Update memory state
    pub fn update_memory_state(
        &self,
        memory_id: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        let now = Utc::now();

        // Get old state for transition record
        if let Some(old_record) = self.get_memory_state(memory_id)? {
            // Record state transition
            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "INSERT INTO state_transitions (memory_id, from_state, to_state, reason_type, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![memory_id, old_record.state, new_state, reason, now.to_rfc3339()],
            )?;
        }

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer.execute(
            "UPDATE memory_states SET state = ?1, state_entered_at = ?2 WHERE memory_id = ?3",
            params![new_state, now.to_rfc3339(), memory_id],
        )?;
        Ok(rows > 0)
    }

    /// Record access to memory (updates state)
    ///
    /// The persisted lifecycle state is *derived* from the memory's reconciled
    /// retention value, not from the access itself. Two reasons:
    ///
    /// 1. The search pipeline calls this for every returned hit, immediately
    ///    after `strengthen_batch_on_access` added its flat `+0.02` to the
    ///    cached column. Writing `'active'` here pinned a memory that had
    ///    already decayed to Dormant/Silent back to Active until the next
    ///    consolidation pass, so `state` described write ordering rather than
    ///    retrievability.
    /// 2. The value the state is derived from must be the same one a reader
    ///    filters and ranks by. Re-stamping [`Storage::reconciled_retention`]
    ///    here — this is the last writer on the search path — collapses the
    ///    additive event bump back onto the canonical value at the instant of
    ///    the access, so a just-retrieved memory is Active because it *is*
    ///    retrievable right now (`R ≈ 1`), not because it accumulated hits.
    ///
    /// The access itself is still recorded unconditionally (`last_access`,
    /// `access_count`), and the create-if-missing path keeps working for ids
    /// that have no node row yet. Unlike the decay refresh
    /// ([`Storage::refresh_memory_state_from_retention`]), this path does not
    /// preserve an explicit non-band state: a successful access is itself
    /// evidence that the memory is retrievable.
    pub fn record_memory_access(&self, memory_id: &str) -> Result<()> {
        let now = Utc::now();

        let reconciled = self.reconciled_retention(memory_id)?;
        let state = reconciled.map_or_else(
            || MemoryState::default().as_str(),
            |value| memory_state_for(value).as_str(),
        );

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;

        if let Some(value) = reconciled {
            writer.execute(
                "UPDATE knowledge_nodes SET retention_strength = ?1 WHERE id = ?2",
                params![value, memory_id],
            )?;
        }

        // Check if state exists (writer can read too)
        let exists: bool = writer.query_row(
            "SELECT EXISTS(SELECT 1 FROM memory_states WHERE memory_id = ?1)",
            params![memory_id],
            |row| row.get(0),
        )?;

        if exists {
            writer.execute(
                "UPDATE memory_states SET
                    last_access = ?1,
                    access_count = access_count + 1,
                    state = ?2,
                    state_entered_at = CASE WHEN state != ?2 THEN ?1 ELSE state_entered_at END
                 WHERE memory_id = ?3",
                params![now.to_rfc3339(), state, memory_id],
            )?;
        } else {
            writer.execute(
                "INSERT INTO memory_states (memory_id, state, last_access, access_count, state_entered_at)
                 VALUES (?1, ?2, ?3, 1, ?3)",
                params![memory_id, state, now.to_rfc3339()],
            )?;
        }
        Ok(())
    }

    /// Re-derive one memory's persisted lifecycle state from its current
    /// `retention_strength` and record the transition when it moved.
    ///
    /// Called by consolidation right after `apply_decay` overwrote the column
    /// with the reconciled value, and by nothing else: it is what makes the
    /// stored lifecycle follow the value as it decays with no access at all,
    /// instead of keeping whatever the last access left behind. A memory with no
    /// lifecycle row is skipped (nothing ever created one), and a row holding an
    /// explicit state such as `suppressed` is left untouched.
    ///
    /// Returns `true` when the stored state changed.
    pub fn refresh_memory_state_from_retention(&self, memory_id: &str) -> Result<bool> {
        let Some(retention) = self.retention_strength(memory_id)? else {
            return Ok(false);
        };
        let Some(record) = self.get_memory_state(memory_id)? else {
            return Ok(false);
        };

        let derived = memory_state_for(retention);
        if record.state == derived.as_str() || !RETENTION_BANDS.contains(&record.state.as_str()) {
            return Ok(false);
        }

        self.update_memory_state(memory_id, derived.as_str(), RETENTION_DECAY_REASON)
    }

    /// The cached `retention_strength` of one memory, or `None` when the id has
    /// no node.
    fn retention_strength(&self, memory_id: &str) -> Result<Option<f64>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        reader
            .query_row(
                "SELECT retention_strength FROM knowledge_nodes WHERE id = ?1",
                params![memory_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub(super) fn row_to_memory_state(row: &rusqlite::Row) -> rusqlite::Result<MemoryStateRecord> {
        let suppressed_json: String = row.get("suppressed_by")?;
        let suppressed_by: Vec<String> = serde_json::from_str(&suppressed_json).unwrap_or_default();

        let parse_opt_dt = |s: Option<String>| -> Option<DateTime<Utc>> {
            s.and_then(|v| {
                DateTime::parse_from_rfc3339(&v)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            })
        };

        Ok(MemoryStateRecord {
            memory_id: row.get("memory_id")?,
            state: row.get("state")?,
            last_access: DateTime::parse_from_rfc3339(&row.get::<_, String>("last_access")?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            access_count: row.get("access_count").unwrap_or(1),
            state_entered_at: DateTime::parse_from_rfc3339(
                &row.get::<_, String>("state_entered_at")?,
            )
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
            suppression_until: parse_opt_dt(row.get("suppression_until").ok().flatten()),
            suppressed_by,
        })
    }

    /// Get state transitions for a memory
    pub fn get_state_transitions(
        &self,
        memory_id: &str,
        limit: i32,
    ) -> Result<Vec<StateTransitionRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM state_transitions WHERE memory_id = ?1 ORDER BY timestamp DESC LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![memory_id, limit], |row| {
            Ok(StateTransitionRecord {
                id: row.get("id")?,
                memory_id: row.get("memory_id")?,
                from_state: row.get("from_state")?,
                to_state: row.get("to_state")?,
                reason_type: row.get("reason_type")?,
                reason_data: row.get("reason_data").ok().flatten(),
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>("timestamp")?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get recent state transitions across all memories (system-wide changelog)
    pub fn get_recent_state_transitions(&self, limit: i32) -> Result<Vec<StateTransitionRecord>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt =
            reader.prepare("SELECT * FROM state_transitions ORDER BY timestamp DESC LIMIT ?1")?;

        let rows = stmt.query_map(params![limit], |row| {
            Ok(StateTransitionRecord {
                id: row.get("id")?,
                memory_id: row.get("memory_id")?,
                from_state: row.get("from_state")?,
                to_state: row.get("to_state")?,
                reason_type: row.get("reason_type")?,
                reason_data: row.get("reason_data").ok().flatten(),
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>("timestamp")?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

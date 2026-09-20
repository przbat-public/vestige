//! Persistence-layer record structs and their row mappers.
//!
//! The records live here (instead of in the domain modules they belong to)
//! for two reasons:
//!
//! 1. They are storage-internal — they mirror SQLite table schemas one-to-one
//!    rather than the public domain types in `crate::memory::*`. Keeping
//!    them in a single file makes the schema↔record correspondence easy to
//!    audit when a migration changes columns.
//!
//! 2. Row mappers (`row_to_intention`, `row_to_insight`, `row_to_connection`)
//!    are `pub(super)` so all the per-domain modules (`intentions.rs`,
//!    `insights.rs`, `connections.rs`) can share them without re-declaring
//!    the column-parsing logic. Moving the mappers next to the structs they
//!    populate keeps that contract obvious.
//!
//! The `MemoryStateRecord` row mapper (`row_to_memory_state`) lives in
//! `states.rs` because it's only used inside the state machine module.

use chrono::{DateTime, Utc};

use super::Storage;

/// Intention data for persistence (matches the intentions table schema)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IntentionRecord {
    pub id: String,
    pub content: String,
    pub trigger_type: String,
    pub trigger_data: String, // JSON
    pub priority: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub fulfilled_at: Option<DateTime<Utc>>,
    pub reminder_count: i32,
    pub last_reminded_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub related_memories: Vec<String>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub source_type: String,
    pub source_data: Option<String>,
}

/// Insight data for persistence (matches the insights table schema)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InsightRecord {
    pub id: String,
    pub insight: String,
    pub source_memories: Vec<String>,
    pub confidence: f64,
    pub novelty_score: f64,
    pub insight_type: String,
    pub generated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub feedback: Option<String>,
    pub applied_count: i32,
}

impl Default for InsightRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            insight: String::new(),
            source_memories: Vec::new(),
            confidence: 0.0,
            novelty_score: 0.0,
            insight_type: String::new(),
            generated_at: Utc::now(),
            tags: Vec::new(),
            feedback: None,
            applied_count: 0,
        }
    }
}

/// Memory connection for activation network
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionRecord {
    pub source_id: String,
    pub target_id: String,
    pub strength: f64,
    pub link_type: String,
    pub created_at: DateTime<Utc>,
    pub last_activated: DateTime<Utc>,
    pub activation_count: i32,
}

/// What a [`MemoryRevision`] records about a memory's life.
///
/// The set is closed: the `kind` column carries a `CHECK` constraint, so a new
/// variant needs a migration and not just an enum arm — the history table is
/// the audit trail, and a value SQLite never validated would make it
/// unqueryable in the one place it matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RevisionKind {
    /// The memory was first written down.
    Create,
    /// Its text was replaced; `old_content` holds the previous wording.
    Edit,
    /// A newer memory took its place.
    Supersede,
    /// It was marked no-longer-valid without being replaced.
    Invalidate,
    /// It was deferred out of retrieval (unresolved reference, failed gate).
    Quarantine,
}

impl RevisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Edit => "edit",
            Self::Supersede => "supersede",
            Self::Invalidate => "invalidate",
            Self::Quarantine => "quarantine",
        }
    }

    /// Parse a stored `kind`. An unrecognised value yields `None` rather than
    /// an error so one row written by a newer build cannot make the whole
    /// history of a memory unreadable.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "create" => Some(Self::Create),
            "edit" => Some(Self::Edit),
            "supersede" => Some(Self::Supersede),
            "invalidate" => Some(Self::Invalidate),
            "quarantine" => Some(Self::Quarantine),
            _ => None,
        }
    }
}

impl std::fmt::Display for RevisionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// One row of a memory's content history — append-only.
///
/// This is what makes "how did the picture change over time" answerable: a
/// memory's current text says nothing about what it used to say, and the
/// life-cycle tables (`state_transitions`, `consolidation_history`) record
/// scheduling, never content.
///
/// `old_content` / `new_content` are free-form TEXT and their meaning depends
/// on `kind`: for `invalidate` they hold the previous and new `valid_until`
/// values, because that is the state the operation actually changed. The
/// alternative — a JSON payload column — would make the common case (an edit's
/// before/after text) unreadable in SQL.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemoryRevision {
    pub id: i64,
    pub node_id: String,
    pub recorded_at: DateTime<Utc>,
    pub kind: RevisionKind,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub reason: Option<String>,
    pub actor: Option<String>,
}

/// One row of `code_refs`: a memory's reference to code, plus the last verdict
/// a resolver reached about it.
///
/// The anchor is kept as a nested [`CodeAnchor`] rather than as nine loose
/// fields because the resolver takes exactly that value; flattening it here
/// would mean rebuilding it at every call site, which is how a field gets
/// dropped on the way to a check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeRef {
    pub id: i64,
    pub node_id: String,
    pub anchor: crate::code_refs::CodeAnchor,
    /// Last time the anchor was checked. `None` means it never was.
    pub resolved_at: Option<DateTime<Utc>>,
    pub verdict: crate::code_refs::AnchorVerdict,
}

/// Memory state record
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryStateRecord {
    pub memory_id: String,
    pub state: String, // 'active', 'dormant', 'silent', 'unavailable'
    pub last_access: DateTime<Utc>,
    pub access_count: i32,
    pub state_entered_at: DateTime<Utc>,
    pub suppression_until: Option<DateTime<Utc>>,
    pub suppressed_by: Vec<String>,
}

/// State transition record for audit trail
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StateTransitionRecord {
    pub id: i64,
    pub memory_id: String,
    pub from_state: String,
    pub to_state: String,
    pub reason_type: String,
    pub reason_data: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Consolidation history record
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsolidationHistoryRecord {
    pub id: i64,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub memories_replayed: i32,
    pub connections_found: i32,
    pub connections_strengthened: i32,
    pub connections_pruned: i32,
    pub insights_generated: i32,
}

/// Dream history record — persists dream metadata for automation triggers
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DreamHistoryRecord {
    pub dreamed_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub memories_replayed: i32,
    pub connections_found: i32,
    pub insights_generated: i32,
    pub memories_strengthened: i32,
    pub memories_compressed: i32,
    // v2.0: 4-Phase dream cycle metrics
    pub phase_nrem1_ms: Option<i64>,
    pub phase_nrem3_ms: Option<i64>,
    pub phase_rem_ms: Option<i64>,
    pub phase_integration_ms: Option<i64>,
    pub summaries_generated: Option<i32>,
    pub emotional_memories_processed: Option<i32>,
    pub creative_connections_found: Option<i32>,
}

// ============================================================================
// Row mappers (associated functions on Storage so visibility stays internal).
// ============================================================================

impl Storage {
    pub(super) fn row_to_intention(row: &rusqlite::Row) -> rusqlite::Result<IntentionRecord> {
        let tags_json: String = row.get("tags")?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_else(|e| {
            tracing::warn!(raw = %tags_json, error = %e, "Corrupt tags JSON in intention row");
            vec![]
        });
        let related_json: String = row.get("related_memories")?;
        let related: Vec<String> = serde_json::from_str(&related_json).unwrap_or_default();

        let parse_opt_dt = |s: Option<String>| -> Option<DateTime<Utc>> {
            s.and_then(|v| {
                DateTime::parse_from_rfc3339(&v)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            })
        };

        Ok(IntentionRecord {
            id: row.get("id")?,
            content: row.get("content")?,
            trigger_type: row.get("trigger_type")?,
            trigger_data: row.get("trigger_data")?,
            priority: row.get("priority")?,
            status: row.get("status")?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at")?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            deadline: parse_opt_dt(row.get("deadline").ok().flatten()),
            fulfilled_at: parse_opt_dt(row.get("fulfilled_at").ok().flatten()),
            reminder_count: row.get("reminder_count").unwrap_or(0),
            last_reminded_at: parse_opt_dt(row.get("last_reminded_at").ok().flatten()),
            notes: row.get("notes").ok().flatten(),
            tags,
            related_memories: related,
            snoozed_until: parse_opt_dt(row.get("snoozed_until").ok().flatten()),
            source_type: row.get("source_type").unwrap_or_else(|_| "api".to_string()),
            source_data: row.get("source_data").ok().flatten(),
        })
    }

    pub(super) fn row_to_insight(row: &rusqlite::Row) -> rusqlite::Result<InsightRecord> {
        let source_json: String = row.get("source_memories")?;
        let source_memories: Vec<String> =
            serde_json::from_str(&source_json).unwrap_or_else(|e| {
                tracing::warn!(raw = %source_json, error = %e, "Corrupt source_memories JSON in insight row");
                vec![]
            });
        let tags_json: String = row.get("tags")?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_else(|e| {
            tracing::warn!(raw = %tags_json, error = %e, "Corrupt tags JSON in insight row");
            vec![]
        });

        Ok(InsightRecord {
            id: row.get("id")?,
            insight: row.get("insight")?,
            source_memories,
            confidence: row.get("confidence")?,
            novelty_score: row.get("novelty_score")?,
            insight_type: row.get("insight_type")?,
            generated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("generated_at")?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            tags,
            feedback: row.get("feedback").ok().flatten(),
            applied_count: row.get("applied_count").unwrap_or(0),
        })
    }

    pub(super) fn row_to_connection(row: &rusqlite::Row) -> rusqlite::Result<ConnectionRecord> {
        Ok(ConnectionRecord {
            source_id: row.get("source_id")?,
            target_id: row.get("target_id")?,
            strength: row.get("strength")?,
            link_type: row.get("link_type")?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at")?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            last_activated: DateTime::parse_from_rfc3339(&row.get::<_, String>("last_activated")?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            activation_count: row.get("activation_count").unwrap_or(0),
        })
    }

    pub(super) fn row_to_memory_revision(row: &rusqlite::Row) -> rusqlite::Result<MemoryRevision> {
        let recorded_at: String = row.get("recorded_at")?;
        let kind: String = row.get("kind")?;

        Ok(MemoryRevision {
            id: row.get("id")?,
            node_id: row.get("node_id")?,
            // A revision with an unparseable timestamp cannot be placed on a
            // timeline, which is its entire purpose — surface it instead of
            // silently stamping it with the current time.
            recorded_at: Storage::parse_timestamp(&recorded_at, "recorded_at")?,
            kind: RevisionKind::parse(&kind).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("unknown memory_revisions.kind '{kind}'"),
                    )),
                )
            })?,
            old_content: row.get("old_content")?,
            new_content: row.get("new_content")?,
            reason: row.get("reason")?,
            actor: row.get("actor")?,
        })
    }
}

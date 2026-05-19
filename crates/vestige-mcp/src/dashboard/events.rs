//! Real-time event system for the Vestige dashboard.
//!
//! Events are emitted by the CognitiveEngine and broadcast to all
//! connected WebSocket clients via a tokio broadcast channel.

use chrono::{DateTime, Utc};
use serde::Serialize;
use ts_rs::TS;

/// Every cognitive operation emits one of these events.
///
/// `serde(tag = "type", content = "data")` produces a discriminated
/// union on the wire (and in the generated TypeScript), so the
/// dashboard's WebSocket handler can `switch (ev.type)` exhaustively.
/// New variants here automatically expand the TS union after
/// `cargo test -p vestige-mcp --lib dashboard::events`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type", content = "data", rename_all_fields = "camelCase")]
#[ts(export, export_to = "VestigeEvent.ts", rename_all_fields = "camelCase")]
pub enum VestigeEvent {
    // -- Memory lifecycle --
    MemoryCreated {
        id: String,
        content_preview: String,
        node_type: String,
        tags: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    MemoryUpdated {
        id: String,
        content_preview: String,
        field: String,
        timestamp: DateTime<Utc>,
    },
    MemoryDeleted {
        id: String,
        timestamp: DateTime<Utc>,
    },
    MemoryPromoted {
        id: String,
        new_retention: f64,
        timestamp: DateTime<Utc>,
    },
    MemoryDemoted {
        id: String,
        new_retention: f64,
        timestamp: DateTime<Utc>,
    },

    // -- Search --
    SearchPerformed {
        query: String,
        result_count: usize,
        result_ids: Vec<String>,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },

    // -- Dream --
    DreamStarted {
        memory_count: usize,
        timestamp: DateTime<Utc>,
    },
    DreamProgress {
        phase: String,
        memory_id: Option<String>,
        progress_pct: f64,
        timestamp: DateTime<Utc>,
    },
    DreamCompleted {
        memories_replayed: usize,
        connections_found: usize,
        insights_generated: usize,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },

    // -- Consolidation --
    ConsolidationStarted {
        timestamp: DateTime<Utc>,
    },
    ConsolidationCompleted {
        nodes_processed: usize,
        decay_applied: usize,
        embeddings_generated: usize,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },

    // -- FSRS --
    RetentionDecayed {
        id: String,
        old_retention: f64,
        new_retention: f64,
        timestamp: DateTime<Utc>,
    },

    // -- Connections --
    ConnectionDiscovered {
        source_id: String,
        target_id: String,
        connection_type: String,
        weight: f64,
        timestamp: DateTime<Utc>,
    },

    // -- Spreading activation --
    ActivationSpread {
        source_id: String,
        activated_ids: Vec<String>,
        timestamp: DateTime<Utc>,
    },

    // -- Importance --
    ImportanceScored {
        content_preview: String,
        composite_score: f64,
        novelty: f64,
        arousal: f64,
        reward: f64,
        attention: f64,
        timestamp: DateTime<Utc>,
    },

    // -- System --
    Heartbeat {
        uptime_secs: u64,
        memory_count: usize,
        avg_retention: f64,
        timestamp: DateTime<Utc>,
    },
}

impl VestigeEvent {
    /// Serialize to JSON string for WebSocket transmission.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

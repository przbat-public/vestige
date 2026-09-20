//! Storage Module
//!
//! SQLite-based storage layer with:
//! - FTS5 full-text search with query sanitization
//! - Embedded vector storage
//! - FSRS-6 state management
//! - Temporal memory support

pub(crate) mod migrations;
mod sqlite;

pub use sqlite::{
    AnchorCounts, CodeRef, ConnectionRecord, ConsolidationHistoryRecord, ContainmentCounts,
    ContradictionReport, DEFAULT_HYBRID_KEYWORD_WEIGHT, DEFAULT_HYBRID_SEMANTIC_WEIGHT,
    DreamHistoryRecord, GateOutcomeCounters, InsightRecord, IntentionRecord, MemoryQualityReport,
    MemoryRevision, ProcessGateCounters, QUALITY_WINDOW_BASIS, QualityRates, RETRACTION_KINDS,
    REVISION_CONTENT_CHAR_LIMIT, Result, RevisionKind, RuleCount, SmartIngestResult,
    SnapshotRestoreReport, StateTransitionRecord, Storage, StorageError, UseCounts,
    default_hybrid_weights,
};

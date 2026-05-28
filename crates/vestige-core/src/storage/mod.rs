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
    ConnectionRecord, ConsolidationHistoryRecord, DreamHistoryRecord,
    DEFAULT_HYBRID_KEYWORD_WEIGHT, DEFAULT_HYBRID_SEMANTIC_WEIGHT, InsightRecord, IntentionRecord,
    Result, SmartIngestResult, StateTransitionRecord, Storage, StorageError,
    default_hybrid_weights,
};

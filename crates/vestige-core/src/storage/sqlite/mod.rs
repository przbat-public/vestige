//! SQLite Storage Implementation
//!
//! Core storage layer with integrated embeddings and vector search.
//!
//! ## File layout
//!
//! Historically this was a single 4 389-LoC monolith. It is being progressively
//! split into per-domain modules so each file has one reason to change. Public
//! API is preserved: every method lives in `impl Storage`, all submodules
//! re-extend the same struct with `impl super::Storage { ... }`, and downstream
//! crates keep importing `vestige_core::Storage` exactly as before.
//!
//! ### Foundation
//!
//! - `error`: `StorageError`, `Result`, and the `SmartIngestResult` payload.
//! - `tags`: tag canonicalization (`normalize_tags`) shared by every writer.
//! - `init`: `Storage` struct, `Storage::new`, connection PRAGMA setup, and
//!   the dimension-migration startup path that primes the vector index.
//! - `helpers`: shared `pub(super)` plumbing — reader pool, RFC3339 parsing,
//!   `row_to_node`, embedding generation, and the access-log writer.
//!
//! ### Per-domain CRUD
//!
//! - `nodes`: CRUD over `knowledge_nodes` (ingest, get, update tags/content,
//!   delete, paginated lists, FTS search, type+tag filter).
//! - `intentions`: CRUD over the `intentions` table (proactive triggers,
//!   deadlines, snoozing).
//! - `insights`: CRUD over the `insights` table (emergent observations
//!   from dream cycles and meta-cognition).
//! - `connections`: weighted graph edges (`memory_connections`) used by
//!   spreading activation, dream replay, and related-memories queries.
//! - `gdpr`: Article 17 erasure paths (single id and tag-based bulk wipe).
//! - `review`: FSRS review and reinforcement — `mark_reviewed`,
//!   `strengthen_on_access`, `promote_memory`/`demote_memory`,
//!   `get_review_queue`, `preview_review`.
//! - `embeddings`: explicit embedding access — readiness probe,
//!   `init_embeddings`, vector accessors, batch `generate_embeddings`.
//! - `temporal`: bitemporal queries (`query_at_time`, `query_time_range`)
//!   and the batched FSRS-6 `apply_decay` pass.
//! - `search`: keyword (FTS5), semantic (HNSW), and hybrid (RRF + three-
//!   signal rerank) retrieval, plus the `recall` dispatcher.
//! - `states`: memory state machine (`memory_states` row + the append-only
//!   `state_transitions` audit log).
//! - `history`: append-only time-series — consolidation history, dream
//!   history, retention snapshots, and the filesystem-backed
//!   `get_last_backup_timestamp` helper.
//! - `maintenance`: autonomic operations — WAL checkpoint, `VACUUM INTO`
//!   backup, retention-floor GC, frequency-based auto-promotion, and the
//!   waking-tag lifecycle that primes dream replay.
//! - `graph`: degree-rank hub lookup plus bounded BFS subgraph fetch for
//!   the dashboard graph view.
//! - `consolidation`: the 17-step `run_consolidation` pipeline that
//!   composes decay, dream cycle, compression, state transitions, ACT-R
//!   activation, w20 optimization, and retention-target GC into one pass.
//! - `smart_ingest` (feature-gated `embeddings+vector-search`):
//!   prediction-error-gated insertion that decides Create / Update /
//!   Supersede / Merge for incoming content and writes connection edges so
//!   memory evolution is observable in the graph view.
//! - `stats`: top-level memory statistics for the dashboard /
//!   `system_status` MCP tool.
//! - `records`: persistence-layer record structs (one per SQLite table)
//!   and the `pub(super)` row mappers shared by the per-domain modules.

mod connections;
mod consolidation;
mod embeddings;
mod error;
mod gdpr;
mod graph;
mod helpers;
mod history;
mod init;
mod insights;
mod intentions;
mod maintenance;
mod nodes;
mod records;
mod review;
mod search;
#[cfg(all(feature = "embeddings", feature = "vector-search"))]
mod smart_ingest;
mod states;
mod stats;
mod tags;
mod temporal;

#[cfg(test)]
mod tests;

pub use error::{Result, SmartIngestResult, StorageError};
pub use init::Storage;
pub use records::{
    ConnectionRecord, ConsolidationHistoryRecord, DreamHistoryRecord, InsightRecord,
    IntentionRecord, MemoryStateRecord, StateTransitionRecord,
};
pub(crate) use tags::normalize_tags;

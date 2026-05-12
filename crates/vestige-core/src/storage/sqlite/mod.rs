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
//! Current submodules:
//!
//! - [`nodes`]: CRUD over `knowledge_nodes` (ingest, get, update tags/content,
//!   delete, paginated lists, FTS search, type+tag filter).
//! - [`intentions`]: CRUD over the `intentions` table (proactive triggers,
//!   deadlines, snoozing).
//! - [`insights`]: CRUD over the `insights` table (emergent observations
//!   from dream cycles and meta-cognition).
//! - [`connections`]: weighted graph edges (`memory_connections`) used by
//!   spreading activation, dream replay, and related-memories queries.
//! - [`gdpr`]: Article 17 erasure paths (single id and tag-based bulk wipe).
//! - [`review`]: FSRS review and reinforcement — `mark_reviewed`,
//!   `strengthen_on_access`, `promote_memory`/`demote_memory`,
//!   `get_review_queue`, `preview_review`.
//! - [`embeddings`]: explicit embedding access — readiness probe,
//!   `init_embeddings`, vector accessors, batch `generate_embeddings`.
//! - [`temporal`]: bitemporal queries (`query_at_time`, `query_time_range`)
//!   and the batched FSRS-6 `apply_decay` pass.
//! - [`search`]: keyword (FTS5), semantic (HNSW), and hybrid (RRF + three-
//!   signal rerank) retrieval, plus the `recall` dispatcher.
//! - [`states`]: memory state machine (`memory_states` row + the append-only
//!   `state_transitions` audit log).
//! - [`history`]: append-only time-series — consolidation history, dream
//!   history, retention snapshots, and the filesystem-backed
//!   `get_last_backup_timestamp` helper.
//! - [`maintenance`]: autonomic operations — WAL checkpoint, `VACUUM INTO`
//!   backup, retention-floor GC, frequency-based auto-promotion, and the
//!   waking-tag lifecycle that primes dream replay.
//! - [`graph`]: degree-rank hub lookup plus bounded BFS subgraph fetch for
//!   the dashboard graph view.
//! - [`consolidation`]: the 17-step `run_consolidation` pipeline that
//!   composes decay, dream cycle, compression, state transitions, ACT-R
//!   activation, w20 optimization, and retention-target GC into one pass.
//! - [`smart_ingest`] (feature-gated `embeddings+vector-search`):
//!   prediction-error-gated insertion that decides Create / Update /
//!   Supersede / Merge for incoming content and writes connection edges so
//!   memory evolution is observable in the graph view.
//! - [`stats`]: top-level memory statistics for the dashboard /
//!   `system_status` MCP tool.
//! - [`records`]: persistence-layer record structs (one per SQLite table)
//!   and the `pub(super)` row mappers shared by the per-domain modules.
//!
//! Everything not yet extracted still lives directly in this file.

mod connections;
mod consolidation;
mod embeddings;
mod gdpr;
mod graph;
mod history;
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
mod temporal;

pub use records::{
    ConnectionRecord, ConsolidationHistoryRecord, DreamHistoryRecord, InsightRecord,
    IntentionRecord, MemoryStateRecord, StateTransitionRecord,
};

use crate::fsrs::FSRSScheduler;
use crate::memory::{ConsolidationResult, KnowledgeNode};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
#[cfg(feature = "embeddings")]
use lru::LruCache;
use rusqlite::{Connection, params};
#[cfg(feature = "embeddings")]
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(feature = "embeddings")]
use crate::embeddings::{EMBEDDING_DIMENSIONS, Embedding, EmbeddingService, matryoshka_truncate};

#[cfg(feature = "vector-search")]
use crate::search::VectorIndex;

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Storage error type
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// Node not found
    #[error("Node not found: {0}")]
    NotFound(String),
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Invalid timestamp
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),
    /// Initialization error
    #[error("Initialization error: {0}")]
    Init(String),
}

/// Storage result type
pub type Result<T> = std::result::Result<T, StorageError>;

/// Result of smart ingest with prediction error gating
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartIngestResult {
    /// Decision made: "create", "update", "supersede", "merge", "reinforce", etc.
    pub decision: String,
    /// The resulting node (new or updated)
    pub node: KnowledgeNode,
    /// ID of superseded memory (if any)
    pub superseded_id: Option<String>,
    /// Similarity to closest existing memory (0.0 - 1.0)
    pub similarity: Option<f32>,
    /// Prediction error (1.0 - similarity)
    pub prediction_error: Option<f32>,
    /// Human-readable explanation of the decision
    pub reason: String,
}

// ============================================================================
// ENTITY NORMALIZATION (Cognee pattern)
// ============================================================================

/// Canonicalize tags to prevent fragmentation across surface forms.
/// "Bug Fix", "bug-fix", "bugfix", "BUG_FIX" all → "bug-fix".
/// Deduplicates after normalization and preserves order.
pub(super) fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(tags.len());
    for tag in tags {
        let normalized = tag.trim().to_lowercase().replace([' ', '_'], "-");
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            result.push(normalized);
        }
    }
    result
}

// ============================================================================
// STORAGE
// ============================================================================

/// Main storage struct with integrated embedding and vector search
///
/// Uses separate reader/writer connections for interior mutability.
/// All methods take `&self` (not `&mut self`), making Storage `Send + Sync`
/// so the MCP layer can use `Arc<Storage>` instead of `Arc<Mutex<Storage>>`.
///
/// ## Lock ordering (must be followed to prevent deadlocks)
///
/// When acquiring multiple locks in the same scope, always lock in this order:
///  1. `writer`
///  2. `reader` / `reader_secondary`
///  3. `scheduler`
///  4. `vector_index`
///  5. `query_cache`
///
/// Most methods lock only ONE of these. The few that need two (e.g.
/// `mark_reviewed` locks `scheduler` then `writer`) follow this order.
///
/// ## Reader pool
///
/// Two reader connections (`reader` + `reader_secondary`) are available for
/// concurrent read operations on SQLite WAL. Hot paths use `acquire_reader()`
/// which tries the secondary first to reduce contention.
pub struct Storage {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
    reader_secondary: Mutex<Connection>,
    scheduler: Mutex<FSRSScheduler>,
    #[cfg(feature = "embeddings")]
    embedding_service: EmbeddingService,
    #[cfg(feature = "vector-search")]
    vector_index: Mutex<VectorIndex>,
    /// LRU cache for query embeddings to avoid re-embedding repeated queries
    #[cfg(feature = "embeddings")]
    query_cache: Mutex<LruCache<String, Vec<f32>>>,
}

impl Storage {
    /// Apply PRAGMAs and optional encryption to a connection
    fn configure_connection(conn: &Connection) -> Result<()> {
        // Apply encryption key if SQLCipher is enabled and key is provided
        #[cfg(feature = "encryption")]
        {
            if let Ok(key) = std::env::var("VESTIGE_ENCRYPTION_KEY")
                && !key.is_empty()
            {
                conn.pragma_update(None, "key", &key)?;
            }
        }

        // Configure SQLite for performance
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA mmap_size = 268435456;
             PRAGMA journal_size_limit = 67108864;
             PRAGMA optimize = 0x10002;",
        )?;

        Ok(())
    }

    /// Create new storage instance
    pub fn new(db_path: Option<PathBuf>) -> Result<Self> {
        let path = match db_path {
            Some(p) => p,
            None => {
                let proj_dirs = ProjectDirs::from("com", "vestige", "core").ok_or_else(|| {
                    StorageError::Init("Could not determine project directories".to_string())
                })?;

                let data_dir = proj_dirs.data_dir();
                std::fs::create_dir_all(data_dir)?;
                // Restrict directory permissions to owner-only on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(0o700);
                    let _ = std::fs::set_permissions(data_dir, perms);
                }
                data_dir.join("vestige.db")
            }
        };

        // Open writer connection
        let writer_conn = Connection::open(&path)?;

        // Restrict database file permissions to owner-only on Unix
        #[cfg(unix)]
        if path.exists() {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }

        Self::configure_connection(&writer_conn)?;

        // Apply migrations on writer only
        super::migrations::apply_migrations(&writer_conn)?;

        // Open primary + secondary reader connections to same path (SQLite WAL
        // supports concurrent readers, so two connections reduce lock contention).
        let reader_conn = Connection::open(&path)?;
        Self::configure_connection(&reader_conn)?;
        let reader_secondary_conn = Connection::open(&path)?;
        Self::configure_connection(&reader_secondary_conn)?;

        #[cfg(feature = "embeddings")]
        let embedding_service = EmbeddingService::new();

        #[cfg(feature = "vector-search")]
        let vector_index = VectorIndex::new()
            .map_err(|e| StorageError::Init(format!("Failed to create vector index: {}", e)))?;

        // Initialize LRU cache for query embeddings (capacity: 100 queries)
        // SAFETY: 100 is always non-zero, this cannot fail
        #[cfg(feature = "embeddings")]
        let query_cache = Mutex::new(LruCache::new(
            NonZeroUsize::new(100).expect("100 is non-zero"),
        ));

        let storage = Self {
            writer: Mutex::new(writer_conn),
            reader: Mutex::new(reader_conn),
            reader_secondary: Mutex::new(reader_secondary_conn),
            scheduler: Mutex::new(FSRSScheduler::default()),
            #[cfg(feature = "embeddings")]
            embedding_service,
            #[cfg(feature = "vector-search")]
            vector_index: Mutex::new(vector_index),
            #[cfg(feature = "embeddings")]
            query_cache,
        };

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        storage.load_embeddings_into_index()?;

        Ok(storage)
    }

    /// Load existing embeddings into vector index, migrating dimensions if needed.
    ///
    /// Three migration paths handled on startup:
    /// - **same dims** (384 → 384): loaded directly, no work.
    /// - **larger stored** (768 → 384): Matryoshka truncate + L2-normalize.
    /// - **smaller stored** (256 → 384): must re-embed from content
    ///   (Matryoshka can't *extend* dimensions). Runs once, updates DB in-place.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    fn load_embeddings_into_index(&self) -> Result<()> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;

        let mut stmt = reader.prepare("SELECT node_id, embedding FROM node_embeddings")?;

        let embeddings: Vec<(String, Vec<u8>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        drop(stmt);
        drop(reader);

        let mut needs_reembed: Vec<String> = Vec::new();

        let mut index = self
            .vector_index
            .lock()
            .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;

        for (node_id, embedding_bytes) in &embeddings {
            if let Some(embedding) = Embedding::from_bytes(embedding_bytes) {
                if embedding.dimensions == EMBEDDING_DIMENSIONS {
                    if let Err(e) = index.add(node_id, &embedding.vector) {
                        tracing::warn!("Failed to load embedding for {}: {}", node_id, e);
                    }
                } else if embedding.dimensions > EMBEDDING_DIMENSIONS {
                    let vector = matryoshka_truncate(embedding.vector);
                    if let Err(e) = index.add(node_id, &vector) {
                        tracing::warn!("Failed to load embedding for {}: {}", node_id, e);
                    }
                } else {
                    needs_reembed.push(node_id.clone());
                }
            }
        }

        drop(index);

        if !needs_reembed.is_empty() {
            let count = needs_reembed.len();
            eprintln!(
                "[vestige] Dimension migration: re-embedding {} memories ({} → {} dims)…",
                count,
                embeddings
                    .first()
                    .and_then(|(_, b)| Embedding::from_bytes(b))
                    .map(|e| e.dimensions)
                    .unwrap_or(0),
                EMBEDDING_DIMENSIONS,
            );
            self.migrate_embeddings(&needs_reembed)?;
            eprintln!(
                "[vestige] Dimension migration complete ({} memories re-embedded)",
                count
            );
        }

        Ok(())
    }

    /// Re-embed a batch of memories and update both the DB and the vector index.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    fn migrate_embeddings(&self, node_ids: &[String]) -> Result<()> {
        let reader = self.acquire_reader()?;
        let placeholders = node_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, content FROM knowledge_nodes WHERE id IN ({})",
            placeholders
        );
        let mut stmt = reader.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            node_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let rows: Vec<(String, String)> = stmt
            .query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        drop(reader);

        let texts: Vec<&str> = rows.iter().map(|(_, c)| c.as_str()).collect();
        let new_embeddings = self
            .embedding_service
            .embed_batch(&texts)
            .map_err(|e| StorageError::Init(format!("Re-embedding failed: {}", e)))?;

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute_batch("BEGIN IMMEDIATE")?;

        let mut update_stmt = writer
            .prepare_cached("UPDATE node_embeddings SET embedding = ?1 WHERE node_id = ?2")?;

        let mut index = self
            .vector_index
            .lock()
            .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;

        for ((node_id, _), emb) in rows.iter().zip(new_embeddings.iter()) {
            let bytes = emb.to_bytes();
            update_stmt.execute(rusqlite::params![bytes, node_id])?;
            if let Err(e) = index.add(node_id, &emb.vector) {
                tracing::warn!("Failed to update vector index for {}: {}", node_id, e);
            }
        }

        drop(update_stmt);
        writer.execute_batch("COMMIT")?;
        drop(writer);

        Ok(())
    }

    /// Acquire a reader connection from the pool.
    ///
    /// Tries the secondary reader first (via `try_lock`) so that the primary
    /// reader stays available for other callers. Falls back to blocking on
    /// the primary reader if both are contended.
    pub(super) fn acquire_reader(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        if let Ok(guard) = self.reader_secondary.try_lock() {
            return Ok(guard);
        }
        self.reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))
    }

    /// Generate embedding for a node
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub(super) fn generate_embedding_for_node(&self, node_id: &str, content: &str) -> Result<()> {
        if !self.embedding_service.is_ready() {
            // Silent skip is dangerous — ingest then thinks everything succeeded but
            // the memory has no embedding and won't participate in semantic search.
            // Surface the skip so callers can detect and run regenerate_embeddings later.
            tracing::warn!(
                node_id = %node_id,
                "Skipping embedding generation: embedding service not ready. \
                 Run 'regenerate_embeddings' MCP tool after the model is loaded \
                 to backfill missing embeddings."
            );
            return Ok(());
        }

        let embedding = self
            .embedding_service
            .embed(content)
            .map_err(|e| StorageError::Init(format!("Embedding failed: {}", e)))?;

        let now = Utc::now();

        {
            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "INSERT OR REPLACE INTO node_embeddings (node_id, embedding, dimensions, model, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    node_id,
                    embedding.to_bytes(),
                    EMBEDDING_DIMENSIONS as i32,
                    "nomic-embed-text-v1.5",
                    now.to_rfc3339(),
                ],
            )?;

            writer.execute(
                "UPDATE knowledge_nodes SET has_embedding = 1, embedding_model = 'nomic-embed-text-v1.5' WHERE id = ?1",
                params![node_id],
            )?;
        }

        let mut index = self
            .vector_index
            .lock()
            .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;
        index
            .add(node_id, &embedding.vector)
            .map_err(|e| StorageError::Init(format!("Vector index add failed: {}", e)))?;

        Ok(())
    }

    /// Parse RFC3339 timestamp
    pub(super) fn parse_timestamp(
        value: &str,
        field_name: &str,
    ) -> rusqlite::Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Invalid {} timestamp '{}': {}", field_name, value, e),
                    )),
                )
            })
    }

    /// Convert a row to KnowledgeNode
    pub(super) fn row_to_node(row: &rusqlite::Row) -> rusqlite::Result<KnowledgeNode> {
        let tags_json: String = row.get("tags")?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_else(|e| {
            tracing::warn!(raw = %tags_json, error = %e, "Corrupt tags JSON in node row");
            vec![]
        });

        let created_at: String = row.get("created_at")?;
        let updated_at: String = row.get("updated_at")?;
        let last_accessed: String = row.get("last_accessed")?;
        let next_review: Option<String> = row.get("next_review")?;

        let created_at = Self::parse_timestamp(&created_at, "created_at")?;
        let updated_at = Self::parse_timestamp(&updated_at, "updated_at")?;
        let last_accessed = Self::parse_timestamp(&last_accessed, "last_accessed")?;

        let next_review = next_review.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        let valid_from: Option<String> = row.get("valid_from").ok().flatten();
        let valid_until: Option<String> = row.get("valid_until").ok().flatten();

        let valid_from = valid_from.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        let valid_until = valid_until.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        let has_embedding: Option<i32> = row.get("has_embedding").ok();
        let embedding_model: Option<String> = row.get("embedding_model").ok().flatten();

        Ok(KnowledgeNode {
            id: row.get("id")?,
            content: row.get("content")?,
            node_type: row.get("node_type")?,
            created_at,
            updated_at,
            last_accessed,
            stability: row.get("stability")?,
            difficulty: row.get("difficulty")?,
            reps: row.get("reps")?,
            lapses: row.get("lapses")?,
            storage_strength: row.get("storage_strength")?,
            retrieval_strength: row.get("retrieval_strength")?,
            retention_strength: row.get("retention_strength")?,
            sentiment_score: row.get("sentiment_score")?,
            sentiment_magnitude: row.get("sentiment_magnitude")?,
            next_review,
            source: row.get("source")?,
            tags,
            valid_from,
            valid_until,
            has_embedding: has_embedding.map(|v| v == 1),
            embedding_model,
            // v2.0 fields
            utility_score: row.get("utility_score").ok(),
            times_retrieved: row.get("times_retrieved").ok(),
            times_useful: row.get("times_useful").ok(),
            emotional_valence: row.get("emotional_valence").ok(),
            flashbulb: row.get::<_, Option<bool>>("flashbulb").ok().flatten(),
            temporal_level: row
                .get::<_, Option<String>>("temporal_level")
                .ok()
                .flatten(),
            // v3.1.0 provenance
            provenance: row
                .get::<_, Option<String>>("provenance")
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str(&s).ok()),
            // v3.3.0 typed memory. Columns may be absent on databases
            // that haven't run migration v11 yet, so default to Raw silently
            // rather than failing the whole row.
            memory_kind: row
                .get::<_, Option<String>>("memory_kind")
                .ok()
                .flatten()
                .map(|s| crate::memory::MemoryKind::parse(&s))
                .unwrap_or_default(),
            subject: row.get::<_, Option<String>>("subject").ok().flatten(),
            predicate: row.get::<_, Option<String>>("predicate").ok().flatten(),
            object: row.get::<_, Option<String>>("object").ok().flatten(),
            episodic_at: row
                .get::<_, Option<String>>("episodic_at")
                .ok()
                .flatten()
                .and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&Utc))
                        .ok()
                }),
            procedural_frequency: row
                .get::<_, Option<String>>("procedural_frequency")
                .ok()
                .flatten(),
        })
    }

    /// Log a memory access event for ACT-R activation computation.
    ///
    /// Lives here (and not in `review.rs` next to `strengthen_on_access`)
    /// because the consolidation pipeline and several other paths still
    /// call it directly. Keeping it adjacent to the writer makes the
    /// lock-discipline obvious at a glance.
    pub(super) fn log_access(&self, node_id: &str, access_type: &str) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "INSERT INTO memory_access_log (node_id, access_type, accessed_at)
             VALUES (?1, ?2, ?3)",
            params![node_id, access_type, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}

// ============================================================================
// PERSISTENCE LAYER
// ----------------------------------------------------------------------------
// Record structs (one per SQLite table) and their `pub(super)` row mappers
// moved to `records.rs`. Per-domain CRUD methods live in dedicated modules.
// ============================================================================

// All per-domain CRUD methods now live in their respective sub-modules.
// Quick reference for "where does this method live?":
//
//   intentions.rs   — Intentions CRUD (save/get/get_active/get_by_status/
//                      update_status/delete/overdue/snooze).
//   insights.rs     — Insights CRUD (save/get/get_pending/mark_feedback/clear).
//   connections.rs  — Connections CRUD (save/get_for_memory/get_all/
//                      strengthen/decay/prune).
//   states.rs       — Memory state machine (save_memory_state,
//                      get_memory_state, get_memories_by_state,
//                      update_memory_state, record_memory_access,
//                      row_to_memory_state, get_state_transitions,
//                      get_recent_state_transitions).
//   history.rs      — Consolidation/dream history and retention metrics
//                      (save_consolidation_history, get_last_consolidation,
//                      get_consolidation_history, save_dream_history,
//                      get_last_dream, get_dream_history, count_memories_since,
//                      get_last_backup_timestamp, get_avg_retention,
//                      get_retention_distribution, get_retention_trend,
//                      save_retention_snapshot, count_memories_below_retention).
//   maintenance.rs  — Autonomic ops (wal_checkpoint, backup_to,
//                      gc_below_retention, auto_promote_frequent_access,
//                      set_waking_tag, clear_waking_tags,
//                      get_waking_tagged_memories).
//   graph.rs        — Graph traversal (get_most_connected_memory,
//                      get_memory_subgraph).
//   records.rs      — Record structs + shared row_to_* mappers.

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsrs::Rating;
    use crate::memory::IngestInput;
    use chrono::Duration;
    use tempfile::tempdir;

    fn create_test_storage() -> Storage {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        Storage::new(Some(db_path)).unwrap()
    }

    #[test]
    fn test_storage_creation() {
        let storage = create_test_storage();
        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.total_nodes, 0);
    }

    #[test]
    fn test_ingest_and_get() {
        let storage = create_test_storage();

        let input = IngestInput {
            content: "Test memory content".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        };

        let node = storage.ingest(input).unwrap();
        assert!(!node.id.is_empty());
        assert_eq!(node.content, "Test memory content");

        let retrieved = storage.get_node(&node.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().content, "Test memory content");
    }

    #[test]
    fn test_typed_memory_roundtrip() {
        use crate::memory::MemoryKind;

        let storage = create_test_storage();
        let event_time = chrono::Utc::now();

        // Default ingest keeps memory_kind = Raw.
        let raw = storage
            .ingest(IngestInput {
                content: "Raw chunk".to_string(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(raw.memory_kind, MemoryKind::Raw);
        assert!(raw.subject.is_none());

        // Typed ingest preserves kind + subject + episodic_at through the
        // SQLite roundtrip (validates migration v11 wired columns end-to-end).
        let episodic = storage
            .ingest(IngestInput {
                content: "Caroline attended the LGBTQ group on 7 May 2023.".to_string(),
                node_type: "event".to_string(),
                memory_kind: MemoryKind::Episodic,
                subject: Some("Caroline".to_string()),
                episodic_at: Some(event_time),
                ..Default::default()
            })
            .unwrap();
        let fetched = storage.get_node(&episodic.id).unwrap().unwrap();
        assert_eq!(fetched.memory_kind, MemoryKind::Episodic);
        assert_eq!(fetched.subject.as_deref(), Some("Caroline"));
        assert_eq!(
            fetched.episodic_at.map(|t| t.timestamp()),
            Some(event_time.timestamp())
        );

        // Semantic triple (subject, predicate, object) also roundtrips.
        let semantic = storage
            .ingest(IngestInput {
                content: "Caroline lives in Berlin.".to_string(),
                memory_kind: MemoryKind::Semantic,
                subject: Some("Caroline".to_string()),
                predicate: Some("lives_in".to_string()),
                object: Some("Berlin".to_string()),
                ..Default::default()
            })
            .unwrap();
        let fetched = storage.get_node(&semantic.id).unwrap().unwrap();
        assert_eq!(fetched.memory_kind, MemoryKind::Semantic);
        assert_eq!(fetched.predicate.as_deref(), Some("lives_in"));
        assert_eq!(fetched.object.as_deref(), Some("Berlin"));

        // Procedural with frequency descriptor.
        let proc = storage
            .ingest(IngestInput {
                content: "Caroline goes to therapy every Tuesday.".to_string(),
                memory_kind: MemoryKind::Procedural,
                subject: Some("Caroline".to_string()),
                procedural_frequency: Some("every Tuesday".to_string()),
                ..Default::default()
            })
            .unwrap();
        let fetched = storage.get_node(&proc.id).unwrap().unwrap();
        assert_eq!(fetched.memory_kind, MemoryKind::Procedural);
        assert_eq!(
            fetched.procedural_frequency.as_deref(),
            Some("every Tuesday")
        );
    }

    #[test]
    fn test_search() {
        let storage = create_test_storage();

        let input = IngestInput {
            content: "The mitochondria is the powerhouse of the cell".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        };

        storage.ingest(input).unwrap();

        let results = storage.search("mitochondria", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("mitochondria"));
    }

    #[test]
    fn test_review() {
        let storage = create_test_storage();

        let input = IngestInput {
            content: "Test review".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        };

        let node = storage.ingest(input).unwrap();
        assert_eq!(node.reps, 0);

        let reviewed = storage.mark_reviewed(&node.id, Rating::Good).unwrap();
        assert_eq!(reviewed.reps, 1);
    }

    #[test]
    fn test_delete() {
        let storage = create_test_storage();

        let input = IngestInput {
            content: "To be deleted".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        };

        let node = storage.ingest(input).unwrap();
        assert!(storage.get_node(&node.id).unwrap().is_some());

        let deleted = storage.delete_node(&node.id).unwrap();
        assert!(deleted);
        assert!(storage.get_node(&node.id).unwrap().is_none());
    }

    #[test]
    fn test_dream_history_save_and_get_last() {
        let storage = create_test_storage();
        let now = Utc::now();

        let record = DreamHistoryRecord {
            dreamed_at: now,
            duration_ms: 1500,
            memories_replayed: 50,
            connections_found: 12,
            insights_generated: 3,
            memories_strengthened: 8,
            memories_compressed: 2,
            phase_nrem1_ms: None,
            phase_nrem3_ms: None,
            phase_rem_ms: None,
            phase_integration_ms: None,
            summaries_generated: None,
            emotional_memories_processed: None,
            creative_connections_found: None,
        };

        let id = storage.save_dream_history(&record).unwrap();
        assert!(id > 0);

        let last = storage.get_last_dream().unwrap();
        assert!(last.is_some());
        // Timestamps should be within 1 second (RFC3339 round-trip)
        let diff = (last.unwrap() - now).num_seconds().abs();
        assert!(diff <= 1, "Timestamp mismatch: diff={}s", diff);
    }

    #[test]
    fn test_dream_history_empty() {
        let storage = create_test_storage();
        let last = storage.get_last_dream().unwrap();
        assert!(last.is_none());
    }

    #[test]
    fn test_count_memories_since() {
        let storage = create_test_storage();
        let before = Utc::now() - Duration::seconds(10);

        for i in 0..5 {
            storage
                .ingest(IngestInput {
                    content: format!("Count test memory {}", i),
                    node_type: "fact".to_string(),
                    ..Default::default()
                })
                .unwrap();
        }

        let count = storage.count_memories_since(before).unwrap();
        assert_eq!(count, 5);

        let future = Utc::now() + Duration::hours(1);
        let count_future = storage.count_memories_since(future).unwrap();
        assert_eq!(count_future, 0);
    }

    #[test]
    fn test_get_last_backup_timestamp_no_panic() {
        // Static method should not panic even if no backups exist
        let _ = Storage::get_last_backup_timestamp();
    }
}

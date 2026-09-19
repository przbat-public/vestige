//! Shared `Storage` helpers — reader pool, row mappers, access logging.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

#[cfg(all(feature = "embeddings", feature = "vector-search"))]
use crate::embeddings::EMBEDDING_DIMENSIONS;
use crate::memory::KnowledgeNode;

use super::Storage;
use super::error::{Result, StorageError};

impl Storage {
    /// Embed text using the storage's embedding service.
    ///
    /// Exposed so callers (e.g. the MCP `force_create` path) can compute
    /// an embedding *once* and then hand it to both
    /// [`Self::semantic_search_by_embedding`] and
    /// [`Self::ingest_with_embedding`] instead of paying the embed cost
    /// twice. Returns `None` when the embedding service is not ready, which
    /// matches the silent fallback behaviour elsewhere in the storage layer.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn embed_text(
        &self,
        text: &str,
    ) -> std::result::Result<crate::embeddings::Embedding, crate::embeddings::EmbeddingError> {
        self.embedding_service.embed(text)
    }

    /// Whether the embedding service is initialised and ready to serve calls.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn embedding_service_ready(&self) -> bool {
        self.embedding_service.is_ready()
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

    /// Generate embedding for a node by embedding `content` from scratch.
    ///
    /// This is the convenience path: callers that don't already have an
    /// `Embedding` in hand just supply the source text. Internally we hand
    /// off to [`Self::store_embedding_for_node`] once the vector is ready
    /// so the persistence logic lives in exactly one place.
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

        self.store_embedding_for_node(node_id, &embedding)
    }

    /// Persist a *pre-computed* embedding for a node.
    ///
    /// Splitting this out from `generate_embedding_for_node` lets the smart
    /// ingest path (`smart_ingest`, `ingest_with_embedding`) reuse the vector
    /// it already paid for instead of re-embedding the same content a second
    /// time inside `Storage::ingest`. The ML audit on 2026-05-20 measured
    /// 2 redundant embed calls per `force_create` ingest and 1 per smart
    /// ingest — embedding is by far the most expensive thing this code path
    /// does, so this matters even for modest write volumes.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub(super) fn store_embedding_for_node(
        &self,
        node_id: &str,
        embedding: &crate::embeddings::Embedding,
    ) -> Result<()> {
        let now = Utc::now();

        {
            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            // Tag the regime the vector was built under so a raw-vs-prefixed
            // mismatch (prefixes enabled without re-embedding) is observable.
            let model_tag = crate::embeddings::embedding_model_tag();
            writer.execute(
                "INSERT OR REPLACE INTO node_embeddings (node_id, embedding, dimensions, model, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    node_id,
                    embedding.to_bytes(),
                    EMBEDDING_DIMENSIONS as i32,
                    model_tag,
                    now.to_rfc3339(),
                ],
            )?;

            writer.execute(
                "UPDATE knowledge_nodes SET has_embedding = 1, embedding_model = ?2 WHERE id = ?1",
                params![node_id, model_tag],
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
            // v3.4.0 typed extensions (Decision matrix, Hub metadata,
            // Insight payload). Defaults to None when the column is absent
            // (pre-v12 databases) or when JSON parsing fails — drift-tolerant
            // by design so one corrupt row never poisons a whole query.
            extra_json: row
                .get::<_, Option<String>>("extra_json")
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

/// Begin a write transaction the way every writer in this crate must: `BEGIN IMMEDIATE`.
///
/// A `DEFERRED` transaction that reads before it writes can fail with `SQLITE_BUSY_SNAPSHOT`
/// when another process commits in between — the CLI running beside the MCP server is the
/// normal case — and SQLite does **not** consult `busy_timeout` for that upgrade, so the write
/// is simply lost. Taking the write lock up front puts the wait under the busy timeout
/// instead, and once `BEGIN IMMEDIATE` succeeds SQLite guarantees no `SQLITE_BUSY` until
/// `COMMIT`.
///
/// **This must go through `transaction_with_behavior`.** An earlier version issued
/// `execute_batch("BEGIN IMMEDIATE")` and then `unchecked_transaction()`, assuming the latter
/// only wraps an already-open transaction. It does not — it calls `Transaction::new_unchecked`,
/// which sends another `BEGIN` — so every call failed with "cannot start a transaction within
/// a transaction", and because the first `BEGIN` had succeeded the connection stayed inside an
/// open transaction: every later write in that process went uncommitted and was lost on exit.
/// Consolidation (whose first step is `apply_decay`) never ran at all, and `gc` reported
/// deletions that readers could not see.
///
/// There is deliberately no retry loop. `busy_timeout` (5 s, set when the connection opens)
/// already makes `BEGIN IMMEDIATE` wait for a competing writer, and a loop returning the
/// transaction cannot be written without the borrow checker rejecting it — the previous
/// hand-rolled retry is what introduced the double `BEGIN` in the first place.
pub(crate) fn begin_write_transaction(conn: &mut Connection) -> Result<rusqlite::Transaction<'_>> {
    Ok(conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?)
}

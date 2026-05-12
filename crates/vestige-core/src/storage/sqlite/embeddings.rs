//! Embedding-vector accessors and bulk generation.
//!
//! The hot init-time helpers (`load_embeddings_into_index`,
//! `migrate_embeddings`, `generate_embedding_for_node`) intentionally stay in
//! `mod.rs` because they are wired into `Storage::new` and are called from
//! the writer-side ingest/update paths in `nodes.rs` and `review.rs`. This
//! module owns only the public surface a caller would reach for explicitly:
//!
//! - [`Storage::is_embedding_ready`] / [`Storage::init_embeddings`]: model
//!   readiness probe + explicit kick to load the embedder on startup.
//! - [`Storage::get_node_embedding`] / [`Storage::get_all_embeddings`]: raw
//!   vector access, used by the vector index and dedup logic.
//! - [`Storage::generate_embeddings`]: batch generator for backfill /
//!   `regenerate_embeddings` MCP tool.

#[cfg(all(feature = "embeddings", feature = "vector-search"))]
use rusqlite::{params, OptionalExtension};

#[cfg(all(feature = "embeddings", feature = "vector-search"))]
use crate::memory::EmbeddingResult;

use super::{Result, Storage};

#[cfg(all(feature = "embeddings", feature = "vector-search"))]
use super::StorageError;

impl Storage {
    /// Check if embedding service is ready
    #[cfg(feature = "embeddings")]
    pub fn is_embedding_ready(&self) -> bool {
        self.embedding_service.is_ready()
    }

    #[cfg(not(feature = "embeddings"))]
    pub fn is_embedding_ready(&self) -> bool {
        false
    }

    /// Initialize the embedding service explicitly
    /// Call this at startup to catch initialization errors early
    #[cfg(feature = "embeddings")]
    pub fn init_embeddings(&self) -> Result<()> {
        self.embedding_service.init().map_err(|e| {
            StorageError::Init(format!("Embedding service initialization failed: {}", e))
        })
    }

    #[cfg(not(feature = "embeddings"))]
    pub fn init_embeddings(&self) -> Result<()> {
        Ok(()) // No-op when embeddings feature is disabled
    }

    /// Get the embedding vector for a node
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn get_node_embedding(&self, node_id: &str) -> Result<Option<Vec<f32>>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT embedding FROM node_embeddings WHERE node_id = ?1"
        )?;

        let embedding_bytes: Option<Vec<u8>> = stmt
            .query_row(params![node_id], |row| row.get(0))
            .optional()?;

        Ok(embedding_bytes.and_then(|bytes| {
            crate::embeddings::Embedding::from_bytes(&bytes).map(|e| e.vector)
        }))
    }

    /// Get all embedding vectors for duplicate detection
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn get_all_embeddings(&self) -> Result<Vec<(String, Vec<f32>)>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader
            .prepare("SELECT node_id, embedding FROM node_embeddings")?;

        let results: Vec<(String, Vec<f32>)> = stmt
            .query_map([], |row| {
                let node_id: String = row.get(0)?;
                let embedding_bytes: Vec<u8> = row.get(1)?;
                Ok((node_id, embedding_bytes))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, bytes)| {
                crate::embeddings::Embedding::from_bytes(&bytes)
                    .map(|e| (id, e.vector))
            })
            .collect();

        Ok(results)
    }

    /// Generate embeddings for nodes
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn generate_embeddings(
        &self,
        node_ids: Option<&[String]>,
        force: bool,
    ) -> Result<EmbeddingResult> {
        if !self.embedding_service.is_ready() {
            self.embedding_service.init().map_err(|e| {
                StorageError::Init(format!("Failed to init embedding service: {}", e))
            })?;
        }

        let mut result = EmbeddingResult::default();

        let nodes: Vec<(String, String)> = {
            let reader = self.reader.lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            if let Some(ids) = node_ids {
                let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let query = format!(
                    "SELECT id, content FROM knowledge_nodes WHERE id IN ({})",
                    placeholders
                );

                let mut result_nodes = Vec::new();
                {
                    let mut stmt = reader.prepare(&query)?;
                    let params: Vec<&dyn rusqlite::ToSql> =
                        ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

                    let rows = stmt.query_map(params.as_slice(), |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?;

                    for r in rows.flatten() {
                        result_nodes.push(r);
                    }
                }
                result_nodes
            } else if force {
                let mut stmt = reader
                    .prepare("SELECT id, content FROM knowledge_nodes")?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                rows.filter_map(|r| r.ok()).collect()
            } else {
                let mut stmt = reader.prepare(
                    "SELECT id, content FROM knowledge_nodes
                         WHERE has_embedding = 0 OR has_embedding IS NULL",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                rows.filter_map(|r| r.ok()).collect()
            }
        };

        for (id, content) in nodes {
            if !force {
                let has_emb: i32 = self
                    .reader.lock()
                    .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?
                    .query_row(
                        "SELECT COALESCE(has_embedding, 0) FROM knowledge_nodes WHERE id = ?1",
                        params![id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                if has_emb == 1 {
                    result.skipped += 1;
                    continue;
                }
            }

            match self.generate_embedding_for_node(&id, &content) {
                Ok(()) => result.successful += 1,
                Err(e) => {
                    result.failed += 1;
                    result.errors.push(format!("{}: {}", id, e));
                }
            }
        }

        Ok(result)
    }
}

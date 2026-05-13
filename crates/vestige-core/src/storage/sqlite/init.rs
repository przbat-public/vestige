//! `Storage` struct: connection pool, embedding service, vector index.
//!
//! Owns construction (`Storage::new`) and dimension-migration startup logic.

use directories::ProjectDirs;
#[cfg(feature = "embeddings")]
use lru::LruCache;
use rusqlite::Connection;
#[cfg(feature = "embeddings")]
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(feature = "embeddings")]
use crate::embeddings::{EMBEDDING_DIMENSIONS, Embedding, EmbeddingService, matryoshka_truncate};
use crate::fsrs::FSRSScheduler;
#[cfg(feature = "vector-search")]
use crate::search::VectorIndex;

use super::error::{Result, StorageError};

#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Connection pools and per-domain caches are accessed directly by sibling submodules of the cohesive `sqlite` component (split-by-responsibility refactor: records, states, embeddings, graph, gdpr, smart_ingest, etc.). Siblings hold the same trust level as the parent module; adding accessor methods for `Mutex<Connection>` handles would only forward `lock()` calls without protecting any additional invariant."
)]
pub struct Storage {
    pub(super) writer: Mutex<Connection>,
    pub(super) reader: Mutex<Connection>,
    pub(super) reader_secondary: Mutex<Connection>,
    pub(super) scheduler: Mutex<FSRSScheduler>,
    #[cfg(feature = "embeddings")]
    pub(super) embedding_service: EmbeddingService,
    #[cfg(feature = "vector-search")]
    pub(super) vector_index: Mutex<VectorIndex>,
    /// LRU cache for query embeddings to avoid re-embedding repeated queries
    #[cfg(feature = "embeddings")]
    pub(super) query_cache: Mutex<LruCache<String, Vec<f32>>>,
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
        crate::storage::migrations::apply_migrations(&writer_conn)?;

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
            let from_dims = embeddings
                .first()
                .and_then(|(_, b)| Embedding::from_bytes(b))
                .map(|e| e.dimensions)
                .unwrap_or(0);
            tracing::info!(
                count,
                from_dims,
                to_dims = EMBEDDING_DIMENSIONS,
                "dimension migration starting: re-embedding {} memories ({} → {} dims)",
                count,
                from_dims,
                EMBEDDING_DIMENSIONS,
            );
            self.migrate_embeddings(&needs_reembed)?;
            tracing::info!(
                count,
                "dimension migration complete ({} memories re-embedded)",
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
}

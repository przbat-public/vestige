//! `Storage` struct: connection pool, embedding service, vector index.
//!
//! Owns construction (`Storage::new`) and dimension-migration startup logic.

use directories::ProjectDirs;
#[cfg(feature = "embeddings")]
use lru::LruCache;
use rusqlite::Connection;
#[cfg(feature = "embeddings")]
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
#[cfg(feature = "vector-search")]
use std::sync::atomic::{AtomicU8, Ordering};

#[cfg(feature = "embeddings")]
use crate::embeddings::{EMBEDDING_DIMENSIONS, Embedding, EmbeddingService, matryoshka_truncate};
use crate::fsrs::FSRSScheduler;
#[cfg(feature = "vector-search")]
use crate::search::VectorIndex;

use super::error::{Result, StorageError};

/// Passphrase for the SQLCipher-encrypted database.
///
/// Set (non-empty) ⇒ the database MUST be encrypted: this process refuses to
/// open a plaintext file, and refuses to start at all if it was built without
/// the `encryption` cargo feature (SQLCipher). Whitespace-only values are
/// treated as unset — an empty passphrase is not encryption.
pub(super) const ENCRYPTION_KEY_ENV: &str = "VESTIGE_ENCRYPTION_KEY";

/// Explicit switch (truthy: `1`, `true`, `yes`, `on`) demanding an encrypted
/// database even when `VESTIGE_ENCRYPTION_KEY` is missing or empty.
///
/// Exists so an operator can state the intent in a unit file / launchd plist
/// without a key material file present: the process then fails closed instead
/// of quietly creating a plaintext database. Without this switch (and without
/// a key) plaintext is still allowed, but only with a warning.
pub(super) const REQUIRE_ENCRYPTION_ENV: &str = "VESTIGE_REQUIRE_ENCRYPTION";

/// What the environment asks of the database file, resolved once per open.
///
/// Modelled as a value (rather than read inline where it is used) so every
/// branch is testable without mutating process-wide environment variables,
/// which would race with every other test in the binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EncryptionConfig {
    /// No passphrase and no explicit request: the database is plaintext.
    Plaintext,
    /// `VESTIGE_ENCRYPTION_KEY` holds a non-empty passphrase.
    Key(String),
    /// `VESTIGE_REQUIRE_ENCRYPTION` is truthy but the passphrase is missing.
    RequestedWithoutKey,
}

/// How the in-memory vector index was populated on startup.
///
/// Exposed for diagnostics: a healthy long-lived process should mostly see
/// `Loaded` (HNSW sidecar restored), with occasional `Rebuilt` when row
/// counts drift (e.g. crash before save, manual DB edit, migration).
#[cfg(feature = "vector-search")]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorIndexSource {
    /// No embeddings in DB — index legitimately empty.
    Empty = 0,
    /// Index re-built from `node_embeddings` table (slow path).
    Rebuilt = 1,
    /// Index restored from `vestige.hnsw` sidecar (fast path, post v3.6).
    Loaded = 2,
}

#[cfg(feature = "vector-search")]
impl VectorIndexSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Rebuilt => "rebuilt",
            Self::Loaded => "loaded",
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            2 => Self::Loaded,
            1 => Self::Rebuilt,
            _ => Self::Empty,
        }
    }
}

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
    /// Resolved DB path — kept so we can derive sidecar paths
    /// (`vestige.hnsw`, `vestige.hnsw.meta.json`) for vector-index persistence
    /// without re-deriving via ProjectDirs.
    pub(super) db_path: PathBuf,
    /// How the vector index was populated on the most recent (re)load.
    /// Surfaced via [`Storage::vector_index_source`].
    #[cfg(feature = "vector-search")]
    pub(super) vector_index_source: AtomicU8,
}

impl Storage {
    /// Read the encryption switches.
    ///
    /// Precedence: a non-empty passphrase wins over everything (it states
    /// intent on its own); otherwise the explicit switch decides between
    /// "fail closed" and plaintext-with-warning.
    pub(super) fn encryption_config(get: &dyn Fn(&str) -> Option<String>) -> EncryptionConfig {
        match get(ENCRYPTION_KEY_ENV).filter(|key| !key.trim().is_empty()) {
            Some(key) => EncryptionConfig::Key(key),
            None if env_truthy(REQUIRE_ENCRYPTION_ENV, get) => {
                EncryptionConfig::RequestedWithoutKey
            }
            None => EncryptionConfig::Plaintext,
        }
    }

    /// [`Self::encryption_config`] against the real process environment.
    fn encryption_config_from_env() -> EncryptionConfig {
        Self::encryption_config(&|name| std::env::var(name).ok())
    }

    /// Apply the encryption policy, PRAGMAs and readability check to a
    /// connection. `secure_delete` is only ever true for the writer.
    ///
    /// `PRAGMA secure_delete = ON` zeroes freed cells and pages instead of just
    /// unlinking them, which is what makes an erasure physical. The cost is
    /// paid on every `DELETE`/shrinking `UPDATE` — extra page writes — so it is
    /// enabled on the writer connection only (readers never delete) and
    /// deliberately not on connections that only read.
    pub(super) fn configure_connection(
        conn: &Connection,
        encryption: &EncryptionConfig,
        secure_delete: bool,
    ) -> Result<()> {
        match encryption {
            EncryptionConfig::Key(key) => {
                #[cfg(feature = "encryption")]
                // Must be the first statement on the connection: SQLCipher
                // derives the file key from it before reading the header.
                conn.pragma_update(None, "key", key)?;

                #[cfg(not(feature = "encryption"))]
                {
                    // Never log the passphrase itself.
                    let _ = key;
                    return Err(StorageError::Init(format!(
                        "{ENCRYPTION_KEY_ENV} is set but this build cannot encrypt: it was compiled \
                         without the `encryption` cargo feature (SQLCipher). Rebuild with \
                         `--features encryption` or unset the variable — refusing to open a \
                         plaintext database"
                    )));
                }
            }
            EncryptionConfig::RequestedWithoutKey => {
                return Err(StorageError::Init(format!(
                    "{REQUIRE_ENCRYPTION_ENV} asks for an encrypted database but {ENCRYPTION_KEY_ENV} \
                     is missing or empty; set a non-empty passphrase (and build with the `encryption` \
                     cargo feature for SQLCipher) — refusing to fall back to a plaintext database"
                )));
            }
            EncryptionConfig::Plaintext => {}
        }

        // Check readability before the PRAGMA batch: `journal_mode = WAL` on a
        // file we cannot decrypt already fails with a bare "file is not a
        // database", which reads like corruption instead of a missing key.
        Self::verify_readable(conn, matches!(encryption, EncryptionConfig::Key(_)))?;

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

        if secure_delete {
            // Zero freed cells/pages instead of merely unlinking them, so an
            // erased row is physically gone from the file. Costs extra page
            // writes on every DELETE and every shrinking UPDATE, so it is set
            // on the writer only (readers never delete) — see
            // `gdpr::post_erasure_cleanup` for the erasure sequence it backs.
            conn.execute_batch("PRAGMA secure_delete = ON;")?;
        }

        Ok(())
    }

    /// Refuse to continue when the file on disk is not a database this
    /// connection can read.
    ///
    /// A SQLCipher file opened without its key is indistinguishable from
    /// garbage to SQLite: both surface as `SQLITE_NOTADB`. Treating that as
    /// "corrupt" and carrying on would let the process create a *new* plaintext
    /// database next to (or over) the encrypted one, so the only safe reading
    /// is "encrypted or corrupt — stop and say why".
    fn verify_readable(conn: &Connection, key_supplied: bool) -> Result<()> {
        match conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        }) {
            Ok(_) => Ok(()),
            Err(e) if Self::is_not_a_database(&e) => Err(StorageError::Init(if key_supplied {
                format!(
                    "{ENCRYPTION_KEY_ENV} was supplied but the database could not be decrypted: \
                     the passphrase is wrong or the file is not a SQLCipher database"
                )
            } else {
                format!(
                    "the database file is not a readable SQLite database — it is encrypted (set \
                     {ENCRYPTION_KEY_ENV}) or corrupt; refusing to continue"
                )
            })),
            Err(e) => Err(e.into()),
        }
    }

    fn is_not_a_database(err: &rusqlite::Error) -> bool {
        matches!(
            err,
            rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::NotADatabase
        )
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

        // Resolved once per open, then reused for every connection so the
        // three connections cannot disagree about how the file is keyed.
        let encryption = Self::encryption_config_from_env();
        if encryption == EncryptionConfig::Plaintext {
            // One warning per process: an operator who never asked for
            // encryption should still learn that the memories land on disk in
            // the clear, without a warning storm from every connection.
            static PLAINTEXT_WARNING: std::sync::Once = std::sync::Once::new();
            PLAINTEXT_WARNING.call_once(|| {
                tracing::warn!(
                    "database is NOT encrypted: neither {ENCRYPTION_KEY_ENV} nor \
                     {REQUIRE_ENCRYPTION_ENV} is set, so memories are stored in plaintext"
                );
            });
        }

        // Open writer connection
        let writer_conn = Connection::open(&path)?;

        // Restrict database file permissions to owner-only on Unix
        #[cfg(unix)]
        if path.exists() {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }

        // The writer is the only connection that deletes, so it is the only
        // one that pays for `secure_delete`.
        Self::configure_connection(&writer_conn, &encryption, true)?;

        // Apply migrations on writer only
        crate::storage::migrations::apply_migrations(&writer_conn)?;

        // Open primary + secondary reader connections to same path (SQLite WAL
        // supports concurrent readers, so two connections reduce lock contention).
        let reader_conn = Connection::open(&path)?;
        Self::configure_connection(&reader_conn, &encryption, false)?;
        let reader_secondary_conn = Connection::open(&path)?;
        Self::configure_connection(&reader_secondary_conn, &encryption, false)?;

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
            db_path: path,
            #[cfg(feature = "vector-search")]
            vector_index_source: AtomicU8::new(VectorIndexSource::Empty as u8),
        };

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        storage.load_embeddings_into_index()?;

        // Report — never perform — the migration an embedding-function upgrade
        // needs. Re-embedding every memory is long and destroys the previous
        // vectors, so it stays an explicit user decision.
        #[cfg(feature = "embeddings")]
        match storage.stale_embedding_space_warning() {
            Ok(Some(message)) => tracing::warn!("{message}"),
            Ok(None) => {}
            Err(e) => tracing::debug!(
                error = %e,
                "could not check for vectors written in an older embedding space"
            ),
        }

        // Pick up a previously-optimized w20 if consolidation has produced
        // one. Best-effort: a missing column or a read error means we keep
        // the FSRS-6 default and log a debug breadcrumb. This must NOT fail
        // boot — operators with old DBs (pre-v14 schema) would be stuck.
        if let Err(e) = storage.apply_personalized_weights() {
            tracing::debug!(
                error = %e,
                "could not apply personalized FSRS weights at boot; falling back to defaults"
            );
        }

        Ok(storage)
    }

    /// Path to the HNSW sidecar binary next to the DB.
    #[cfg(feature = "vector-search")]
    pub(super) fn vector_index_path(&self) -> PathBuf {
        // VectorIndex::save() also writes `vestige.hnsw.mappings.json` next to
        // the binary — that file is paired by USearch with `with_extension`.
        // We add our own `.meta.json` so we can validate the cache cheaply.
        path_with_suffix(&self.db_path, "hnsw")
    }

    /// Sidecar metadata describing what's in the HNSW file. Versioned so we
    /// can refuse to load older formats without crashing.
    #[cfg(feature = "vector-search")]
    pub(super) fn vector_index_meta_path(&self) -> PathBuf {
        path_with_suffix(&self.db_path, "hnsw.meta.json")
    }

    /// How the vector index was populated on the most recent (re)load.
    /// Defaults to `Empty` before [`Storage::new`] finishes.
    #[cfg(feature = "vector-search")]
    pub fn vector_index_source(&self) -> VectorIndexSource {
        VectorIndexSource::from_u8(self.vector_index_source.load(Ordering::Relaxed))
    }

    /// One actionable line when the store holds vectors produced by an older
    /// embedding function, or `None` when every stored vector is current.
    ///
    /// Rejecting a sidecar only rebuilds the *index*; the vectors themselves
    /// stay in the previous space until they are recomputed from content. That
    /// is a long, user-visible operation, so this only reports it — nothing in
    /// storage re-embeds on its own. Returned as a string rather than logged
    /// here so the wording is testable.
    #[cfg(feature = "embeddings")]
    pub(super) fn stale_embedding_space_warning(&self) -> Result<Option<String>> {
        let current_tag = crate::embeddings::embedding_model_tag();
        let reader = self.acquire_reader()?;
        // `COALESCE` counts a NULL tag as stale too: a row with a vector but
        // no recorded provenance cannot be proven current.
        let stale: i64 = reader.query_row(
            "SELECT COUNT(*) FROM knowledge_nodes
             WHERE has_embedding = 1 AND COALESCE(embedding_model, '') <> ?1",
            rusqlite::params![current_tag],
            |row| row.get(0),
        )?;

        if stale == 0 {
            return Ok(None);
        }
        Ok(Some(format!(
            "{stale} memories hold vectors from an older embedding function (tag other than \
             {current_tag}); semantic search mixes embedding spaces until they are recomputed \
             — run `regenerate_embeddings` with `force: true` to re-embed them"
        )))
    }

    /// Cheap state fingerprint of the `node_embeddings` table.
    ///
    /// The HNSW sidecar is only valid for the exact embedding rows it was
    /// written from. A row count alone misses an in-place vector rewrite
    /// (`update_node_content` / `regenerate_embeddings`): the count stays the
    /// same while the vector's semantics change, and the next boot would load
    /// a binary that answers semantic queries with the *old* wording.
    /// `store_embedding_for_node` refreshes `created_at` on every write, so
    /// `(COUNT(*), MAX(created_at))` changes whenever an embedding row is
    /// added, replaced or deleted.
    ///
    /// It also misses an upgrade of the embedding *function*: the rows are
    /// untouched by an upgrade, so a sidecar full of vectors from the previous
    /// space would still look current and semantic search would rank a query
    /// from one space against documents from another. The current space id is
    /// therefore part of the stamp.
    #[cfg(feature = "vector-search")]
    fn embeddings_fingerprint(&self) -> Result<String> {
        let reader = self.acquire_reader()?;
        let (rows, newest): (i64, Option<String>) = reader.query_row(
            "SELECT COUNT(*), MAX(created_at) FROM node_embeddings",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let state = format!("{}:{}", rows, newest.unwrap_or_default());

        #[cfg(feature = "embeddings")]
        return Ok(format!(
            "{}|{}",
            crate::embeddings::embedding_space_fingerprint(),
            state
        ));

        #[cfg(not(feature = "embeddings"))]
        Ok(state)
    }

    /// Persist the current HNSW index to its sidecar.
    ///
    /// Safe to call frequently — it's a write, not a serialize-from-scratch.
    /// On error returns `Err` but **also** removes the (now-stale) meta file
    /// so a subsequent restart will rebuild from SQLite rather than load a
    /// half-written binary. Best-effort: callers may ignore the error.
    #[cfg(feature = "vector-search")]
    pub fn persist_vector_index(&self) -> Result<()> {
        // Read the embedding state *before* taking the index lock — the sidecar
        // is valid only for exactly this state, and the lock order used
        // elsewhere in this module is reader/writer first, index second.
        let fingerprint = self.embeddings_fingerprint()?;

        let index = self
            .vector_index
            .lock()
            .map_err(|_| StorageError::Init("Vector index lock poisoned".into()))?;
        let row_count = index.len();
        let path = self.vector_index_path();
        index
            .save(&path)
            .map_err(|e| StorageError::Init(format!("vector index save failed: {}", e)))?;

        // The meta is built as a map (not a `json!` literal) so the embedding
        // space can be added only in builds that have an embedding function.
        let mut meta = serde_json::Map::new();
        meta.insert("schema".to_string(), 1.into());
        meta.insert("row_count".to_string(), row_count.into());
        meta.insert("dimensions".to_string(), index.dimensions().into());
        meta.insert("embeddings_fingerprint".to_string(), fingerprint.into());
        #[cfg(feature = "embeddings")]
        meta.insert(
            "embedding_space".to_string(),
            crate::embeddings::embedding_space_fingerprint().into(),
        );
        let meta = serde_json::Value::Object(meta);
        let meta_path = self.vector_index_meta_path();
        if let Err(e) = std::fs::write(
            &meta_path,
            serde_json::to_string(&meta)
                .map_err(|e| StorageError::Init(format!("meta serialize: {}", e)))?,
        ) {
            // Roll back: a binary without metadata can't be safely loaded
            // (we wouldn't be able to validate row count on next boot).
            let _ = std::fs::remove_file(&path);
            return Err(StorageError::Init(format!("meta write failed: {}", e)));
        }
        Ok(())
    }

    /// Load existing embeddings into vector index, migrating dimensions if needed.
    ///
    /// Three migration paths handled on startup:
    /// - **same dims** (384 → 384): loaded directly, no work.
    /// - **larger stored** (768 → 384): Matryoshka truncate + L2-normalize.
    /// - **smaller stored** (256 → 384): must re-embed from content
    ///   (Matryoshka can't *extend* dimensions). Runs once, updates DB in-place.
    ///
    /// **Fast path:** if a `vestige.hnsw` sidecar exists whose meta
    /// records the same row count as the current `node_embeddings` table, we
    /// `VectorIndex::load` it and skip the per-row insert loop entirely. On
    /// any mismatch, validation failure, or load error we fall back to the
    /// rebuild path below — the sidecar is purely a cache, never the source
    /// of truth.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub(super) fn load_embeddings_into_index(&self) -> Result<()> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;

        // Cheap row count first — lets us early-out for empty DBs and gives
        // the sidecar-load path a checkable invariant.
        let db_row_count: usize = reader
            .query_row("SELECT COUNT(*) FROM node_embeddings", [], |row| row.get(0))
            .map(|c: i64| c as usize)
            .unwrap_or(0);

        if db_row_count == 0 {
            drop(reader);
            self.vector_index_source
                .store(VectorIndexSource::Empty as u8, Ordering::Relaxed);
            return Ok(());
        }

        // Sidecar fast path. Errors here are non-fatal — log and fall through
        // to the rebuild path. The cache file is recoverable but never
        // authoritative.
        drop(reader);
        if self.try_load_vector_index_from_sidecar(db_row_count) {
            self.vector_index_source
                .store(VectorIndexSource::Loaded as u8, Ordering::Relaxed);
            return Ok(());
        }

        // Slow path: rebuild from SQLite (also handles dimension migrations).
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

        self.vector_index_source
            .store(VectorIndexSource::Rebuilt as u8, Ordering::Relaxed);

        // Best-effort persist so the next startup hits the sidecar fast path.
        // Failure to save the cache is never fatal — the rebuild has already
        // succeeded, the index is usable, and we'll just rebuild again next
        // time.
        if let Err(e) = self.persist_vector_index() {
            tracing::warn!(
                error = %e,
                "vector index sidecar save failed — next startup will rebuild from SQLite"
            );
        }

        Ok(())
    }

    /// Try to populate `self.vector_index` from the on-disk sidecar.
    ///
    /// Returns `true` only when:
    /// 1. Both `vestige.hnsw` and `vestige.hnsw.meta.json` exist.
    /// 2. Meta schema is recognized.
    /// 3. `meta.embedding_space` is the space this build produces.
    /// 4. `meta.row_count` matches the current DB row count.
    /// 5. `meta.embeddings_fingerprint` matches the current embedding state.
    /// 6. `VectorIndex::load` and the mappings sidecar both succeed.
    /// 7. Loaded index reports the expected size.
    ///
    /// Any failure is logged and returns `false` so the caller falls through
    /// to the rebuild path.
    #[cfg(feature = "vector-search")]
    fn try_load_vector_index_from_sidecar(&self, expected_rows: usize) -> bool {
        use crate::search::VectorIndexConfig;

        let bin = self.vector_index_path();
        let meta = self.vector_index_meta_path();
        if !bin.exists() || !meta.exists() {
            return false;
        }

        // Validate meta first — cheaper than touching the binary.
        let meta_str = match std::fs::read_to_string(&meta) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, "no readable vector index meta — rebuilding");
                return false;
            }
        };
        let meta_val: serde_json::Value = match serde_json::from_str(&meta_str) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "vector index meta unparseable — rebuilding");
                return false;
            }
        };
        let schema = meta_val.get("schema").and_then(|v| v.as_u64()).unwrap_or(0);
        if schema != 1 {
            tracing::info!(schema, "vector index meta schema mismatch — rebuilding");
            return false;
        }

        // An upgrade of the embedding function leaves every row untouched, so
        // row count and embedding-state stamp both still match; without this
        // check the process would load vectors from the previous space and
        // silently rank cross-space distances. A meta written before this
        // field existed has no `embedding_space` and is rebuilt once.
        #[cfg(feature = "embeddings")]
        {
            let expected_space = crate::embeddings::embedding_space_fingerprint();
            let meta_space = meta_val
                .get("embedding_space")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if meta_space != expected_space {
                tracing::info!(
                    meta_space,
                    %expected_space,
                    "vector index sidecar was written in a different embedding space — rebuilding"
                );
                return false;
            }
        }

        let meta_rows = meta_val
            .get("row_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        if meta_rows != expected_rows {
            tracing::info!(
                meta_rows,
                db_rows = expected_rows,
                "vector index sidecar stale (row count drift) — rebuilding"
            );
            return false;
        }

        // Row count alone cannot see an in-place vector rewrite (memory edit),
        // so also compare the embedding-state fingerprint. A sidecar written by
        // an older build has no fingerprint and is conservatively rebuilt once.
        let expected_fingerprint = match self.embeddings_fingerprint() {
            Ok(fingerprint) => fingerprint,
            Err(e) => {
                tracing::warn!(error = %e, "cannot fingerprint embeddings — rebuilding vector index");
                return false;
            }
        };
        let meta_fingerprint = meta_val
            .get("embeddings_fingerprint")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if meta_fingerprint != expected_fingerprint {
            tracing::info!(
                meta_fingerprint,
                db_fingerprint = %expected_fingerprint,
                "vector index sidecar stale (embedding state changed) — rebuilding"
            );
            return false;
        }

        // Try the actual load. USearch's load is allocation-heavy but
        // ~constant time, far cheaper than per-row insert + HNSW link
        // construction.
        let loaded = match VectorIndex::load(&bin, VectorIndexConfig::default()) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "vector index load failed — rebuilding from SQLite");
                return false;
            }
        };
        if loaded.len() != expected_rows {
            tracing::warn!(
                loaded = loaded.len(),
                expected = expected_rows,
                "loaded HNSW index size disagrees with DB — rebuilding"
            );
            return false;
        }

        // Swap in.
        match self.vector_index.lock() {
            Ok(mut slot) => *slot = loaded,
            Err(_) => {
                tracing::warn!("vector index lock poisoned — rebuilding");
                return false;
            }
        }
        tracing::info!(
            rows = expected_rows,
            "Vector index restored from sidecar — skipped per-row HNSW rebuild"
        );
        true
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

/// Truthiness for Vestige's own switches: `1`, `true`, `yes`, `on`
/// (case-insensitive, surrounding whitespace ignored). Unset or anything else
/// is false — a typo must never be read as consent to anything.
fn env_truthy(name: &str, get: &dyn Fn(&str) -> Option<String>) -> bool {
    get(name)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Compute `<db_dir>/<db_stem>.<suffix>` so sidecar paths live next to the DB
/// without colliding with other files. `with_extension` works because every
/// supported DB extension is a single component.
fn path_with_suffix(db: &Path, suffix: &str) -> PathBuf {
    db.with_extension(suffix)
}

//! Local Semantic Embeddings
//!
//! Uses fastembed v5.11 for local inference.
//!
//! ## Models
//!
//! - **Default**: Nomic Embed Text v1.5 (ONNX, 768d → 384d Matryoshka, 8192 context)
//! - **Optional**: Nomic Embed Text v2 MoE (Candle, 475M params, 305M active, 8 experts)
//!   Enable with `nomic-v2` feature flag + `metal` for Apple Silicon acceleration.
//! - **Test only**: a deterministic, offline stand-in embedder enabled by
//!   `VESTIGE_TEST_MOCK_EMBEDDINGS=1` (see [`mock_embedding`]). It keeps the
//!   semantic path (embeddings → HNSW → RRF) covered without downloading the
//!   ONNX model, and never touches the network.

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::{Mutex, OnceLock};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Embedding dimensions after Matryoshka truncation.
/// 384 dims: best quality/size tradeoff (~1% loss on MTEB vs full 768).
/// Previous: 256 (migrated automatically on first search after upgrade).
pub const EMBEDDING_DIMENSIONS: usize = 384;

/// Maximum text length for embedding (truncated if longer)
pub const MAX_TEXT_LENGTH: usize = 8192;

/// Batch size for efficient embedding generation
pub const BATCH_SIZE: usize = 32;

// ----------------------------------------------------------------------------
// Nomic task-instruction prefixes
// ----------------------------------------------------------------------------
//
// nomic-embed-text-v1.5 is trained as an *asymmetric* bi-encoder: the model
// card states the input MUST carry a task-instruction prefix, and fastembed
// does NOT add one. Documents are embedded as `search_document: …` and
// queries as `search_query: …` so the model can break query/document
// symmetry — exactly what it was fine-tuned to do.
//
// This is gated behind `VESTIGE_NOMIC_PREFIXES` and OFF by default because
// turning it on changes the embedding space: any embedding stored under the
// old (raw, prefix-less) regime is NOT comparable to a query embedded with a
// prefix. Enabling it is therefore a coupled operation — flip the flag AND
// re-embed the store with the `regenerate_embeddings` MCP tool. The
// `embedding_model` column records which regime each vector was built under
// (`nomic-embed-text-v1.5` vs `…+prefix`) so the mismatch is observable.

/// Prefix applied to query text when prefixes are enabled.
pub const NOMIC_QUERY_PREFIX: &str = "search_query: ";

/// Prefix applied to stored/indexed document text when prefixes are enabled.
pub const NOMIC_DOCUMENT_PREFIX: &str = "search_document: ";

/// Which side of the asymmetric bi-encoder a piece of text represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedTask {
    /// Content being stored/indexed (`search_document:`).
    Document,
    /// A retrieval query (`search_query:`).
    Query,
}

impl EmbedTask {
    /// The prefix to prepend for this task, or `""` when prefixes are disabled.
    fn prefix(self) -> &'static str {
        if !nomic_prefixes_enabled() {
            return "";
        }
        match self {
            EmbedTask::Document => NOMIC_DOCUMENT_PREFIX,
            EmbedTask::Query => NOMIC_QUERY_PREFIX,
        }
    }
}

/// Parse a boolean-ish environment value.
///
/// Truthy: `1`, `true`, `yes`, `on` (case-insensitive, surrounding whitespace
/// ignored). Anything else — including an unset or empty value — is false.
#[must_use]
pub fn is_truthy_env_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Read a boolean-ish environment variable (see [`is_truthy_env_value`]).
fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| is_truthy_env_value(&v))
        .unwrap_or(false)
}

/// Whether Nomic task prefixes are enabled (env `VESTIGE_NOMIC_PREFIXES`).
///
/// Read once and cached for the process lifetime so the regime cannot drift
/// mid-run (which would corrupt the query cache). Truthy values: `1`, `true`,
/// `yes`, `on` (case-insensitive). Default: `false`.
#[must_use]
pub fn nomic_prefixes_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_truthy("VESTIGE_NOMIC_PREFIXES"))
}

// ----------------------------------------------------------------------------
// Deterministic offline test embedder
// ----------------------------------------------------------------------------
//
// `VESTIGE_TEST_MOCK_EMBEDDINGS` is documented in CONTRIBUTING.md and
// docs/CONFIGURATION.md as the switch that lets the test suite exercise the
// semantic path without downloading the ~547 MB ONNX model. It is read HERE —
// this is the only place in the codebase that may consult it, so the switch can
// never silently change behaviour in code that is not about embedding.

/// Env var that replaces the ONNX model with [`mock_embedding`].
pub const MOCK_EMBEDDINGS_ENV: &str = "VESTIGE_TEST_MOCK_EMBEDDINGS";

/// `model` name reported by [`EmbeddingService::model_name`] while the mock
/// embedder is active.
///
/// Deliberately *not* wired into [`embedding_model_tag`]: that tag describes the
/// Nomic prefix regime persisted alongside vectors and is asserted by tests that
/// know nothing about the mock switch, so coupling the two would make those
/// tests fail whenever the mock is enabled.
pub const MOCK_EMBEDDING_MODEL_TAG: &str = "vestige-test-mock-embedder";

/// Test-only override of the mock switch.
///
/// Unit tests must not enable the mock through the environment: `set_var` races
/// with every concurrent reader in the process, so a stray observation from an
/// unrelated test would silently swap the real model for a hash. Tests that need
/// the mock hold [`MockEmbeddingsOverride`] instead, which touches only this
/// atomic. `0` = defer to the environment, `1` = force on.
#[cfg(test)]
static MOCK_OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// RAII guard forcing the mock embedder on for the duration of a unit test.
#[cfg(test)]
pub(crate) struct MockEmbeddingsOverride;

#[cfg(test)]
impl MockEmbeddingsOverride {
    pub(crate) fn on() -> Self {
        MOCK_OVERRIDE.store(1, std::sync::atomic::Ordering::Relaxed);
        Self
    }
}

#[cfg(test)]
impl Drop for MockEmbeddingsOverride {
    fn drop(&mut self) {
        MOCK_OVERRIDE.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Whether the deterministic mock embedder is enabled.
///
/// Reads `VESTIGE_TEST_MOCK_EMBEDDINGS` (documented in CONTRIBUTING.md and
/// docs/CONFIGURATION.md) on every call — `true`, `1`, `yes` or `on`. It is
/// deliberately **not** cached, so a process cannot be stuck in one mode for its
/// lifetime; the read costs orders of magnitude less than the ONNX forward pass
/// it replaces.
#[must_use]
pub fn mock_embeddings_enabled() -> bool {
    #[cfg(test)]
    {
        if MOCK_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) == 1 {
            return true;
        }
    }
    env_truthy(MOCK_EMBEDDINGS_ENV)
}

/// FNV-1a 64-bit hash — tiny, stable across runs/platforms, no dependencies.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Deterministic stand-in for the ONNX forward pass.
///
/// Contract relied upon by `tests/e2e` and by the unit tests below:
///
/// 1. **Deterministic** — same input, same vector, on every run, machine and
///    call site (no RNG, no clock, no I/O). `embed`, `embed_query` and
///    `embed_batch` all funnel through here.
/// 2. **Distinct** — different inputs produce different vectors, so
///    "identical text matches, different text does not" assertions are real.
/// 3. **Shaped like the real thing** — exactly [`EMBEDDING_DIMENSIONS`] values
///    and L2-normalized, so cosine similarity, HNSW indexing and RRF behave as
///    they do in production.
/// 4. **Lexically meaningful** — texts that share tokens score a higher cosine
///    similarity than disjoint texts, so ranking assertions in tests are not
///    degenerate coin flips. The effect is deliberately modest: disjoint
///    boilerplate stays far below the 0.85 consolidation dedup cutoff.
#[must_use]
pub fn mock_embedding(text: &str) -> Embedding {
    /// Weight of one token occurrence in its hashed dimension.
    const TOKEN_WEIGHT: f32 = 1.0;
    /// Amplitude of the per-content jitter that separates texts sharing a
    /// token bag. Small enough that shared tokens dominate the ranking.
    const CONTENT_JITTER: f32 = 0.1;

    let mut vector = vec![0.0_f32; EMBEDDING_DIMENSIONS];

    // 1. Token signal: every token bumps one signed dimension, so two texts
    //    sharing vocabulary (in any order) point in a similar direction.
    for token in text.split(|c: char| !c.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        let hash = fnv1a64(token.to_lowercase().as_bytes());
        let slot = (hash % EMBEDDING_DIMENSIONS as u64) as usize;
        let sign = if hash & (1 << 63) == 0 { 1.0 } else { -1.0 };
        vector[slot] += sign * TOKEN_WEIGHT;
    }

    // 2. Content signal: a hash-seeded xorshift64* drawn over every dimension
    //    keeps two different texts apart even when their token bags match
    //    (e.g. "a b" vs "b a") and keeps the vector non-zero for one token.
    let mut state = fnv1a64(text.trim().as_bytes()) | 1;
    for slot in &mut vector {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let rand = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        // 53-bit mantissa → uniform in [0, 1).
        let unit = ((rand >> 11) as f64 / (1u64 << 53) as f64) as f32;
        *slot += (unit - 0.5) * CONTENT_JITTER;
    }

    // Already `EMBEDDING_DIMENSIONS` long, so this only L2-normalizes.
    Embedding::new(matryoshka_truncate(vector))
}

/// The `embedding_model` tag to persist alongside each vector, encoding the
/// active prefix regime so a later read can detect a raw-vs-prefixed mismatch.
#[must_use]
pub fn embedding_model_tag() -> &'static str {
    if nomic_prefixes_enabled() {
        "nomic-embed-text-v1.5+prefix"
    } else {
        "nomic-embed-text-v1.5"
    }
}

// ============================================================================
// GLOBAL MODEL (with Mutex for fastembed v5 API)
// ============================================================================
//
// Architecture note: A single model instance behind Mutex serializes all
// embedding operations. This is acceptable because:
//  1. All callers use `tokio::task::spawn_blocking`, so contention happens
//     on the blocking thread pool — not the async runtime.
//  2. fastembed's `TextEmbedding` loads the ONNX model into memory (~200MB).
//     Multiple instances would multiply memory usage with no shared weights.
//
// If throughput becomes a bottleneck (profiled, not assumed), consider:
//  - A pool of 2-3 instances with `try_lock` round-robin (2-3× memory).
//  - Moving to a dedicated embedding microservice.
//  - Batching multiple embed requests into a single model call.

/// Result type for model initialization
static EMBEDDING_MODEL_RESULT: OnceLock<Result<Mutex<TextEmbedding>, String>> = OnceLock::new();

/// Get the default cache directory for fastembed models.
///
/// Resolution order:
/// 1. `FASTEMBED_CACHE_PATH` env var (explicit override)
/// 2. Platform cache dir via `directories::ProjectDirs::from("", "vestige", "vestige")`
///    - Linux:   `$XDG_CACHE_HOME/vestige/fastembed` (typically `~/.cache/vestige/fastembed`)
///    - macOS:   `~/Library/Caches/vestige.vestige/fastembed`
///    - Windows: `%LOCALAPPDATA%\vestige\vestige\cache\fastembed`
/// 3. `~/.cache/vestige/fastembed` (home-dir fallback)
/// 4. `.fastembed_cache` relative to CWD (absolute last resort, should never trigger)
///
/// `qualifier=""` keeps the project_path collapsed to `<organization>.<application>`
/// on macOS (`vestige.vestige`) and to `<application>` on Linux (`vestige`) — both
/// shorter than the data-dir identifier `com.vestige.core`. The cache identifier is
/// intentionally separate from the data identifier so a model upgrade does not
/// invalidate the user's memory database.
pub(crate) fn get_cache_dir() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("FASTEMBED_CACHE_PATH") {
        return std::path::PathBuf::from(path);
    }

    if let Some(proj_dirs) = directories::ProjectDirs::from("", "vestige", "vestige") {
        return proj_dirs.cache_dir().join("fastembed");
    }

    // Fallback to home directory
    if let Some(base_dirs) = directories::BaseDirs::new() {
        return base_dirs.home_dir().join(".cache/vestige/fastembed");
    }

    // Last resort fallback (shouldn't happen in practice)
    std::path::PathBuf::from(".fastembed_cache")
}

/// Initialize the global embedding model
/// Using nomic-embed-text-v1.5 (768d) - 8192 token context, Matryoshka support
fn get_model() -> Result<std::sync::MutexGuard<'static, TextEmbedding>, EmbeddingError> {
    let result = EMBEDDING_MODEL_RESULT.get_or_init(|| {
        // Get cache directory (respects FASTEMBED_CACHE_PATH env var)
        let cache_dir = get_cache_dir();

        // Create cache directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!("Failed to create cache directory {:?}: {}", cache_dir, e);
        }

        // nomic-embed-text-v1.5: 768 dimensions, 8192 token context
        // Matryoshka representation learning, fully open source
        let options = InitOptions::new(EmbeddingModel::NomicEmbedTextV15)
            .with_show_download_progress(true)
            .with_cache_dir(cache_dir);

        TextEmbedding::try_new(options)
            .map(Mutex::new)
            .map_err(|e| {
                format!(
                    "Failed to initialize nomic-embed-text-v1.5 embedding model: {}. \
                    Ensure ONNX runtime is available and model files can be downloaded.",
                    e
                )
            })
    });

    match result {
        Ok(model) => model
            .lock()
            .map_err(|e| EmbeddingError::ModelInit(format!("Lock poisoned: {}", e))),
        Err(err) => Err(EmbeddingError::ModelInit(err.clone())),
    }
}

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Embedding error types
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EmbeddingError {
    /// Failed to initialize the embedding model
    ModelInit(String),
    /// Failed to generate embedding
    EmbeddingFailed(String),
    /// Invalid input (empty, too long, etc.)
    InvalidInput(String),
}

impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbeddingError::ModelInit(e) => write!(f, "Model initialization failed: {}", e),
            EmbeddingError::EmbeddingFailed(e) => write!(f, "Embedding generation failed: {}", e),
            EmbeddingError::InvalidInput(e) => write!(f, "Invalid input: {}", e),
        }
    }
}

impl std::error::Error for EmbeddingError {}

// ============================================================================
// EMBEDDING TYPE
// ============================================================================

/// A semantic embedding vector
#[derive(Debug, Clone)]
pub struct Embedding {
    /// The embedding vector
    pub vector: Vec<f32>,
    /// Dimensions of the vector
    pub dimensions: usize,
}

impl Embedding {
    /// Create a new embedding from a vector
    pub fn new(vector: Vec<f32>) -> Self {
        let dimensions = vector.len();
        Self { vector, dimensions }
    }

    /// Compute cosine similarity with another embedding
    pub fn cosine_similarity(&self, other: &Embedding) -> f32 {
        if self.dimensions != other.dimensions {
            return 0.0;
        }
        cosine_similarity(&self.vector, &other.vector)
    }

    /// Compute Euclidean distance with another embedding
    pub fn euclidean_distance(&self, other: &Embedding) -> f32 {
        if self.dimensions != other.dimensions {
            return f32::MAX;
        }
        euclidean_distance(&self.vector, &other.vector)
    }

    /// Normalize the embedding vector to unit length
    pub fn normalize(&mut self) {
        let norm = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut self.vector {
                *x /= norm;
            }
        }
    }

    /// Check if the embedding is normalized (unit length)
    pub fn is_normalized(&self) -> bool {
        let norm = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        (norm - 1.0).abs() < 0.001
    }

    /// Convert to bytes for storage
    pub fn to_bytes(&self) -> Vec<u8> {
        self.vector.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(4) {
            return None;
        }
        let vector: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        Some(Self::new(vector))
    }
}

// ============================================================================
// EMBEDDING SERVICE
// ============================================================================

/// Service for generating and managing embeddings
pub struct EmbeddingService {
    _unused: (),
}

impl Default for EmbeddingService {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingService {
    /// Create a new embedding service
    pub fn new() -> Self {
        Self { _unused: () }
    }

    /// Check if the model is ready
    pub fn is_ready(&self) -> bool {
        if mock_embeddings_enabled() {
            // The mock embedder is pure computation — it cannot fail to load.
            return true;
        }
        match get_model() {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("Embedding model not ready: {}", e);
                false
            }
        }
    }

    /// Check if the model is ready and return the error if not
    pub fn check_ready(&self) -> Result<(), EmbeddingError> {
        if mock_embeddings_enabled() {
            return Ok(());
        }
        get_model().map(|_| ())
    }

    /// Initialize the model (downloads if necessary)
    pub fn init(&self) -> Result<(), EmbeddingError> {
        if mock_embeddings_enabled() {
            // No download, no ONNX session: the switch exists precisely so
            // tests (and CI) never depend on either.
            return Ok(());
        }
        let _model = get_model()?; // Ensures model is loaded and returns any init errors
        Ok(())
    }

    /// Get the model name
    pub fn model_name(&self) -> &'static str {
        if mock_embeddings_enabled() {
            MOCK_EMBEDDING_MODEL_TAG
        } else {
            #[cfg(feature = "nomic-v2")]
            {
                "nomic-ai/nomic-embed-text-v2-moe"
            }
            #[cfg(not(feature = "nomic-v2"))]
            {
                "nomic-ai/nomic-embed-text-v1.5"
            }
        }
    }

    /// Get the embedding dimensions
    pub fn dimensions(&self) -> usize {
        EMBEDDING_DIMENSIONS
    }

    /// Generate embedding for a single piece of stored/indexed content.
    ///
    /// Applies the `search_document:` prefix when prefixes are enabled. Most
    /// call sites (ingest, regeneration) want this; queries must use
    /// [`Self::embed_query`] so the asymmetric bi-encoder works as trained.
    pub fn embed(&self, text: &str) -> Result<Embedding, EmbeddingError> {
        self.embed_task(text, EmbedTask::Document)
    }

    /// Generate embedding for a retrieval query.
    ///
    /// Applies the `search_query:` prefix when prefixes are enabled. Pairs
    /// with [`Self::embed`] for documents.
    pub fn embed_query(&self, text: &str) -> Result<Embedding, EmbeddingError> {
        self.embed_task(text, EmbedTask::Query)
    }

    /// Generate an embedding for `text` under the given [`EmbedTask`].
    ///
    /// The task prefix is prepended *before* length truncation so the prefix
    /// is never the thing that gets clipped. The empty-input guard checks the
    /// caller's text, not the prefixed form.
    pub fn embed_task(&self, text: &str, task: EmbedTask) -> Result<Embedding, EmbeddingError> {
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput(
                "Text cannot be empty".to_string(),
            ));
        }

        let prefixed = apply_prefix(task, text);

        // Truncate if too long (char-boundary safe)
        let input = if prefixed.len() > MAX_TEXT_LENGTH {
            let mut end = MAX_TEXT_LENGTH;
            while !prefixed.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            &prefixed[..end]
        } else {
            &prefixed
        };

        // Checked before `get_model()` so an enabled mock never loads (or
        // downloads) the ONNX model, not even lazily.
        if mock_embeddings_enabled() {
            return Ok(mock_embedding(input));
        }

        let mut model = get_model()?;

        let embeddings = model
            .embed(vec![input], None)
            .map_err(|e| EmbeddingError::EmbeddingFailed(e.to_string()))?;

        if embeddings.is_empty() {
            return Err(EmbeddingError::EmbeddingFailed(
                "No embedding generated".to_string(),
            ));
        }

        Ok(Embedding::new(matryoshka_truncate(embeddings[0].clone())))
    }

    /// Generate embeddings for multiple texts (batch processing)
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mock = mock_embeddings_enabled();
        let mut all_embeddings = Vec::with_capacity(texts.len());

        // Process in batches for efficiency. Batch embedding is only used for
        // stored content (ingest/regeneration), so every item is a document.
        for chunk in texts.chunks(BATCH_SIZE) {
            // Prefix first, then truncate, so the prefix is never clipped.
            let prefixed: Vec<String> = chunk
                .iter()
                .map(|t| apply_prefix(EmbedTask::Document, t))
                .collect();

            let truncated: Vec<String> = prefixed
                .iter()
                .map(|t| {
                    if t.len() > MAX_TEXT_LENGTH {
                        let mut end = MAX_TEXT_LENGTH;
                        while !t.is_char_boundary(end) && end > 0 {
                            end -= 1;
                        }
                        t[..end].to_string()
                    } else {
                        t.clone()
                    }
                })
                .collect();

            // Mock path never touches the model: same vectors, no ONNX, and
            // identical to what `embed` returns for the same text.
            if mock {
                all_embeddings.extend(truncated.iter().map(|t| mock_embedding(t)));
                continue;
            }

            let mut model = get_model()?;
            let embeddings = model
                .embed(
                    truncated.iter().map(String::as_str).collect::<Vec<_>>(),
                    None,
                )
                .map_err(|e| EmbeddingError::EmbeddingFailed(e.to_string()))?;

            for emb in embeddings {
                all_embeddings.push(Embedding::new(matryoshka_truncate(emb)));
            }
        }

        Ok(all_embeddings)
    }

    /// Find most similar embeddings to a query
    pub fn find_similar(
        &self,
        query_embedding: &Embedding,
        candidate_embeddings: &[Embedding],
        top_k: usize,
    ) -> Vec<(usize, f32)> {
        let mut similarities: Vec<(usize, f32)> = candidate_embeddings
            .iter()
            .enumerate()
            .map(|(i, emb)| (i, query_embedding.cosine_similarity(emb)))
            .collect();

        // Sort by similarity (highest first)
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        similarities.into_iter().take(top_k).collect()
    }
}

// ============================================================================
// SIMILARITY FUNCTIONS
// ============================================================================

/// Prepend the Nomic task prefix to `text` for the given task.
///
/// Returns an owned `String`. When prefixes are disabled (the default) this is
/// just `text.to_string()`, so the only cost in the common path is one
/// allocation — negligible next to the ONNX forward pass.
#[inline]
fn apply_prefix(task: EmbedTask, text: &str) -> String {
    let prefix = task.prefix();
    if prefix.is_empty() {
        text.to_string()
    } else {
        format!("{prefix}{text}")
    }
}

/// Apply Matryoshka truncation: truncate to EMBEDDING_DIMENSIONS and L2-normalize
///
/// Nomic Embed v1.5 supports Matryoshka Representation Learning, meaning the
/// first N dimensions of the 768-dim output ARE a valid N-dimensional
/// embedding with minimal quality loss. We truncate to 384 dims, which costs
/// ~1% MTEB versus the full 768 (the Nomic MRL card reports ~0.5–1% for 384
/// vs ~2% for 256 — we picked 384 as the best quality/size knee for HNSW
/// memory footprint). The L2-renormalisation below is what makes the
/// truncated prefix a unit vector again so cosine similarity comparisons
/// against full-dim queries stay correct.
#[inline]
pub fn matryoshka_truncate(mut vector: Vec<f32>) -> Vec<f32> {
    if vector.len() > EMBEDDING_DIMENSIONS {
        vector.truncate(EMBEDDING_DIMENSIONS);
    }
    // L2-normalize the truncated vector
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut vector {
            *x /= norm;
        }
    }
    vector
}

/// Compute cosine similarity between two vectors
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let mut dot_product = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot_product += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denominator = (norm_a * norm_b).sqrt();
    if denominator > 0.0 {
        dot_product / denominator
    } else {
        0.0
    }
}

/// Compute Euclidean distance between two vectors
#[inline]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Compute dot product between two vectors
#[inline]
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_distance_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let dist = euclidean_distance(&a, &b);
        assert!(dist.abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let dist = euclidean_distance(&a, &b);
        assert!((dist - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_embedding_to_from_bytes() {
        let original = Embedding::new(vec![1.5, 2.5, 3.5, 4.5]);
        let bytes = original.to_bytes();
        let restored = Embedding::from_bytes(&bytes).unwrap();

        assert_eq!(original.vector.len(), restored.vector.len());
        for (a, b) in original.vector.iter().zip(restored.vector.iter()) {
            assert!((a - b).abs() < 0.0001);
        }
    }

    #[test]
    fn test_embedding_normalize() {
        let mut emb = Embedding::new(vec![3.0, 4.0]);
        emb.normalize();

        // Should be unit length
        assert!(emb.is_normalized());

        // Components should be 0.6 and 0.8 (3/5 and 4/5)
        assert!((emb.vector[0] - 0.6).abs() < 0.0001);
        assert!((emb.vector[1] - 0.8).abs() < 0.0001);
    }

    #[test]
    fn test_find_similar() {
        let service = EmbeddingService::new();

        let query = Embedding::new(vec![1.0, 0.0, 0.0]);
        let candidates = vec![
            Embedding::new(vec![1.0, 0.0, 0.0]),  // Most similar
            Embedding::new(vec![0.7, 0.7, 0.0]),  // Somewhat similar
            Embedding::new(vec![0.0, 1.0, 0.0]),  // Orthogonal
            Embedding::new(vec![-1.0, 0.0, 0.0]), // Opposite
        ];

        let results = service.find_similar(&query, &candidates, 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 0); // First candidate should be most similar
        assert!((results[0].1 - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_embedding_dimensions_constant_is_384() {
        assert_eq!(
            EMBEDDING_DIMENSIONS, 384,
            "EMBEDDING_DIMENSIONS should be 384 after Matryoshka upgrade"
        );
    }

    #[test]
    fn test_matryoshka_truncate_768_to_384() {
        let full_768: Vec<f32> = (0..768).map(|i| (i as f32 * 0.01).sin()).collect();
        let truncated = matryoshka_truncate(full_768.clone());

        assert_eq!(truncated.len(), 384);

        // Result should be L2-normalized
        let norm: f32 = truncated.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.001,
            "Matryoshka truncation should L2-normalize the output"
        );

        // The truncated vector's direction should match the first 384 of the original
        let prefix = &full_768[..384];
        let prefix_norm: f32 = prefix.iter().map(|x| x * x).sum::<f32>().sqrt();
        let cos_sim: f32 = truncated
            .iter()
            .zip(prefix.iter())
            .map(|(a, b)| a * b / prefix_norm)
            .sum();
        assert!(
            (cos_sim - 1.0).abs() < 0.01,
            "Truncated vector should point in same direction as the prefix"
        );
    }

    #[test]
    fn test_matryoshka_truncate_already_384() {
        let exact: Vec<f32> = (0..384).map(|i| (i as f32 * 0.01).cos()).collect();
        let result = matryoshka_truncate(exact.clone());

        assert_eq!(result.len(), 384);
        // Still L2-normalized even if already the right size
        let norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_matryoshka_truncate_smaller_than_384_passes_through() {
        let small: Vec<f32> = vec![3.0, 4.0];
        let result = matryoshka_truncate(small);

        assert_eq!(
            result.len(),
            2,
            "Vectors smaller than 384 should not be extended"
        );
        // Should still be normalized
        let norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn nomic_prefix_constants_match_model_card() {
        // The exact strings the nomic-embed-text-v1.5 model card mandates.
        assert_eq!(NOMIC_QUERY_PREFIX, "search_query: ");
        assert_eq!(NOMIC_DOCUMENT_PREFIX, "search_document: ");
    }

    #[test]
    fn nomic_prefixes_default_off() {
        // Unit-test process does not set VESTIGE_NOMIC_PREFIXES, so the regime
        // is the backward-compatible raw one and the model tag is unchanged.
        assert!(!nomic_prefixes_enabled());
        assert_eq!(embedding_model_tag(), "nomic-embed-text-v1.5");
    }

    #[test]
    fn apply_prefix_is_noop_when_disabled() {
        // With prefixes off (default), document and query text pass through
        // unchanged — guarantees byte-for-byte backward compatibility.
        assert_eq!(
            apply_prefix(EmbedTask::Document, "hello world"),
            "hello world"
        );
        assert_eq!(apply_prefix(EmbedTask::Query, "hello world"), "hello world");
    }

    #[test]
    fn test_embedding_from_bytes_roundtrip_384() {
        let original = Embedding::new((0..384).map(|i| (i as f32 * 0.001).sin()).collect());
        let bytes = original.to_bytes();
        assert_eq!(bytes.len(), 384 * 4, "384 f32s should be 1536 bytes");

        let restored = Embedding::from_bytes(&bytes).unwrap();
        assert_eq!(restored.dimensions, 384);
        for (a, b) in original.vector.iter().zip(restored.vector.iter()) {
            assert!((a - b).abs() < 1e-7);
        }
    }

    // ========================================================================
    // VESTIGE_TEST_MOCK_EMBEDDINGS
    // ========================================================================
    //
    // The mock is enabled through `MockEmbeddingsOverride` rather than
    // `std::env::set_var`: the environment is process-wide and `set_var` is
    // `unsafe` in Rust 2024 precisely because it races with concurrent readers,
    // so an unrelated test could observe the flag and silently get hash vectors
    // instead of the real model. The override touches one atomic and is undone
    // by `Drop`, even on panic. The *environment* path is covered end to end by
    // `tests/e2e/tests/mcp` (the spawned server reports
    // `embeddingServiceReady: true` and `hasEmbedding: true` with no ONNX model
    // available, which is only possible if it read the variable).

    #[test]
    fn is_truthy_env_value_accepts_only_documented_spellings() {
        for value in ["1", "true", "TRUE", " yes ", "On"] {
            assert!(is_truthy_env_value(value), "{value:?} must be truthy");
        }
        for value in ["", " ", "0", "false", "no", "off", "2", "maybe"] {
            assert!(!is_truthy_env_value(value), "{value:?} must be falsy");
        }
    }

    /// Pin the documented variable name: CONTRIBUTING.md, docs/CONFIGURATION.md
    /// and the CI workflow all refer to this exact string.
    #[test]
    fn mock_embeddings_env_var_name_is_pinned() {
        assert_eq!(MOCK_EMBEDDINGS_ENV, "VESTIGE_TEST_MOCK_EMBEDDINGS");
    }

    /// The switch must produce a deterministic, normalized, text-specific
    /// vector of the documented width — and must do it without ONNX, which is
    /// what lets the E2E suite cover the semantic path with no model download.
    #[test]
    fn mock_embeddings_are_deterministic_normalized_and_text_specific() {
        let _guard = MockEmbeddingsOverride::on();
        assert!(
            mock_embeddings_enabled(),
            "guard must have enabled the mock embedder"
        );

        let service = EmbeddingService::new();

        // Readiness/init must short-circuit the model: no ONNX session, no
        // ~547 MB download, no network.
        assert!(service.is_ready(), "mock embedder is always ready");
        assert!(service.check_ready().is_ok());
        assert!(service.init().is_ok());

        let text_a = "Rust enforces memory safety without a garbage collector";
        let text_b = "Sourdough needs a twelve hour cold ferment";

        let first = service.embed(text_a).expect("mock embed must succeed");
        let second = service
            .embed(text_a)
            .expect("mock embed must be repeatable");
        let other = service.embed(text_b).expect("mock embed must succeed");

        // 1. Dimension contract.
        assert_eq!(first.dimensions, EMBEDDING_DIMENSIONS);
        assert_eq!(first.vector.len(), EMBEDDING_DIMENSIONS);
        assert_eq!(other.vector.len(), EMBEDDING_DIMENSIONS);

        // 2. Normalized like the real Matryoshka output.
        assert!(
            first.is_normalized(),
            "mock vectors must be L2-normalized, norm was off"
        );

        // 3. Identical text → identical vector, bit for bit.
        assert_eq!(
            first.vector, second.vector,
            "identical text must produce an identical vector"
        );

        // 4. Different text → different vector (guards against a constant mock
        //    that would make every "semantic" assertion vacuous).
        assert_ne!(
            first.vector, other.vector,
            "different text must produce a different vector"
        );
        assert!(
            first.cosine_similarity(&other) < 0.5,
            "unrelated texts must not look similar, got {}",
            first.cosine_similarity(&other)
        );

        // 5. Batch and single paths agree — both funnel into `mock_embedding`.
        let batch = service
            .embed_batch(&[text_a, text_b])
            .expect("mock batch embed must succeed");
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].vector, first.vector);
        assert_eq!(batch[1].vector, other.vector);

        // 6. The active "model" is identifiable while the mock is on. The
        //    persisted `embedding_model_tag` deliberately stays on the Nomic
        //    regime (see its docs): tests that assert it know nothing about the
        //    mock switch and must not depend on whether it is enabled.
        assert_eq!(service.model_name(), MOCK_EMBEDDING_MODEL_TAG);
        assert_ne!(embedding_model_tag(), MOCK_EMBEDDING_MODEL_TAG);
    }

    /// Pure-function property (no env var needed): shared vocabulary must rank
    /// higher than disjoint vocabulary, while staying below the 0.85 dedup
    /// cutoff used by consolidation — otherwise enabling the mock would make
    /// unrelated memories merge.
    #[test]
    fn mock_embedding_rewards_shared_vocabulary() {
        let base = mock_embedding("consolidation never deletes low retention memories");
        let paraphrase =
            mock_embedding("low retention memories are never deleted by consolidation");
        let unrelated = mock_embedding("sourdough starter hydration ratios for bread");

        let shared = base.cosine_similarity(&paraphrase);
        let disjoint = base.cosine_similarity(&unrelated);

        assert!(
            shared > disjoint,
            "shared vocabulary must outrank disjoint text: {shared} vs {disjoint}"
        );
        assert!(
            shared < 0.85,
            "mock similarity must stay below the consolidation dedup cutoff, got {shared}"
        );
        assert!(
            base.is_normalized(),
            "mock_embedding must always return a unit vector"
        );
    }
}

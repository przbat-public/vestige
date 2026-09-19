//! Semantic Embeddings Module
//!
//! Provides local embedding generation using fastembed (ONNX-based).
//! No external API calls required - 100% local and private.
//!
//! Supports:
//! - Text embedding generation (768-dimensional vectors via nomic-embed-text-v1.5)
//! - Cosine similarity computation
//! - Batch embedding for efficiency
//! - Hybrid multi-model fusion (future)

mod code;
mod hybrid;
mod local;

pub(crate) use local::get_cache_dir;
pub use local::{
    BATCH_SIZE, EMBEDDING_DIMENSIONS, EmbedTask, Embedding, EmbeddingError, EmbeddingService,
    MAX_TEXT_LENGTH, MOCK_EMBEDDING_MODEL_TAG, MOCK_EMBEDDINGS_ENV, NOMIC_DOCUMENT_PREFIX,
    NOMIC_QUERY_PREFIX, cosine_similarity, dot_product, embedding_model_tag, euclidean_distance,
    is_truthy_env_value, matryoshka_truncate, mock_embedding, mock_embeddings_enabled,
    nomic_prefixes_enabled,
};

pub use code::CodeEmbedding;
pub use hybrid::HybridEmbedding;

//! Search Module
//!
//! Provides high-performance search capabilities:
//! - Vector search using HNSW (USearch)
//! - Keyword search using BM25/FTS5
//! - Hybrid search with RRF fusion
//! - Temporal-aware search
//! - Reranking for precision

pub mod decompose;
mod hybrid;
pub mod hyde;
mod keyword;
pub mod late_interaction;
mod mmr;
mod reranker;
mod temporal;
mod vector;

pub use vector::{
    DEFAULT_CONNECTIVITY, DEFAULT_DIMENSIONS, VectorIndex, VectorIndexConfig, VectorIndexStats,
    VectorSearchError,
};

pub use keyword::{KeywordSearcher, sanitize_fts5_query};

pub use hybrid::{HybridSearchConfig, HybridSearcher, linear_combination, reciprocal_rank_fusion};

// Maximal Marginal Relevance — diversity-aware final selection for multi-hop synthesis
pub use mmr::mmr_select;

pub use temporal::TemporalSearcher;

// Reranking for +15-20% precision
pub use reranker::{
    DEFAULT_RERANK_COUNT, DEFAULT_RETRIEVAL_COUNT, RerankedResult, Reranker, RerankerConfig,
    RerankerError,
};

// v2.0: HyDE-inspired query expansion for improved semantic search
pub use hyde::{QueryIntent, centroid_embedding, classify_intent, expand_query};

// ColBERT late-interaction reranking (Pattern 1: rescore top-K via MaxSim).
// Scoring + reranker are pure (always compiled); the ONNX embedder is gated.
#[cfg(feature = "late-interaction")]
pub use late_interaction::{ColbertConfig, ColbertEmbedder, ColbertError};
pub use late_interaction::{
    LateRanked, TokenEmbedder, late_interaction_enabled, maxsim, maxsim_normalized,
    rerank_late_interaction,
};

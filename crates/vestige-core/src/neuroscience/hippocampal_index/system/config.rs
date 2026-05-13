//! Configuration knobs for the hippocampal indexing engine.

use super::super::index::INDEX_EMBEDDING_DIM;

/// Configuration for the hippocampal index
#[derive(Debug, Clone)]
pub struct HippocampalIndexConfig {
    /// Dimension for semantic summaries (compressed embedding)
    pub summary_dimensions: usize,
    /// Minimum link strength to keep
    pub link_prune_threshold: f32,
    /// Days before "recently created" flag is cleared
    pub recently_created_days: u32,
    /// Access count threshold for "frequently accessed" flag
    pub frequently_accessed_threshold: u32,
    /// Weights for combined score calculation
    pub semantic_weight: f32,
    pub text_weight: f32,
    pub temporal_weight: f32,
    pub importance_weight: f32,
}

impl Default for HippocampalIndexConfig {
    fn default() -> Self {
        Self {
            summary_dimensions: INDEX_EMBEDDING_DIM, // 128 vs 384 for full
            link_prune_threshold: 0.1,
            recently_created_days: 7,
            frequently_accessed_threshold: 10,
            semantic_weight: 0.5,
            text_weight: 0.2,
            temporal_weight: 0.15,
            importance_weight: 0.15,
        }
    }
}

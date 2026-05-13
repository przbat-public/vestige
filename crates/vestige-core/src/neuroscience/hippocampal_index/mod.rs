//! # Hippocampal Indexing Theory Implementation
//!
//! Based on Teyler and Rudy's (2007) indexing theory: The hippocampus stores
//! INDICES (pointers), not content. Content is distributed across neocortex.
//!
//! ## Theory Background
//!
//! Just as the hippocampus creates sparse, orthogonal representations that serve
//! as indices to cortical memories, this system separates:
//!
//! - **Index Layer**: Compact, searchable, in-memory (like hippocampus)
//! - **Content Layer**: Detailed, distributed storage (like neocortex)
//!
//! ## Two-Phase Retrieval
//!
//! 1. **Phase 1 (Hippocampal)**: Fast search over compact indices
//!    - Semantic summary embeddings (compressed)
//!    - Temporal markers
//!    - Importance flags
//!
//! 2. **Phase 2 (Neocortical)**: Full content retrieval
//!    - Follow content pointers
//!    - Retrieve from appropriate storage
//!    - Reconstruct full memory
//!
//! ## Module Layout (split from a 2.3K-line monolith)
//!
//! - `types`  — `HippocampalIndexError`, `MemoryBarcode`, `BarcodeGenerator`,
//!   `TemporalMarker`, `ImportanceFlags`, `ContentType`, `StorageLocation`,
//!   `ContentPointer`, `IndexLink`, `AssociationLinkType`
//! - `index`  — `MemoryIndex`, `IndexQuery`, `IndexMatch`, `FullMemory`
//! - `store`  — `ContentStore` (the "neocortical" backing store)
//! - `system` — `HippocampalIndexConfig`, `HippocampalIndex`, migration helpers
//!
//! Public surface is preserved through `pub use` re-exports.
//!
//! ## References
//!
//! - Teyler, T. J., & Rudy, J. W. (2007). The hippocampal indexing theory and
//!   episodic memory: Updating the index. Hippocampus, 17(12), 1158-1169.
//! - McClelland, J. L., McNaughton, B. L., & O'Reilly, R. C. (1995).
//!   Why there are complementary learning systems in the hippocampus and neocortex.

// Note: When using with the embeddings feature, cosine_similarity
// and EMBEDDING_DIMENSIONS can be imported from crate::embeddings

mod index;
mod store;
mod system;
mod types;

pub use index::{FullMemory, INDEX_EMBEDDING_DIM, IndexMatch, IndexQuery, MemoryIndex};
pub use store::ContentStore;
pub use system::{
    HippocampalIndex, HippocampalIndexConfig, HippocampalIndexStats, MigrationNode, MigrationResult,
};
pub use types::{
    AssociationLinkType, BarcodeGenerator, ContentPointer, ContentType, HippocampalIndexError,
    ImportanceFlags, IndexLink, MemoryBarcode, Result, StorageLocation, TemporalMarker,
};

#[cfg(test)]
mod tests;

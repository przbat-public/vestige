//! Cognitive search pipeline split by responsibility.
//!
//! The pipeline runs sequentially through three phases, each in its own
//! submodule. The flow matches `ARCHITECTURE.md`:
//!
//! ```text
//! retrieval  →  scoring  →  finalize
//!     │           │            │
//!     │           │            └── stages 6, 7, format, token budget, metacognition
//!     │           └── stages 3..5G (every score adjustment)
//!     └── stages 0, 1, 2, 2B (gate, hybrid+decompose, rerank, dedup)
//! ```
//!
//! Splitting the original 886-line `execute.rs` into three phase modules
//! keeps each file under 400 LOC. The engine lock is taken by *waiting*
//! (`lock().await`) in every stage: a stage that skipped itself on a busy lock
//! made the ranking depend on lock timing, so the same query could return two
//! different orders.

use serde_json::Value;

use vestige_core::SearchResult;

pub(in crate::tools::search_unified) mod finalize;
pub(in crate::tools::search_unified) mod retrieval;
pub(in crate::tools::search_unified) mod scoring;

/// Static configuration derived from validated `SearchArgs` once, then
/// passed by reference through every phase. Keeping this immutable
/// removes a class of "did stage N mutate the limit?" bugs.
#[derive(Debug, Clone, Copy)]
pub(super) struct PipelineConfig {
    pub detail_level: &'static str,
    pub retrieval_mode: &'static str,
    pub limit: i32,
    pub min_retention: f64,
    pub min_similarity: f32,
}

/// Output of the retrieval phase — the candidate set and the dedup count.
pub(super) struct RetrievalOutput {
    pub results: Vec<SearchResult>,
    pub dedup_removed: usize,
}

/// Output of the scoring phase — adjusted candidate set plus diagnostic
/// counters that the finalize phase surfaces in the response payload.
pub(super) struct ScoringOutput {
    pub results: Vec<SearchResult>,
    pub suppressed_count: usize,
    pub prune_removed: usize,
    pub reinstatement_info: Option<Value>,
    pub associations: Vec<Value>,
}

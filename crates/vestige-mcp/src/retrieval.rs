//! Feature-aware retrieval facade for the no-embeddings build.
//!
//! `--no-default-features` (the "Build (no embeddings)" recipe in `AGENTS.md`,
//! and the feature set `release.yml` shipped for `x86_64-apple-darwin`) removes
//! `vestige_core::embeddings` and `vestige_core::search`, and with them
//! `Storage::hybrid_search` and `Storage::get_node_embedding`. Every retrieval
//! call site therefore needs two shapes: the hybrid path, and a keyword-only
//! path that keeps the tool answering on FTS5 alone.
//!
//! This module is that fork, in one place. The forks used to be written per
//! call site (`tools::precompute::fetch_sources`,
//! `search_unified::pipeline::scoring`), and the sites that never got one made
//! the whole configuration fail to compile: `cargo check -p vestige-mcp
//! --no-default-features` reported 17 errors, so the documented mode had never
//! actually been built. One facade means the next `hybrid_search` caller cannot
//! reintroduce that.
//!
//! The keyword-only path is a *documented* reduction rather than a silent one:
//! `docs/CONFIGURATION.md` describes this configuration as "keyword-only
//! search", and `main` warns once at startup about exactly which retrieval
//! stages are missing.

use vestige_core::{SearchResult, Storage};
// Only the keyword-only wrapper constructs a `MatchType`; the hybrid path
// returns whatever `Storage::hybrid_search` built.
#[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
use vestige_core::MatchType;

/// Whether this binary can embed a query and search the HNSW vector index.
///
/// Both features are required, mirroring the gate on `Storage::hybrid_search`
/// in `vestige-core` — `embeddings` alone gives an embedder with no index to
/// search, `vector-search` alone gives an index with no vectors to put in it.
pub const SEMANTIC_RETRIEVAL: bool = cfg!(all(feature = "embeddings", feature = "vector-search"));

/// Retrieval entry point used by every tool and dashboard handler.
///
/// With `embeddings` + `vector-search` this is `Storage::hybrid_search`
/// verbatim (RRF fusion of BM25 and HNSW cosine, then the three-signal
/// recency/importance/relevance rerank). Without them it is FTS5 keyword
/// search, so the caller keeps a working tool instead of a compile error.
pub fn hybrid_search(
    storage: &Storage,
    query: &str,
    limit: i32,
    keyword_weight: f32,
    semantic_weight: f32,
) -> vestige_core::Result<Vec<SearchResult>> {
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        storage.hybrid_search(query, limit, keyword_weight, semantic_weight)
    }
    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        // Weights are meaningless on the keyword-only path; name them so a
        // future reader does not think one of the two channels is silently
        // weighted to zero.
        let _ = (keyword_weight, semantic_weight);
        let nodes = storage.keyword_search(query, limit, 0.0)?;
        Ok(nodes
            .into_iter()
            .enumerate()
            .map(|(rank, node)| keyword_only_result(node, rank))
            .collect())
    }
}

/// The node embedding for `node_id`, or `None` when it cannot be read.
///
/// Callers treat a missing vector as "this memory has no semantic signal"
/// (the hippocampal index and the dream engine both accept `Option<Vec<f32>>`),
/// which is exactly what a build without embedding storage can offer. Errors
/// are deliberately collapsed to `None`, matching the previous `.ok().flatten()`
/// at every call site: a read failure here must never abort a search.
pub fn node_embedding(storage: &Storage, node_id: &str) -> Option<Vec<f32>> {
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        storage.get_node_embedding(node_id).ok().flatten()
    }
    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let _ = (storage, node_id);
        None
    }
}

/// Wrap a keyword hit as a [`SearchResult`].
///
/// `combined_score` is a *rank* proxy, not a BM25 value: the public keyword
/// path (`Storage::keyword_search`) returns nodes ordered by FTS5 `rank` and
/// exposes no score, and inventing a numeric blend here would put a number on
/// the wire that means something different from the hybrid build's
/// `combined_score` (0.2*recency + 0.3*importance + 0.5*relevance). A strictly
/// decreasing function of rank keeps the FTS5 order intact through the pipeline
/// stages that re-sort by `combined_score`, and stays inside the [0, 1] range
/// the wire contract and `min_similarity` semantics assume.
#[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
fn keyword_only_result(node: vestige_core::KnowledgeNode, rank: usize) -> SearchResult {
    SearchResult {
        node,
        keyword_score: None,
        semantic_score: None,
        combined_score: 1.0 / (1.0 + rank as f32),
        match_type: MatchType::Keyword,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_retrieval_flag_matches_the_cfg_gate_on_hybrid_search() {
        // The flag and the cfg gates must not drift: a `true` flag with the
        // keyword-only body would make startup claim semantic search works.
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        assert!(SEMANTIC_RETRIEVAL);
        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        assert!(!SEMANTIC_RETRIEVAL);
    }

    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    #[test]
    fn keyword_only_scores_are_bounded_and_strictly_decreasing_in_rank() {
        let mut previous = f32::INFINITY;
        for rank in 0..50 {
            let node = vestige_core::KnowledgeNode::default();
            let result = keyword_only_result(node, rank);
            assert!((0.0..=1.0).contains(&result.combined_score));
            assert!(result.combined_score < previous);
            assert_eq!(result.match_type, MatchType::Keyword);
            previous = result.combined_score;
        }
    }
}

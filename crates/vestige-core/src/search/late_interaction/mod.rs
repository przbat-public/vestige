//! ColBERT late-interaction reranking (Khattab & Zaharia 2020).
//!
//! A second-stage reranker that rescores the top-K candidates from hybrid
//! retrieval using token-level MaxSim instead of a single pooled-vector
//! similarity. This is the "Pattern 1" deployment from the 2026 retrieval
//! literature: cheap bi-encoder HNSW recall first, expensive late interaction
//! only over the survivors.
//!
//! ## Layout
//!
//! - [`maxsim`] — the pure scoring operator (no model, always compiled, unit-
//!   tested in the default build).
//! - `colbert` — the ONNX token embedder via `ort` (feature `late-interaction`).
//!
//! The [`TokenEmbedder`] trait is the seam between the two: the reranker is
//! generic over it, so the scoring/ranking logic is testable with a mock and
//! the heavy ONNX path is swapped in only when the feature is on.
//!
//! Inert unless `VESTIGE_LATE_INTERACTION` is set — see [`late_interaction_enabled`].

mod maxsim;

#[cfg(feature = "late-interaction")]
mod colbert;

pub use maxsim::{maxsim, maxsim_normalized};

#[cfg(feature = "late-interaction")]
pub use colbert::{ColbertConfig, ColbertEmbedder, ColbertError};

use std::sync::OnceLock;

/// Whether ColBERT late-interaction reranking is enabled.
///
/// Reads `VESTIGE_LATE_INTERACTION` once and caches it. Truthy: `1`, `true`,
/// `yes`, `on` (case-insensitive). Default `false` — the existing Jina
/// cross-encoder remains the reranker until this is explicitly turned on. Even
/// when enabled, the reranker degrades to a no-op (returns candidates in input
/// order) if the model is unavailable, so a missing download never breaks search.
#[must_use]
pub fn late_interaction_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("VESTIGE_LATE_INTERACTION")
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

/// Produces ColBERT-style per-token vectors. Implemented by the ONNX embedder
/// behind the `late-interaction` feature; mocked in tests.
///
/// Queries and documents are embedded **differently** (ColBERT prepends a `[Q]`
/// vs `[D]` marker and pads queries with `[MASK]` to a fixed length), so the
/// two methods are distinct on purpose — do not collapse them.
pub trait TokenEmbedder {
    /// Per-token vectors for a query (L2-normalized rows).
    fn embed_query_tokens(&self, text: &str) -> Result<Vec<Vec<f32>>, String>;
    /// Per-token vectors for a stored document (L2-normalized rows).
    fn embed_document_tokens(&self, text: &str) -> Result<Vec<Vec<f32>>, String>;
}

/// A candidate after late-interaction rescoring.
#[derive(Debug, Clone)]
pub struct LateRanked<T> {
    /// The original item, carried through unchanged.
    pub item: T,
    /// MaxSim score (higher is more relevant).
    pub score: f32,
    /// Index of this item in the input list, before reranking.
    pub original_rank: usize,
}

/// Rerank `candidates` (paired `(item, text)`) by ColBERT MaxSim against
/// `query`, returning the top `top_k` by descending score.
///
/// The query is embedded once; each candidate document is embedded and scored
/// with [`maxsim`]. A candidate whose document fails to embed keeps a
/// `f32::NEG_INFINITY` score so it sinks to the bottom rather than poisoning the
/// whole call. If the query itself fails to embed, the error propagates so the
/// caller can fall back to the prior ranking.
pub fn rerank_late_interaction<T, E>(
    embedder: &E,
    query: &str,
    candidates: Vec<(T, String)>,
    top_k: usize,
) -> Result<Vec<LateRanked<T>>, String>
where
    E: TokenEmbedder + ?Sized,
{
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let query_tokens = embedder.embed_query_tokens(query)?;

    let mut ranked: Vec<LateRanked<T>> = candidates
        .into_iter()
        .enumerate()
        .map(|(rank, (item, text))| {
            let score = match embedder.embed_document_tokens(&text) {
                Ok(doc_tokens) => maxsim(&query_tokens, &doc_tokens),
                Err(_) => f32::NEG_INFINITY,
            };
            LateRanked {
                item,
                score,
                original_rank: rank,
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.truncate(top_k);
    Ok(ranked)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock embedder: turns each whitespace token into a one-hot-ish vector
    /// keyed by its first byte, so MaxSim rewards lexical token overlap. Enough
    /// to exercise the ranking/sorting/error logic without ONNX.
    struct MockEmbedder;

    fn onehot(byte: u8) -> Vec<f32> {
        let mut v = vec![0.0; 256];
        v[byte as usize] = 1.0;
        v
    }

    impl TokenEmbedder for MockEmbedder {
        fn embed_query_tokens(&self, text: &str) -> Result<Vec<Vec<f32>>, String> {
            if text == "FAIL" {
                return Err("forced query failure".into());
            }
            Ok(text
                .split_whitespace()
                .filter_map(|w| w.bytes().next())
                .map(onehot)
                .collect())
        }
        fn embed_document_tokens(&self, text: &str) -> Result<Vec<Vec<f32>>, String> {
            if text == "DOC_FAIL" {
                return Err("forced doc failure".into());
            }
            self.embed_query_tokens(text)
        }
    }

    #[test]
    fn ranks_higher_token_overlap_first() {
        let candidates = vec![
            (1, "apple banana cherry".to_string()),   // overlaps "apple"
            (2, "xenon yak zebra".to_string()),       // no overlap
            (3, "apple apricot avocado".to_string()), // overlaps "apple" + a*
        ];
        let out = rerank_late_interaction(&MockEmbedder, "apple avocado", candidates, 3).unwrap();
        // Candidate 3 shares both query-token first-bytes ('a' for both) the
        // most; candidate 2 (no 'a'/'a') ranks last.
        assert_eq!(out.len(), 3);
        assert_eq!(out[2].item, 2, "non-overlapping doc must rank last");
        assert!(out[0].score >= out[1].score && out[1].score >= out[2].score);
    }

    #[test]
    fn truncates_to_top_k() {
        let candidates = vec![
            (1, "a".to_string()),
            (2, "b".to_string()),
            (3, "c".to_string()),
        ];
        let out = rerank_late_interaction(&MockEmbedder, "a", candidates, 2).unwrap();
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn failed_document_sinks_to_bottom() {
        let candidates = vec![(1, "DOC_FAIL".to_string()), (2, "apple".to_string())];
        let out = rerank_late_interaction(&MockEmbedder, "apple", candidates, 2).unwrap();
        assert_eq!(out[0].item, 2, "embeddable doc beats the failed one");
        assert_eq!(out[1].item, 1);
        assert_eq!(out[1].score, f32::NEG_INFINITY);
    }

    #[test]
    fn query_failure_propagates() {
        let candidates = vec![(1, "apple".to_string())];
        assert!(rerank_late_interaction(&MockEmbedder, "FAIL", candidates, 1).is_err());
    }

    #[test]
    fn empty_candidates_is_ok() {
        let out = rerank_late_interaction(&MockEmbedder, "q", Vec::<(i32, String)>::new(), 5);
        assert!(out.unwrap().is_empty());
    }
}

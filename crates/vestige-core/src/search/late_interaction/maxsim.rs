//! MaxSim — the late-interaction scoring operator (Khattab & Zaharia 2020).
//!
//! ColBERT represents a query and a document each as a *set* of per-token
//! vectors instead of one pooled vector. Relevance is the sum, over query
//! tokens, of the maximum similarity that token achieves against any document
//! token:
//!
//! ```text
//! score(q, d) = Σ_{i ∈ q} max_{j ∈ d} (q_i · d_j)
//! ```
//!
//! Token vectors are expected to be L2-normalized (the ColBERT ONNX embedder
//! normalizes them), so the per-pair similarity is a plain dot product. This
//! module is pure arithmetic — no model, no ONNX — so it is unit-tested in
//! isolation and shared by the reranker.

/// Dot product of two equal-length vectors. Tokens are all the same width
/// (128 for ColBERTv2), so a length mismatch is a programming error; we clamp
/// to the shorter slice rather than panic to keep the hot path branch-free.
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Raw MaxSim score: `Σ_q max_d (q·d)`.
///
/// For L2-normalized inputs the result lies in `[-query.len(), query.len()]`
/// (typically `[0, query.len()]` in practice). Returns `0.0` when either side
/// is empty. Use [`maxsim_normalized`] when a length-stable score is needed.
#[must_use]
pub fn maxsim(query: &[Vec<f32>], doc: &[Vec<f32>]) -> f32 {
    if query.is_empty() || doc.is_empty() {
        return 0.0;
    }
    query
        .iter()
        .map(|q| {
            doc.iter()
                .map(|d| dot(q, d))
                .fold(f32::NEG_INFINITY, f32::max)
        })
        .sum()
}

/// MaxSim divided by the query token count → mean max-similarity per query
/// token, in `[-1, 1]` for normalized inputs. Length-stable, so it can feed a
/// fixed threshold or be compared across queries of different lengths.
#[must_use]
pub fn maxsim_normalized(query: &[Vec<f32>], doc: &[Vec<f32>]) -> f32 {
    if query.is_empty() {
        return 0.0;
    }
    maxsim(query, doc) / query.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[f32]) -> Vec<f32> {
        xs.to_vec()
    }

    #[test]
    fn identical_single_token_scores_one() {
        let q = vec![v(&[1.0, 0.0, 0.0])];
        let d = vec![v(&[1.0, 0.0, 0.0])];
        assert!((maxsim(&q, &d) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_tokens_score_zero() {
        let q = vec![v(&[1.0, 0.0, 0.0])];
        let d = vec![v(&[0.0, 1.0, 0.0])];
        assert!(maxsim(&q, &d).abs() < 1e-6);
    }

    #[test]
    fn each_query_token_takes_its_best_doc_token() {
        // Two query tokens, each matching a different doc token exactly.
        let q = vec![v(&[1.0, 0.0, 0.0]), v(&[0.0, 1.0, 0.0])];
        let d = vec![
            v(&[1.0, 0.0, 0.0]), // best for q0 (dot 1.0)
            v(&[0.0, 1.0, 0.0]), // best for q1 (dot 1.0)
            v(&[0.0, 0.0, 1.0]), // irrelevant
        ];
        // Sum of the two per-query maxima = 1.0 + 1.0 = 2.0.
        assert!((maxsim(&q, &d) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn picks_the_maximum_not_the_sum_over_doc() {
        // One query token; two doc tokens with different similarity. MaxSim
        // must take the MAX (0.9), not the sum (0.9 + 0.1).
        let q = vec![v(&[1.0, 0.0])];
        let d = vec![v(&[0.9, 0.435_889_9]), v(&[0.1, 0.994_987_4])];
        let score = maxsim(&q, &d);
        assert!((score - 0.9).abs() < 1e-3, "expected ~0.9, got {score}");
    }

    #[test]
    fn empty_sides_score_zero() {
        assert_eq!(maxsim(&[], &[v(&[1.0])]), 0.0);
        assert_eq!(maxsim(&[v(&[1.0])], &[]), 0.0);
        assert_eq!(maxsim_normalized(&[], &[v(&[1.0])]), 0.0);
    }

    #[test]
    fn normalized_divides_by_query_length() {
        let q = vec![v(&[1.0, 0.0, 0.0]), v(&[0.0, 1.0, 0.0])];
        let d = vec![v(&[1.0, 0.0, 0.0]), v(&[0.0, 1.0, 0.0])];
        // raw = 2.0, normalized = 2.0 / 2 tokens = 1.0
        assert!((maxsim(&q, &d) - 2.0).abs() < 1e-6);
        assert!((maxsim_normalized(&q, &d) - 1.0).abs() < 1e-6);
    }
}

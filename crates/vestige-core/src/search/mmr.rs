//! Maximal Marginal Relevance (Carbonell & Goldstein 1998).
//!
//! ## Why this exists
//!
//! Multi-hop questions are Vestige's weakest retrieval category. The failure
//! mode is *not* "gold evidence missing from top-K" (recall is high) but
//! "cannot synthesize across multiple memories": a pure relevance ranking
//! tends to fill the context window with K near-duplicate memories about the
//! single most-relevant fact, starving the answerer of the *other* facts a
//! multi-hop answer must combine.
//!
//! MMR fixes the *evidence set*, not the ranking model. It greedily builds the
//! final selection to balance relevance against novelty:
//!
//! ```text
//! next = argmax_{d ∉ S} [ λ·rel(d) − (1−λ)·max_{s ∈ S} sim(d, s) ]
//! ```
//!
//! With `λ = 1.0` this is pure relevance (identity reranking); lower `λ`
//! trades relevance for diversity. This module is pure arithmetic and generic
//! over the similarity function, so it is unit-tested in isolation and reused
//! by the search pipeline (where `sim` is content overlap).

/// Greedily select up to `top_k` items by Maximal Marginal Relevance.
///
/// `items` are `(value, relevance)` pairs; `relevance` is min-max normalized
/// internally to `[0, 1]` so `λ` balances it against `similarity` (also
/// expected in `[0, 1]`). `similarity(a, b)` is the redundancy penalty between
/// two candidates. Input order is treated as the relevance tie-breaker.
///
/// Returns the selected values in MMR order. `lambda` is clamped to `[0, 1]`.
pub fn mmr_select<T>(
    items: Vec<(T, f32)>,
    similarity: impl Fn(&T, &T) -> f32,
    lambda: f32,
    top_k: usize,
) -> Vec<T> {
    let n = items.len();
    if n == 0 || top_k == 0 {
        return Vec::new();
    }
    let lambda = lambda.clamp(0.0, 1.0);

    // Min-max normalize relevance so it shares the [0,1] scale with similarity.
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for (_, r) in &items {
        lo = lo.min(*r);
        hi = hi.max(*r);
    }
    let span = hi - lo;
    let norm_rel: Vec<f32> = items
        .iter()
        .map(|(_, r)| if span > 0.0 { (r - lo) / span } else { 1.0 })
        .collect();

    let values: Vec<T> = items.into_iter().map(|(v, _)| v).collect();
    let limit = top_k.min(n);
    let mut selected: Vec<usize> = Vec::with_capacity(limit);
    let mut remaining: Vec<usize> = (0..n).collect();
    // Running maximum similarity of each candidate to the already-selected set, updated
    // only for the item that was just picked. Recomputing it from `selected` every round
    // costs O(n·k²) `similarity` calls; the greedy step only ever consumes the maximum, so
    // each pair needs to be scored once — O(n·k). The callers pass a content-overlap
    // closure that builds two HashSets per call, so the saving is allocation-shaped:
    // at n = k = 100 it is 166_650 -> 4_950 calls (333_300 -> 9_900 HashSet builds).
    // `f32::max` is associative and commutative, so the cached maximum is bit-identical
    // to the per-round fold it replaces.
    let mut max_sim: Vec<f32> = vec![0.0; n];

    while selected.len() < limit && !remaining.is_empty() {
        let mut best_pos = 0usize;
        let mut best_score = f32::NEG_INFINITY;

        for (pos, &cand) in remaining.iter().enumerate() {
            let mmr = lambda * norm_rel[cand] - (1.0 - lambda) * max_sim[cand];
            // `>` (not `>=`) keeps the earlier, higher-relevance candidate on ties.
            if mmr > best_score {
                best_score = mmr;
                best_pos = pos;
            }
        }

        let picked = remaining.swap_remove(best_pos);
        selected.push(picked);
        for &cand in &remaining {
            max_sim[cand] = max_sim[cand].max(similarity(&values[cand], &values[picked]));
        }
    }

    // Materialize selected values in MMR order. Walk originals once, emitting in
    // the order `selected` records, without cloning T.
    let mut keep_rank = vec![usize::MAX; n];
    for (rank, &idx) in selected.iter().enumerate() {
        keep_rank[idx] = rank;
    }
    let mut out: Vec<(usize, T)> = values
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let r = keep_rank[i];
            (r != usize::MAX).then_some((r, v))
        })
        .collect();
    out.sort_by_key(|(r, _)| *r);
    out.into_iter().map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Similarity: 1.0 if same first char, else 0.0 — a crude "same topic" proxy.
    fn first_char_sim(a: &&str, b: &&str) -> f32 {
        match (a.chars().next(), b.chars().next()) {
            (Some(x), Some(y)) if x == y => 1.0,
            _ => 0.0,
        }
    }

    #[test]
    fn lambda_one_is_pure_relevance_order() {
        let items = vec![("apple", 0.9), ("apricot", 0.8), ("banana", 0.7)];
        let out = mmr_select(items, first_char_sim, 1.0, 3);
        assert_eq!(out, vec!["apple", "apricot", "banana"]);
    }

    #[test]
    fn diversity_breaks_up_redundant_top_hits() {
        // Two highly-relevant same-topic items ('a') + one distinct lower-relevance ('b').
        // Pure relevance would pick both 'a's first; MMR with diversity should
        // pick one 'a' then the distinct 'b' before the redundant second 'a'.
        let items = vec![("apple", 0.95), ("apricot", 0.90), ("banana", 0.60)];
        let out = mmr_select(items, first_char_sim, 0.5, 2);
        assert_eq!(out[0], "apple", "most relevant first");
        assert_eq!(
            out[1], "banana",
            "diverse second beats redundant same-topic"
        );
    }

    #[test]
    fn respects_top_k() {
        let items = vec![("a", 0.9), ("b", 0.8), ("c", 0.7), ("d", 0.6)];
        let out = mmr_select(items, first_char_sim, 0.7, 2);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn empty_or_zero_k_returns_empty() {
        assert!(mmr_select(Vec::<(&str, f32)>::new(), first_char_sim, 0.7, 5).is_empty());
        assert!(mmr_select(vec![("a", 1.0)], first_char_sim, 0.7, 0).is_empty());
    }

    #[test]
    fn single_item_passes_through() {
        let out = mmr_select(vec![("only", 0.5)], first_char_sim, 0.7, 5);
        assert_eq!(out, vec!["only"]);
    }

    /// The pre-cache implementation: recomputes the maximum similarity of every candidate
    /// from scratch on every round. Kept as the behavioural oracle for the cached version.
    fn recomputing_mmr<T: Clone>(
        items: Vec<(T, f32)>,
        similarity: impl Fn(&T, &T) -> f32,
        lambda: f32,
        top_k: usize,
    ) -> Vec<T> {
        let n = items.len();
        let lambda = lambda.clamp(0.0, 1.0);
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for (_, r) in &items {
            lo = lo.min(*r);
            hi = hi.max(*r);
        }
        let span = hi - lo;
        let norm_rel: Vec<f32> = items
            .iter()
            .map(|(_, r)| if span > 0.0 { (r - lo) / span } else { 1.0 })
            .collect();
        let values: Vec<T> = items.into_iter().map(|(v, _)| v).collect();

        let mut selected: Vec<usize> = Vec::new();
        let mut remaining: Vec<usize> = (0..n).collect();
        while selected.len() < top_k.min(n) && !remaining.is_empty() {
            let mut best_pos = 0usize;
            let mut best_score = f32::NEG_INFINITY;
            for (pos, &cand) in remaining.iter().enumerate() {
                let max_sim = selected
                    .iter()
                    .map(|&s| similarity(&values[cand], &values[s]))
                    .fold(0.0_f32, f32::max);
                let mmr = lambda * norm_rel[cand] - (1.0 - lambda) * max_sim;
                if mmr > best_score {
                    best_score = mmr;
                    best_pos = pos;
                }
            }
            selected.push(remaining.swap_remove(best_pos));
        }

        let mut keep_rank = vec![usize::MAX; n];
        for (rank, &idx) in selected.iter().enumerate() {
            keep_rank[idx] = rank;
        }
        let mut out: Vec<(usize, T)> = values
            .into_iter()
            .enumerate()
            .filter_map(|(i, v)| (keep_rank[i] != usize::MAX).then_some((keep_rank[i], v)))
            .collect();
        out.sort_by_key(|(r, _)| *r);
        out.into_iter().map(|(_, v)| v).collect()
    }

    #[test]
    fn cached_max_similarity_changes_neither_the_ranking_nor_the_call_count_class() {
        // The greedy loop used to rebuild "max similarity to the selected set" for every
        // candidate on every round: O(n·k²) similarity calls, each one a pair of HashSets
        // in the search pipeline. The cache must score each pair exactly once (O(n·k)) and
        // return the identical selection, including tie-breaks.
        use std::cell::Cell;

        let n = 16usize;
        let items: Vec<(usize, f32)> = (0..n).map(|i| (i, 1.0 - i as f32 * 0.01)).collect();
        let topic = |v: &usize| v % 3;
        let calls = Cell::new(0usize);

        let out = {
            let counting = |a: &usize, b: &usize| {
                calls.set(calls.get() + 1);
                if topic(a) == topic(b) { 1.0 } else { 0.0 }
            };
            mmr_select(items.clone(), counting, 0.5, n)
        };

        let reference = recomputing_mmr(
            items,
            |a: &usize, b: &usize| if topic(a) == topic(b) { 1.0 } else { 0.0 },
            0.5,
            n,
        );
        assert_eq!(out, reference, "caching must not move a single item");

        // 680 calls before (sum over rounds r of (n-r)*r), 120 after (sum of n-1-r).
        assert_eq!(
            calls.get(),
            (0..n).map(|r| n - 1 - r).sum::<usize>(),
            "each candidate pair must be scored once"
        );
    }
}

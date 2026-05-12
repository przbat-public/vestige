//! Hybrid Search (Keyword + Semantic + RRF)
//!
//! Combines keyword (BM25/FTS5) and semantic (embedding) search
//! using Reciprocal Rank Fusion for optimal results.

use std::collections::HashMap;

// ============================================================================
// FUSION ALGORITHMS
// ============================================================================

/// Reciprocal Rank Fusion for combining search results
///
/// Combines keyword (BM25) and semantic search results using the RRF formula:
/// score(d) = sum of 1/(k + rank(d)) across all result lists
///
/// RRF is effective because:
/// - It normalizes across different scoring scales
/// - It rewards items appearing in multiple result lists
/// - The k parameter (typically 60) dampens the effect of high ranks
///
/// # Arguments
/// * `keyword_results` - Results from keyword search (id, score)
/// * `semantic_results` - Results from semantic search (id, score)
/// * `k` - Fusion constant (default 60.0)
///
/// # Returns
/// Combined results sorted by RRF score
pub fn reciprocal_rank_fusion(
    keyword_results: &[(String, f32)],
    semantic_results: &[(String, f32)],
    k: f32,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    // Add keyword search scores
    for (rank, (key, _)) in keyword_results.iter().enumerate() {
        *scores.entry(key.clone()).or_default() += 1.0 / (k + rank as f32);
    }

    // Add semantic search scores
    for (rank, (key, _)) in semantic_results.iter().enumerate() {
        *scores.entry(key.clone()).or_default() += 1.0 / (k + rank as f32);
    }

    // Sort by combined score
    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    results
}

/// Linear combination of search results with weights
///
/// Combines results using weighted sum of normalized scores.
/// Good when you have prior knowledge about relative importance.
///
/// # Arguments
/// * `keyword_results` - Results from keyword search
/// * `semantic_results` - Results from semantic search
/// * `keyword_weight` - Weight for keyword results (0.0 to 1.0)
/// * `semantic_weight` - Weight for semantic results (0.0 to 1.0)
pub fn linear_combination(
    keyword_results: &[(String, f32)],
    semantic_results: &[(String, f32)],
    keyword_weight: f32,
    semantic_weight: f32,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    // Normalize and add keyword search scores
    let max_keyword = keyword_results
        .first()
        .map(|(_, s)| *s)
        .unwrap_or(1.0)
        .max(0.001);
    for (key, score) in keyword_results {
        *scores.entry(key.clone()).or_default() += (score / max_keyword) * keyword_weight;
    }

    // Normalize and add semantic search scores
    let max_semantic = semantic_results
        .first()
        .map(|(_, s)| *s)
        .unwrap_or(1.0)
        .max(0.001);
    for (key, score) in semantic_results {
        *scores.entry(key.clone()).or_default() += (score / max_semantic) * semantic_weight;
    }

    // Sort by combined score
    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    results
}

// ============================================================================
// HYBRID SEARCH CONFIGURATION
// ============================================================================

/// Configuration for hybrid search
#[derive(Debug, Clone)]
pub struct HybridSearchConfig {
    /// Weight for keyword (BM25/FTS5) results
    pub keyword_weight: f32,
    /// Weight for semantic (embedding) results
    pub semantic_weight: f32,
    /// RRF constant (higher = more uniform weighting)
    pub rrf_k: f32,
    /// Minimum semantic similarity threshold
    pub min_semantic_similarity: f32,
    /// Number of results to fetch from each source before fusion
    pub source_limit_multiplier: usize,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            keyword_weight: 0.3,
            semantic_weight: 0.7,
            rrf_k: 60.0,
            min_semantic_similarity: 0.3,
            source_limit_multiplier: 2,
        }
    }
}

// ============================================================================
// HYBRID SEARCHER
// ============================================================================

/// Hybrid search combining keyword and semantic search
pub struct HybridSearcher {
    config: HybridSearchConfig,
}

impl Default for HybridSearcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridSearcher {
    /// Create a new hybrid searcher with default config
    pub fn new() -> Self {
        Self {
            config: HybridSearchConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: HybridSearchConfig) -> Self {
        Self { config }
    }

    /// Get current configuration
    pub fn config(&self) -> &HybridSearchConfig {
        &self.config
    }

    /// Fuse keyword and semantic results using RRF
    pub fn fuse_rrf(
        &self,
        keyword_results: &[(String, f32)],
        semantic_results: &[(String, f32)],
    ) -> Vec<(String, f32)> {
        reciprocal_rank_fusion(keyword_results, semantic_results, self.config.rrf_k)
    }

    /// Fuse results using linear combination
    pub fn fuse_linear(
        &self,
        keyword_results: &[(String, f32)],
        semantic_results: &[(String, f32)],
    ) -> Vec<(String, f32)> {
        linear_combination(
            keyword_results,
            semantic_results,
            self.config.keyword_weight,
            self.config.semantic_weight,
        )
    }

    /// Determine if semantic search should be used based on query
    ///
    /// Semantic search is more effective for:
    /// - Conceptual queries
    /// - Questions
    /// - Natural language
    ///
    /// Keyword search is more effective for:
    /// - Exact terms
    /// - Code/identifiers
    /// - Specific phrases
    pub fn should_use_semantic(&self, query: &str) -> bool {
        // Heuristics for when semantic search is useful
        let is_question = query.contains('?')
            || query.to_lowercase().starts_with("what ")
            || query.to_lowercase().starts_with("how ")
            || query.to_lowercase().starts_with("why ")
            || query.to_lowercase().starts_with("when ");

        let is_conceptual = query.split_whitespace().count() >= 3
            && !query.contains('(')
            && !query.contains('{')
            && !query.contains('=');

        is_question || is_conceptual
    }

    /// Calculate the effective limit for source queries
    pub fn effective_source_limit(&self, target_limit: usize) -> usize {
        target_limit * self.config.source_limit_multiplier
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reciprocal_rank_fusion() {
        let keyword = vec![
            ("doc-1".to_string(), 0.9),
            ("doc-2".to_string(), 0.8),
            ("doc-3".to_string(), 0.7),
        ];
        let semantic = vec![
            ("doc-2".to_string(), 0.95),
            ("doc-1".to_string(), 0.85),
            ("doc-4".to_string(), 0.75),
        ];

        let results = reciprocal_rank_fusion(&keyword, &semantic, 60.0);

        // doc-1 and doc-2 appear in both, should be at top
        assert!(results.iter().any(|(k, _)| k == "doc-1"));
        assert!(results.iter().any(|(k, _)| k == "doc-2"));

        // Results should be sorted by score descending
        for i in 1..results.len() {
            assert!(results[i - 1].1 >= results[i].1);
        }
    }

    #[test]
    fn test_linear_combination() {
        let keyword = vec![("doc-1".to_string(), 1.0), ("doc-2".to_string(), 0.5)];
        let semantic = vec![("doc-2".to_string(), 1.0), ("doc-3".to_string(), 0.5)];

        let results = linear_combination(&keyword, &semantic, 0.5, 0.5);

        // doc-2 appears in both with high scores, should be first or second
        let doc2_pos = results.iter().position(|(k, _)| k == "doc-2");
        assert!(doc2_pos.is_some());
    }

    #[test]
    fn test_hybrid_searcher() {
        let searcher = HybridSearcher::new();

        // Semantic queries
        assert!(searcher.should_use_semantic("What is the meaning of life?"));
        assert!(searcher.should_use_semantic("how does memory work"));

        // Keyword queries
        assert!(!searcher.should_use_semantic("fn main()"));
        assert!(!searcher.should_use_semantic("error"));
    }

    #[test]
    fn test_effective_source_limit() {
        let searcher = HybridSearcher::new();
        assert_eq!(searcher.effective_source_limit(10), 20);
    }

    #[test]
    fn test_rrf_with_empty_results() {
        let keyword: Vec<(String, f32)> = vec![];
        let semantic = vec![("doc-1".to_string(), 0.9)];

        let results = reciprocal_rank_fusion(&keyword, &semantic, 60.0);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "doc-1");
    }

    #[test]
    fn test_linear_with_unequal_weights() {
        let keyword = vec![("doc-1".to_string(), 1.0)];
        let semantic = vec![("doc-2".to_string(), 1.0)];

        // Heavy keyword weight
        let results = linear_combination(&keyword, &semantic, 0.9, 0.1);

        // doc-1 should have higher score
        let doc1_score = results.iter().find(|(k, _)| k == "doc-1").map(|(_, s)| *s);
        let doc2_score = results.iter().find(|(k, _)| k == "doc-2").map(|(_, s)| *s);

        assert!(doc1_score.unwrap() > doc2_score.unwrap());
    }

    // ========== RRF-specific deep tests ==========

    #[test]
    fn test_rrf_rewards_documents_in_both_lists() {
        let keyword = vec![("only-kw".to_string(), 1.0), ("both".to_string(), 0.5)];
        let semantic = vec![("only-sem".to_string(), 1.0), ("both".to_string(), 0.5)];

        let results = reciprocal_rank_fusion(&keyword, &semantic, 60.0);

        let both_score = results.iter().find(|(k, _)| k == "both").unwrap().1;
        let only_kw = results.iter().find(|(k, _)| k == "only-kw").unwrap().1;
        let only_sem = results.iter().find(|(k, _)| k == "only-sem").unwrap().1;

        assert!(
            both_score > only_kw,
            "RRF should rank 'both' higher than single-list docs"
        );
        assert!(
            both_score > only_sem,
            "RRF should rank 'both' higher than single-list docs"
        );
    }

    #[test]
    fn test_rrf_is_scale_invariant() {
        let kw_high = vec![("a".to_string(), 100.0), ("b".to_string(), 50.0)];
        let sem_low = vec![("b".to_string(), 0.01), ("a".to_string(), 0.005)];

        let results = reciprocal_rank_fusion(&kw_high, &sem_low, 60.0);

        // 'a' is rank 0 in kw and rank 1 in sem → score = 1/60 + 1/61
        // 'b' is rank 1 in kw and rank 0 in sem → score = 1/61 + 1/60
        // Both should have identical combined scores since they have symmetrical ranks.
        let a_score = results.iter().find(|(k, _)| k == "a").unwrap().1;
        let b_score = results.iter().find(|(k, _)| k == "b").unwrap().1;
        assert!(
            (a_score - b_score).abs() < 1e-6,
            "Symmetric ranks should yield equal RRF scores regardless of raw score magnitudes"
        );
    }

    #[test]
    fn test_rrf_k_parameter_affects_score_distribution() {
        let keyword = vec![
            ("top".to_string(), 1.0),
            ("mid".to_string(), 0.5),
            ("low".to_string(), 0.1),
        ];
        let semantic: Vec<(String, f32)> = vec![];

        let results_k1 = reciprocal_rank_fusion(&keyword, &semantic, 1.0);
        let results_k60 = reciprocal_rank_fusion(&keyword, &semantic, 60.0);

        // With k=1, top result gets 1/(1+0) = 1.0, second gets 1/(1+1) = 0.5
        // With k=60, top result gets 1/(60+0) ≈ 0.017, second gets 1/(60+1) ≈ 0.016
        // Low k → larger gap between ranks; high k → more uniform
        let gap_k1 = results_k1[0].1 - results_k1[1].1;
        let gap_k60 = results_k60[0].1 - results_k60[1].1;
        assert!(
            gap_k1 > gap_k60,
            "Lower k should produce larger score gaps between ranks"
        );
    }
}

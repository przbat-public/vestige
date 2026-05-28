//! Search repository for [`super::Storage`].
//!
//! Three retrieval engines plus the dispatcher:
//!
//! - **Keyword** — FTS5 BM25 over `knowledge_fts`, with FTS5-token
//!   sanitization (`crate::fts::sanitize_fts5_query`) and a retention
//!   floor.
//! - **Semantic** — query embedding (LRU-cached) against the HNSW vector
//!   index; returns nodes with cosine similarity above the threshold.
//! - **Hybrid** — runs both, fuses ranks via Reciprocal Rank Fusion
//!   (k = 60, see Cormack et al. SIGIR 2009), then reranks the top
//!   candidates with the three-signal heuristic from Park et al.
//!   "Generative Agents" (UIST 2023): recency × importance × relevance.
//!
//! [`Storage::recall`] is the public entry point; it dispatches on
//! [`SearchMode`] and unconditionally fires `strengthen_batch_on_access`
//! afterwards to capture the Testing Effect (Roediger & Karpicke 2006).

use chrono::Utc;
use rusqlite::params;

use crate::fts::sanitize_fts5_query;
use crate::memory::{KnowledgeNode, RecallInput, SearchMode};
#[cfg(all(feature = "embeddings", feature = "vector-search"))]
use crate::memory::{MatchType, SearchResult, SimilarityResult};
#[cfg(all(feature = "embeddings", feature = "vector-search"))]
use crate::search::hyde;
#[cfg(feature = "vector-search")]
use crate::search::reciprocal_rank_fusion;

use super::{Result, Storage, StorageError};

/// Default weight applied to the *keyword* (FTS5/BM25) channel when blending
/// keyword and semantic scores inside [`Storage::hybrid_search`]. Tuned for
/// general-purpose recall over LOCOMO-style conversational memory in
/// May 2026 — paraphrase-heavy data benefits from a semantic-leaning blend.
///
/// Override at runtime with the env var `VESTIGE_HYBRID_KEYWORD_WEIGHT`.
pub const DEFAULT_HYBRID_KEYWORD_WEIGHT: f32 = 0.3;

/// Default weight applied to the *semantic* (HNSW cosine) channel. See
/// [`DEFAULT_HYBRID_KEYWORD_WEIGHT`]. Override with
/// `VESTIGE_HYBRID_SEMANTIC_WEIGHT`.
pub const DEFAULT_HYBRID_SEMANTIC_WEIGHT: f32 = 0.7;

/// Resolve the active hybrid-search weights, honouring runtime overrides.
///
/// Reads `VESTIGE_HYBRID_KEYWORD_WEIGHT` / `VESTIGE_HYBRID_SEMANTIC_WEIGHT`
/// from the environment once per call (cheap; not on a hot loop). Returns
/// `(keyword_weight, semantic_weight)`, both clamped to `[0.0, 1.0]`.
///
/// Falls back to the compiled defaults when either variable is unset or
/// fails to parse. Negative or NaN inputs are silently replaced with the
/// default to avoid feeding garbage into the rerank.
pub fn default_hybrid_weights() -> (f32, f32) {
    fn parse_clamped(var: &str, fallback: f32) -> f32 {
        match std::env::var(var) {
            Ok(raw) => raw
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0)
                .unwrap_or(fallback),
            Err(_) => fallback,
        }
    }
    (
        parse_clamped(
            "VESTIGE_HYBRID_KEYWORD_WEIGHT",
            DEFAULT_HYBRID_KEYWORD_WEIGHT,
        ),
        parse_clamped(
            "VESTIGE_HYBRID_SEMANTIC_WEIGHT",
            DEFAULT_HYBRID_SEMANTIC_WEIGHT,
        ),
    )
}

impl Storage {
    /// Recall memories matching a query
    pub fn recall(&self, input: RecallInput) -> Result<Vec<KnowledgeNode>> {
        let nodes = match input.search_mode {
            SearchMode::Keyword => {
                self.keyword_search(&input.query, input.limit, input.min_retention)?
            }
            #[cfg(all(feature = "embeddings", feature = "vector-search"))]
            SearchMode::Semantic => {
                let results = self.semantic_search(&input.query, input.limit, 0.3)?;
                results.into_iter().map(|r| r.node).collect()
            }
            #[cfg(all(feature = "embeddings", feature = "vector-search"))]
            SearchMode::Hybrid => {
                let (kw, sem) = default_hybrid_weights();
                let results = self.hybrid_search(&input.query, input.limit, kw, sem)?;
                results.into_iter().map(|r| r.node).collect()
            }
            #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
            _ => self.keyword_search(&input.query, input.limit, input.min_retention)?,
        };

        // Auto-strengthen memories on access (Testing Effect - Roediger & Karpicke 2006)
        // This implements "use it or lose it" - accessed memories get stronger
        let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        let _ = self.strengthen_batch_on_access(&ids); // Ignore errors, don't fail recall

        Ok(nodes)
    }

    /// Keyword search via FTS5 (public API).
    ///
    /// Returns nodes matching `query` after FTS5 sanitization, filtered by
    /// `min_retention` (0.0 disables filtering). Used by Content Intelligence
    /// Pipeline relation-edge builder (see `vestige-mcp::smart_ingest`).
    pub fn keyword_search(
        &self,
        query: &str,
        limit: i32,
        min_retention: f64,
    ) -> Result<Vec<KnowledgeNode>> {
        let sanitized_query = sanitize_fts5_query(query);

        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT n.* FROM knowledge_nodes n
             JOIN knowledge_fts fts ON n.id = fts.id
             WHERE knowledge_fts MATCH ?1
             AND n.retention_strength >= ?2
             ORDER BY n.retention_strength DESC
             LIMIT ?3",
        )?;

        let nodes = stmt.query_map(params![sanitized_query, min_retention, limit], |row| {
            Self::row_to_node(row)
        })?;

        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }

    /// Get query embedding from cache or compute it
    #[cfg(feature = "embeddings")]
    fn get_query_embedding(&self, query: &str) -> Result<Vec<f32>> {
        // Check cache first
        {
            let mut cache = self
                .query_cache
                .lock()
                .map_err(|_| StorageError::Init("Query cache lock poisoned".to_string()))?;
            if let Some(cached) = cache.get(query) {
                return Ok(cached.clone());
            }
        }

        // Not in cache, compute embedding
        let embedding = self
            .embedding_service
            .embed(query)
            .map_err(|e| StorageError::Init(format!("Failed to embed query: {}", e)))?;

        // Store in cache
        {
            let mut cache = self
                .query_cache
                .lock()
                .map_err(|_| StorageError::Init("Query cache lock poisoned".to_string()))?;
            cache.put(query.to_string(), embedding.vector.clone());
        }

        Ok(embedding.vector)
    }

    /// Semantic search
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn semantic_search(
        &self,
        query: &str,
        limit: i32,
        min_similarity: f32,
    ) -> Result<Vec<SimilarityResult>> {
        if !self.embedding_service.is_ready() {
            return Err(StorageError::Init("Embedding model not ready".to_string()));
        }

        let query_embedding = self.get_query_embedding(query)?;

        let index = self
            .vector_index
            .lock()
            .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;

        let results = index
            .search_with_threshold(&query_embedding, limit as usize, min_similarity)
            .map_err(|e| StorageError::Init(format!("Vector search failed: {}", e)))?;

        let mut similarity_results = Vec::with_capacity(results.len());

        for (node_id, similarity) in results {
            if let Some(node) = self.get_node(&node_id)? {
                similarity_results.push(SimilarityResult { node, similarity });
            }
        }

        Ok(similarity_results)
    }

    /// Hybrid keyword + semantic search.
    ///
    /// Two-stage scoring:
    /// 1. **Reciprocal Rank Fusion (k = 60)** picks the top candidates from
    ///    the union of keyword (FTS5/BM25) and semantic (HNSW) result lists.
    ///    RRF is rank-based and normalises across the incomparable raw score
    ///    scales of BM25 and cosine similarity. See Cormack et al. 2009.
    /// 2. **Weighted blending** of the *raw* keyword and semantic scores
    ///    produces the final `combined_score` carried on each result. RRF
    ///    decides *which* documents survive, the weighted blend decides
    ///    *how strongly* downstream pipeline stages (temporal, emotional,
    ///    competition) treat them.
    ///
    /// The two stages serve different purposes — RRF chooses the candidate
    /// set, the weighted blend gives the pipeline a continuous relevance
    /// signal it can multiply against. We deliberately do *not* propagate
    /// the RRF score to `combined_score` because RRF saturates close to
    /// `2/k` and would compress the dynamic range subsequent boosters need.
    ///
    /// Sensible weight ranges (see [`DEFAULT_HYBRID_KEYWORD_WEIGHT`] /
    /// [`DEFAULT_HYBRID_SEMANTIC_WEIGHT`]):
    /// - `(0.3, 0.7)` — default, biased toward semantic recall
    /// - `(0.2, 0.8)` — temporal / reflection queries (paraphrase-heavy)
    /// - `(0.4, 0.6)` — benchmark/LOCOMO (more keyword grounding)
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn hybrid_search(
        &self,
        query: &str,
        limit: i32,
        keyword_weight: f32,
        semantic_weight: f32,
    ) -> Result<Vec<SearchResult>> {
        let keyword_results = self.keyword_search_with_scores(query, limit * 2)?;

        let semantic_results = if self.embedding_service.is_ready() {
            self.semantic_search_raw(query, limit * 2)?
        } else {
            vec![]
        };

        // RRF (k=60) normalizes across incomparable score scales and
        // rewards documents appearing in both result lists.
        let combined = if !semantic_results.is_empty() {
            reciprocal_rank_fusion(&keyword_results, &semantic_results, 60.0)
        } else {
            keyword_results.clone()
        };

        // Collect top candidate IDs and scores
        let top_candidates: Vec<(String, f32)> =
            combined.into_iter().take(limit as usize).collect();
        let top_ids: Vec<String> = top_candidates.iter().map(|(id, _)| id.clone()).collect();

        // Bulk-fetch all nodes in one query instead of N get_node calls
        let bulk_nodes = self.get_nodes_bulk(&top_ids)?;
        let node_map: std::collections::HashMap<String, KnowledgeNode> =
            bulk_nodes.into_iter().map(|n| (n.id.clone(), n)).collect();

        // Build score lookup maps from keyword/semantic results
        let kw_scores: std::collections::HashMap<&str, f32> = keyword_results
            .iter()
            .map(|(id, s)| (id.as_str(), *s))
            .collect();
        let sem_scores: std::collections::HashMap<&str, f32> = semantic_results
            .iter()
            .map(|(id, s)| (id.as_str(), *s))
            .collect();

        let mut results = Vec::with_capacity(top_candidates.len());

        for (node_id, combined_score) in &top_candidates {
            if let Some(node) = node_map.get(node_id) {
                let keyword_score = kw_scores.get(node_id.as_str()).copied();
                let semantic_score = sem_scores.get(node_id.as_str()).copied();

                let match_type = match (keyword_score.is_some(), semantic_score.is_some()) {
                    (true, true) => MatchType::Both,
                    (true, false) => MatchType::Keyword,
                    (false, true) => MatchType::Semantic,
                    (false, false) => MatchType::Keyword,
                };

                let weighted_score = match (keyword_score, semantic_score) {
                    (Some(kw), Some(sem)) => kw * keyword_weight + sem * semantic_weight,
                    (Some(kw), None) => kw * keyword_weight,
                    (None, Some(sem)) => sem * semantic_weight,
                    (None, None) => *combined_score,
                };

                results.push(SearchResult {
                    node: node.clone(),
                    keyword_score,
                    semantic_score,
                    combined_score: weighted_score,
                    match_type,
                });
            }
        }

        // Bulk-fetch ACT-R activations in one query instead of N individual reads
        let activation_map: std::collections::HashMap<String, f64> = {
            let result_ids: Vec<String> = results.iter().map(|r| r.node.id.clone()).collect();
            if result_ids.is_empty() {
                std::collections::HashMap::new()
            } else {
                let reader = self.acquire_reader()?;
                let placeholders = result_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "SELECT id, COALESCE(activation, 0.0) FROM knowledge_nodes WHERE id IN ({})",
                    placeholders
                );
                let mut stmt = reader.prepare(&sql)?;
                let params: Vec<&dyn rusqlite::ToSql> = result_ids
                    .iter()
                    .map(|s| s as &dyn rusqlite::ToSql)
                    .collect();
                stmt.query_map(params.as_slice(), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect()
            }
        };

        // Three-signal reranking (Park et al. Generative Agents 2023)
        let now = Utc::now();
        for result in &mut results {
            let hours_since = (now - result.node.last_accessed).num_seconds() as f64 / 3600.0;
            let recency = 0.995_f64.powf(hours_since.max(0.0));

            let activation = activation_map.get(&result.node.id).copied().unwrap_or(0.0);
            let importance = ((activation + 2.0) / 7.0).clamp(0.0, 1.0);

            let relevance = result.combined_score as f64;

            let final_score = 0.2 * recency + 0.3 * importance + 0.5 * relevance;
            result.combined_score = final_score as f32;
        }

        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// Keyword search returning scores
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    fn keyword_search_with_scores(&self, query: &str, limit: i32) -> Result<Vec<(String, f32)>> {
        let sanitized_query = sanitize_fts5_query(query);

        let reader = self.acquire_reader()?;
        let mut stmt = reader.prepare(
            "SELECT n.id, rank FROM knowledge_nodes n
             JOIN knowledge_fts fts ON n.id = fts.id
             WHERE knowledge_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let results: Vec<(String, f32)> = stmt
            .query_map(params![sanitized_query, limit], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)? as f32))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, rank)| (id, (-rank).max(0.0)))
            .collect();

        if results.is_empty() {
            return Ok(vec![]);
        }

        let max_score = results.iter().map(|(_, s)| *s).fold(0.0_f32, f32::max);
        if max_score > 0.0 {
            Ok(results
                .into_iter()
                .map(|(id, s)| (id, s / max_score))
                .collect())
        } else {
            Ok(results)
        }
    }

    /// Semantic search with a caller-supplied query embedding.
    ///
    /// `semantic_search_raw` re-embeds the query string and, for conceptual
    /// intents, HyDE-expands it across several variants. That is the right
    /// behaviour for user queries, but for `smart_ingest` we already paid the
    /// embedding cost on `input.content` to feed the prediction-error gate —
    /// re-embedding the exact same text just to look up neighbours was
    /// doubling the work on every ingest call (and ~tripling it once HyDE
    /// fired). This variant skips the embedding step and the HyDE expansion,
    /// going straight to the vector index.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn semantic_search_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: i32,
    ) -> Result<Vec<(String, f32)>> {
        if !self.embedding_service.is_ready() {
            return Ok(vec![]);
        }
        let index = self
            .vector_index
            .lock()
            .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;
        index
            .search(query_embedding, limit as usize)
            .map_err(|e| StorageError::Init(format!("Vector search failed: {}", e)))
    }

    /// Semantic search returning (node_id, similarity_score) pairs
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn semantic_search_raw(&self, query: &str, limit: i32) -> Result<Vec<(String, f32)>> {
        if !self.embedding_service.is_ready() {
            return Ok(vec![]);
        }

        // HyDE query expansion: for conceptual queries, embed expanded variants
        // and use the centroid for broader semantic coverage
        let intent = hyde::classify_intent(query);
        let query_embedding = match intent {
            hyde::QueryIntent::Definition
            | hyde::QueryIntent::HowTo
            | hyde::QueryIntent::Reasoning
            | hyde::QueryIntent::Lookup => {
                let variants = hyde::expand_query(query);
                let embeddings: Vec<Vec<f32>> = variants
                    .iter()
                    .filter_map(|v| self.get_query_embedding(v).ok())
                    .collect();
                if embeddings.len() > 1 {
                    hyde::centroid_embedding(&embeddings)
                } else {
                    self.get_query_embedding(query)?
                }
            }
            _ => self.get_query_embedding(query)?,
        };

        let index = self
            .vector_index
            .lock()
            .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;

        index
            .search(&query_embedding, limit as usize)
            .map_err(|e| StorageError::Init(format!("Vector search failed: {}", e)))
    }
}

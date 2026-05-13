//! Engine: the public `HippocampalIndex` type plus stats.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Duration, Utc};

use super::super::index::{FullMemory, IndexMatch, IndexQuery, MemoryIndex};
use super::super::store::ContentStore;
use super::super::types::{
    AssociationLinkType, BarcodeGenerator, ContentPointer, ContentType, HippocampalIndexError,
    ImportanceFlags, IndexLink, MemoryBarcode, Result, TemporalMarker,
};
use super::config::HippocampalIndexConfig;

/// Separates memory index from content storage
///
/// Based on Teyler and Rudy's hippocampal indexing theory:
/// - Index is compact and fast to search (hippocampus)
/// - Content is detailed and stored separately (neocortex)
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Fields are accessed by sibling submodules of the cohesive `system` component (split-by-responsibility refactor); siblings have the same trust level as the parent module."
)]
pub struct HippocampalIndex {
    /// Index entries by barcode
    pub(super) indices: Arc<RwLock<HashMap<String, MemoryIndex>>>,
    /// Content store reference
    pub(super) content_store: ContentStore,
    /// Barcode generator
    pub(super) barcode_generator: Arc<RwLock<BarcodeGenerator>>,
    /// Configuration
    pub(super) config: HippocampalIndexConfig,
}

impl HippocampalIndex {
    /// Create a new hippocampal index
    pub fn new() -> Self {
        Self::with_config(HippocampalIndexConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: HippocampalIndexConfig) -> Self {
        Self {
            indices: Arc::new(RwLock::new(HashMap::new())),
            content_store: ContentStore::new(),
            barcode_generator: Arc::new(RwLock::new(BarcodeGenerator::new())),
            config,
        }
    }

    /// Set the content store
    pub fn with_content_store(mut self, store: ContentStore) -> Self {
        self.content_store = store;
        self
    }

    /// Index a new memory
    pub fn index_memory(
        &self,
        memory_id: &str,
        content: &str,
        node_type: &str,
        created_at: DateTime<Utc>,
        semantic_embedding: Option<Vec<f32>>,
    ) -> Result<MemoryBarcode> {
        // Generate barcode
        let barcode = {
            let mut generator = self
                .barcode_generator
                .write()
                .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;
            generator.generate(content, created_at)
        };

        // Create preview
        let preview: String = content.chars().take(100).collect();

        // Create index entry
        let mut index = MemoryIndex::new(
            barcode,
            memory_id.to_string(),
            node_type.to_string(),
            created_at,
            preview,
        );

        // Compress embedding if provided
        if let Some(embedding) = semantic_embedding {
            let summary = self.compress_embedding(&embedding);
            index.semantic_summary = summary;
        }

        // Set initial importance flags
        index.importance_flags.set_recently_created(true);

        // Add default content pointer (assumes SQLite storage)
        index.add_content_pointer(ContentPointer::sqlite(
            "knowledge_nodes",
            barcode.id as i64,
            ContentType::Text,
        ));

        // Store in index
        {
            let mut indices = self
                .indices
                .write()
                .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;
            indices.insert(memory_id.to_string(), index);
        }

        Ok(barcode)
    }

    /// Compress a full embedding to index dimensions
    pub(in crate::neuroscience::hippocampal_index) fn compress_embedding(
        &self,
        embedding: &[f32],
    ) -> Vec<f32> {
        if embedding.len() <= self.config.summary_dimensions {
            return embedding.to_vec();
        }

        // Simple compression: take evenly spaced samples
        // In production, would use PCA or learned compression
        let step = embedding.len() as f32 / self.config.summary_dimensions as f32;
        let mut compressed = Vec::with_capacity(self.config.summary_dimensions);

        for i in 0..self.config.summary_dimensions {
            let idx = (i as f32 * step) as usize;
            compressed.push(embedding[idx.min(embedding.len() - 1)]);
        }

        // Normalize
        let norm: f32 = compressed.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut compressed {
                *x /= norm;
            }
        }

        compressed
    }

    /// Phase 1: Fast index search (hippocampus-like)
    pub fn search_indices(&self, query: &IndexQuery) -> Result<Vec<IndexMatch>> {
        let indices = self
            .indices
            .read()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        let mut matches: Vec<IndexMatch> = Vec::new();

        for index in indices.values() {
            // Apply filters
            if !self.passes_filters(index, query) {
                continue;
            }

            let mut match_result = IndexMatch::new(index.clone());

            // Calculate semantic score
            if let Some(ref query_embedding) = query.semantic_embedding
                && !index.semantic_summary.is_empty()
            {
                let query_compressed = self.compress_embedding(query_embedding);
                match_result.semantic_score =
                    self.cosine_similarity(&query_compressed, &index.semantic_summary);

                if match_result.semantic_score < query.min_similarity {
                    continue;
                }
            }

            // Calculate text score
            if let Some(ref text_query) = query.text_query {
                match_result.text_score = self.text_match_score(text_query, &index.preview);
            }

            // Calculate temporal score (recency)
            match_result.temporal_score = self.temporal_score(&index.temporal_marker);

            // Calculate importance score
            match_result.importance_score = self.importance_score(&index.importance_flags);

            // Calculate combined score
            match_result.calculate_combined(
                self.config.semantic_weight,
                self.config.text_weight,
                self.config.temporal_weight,
                self.config.importance_weight,
            );

            matches.push(match_result);
        }

        // Sort by combined score
        matches.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply limit
        matches.truncate(query.limit);

        Ok(matches)
    }

    /// Check if an index passes query filters
    fn passes_filters(&self, index: &MemoryIndex, query: &IndexQuery) -> bool {
        // Time range filter
        if let Some((start, end)) = query.time_range
            && (index.temporal_marker.created_at < start || index.temporal_marker.created_at > end)
        {
            return false;
        }

        // Importance flags filter
        if let Some(ref required) = query.required_flags
            && !index.matches_importance(required.to_bits())
        {
            return false;
        }

        // Node type filter
        if let Some(ref types) = query.node_types
            && !types.contains(&index.node_type)
        {
            return false;
        }

        true
    }

    /// Calculate cosine similarity between two vectors
    pub(super) fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a * norm_b)
        } else {
            0.0
        }
    }

    /// Calculate text match score
    fn text_match_score(&self, query: &str, preview: &str) -> f32 {
        let query_lower = query.to_lowercase();
        let preview_lower = preview.to_lowercase();

        // Simple scoring: check for word matches
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let preview_words: Vec<&str> = preview_lower.split_whitespace().collect();

        if query_words.is_empty() {
            return 0.0;
        }

        let matches = query_words
            .iter()
            .filter(|q| preview_words.iter().any(|p| p.contains(*q)))
            .count();

        matches as f32 / query_words.len() as f32
    }

    /// Calculate temporal score (recency-based)
    fn temporal_score(&self, temporal: &TemporalMarker) -> f32 {
        let recency_days = temporal.recency_days();

        // Exponential decay with 14-day half-life
        let recency_score = 0.5_f32.powf(recency_days as f32 / 14.0);

        // Boost for frequently accessed
        let frequency_boost = if temporal.access_count > 10 {
            1.2
        } else if temporal.access_count > 5 {
            1.1
        } else {
            1.0
        };

        (recency_score * frequency_boost).min(1.0)
    }

    /// Calculate importance score from flags
    fn importance_score(&self, flags: &ImportanceFlags) -> f32 {
        let mut score = 0.0_f32;

        if flags.is_emotional() {
            score += 0.2;
        }
        if flags.is_frequently_accessed() {
            score += 0.2;
        }
        if flags.is_user_starred() {
            score += 0.25;
        }
        if flags.has_high_retention() {
            score += 0.15;
        }
        if flags.has_associations() {
            score += 0.1;
        }
        if flags.is_recently_created() {
            score += 0.1;
        }

        score.min(1.0)
    }

    /// Phase 2: Content retrieval (neocortex-like)
    pub fn retrieve_content(&self, index: &MemoryIndex) -> Result<FullMemory> {
        // For now, return a partial memory with available index data
        // Full retrieval would require integration with Storage
        Ok(FullMemory {
            barcode: index.barcode,
            memory_id: index.memory_id.clone(),
            content: index.preview.clone(), // Would retrieve full content
            node_type: index.node_type.clone(),
            created_at: index.temporal_marker.created_at,
            last_accessed: index.temporal_marker.last_accessed,
            embedding: None, // Would retrieve from vector store
            tags: Vec::new(),
            source: None,
            stability: 1.0,
            difficulty: 5.0,
            next_review: None,
            retention_strength: 1.0,
        })
    }

    /// Combined retrieval: search then retrieve
    pub fn recall(&self, query: &str, limit: usize) -> Result<Vec<FullMemory>> {
        let index_query = IndexQuery::from_text(query).with_limit(limit);
        let matches = self.search_indices(&index_query)?;

        let mut memories = Vec::with_capacity(matches.len());
        for m in matches {
            // Record access
            if let Ok(mut indices) = self.indices.write()
                && let Some(index) = indices.get_mut(&m.index.memory_id)
            {
                index.record_access();
            }

            match self.retrieve_content(&m.index) {
                Ok(memory) => memories.push(memory),
                Err(e) => {
                    tracing::warn!(
                        "Failed to retrieve content for {}: {}",
                        m.index.memory_id,
                        e
                    )
                }
            }
        }

        Ok(memories)
    }

    /// Recall with semantic embedding
    pub fn recall_semantic(
        &self,
        embedding: Vec<f32>,
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<FullMemory>> {
        let query = IndexQuery::from_embedding(embedding)
            .with_limit(limit)
            .with_min_similarity(min_similarity);

        let matches = self.search_indices(&query)?;

        let mut memories = Vec::with_capacity(matches.len());
        for m in matches {
            if let Ok(memory) = self.retrieve_content(&m.index) {
                memories.push(memory);
            }
        }

        Ok(memories)
    }

    /// Add association between memories
    pub fn add_association(
        &self,
        from_id: &str,
        to_id: &str,
        strength: f32,
        link_type: AssociationLinkType,
    ) -> Result<()> {
        let mut indices = self
            .indices
            .write()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        // Get target barcode
        let to_barcode = indices
            .get(to_id)
            .map(|i| i.barcode)
            .ok_or_else(|| HippocampalIndexError::NotFound(to_id.to_string()))?;

        // Add link to source
        if let Some(from_index) = indices.get_mut(from_id) {
            let link = IndexLink::new(to_barcode, strength, link_type);
            from_index.add_link(link);

            // Update has_associations flag
            from_index.importance_flags.set_has_associations(true);
        } else {
            return Err(HippocampalIndexError::NotFound(from_id.to_string()));
        }

        Ok(())
    }

    /// Get associated memories (spreading activation)
    pub fn get_associations(&self, memory_id: &str, depth: usize) -> Result<Vec<IndexMatch>> {
        let indices = self
            .indices
            .read()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        let source = indices
            .get(memory_id)
            .ok_or_else(|| HippocampalIndexError::NotFound(memory_id.to_string()))?;

        let mut associations = Vec::new();
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        visited.insert(memory_id.to_string());

        self.collect_associations(
            &indices,
            source,
            &mut associations,
            &mut visited,
            depth,
            1.0,
        );

        // Sort by combined score
        associations.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(associations)
    }

    /// Recursively collect associations
    #[allow(clippy::only_used_in_recursion)]
    fn collect_associations(
        &self,
        indices: &HashMap<String, MemoryIndex>,
        source: &MemoryIndex,
        associations: &mut Vec<IndexMatch>,
        visited: &mut std::collections::HashSet<String>,
        remaining_depth: usize,
        decay_factor: f32,
    ) {
        if remaining_depth == 0 {
            return;
        }

        for link in &source.association_links {
            // Find target by barcode
            if let Some((target_id, target)) = indices
                .iter()
                .find(|(_, i)| i.barcode == link.target_barcode)
            {
                if visited.contains(target_id) {
                    continue;
                }
                visited.insert(target_id.clone());

                let mut match_result = IndexMatch::new(target.clone());
                match_result.combined_score = link.strength * decay_factor;
                associations.push(match_result);

                // Recurse with decay
                self.collect_associations(
                    indices,
                    target,
                    associations,
                    visited,
                    remaining_depth - 1,
                    decay_factor * 0.7, // Decay for each hop
                );
            }
        }
    }

    /// Update importance flags for all indices
    pub fn update_importance_flags(&self) -> Result<()> {
        let mut indices = self
            .indices
            .write()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        let now = Utc::now();
        let recently_threshold = Duration::days(self.config.recently_created_days as i64);

        for index in indices.values_mut() {
            // Update recently_created flag
            let age = now - index.temporal_marker.created_at;
            index
                .importance_flags
                .set_recently_created(age < recently_threshold);

            // Update frequently_accessed flag
            index.importance_flags.set_frequently_accessed(
                index.temporal_marker.access_count >= self.config.frequently_accessed_threshold,
            );
        }

        Ok(())
    }

    /// Prune weak association links
    pub fn prune_weak_links(&self) -> Result<usize> {
        let mut indices = self
            .indices
            .write()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        let mut pruned_count = 0;
        for index in indices.values_mut() {
            let before = index.association_links.len();
            index.prune_weak_links(self.config.link_prune_threshold);
            pruned_count += before - index.association_links.len();
        }

        Ok(pruned_count)
    }

    /// Get index by memory ID
    pub fn get_index(&self, memory_id: &str) -> Result<Option<MemoryIndex>> {
        let indices = self
            .indices
            .read()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        Ok(indices.get(memory_id).cloned())
    }

    /// Remove an index
    pub fn remove_index(&self, memory_id: &str) -> Result<Option<MemoryIndex>> {
        let mut indices = self
            .indices
            .write()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        Ok(indices.remove(memory_id))
    }

    /// Get total number of indices
    pub fn len(&self) -> usize {
        self.indices.read().map(|i| i.len()).unwrap_or(0)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get statistics
    pub fn stats(&self) -> HippocampalIndexStats {
        let indices = self.indices.read().ok();
        let (cache_entries, cache_size) = self.content_store.cache_stats();

        let (total_indices, total_links, total_pointers) = indices
            .map(|i| {
                let total = i.len();
                let links: usize = i.values().map(|idx| idx.association_links.len()).sum();
                let pointers: usize = i.values().map(|idx| idx.content_pointers.len()).sum();
                (total, links, pointers)
            })
            .unwrap_or((0, 0, 0));

        HippocampalIndexStats {
            total_indices,
            total_association_links: total_links,
            total_content_pointers: total_pointers,
            cache_entries,
            cache_size_bytes: cache_size,
            index_dimensions: self.config.summary_dimensions,
        }
    }
}

impl Default for HippocampalIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for the hippocampal index
#[derive(Debug, Clone)]
pub struct HippocampalIndexStats {
    /// Total number of indices
    pub total_indices: usize,
    /// Total number of association links
    pub total_association_links: usize,
    /// Total number of content pointers
    pub total_content_pointers: usize,
    /// Number of entries in content cache
    pub cache_entries: usize,
    /// Size of content cache in bytes
    pub cache_size_bytes: usize,
    /// Index embedding dimensions
    pub index_dimensions: usize,
}

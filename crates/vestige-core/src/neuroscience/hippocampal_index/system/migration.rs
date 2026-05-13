//! Migration helpers: bulk import existing memories into the index.

use chrono::{DateTime, Utc};

use super::super::types::{
    AssociationLinkType, ContentPointer, ContentType, HippocampalIndexError, MemoryBarcode, Result,
};
use super::engine::HippocampalIndex;

/// Result of migrating existing memories to indexed format
#[derive(Debug, Clone, Default)]
pub struct MigrationResult {
    /// Number of memories successfully migrated
    pub migrated: usize,
    /// Number of memories that failed migration
    pub failed: usize,
    /// Number of memories skipped (already indexed)
    pub skipped: usize,
    /// Error messages for failures
    pub errors: Vec<String>,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl HippocampalIndex {
    /// Migrate a KnowledgeNode to indexed format
    #[allow(clippy::too_many_arguments)]
    pub fn migrate_node(
        &self,
        node_id: &str,
        content: &str,
        node_type: &str,
        created_at: DateTime<Utc>,
        embedding: Option<Vec<f32>>,
        retention_strength: f64,
        sentiment_magnitude: f64,
    ) -> Result<MemoryBarcode> {
        // Check if already indexed
        if let Ok(indices) = self.indices.read()
            && indices.contains_key(node_id)
        {
            return Err(HippocampalIndexError::MigrationError(
                "Node already indexed".to_string(),
            ));
        }

        // Create the index
        let barcode = self.index_memory(node_id, content, node_type, created_at, embedding)?;

        // Update importance flags based on existing data
        if let Ok(mut indices) = self.indices.write()
            && let Some(index) = indices.get_mut(node_id)
        {
            // Set high retention flag if applicable
            if retention_strength > 0.7 {
                index.importance_flags.set_high_retention(true);
            }

            // Set emotional flag if applicable
            if sentiment_magnitude > 0.5 {
                index.importance_flags.set_emotional(true);
            }

            // Add SQLite content pointer
            index.content_pointers.clear();
            index.add_content_pointer(ContentPointer::sqlite(
                "knowledge_nodes",
                barcode.id as i64,
                ContentType::Text,
            ));
        }

        Ok(barcode)
    }

    /// Batch migrate multiple nodes
    pub fn migrate_batch(&self, nodes: Vec<MigrationNode>) -> MigrationResult {
        let start = std::time::Instant::now();
        let mut result = MigrationResult::default();

        for node in nodes {
            match self.migrate_node(
                &node.id,
                &node.content,
                &node.node_type,
                node.created_at,
                node.embedding,
                node.retention_strength,
                node.sentiment_magnitude,
            ) {
                Ok(_) => result.migrated += 1,
                Err(HippocampalIndexError::MigrationError(msg))
                    if msg == "Node already indexed" =>
                {
                    result.skipped += 1;
                }
                Err(e) => {
                    result.failed += 1;
                    result.errors.push(format!("{}: {}", node.id, e));
                }
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Create associations from semantic similarity
    pub fn create_semantic_associations(
        &self,
        memory_id: &str,
        similarity_threshold: f32,
        max_associations: usize,
    ) -> Result<usize> {
        let indices = self
            .indices
            .read()
            .map_err(|e| HippocampalIndexError::LockError(e.to_string()))?;

        let source = indices
            .get(memory_id)
            .ok_or_else(|| HippocampalIndexError::NotFound(memory_id.to_string()))?;

        if source.semantic_summary.is_empty() {
            return Ok(0);
        }

        // Find similar memories
        let mut candidates: Vec<(String, f32)> = Vec::new();
        for (id, index) in indices.iter() {
            if id == memory_id || index.semantic_summary.is_empty() {
                continue;
            }

            let similarity =
                self.cosine_similarity(&source.semantic_summary, &index.semantic_summary);
            if similarity >= similarity_threshold {
                candidates.push((id.clone(), similarity));
            }
        }

        // Sort by similarity
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(max_associations);

        drop(indices); // Release read lock

        // Add associations
        let mut added = 0;
        for (target_id, strength) in candidates {
            if self
                .add_association(
                    memory_id,
                    &target_id,
                    strength,
                    AssociationLinkType::Semantic,
                )
                .is_ok()
            {
                added += 1;
            }
        }

        Ok(added)
    }
}

/// Node data for migration
#[derive(Debug, Clone)]
pub struct MigrationNode {
    /// Node ID
    pub id: String,
    /// Content
    pub content: String,
    /// Node type
    pub node_type: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Embedding (optional)
    pub embedding: Option<Vec<f32>>,
    /// Retention strength
    pub retention_strength: f64,
    /// Sentiment magnitude
    pub sentiment_magnitude: f64,
}

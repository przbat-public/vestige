//! Node repository — CRUD over `knowledge_nodes`.
//!
//! Carved out of the original monolithic `sqlite.rs` (4 389 LoC) so that each
//! domain lives in a focused file. This module owns the basic life-cycle of a
//! `KnowledgeNode`: creation, lookup, content/tag updates, deletion, and the
//! handful of simple list/search queries that take only an FTS or filter
//! predicate. Everything that needs FSRS scheduling, embeddings, or vector
//! search lives in sibling modules (see `mod.rs`).
//!
//! All methods extend the same `Storage` struct, so the public API is
//! preserved bit-for-bit.

use chrono::{Duration, Utc};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::fts::sanitize_fts5_query;
use crate::memory::{IngestInput, KnowledgeNode};

use super::{normalize_tags, Result, Storage, StorageError};

impl Storage {
    /// Ingest a new memory
    pub fn ingest(&self, mut input: IngestInput) -> Result<KnowledgeNode> {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();

        // Entity normalization (Cognee pattern): canonicalize tags to prevent
        // the same concept from fragmenting across different surface forms.
        // e.g. "bug-fix", "Bug Fix", "bugfix" → "bug-fix"
        input.tags = normalize_tags(&input.tags);

        let fsrs_state = self.scheduler.lock()
            .map_err(|_| StorageError::Init("Scheduler lock poisoned".into()))?
            .new_card();

        // Sentiment boost for stability
        let sentiment_boost = if input.sentiment_magnitude > 0.0 {
            1.0 + (input.sentiment_magnitude * 0.5)
        } else {
            1.0
        };

        let tags_json = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".to_string());
        let next_review = now + Duration::days(fsrs_state.scheduled_days as i64);
        let valid_from_str = input.valid_from.map(|dt| dt.to_rfc3339());
        let valid_until_str = input.valid_until.map(|dt| dt.to_rfc3339());
        let provenance_json = input.provenance
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());

        {
            let writer = self.writer.lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "INSERT INTO knowledge_nodes (
                    id, content, node_type, created_at, updated_at, last_accessed,
                    stability, difficulty, reps, lapses, learning_state,
                    storage_strength, retrieval_strength, retention_strength,
                    sentiment_score, sentiment_magnitude, next_review, scheduled_days,
                    source, tags, valid_from, valid_until, has_embedding, embedding_model,
                    provenance,
                    memory_kind, subject, predicate, object, episodic_at, procedural_frequency
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10, ?11,
                    ?12, ?13, ?14,
                    ?15, ?16, ?17, ?18,
                    ?19, ?20, ?21, ?22, ?23, ?24,
                    ?25,
                    ?26, ?27, ?28, ?29, ?30, ?31
                )",
                params![
                    id,
                    input.content,
                    input.node_type,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    fsrs_state.stability * sentiment_boost,
                    fsrs_state.difficulty,
                    fsrs_state.reps,
                    fsrs_state.lapses,
                    "new",
                    1.0,
                    1.0,
                    1.0,
                    input.sentiment_score,
                    input.sentiment_magnitude,
                    next_review.to_rfc3339(),
                    fsrs_state.scheduled_days,
                    input.source,
                    tags_json,
                    valid_from_str,
                    valid_until_str,
                    0,
                    Option::<String>::None,
                    provenance_json,
                    // Tier 4 typed memory fields. IngestInput passes through
                    // optional typed metadata; missing values default to Raw.
                    input.memory_kind.as_str(),
                    input.subject.as_deref(),
                    input.predicate.as_deref(),
                    input.object.as_deref(),
                    input.episodic_at.map(|t| t.to_rfc3339()),
                    input.procedural_frequency.as_deref(),
                ],
            )?;
        }

        // Generate embedding if available
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        if let Err(e) = self.generate_embedding_for_node(&id, &input.content) {
            tracing::warn!("Failed to generate embedding for {}: {}", id, e);
        }

        self.get_node(&id)?
            .ok_or_else(|| StorageError::NotFound(id))
    }

    /// Update the tags of an existing node.
    ///
    /// Tags are stored as a JSON array in the `tags` column of `knowledge_nodes`.
    /// We normalize first (trim, dedup, drop empties) so callers do not need to
    /// pre-clean. The node's `updated_at` is bumped so dashboards can detect the
    /// change. Embeddings are not regenerated — tags are not part of the
    /// embedding text in this storage layer.
    pub fn update_node_tags(&self, id: &str, new_tags: &[String]) -> Result<()> {
        let normalized = normalize_tags(new_tags);
        let tags_json = serde_json::to_string(&normalized)
            .map_err(|e| StorageError::Init(format!("Failed to serialize tags: {}", e)))?;
        let now = Utc::now();

        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "UPDATE knowledge_nodes SET tags = ?1, updated_at = ?2 WHERE id = ?3",
            params![tags_json, now.to_rfc3339(), id],
        )?;
        Ok(())
    }

    /// Update the content of an existing node
    pub fn update_node_content(&self, id: &str, new_content: &str) -> Result<()> {
        let now = Utc::now();

        {
            let writer = self.writer.lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "UPDATE knowledge_nodes SET content = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_content, now.to_rfc3339(), id],
            )?;
        }

        // Regenerate embedding for updated content
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        {
            // Remove old embedding from index
            if let Ok(mut index) = self.vector_index.lock() {
                let _ = index.remove(id);
            }
            // Generate new embedding
            if let Err(e) = self.generate_embedding_for_node(id, new_content) {
                tracing::warn!("Failed to regenerate embedding for {}: {}", id, e);
            }
        }

        Ok(())
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Result<Option<KnowledgeNode>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader
            .prepare("SELECT * FROM knowledge_nodes WHERE id = ?1")?;

        let node = stmt
            .query_row(params![id], Self::row_to_node)
            .optional()?;
        Ok(node)
    }

    /// Fetch multiple nodes in a single query (avoids N+1).
    pub fn get_nodes_bulk(&self, ids: &[String]) -> Result<Vec<KnowledgeNode>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let reader = self.acquire_reader()?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT * FROM knowledge_nodes WHERE id IN ({})", placeholders);
        let mut stmt = reader.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let nodes = stmt
            .query_map(params.as_slice(), Self::row_to_node)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(nodes)
    }

    /// Delete a node
    pub fn delete_node(&self, id: &str) -> Result<bool> {
        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let rows = writer
            .execute("DELETE FROM knowledge_nodes WHERE id = ?1", params![id])?;

        // Clean up vector index to prevent stale search results
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        if rows > 0
            && let Ok(mut index) = self.vector_index.lock() {
                let _ = index.remove(id);
            }

        Ok(rows > 0)
    }

    /// Search with full-text search
    pub fn search(&self, query: &str, limit: i32) -> Result<Vec<KnowledgeNode>> {
        let sanitized_query = sanitize_fts5_query(query);

        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT n.* FROM knowledge_nodes n
             JOIN knowledge_fts fts ON n.id = fts.id
             WHERE knowledge_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let nodes = stmt.query_map(params![sanitized_query, limit], Self::row_to_node)?;

        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }

    /// Get all nodes (paginated)
    pub fn get_all_nodes(&self, limit: i32, offset: i32) -> Result<Vec<KnowledgeNode>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM knowledge_nodes
             ORDER BY created_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;

        let nodes = stmt.query_map(params![limit, offset], Self::row_to_node)?;

        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }

    /// Get nodes by type and optional tag filter
    ///
    /// This is used for codebase context retrieval where we need to query
    /// by node_type (pattern/decision) and filter by codebase tag.
    pub fn get_nodes_by_type_and_tag(
        &self,
        node_type: &str,
        tag_filter: Option<&str>,
        limit: i32,
    ) -> Result<Vec<KnowledgeNode>> {
        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        match tag_filter {
            Some(tag) => {
                // Query with tag filter using JSON LIKE search
                // Tags are stored as JSON array, e.g., '["pattern", "codebase", "codebase:vestige"]'
                let tag_pattern = format!("%\"{}%", tag);
                let mut stmt = reader.prepare(
                    "SELECT * FROM knowledge_nodes
                     WHERE node_type = ?1
                     AND tags LIKE ?2
                     ORDER BY retention_strength DESC, created_at DESC
                     LIMIT ?3",
                )?;
                let rows = stmt.query_map(params![node_type, tag_pattern, limit], |row| {
                    Self::row_to_node(row)
                })?;
                let mut nodes = Vec::new();
                for node in rows.flatten() {
                    nodes.push(node);
                }
                Ok(nodes)
            }
            None => {
                // Query without tag filter
                let mut stmt = reader.prepare(
                    "SELECT * FROM knowledge_nodes
                     WHERE node_type = ?1
                     ORDER BY retention_strength DESC, created_at DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![node_type, limit], Self::row_to_node)?;
                let mut nodes = Vec::new();
                for node in rows.flatten() {
                    nodes.push(node);
                }
                Ok(nodes)
            }
        }
    }
}

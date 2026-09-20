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
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::fsrs::{DEFAULT_MAX_SENTIMENT_BOOST, apply_sentiment_boost};
use crate::fts::sanitize_fts5_query;
use crate::memory::{IngestInput, KnowledgeNode};

use super::records::RevisionKind;
use super::{Result, Storage, StorageError, normalize_tags};

impl Storage {
    /// Ingest a new memory using a pre-computed embedding.
    ///
    /// Equivalent to [`Self::ingest`] except the caller has already paid the
    /// cost of running the embedding model on `input.content`. The smart
    /// ingest path uses this to avoid re-embedding the same string twice
    /// (once for the prediction-error gate, once for storage).
    ///
    /// The embedding is taken on trust — we do not re-verify that it was
    /// produced from `input.content`. Mis-pairing here would corrupt
    /// retrieval, so only call this from paths where the embedding's
    /// provenance is obvious.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    pub fn ingest_with_embedding(
        &self,
        input: IngestInput,
        embedding: &crate::embeddings::Embedding,
    ) -> Result<KnowledgeNode> {
        self.ingest_inner(input, Some(embedding))
    }

    /// Ingest a new memory
    pub fn ingest(&self, input: IngestInput) -> Result<KnowledgeNode> {
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        return self.ingest_inner(input, None);
        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        return self.ingest_inner(input);
    }

    /// Shared implementation behind `ingest` / `ingest_with_embedding`.
    ///
    /// Centralises the SQL insert + sentiment-boost + embedding logic so the
    /// public API stays slim. The `precomputed_embedding` argument is only
    /// honoured when the `embeddings` + `vector-search` features are active;
    /// the no-features build silently ignores it (the parameter is removed
    /// at compile time by the `#[cfg]` on the function signature).
    fn ingest_inner(
        &self,
        mut input: IngestInput,
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        precomputed_embedding: Option<&crate::embeddings::Embedding>,
    ) -> Result<KnowledgeNode> {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();

        // Entity normalization (Cognee pattern): canonicalize tags to prevent
        // the same concept from fragmenting across different surface forms.
        // e.g. "bug-fix", "Bug Fix", "bugfix" → "bug-fix"
        input.tags = normalize_tags(&input.tags);

        let fsrs_state = self
            .scheduler
            .lock()
            .map_err(|_| StorageError::Init("Scheduler lock poisoned".into()))?
            .new_card();

        // Emotional-memory stability boost. Routed through the shared FSRS
        // helper so a fresh ingest and the first review use the same curve
        // and the same cap (`DEFAULT_MAX_SENTIMENT_BOOST`). Pre-v3.4.1 this
        // path hard-coded `1 + 0.5·magnitude` (cap 1.5×) while the scheduler
        // already used `apply_sentiment_boost` with cap 2.0×, so the
        // first review silently widened the boost — small but real source
        // of stability drift between ingest and the first FSRS update.
        let boosted_initial_stability = if input.sentiment_magnitude > 0.0 {
            apply_sentiment_boost(
                fsrs_state.stability,
                input.sentiment_magnitude,
                DEFAULT_MAX_SENTIMENT_BOOST,
            )
        } else {
            fsrs_state.stability
        };

        let tags_json = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".to_string());
        let next_review = now + Duration::days(fsrs_state.scheduled_days as i64);
        let valid_from_str = input.valid_from.map(|dt| dt.to_rfc3339());
        let valid_until_str = input.valid_until.map(|dt| dt.to_rfc3339());
        let provenance_json = input
            .provenance
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());
        // Typed extension payload (Decision matrix, Hub metadata, etc.).
        // Stored verbatim — the producer is responsible for shape validation
        // before calling ingest. Stored as NULL when not supplied so legacy
        // rows stay byte-identical to pre-v12.
        let extra_json_str = input
            .extra_json
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".to_string()));
        // Serialized out here rather than inline in the `params!` list so the
        // NULL/`[]` distinction stays visible: a clean row has no findings and
        // stores NULL, a flagged row stores the array it was given.
        let self_contained_findings_json = input
            .self_contained_findings
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));

        {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            let tx = super::helpers::begin_write_transaction(&mut writer)?;

            // The genesis of the timeline, in the same transaction as the row
            // it describes — a memory with no `create` revision would have a
            // history that starts mid-story. Recorded before the INSERT because
            // the insert consumes `input.content`.
            //
            // The actor travels with it: the revision is the only place a reader
            // can learn which agent and which conversation wrote this memory,
            // and a NULL there is unrecoverable later.
            Self::record_revision(
                &tx,
                &id,
                RevisionKind::Create,
                None,
                Some(input.content.as_str()),
                input.source.as_deref(),
                input.actor.as_deref(),
            )?;

            tx.execute(
                "INSERT INTO knowledge_nodes (
                    id, content, node_type, created_at, updated_at, last_accessed, recorded_at,
                    stability, difficulty, reps, lapses, learning_state,
                    storage_strength, retrieval_strength, retention_strength,
                    sentiment_score, sentiment_magnitude, next_review, scheduled_days,
                    source, tags, valid_from, valid_until, has_embedding, embedding_model,
                    provenance,
                    memory_kind, subject, predicate, object, episodic_at, procedural_frequency,
                    extra_json,
                    self_contained, self_contained_findings
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                    ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15,
                    ?16, ?17, ?18, ?19,
                    ?20, ?21, ?22, ?23, ?24, ?25,
                    ?26,
                    ?27, ?28, ?29, ?30, ?31, ?32,
                    ?33,
                    ?34, ?35
                )",
                params![
                    id,
                    input.content,
                    input.node_type,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    // Record time. Immutable for the life of the row; callers
                    // that need "when was this true in the world" use
                    // `valid_from` instead.
                    now.to_rfc3339(),
                    boosted_initial_stability,
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
                    // Typed memory fields. IngestInput passes through
                    // optional typed metadata; missing values default to Raw.
                    input.memory_kind.as_str(),
                    input.subject.as_deref(),
                    input.predicate.as_deref(),
                    input.object.as_deref(),
                    input.episodic_at.map(|t| t.to_rfc3339()),
                    input.procedural_frequency.as_deref(),
                    extra_json_str,
                    // Self-containedness verdict (V18). NULL when the caller
                    // did not run the gate: an unmarked row must stay
                    // distinguishable from a checked-clean one.
                    input.self_contained.map(|ok| if ok { 1 } else { 0 }),
                    self_contained_findings_json,
                ],
            )?;

            tx.commit()?;
        }

        // Generate or reuse the embedding. When a caller supplies a
        // pre-computed embedding (e.g. `smart_ingest`, which already paid for
        // it during the prediction-error probe) we hand straight to
        // `store_embedding_for_node` and skip the second embed pass.
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        {
            let result = match precomputed_embedding {
                Some(emb) => self.store_embedding_for_node(&id, emb),
                None => self.generate_embedding_for_node(&id, &input.content),
            };
            if let Err(e) = result {
                tracing::warn!("Failed to generate embedding for {}: {}", id, e);
            }
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

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "UPDATE knowledge_nodes SET tags = ?1, updated_at = ?2 WHERE id = ?3",
            params![tags_json, now.to_rfc3339(), id],
        )?;
        Ok(())
    }

    /// Replace the `extra_json` column on an existing node.
    ///
    /// Passing `None` clears the column (SQL NULL). Callers that want to
    /// surgically update one slot — e.g. flipping `extra_json.insight.
    /// validatedByAgent` on promote — should `get_node`, mutate
    /// `extra_json` in memory (typically through
    /// [`memory::merge_insight_into_extra`] or similar helpers), then
    /// pass the result back here. Bumps `updated_at` for change
    /// detection. Embeddings are untouched (content didn't change).
    pub fn update_node_extra_json(
        &self,
        id: &str,
        new_extra_json: Option<&serde_json::Value>,
    ) -> Result<()> {
        let serialized = match new_extra_json {
            Some(v) => Some(serde_json::to_string(v).map_err(|e| {
                StorageError::Init(format!("Failed to serialize extra_json: {}", e))
            })?),
            None => None,
        };
        let now = Utc::now();

        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "UPDATE knowledge_nodes SET extra_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![serialized, now.to_rfc3339(), id],
        )?;
        Ok(())
    }

    /// Update the content of an existing node and the vector that indexes it.
    ///
    /// The replacement vector is computed *before* anything is written. The
    /// previous order — `index.remove` first, re-embed second — dropped the
    /// memory out of HNSW whenever the embedding backend was unavailable or
    /// failed, and still returned `Ok(())`: semantic search lost the memory
    /// until someone ran `regenerate_embeddings { force: true }`, while
    /// `has_embedding` stayed 1 so no automatic backfill ever noticed it.
    ///
    /// Failing closed (no content write, no index change, `Err`) keeps the
    /// stored text and the vector that indexes it in agreement; a caller that
    /// sees the error can retry once the backend is back.
    ///
    /// The previous wording is preserved in `memory_revisions` — see
    /// [`Self::update_node_content_with_revision`], which this delegates to.
    pub fn update_node_content(&self, id: &str, new_content: &str) -> Result<()> {
        self.update_node_content_with_revision(id, new_content, None)
    }

    /// [`Self::update_node_content`] with the reason for the edit recorded on
    /// the revision.
    ///
    /// Both the content write and its `edit` revision happen inside one
    /// transaction, so the history cannot gain an entry for an edit that was
    /// rolled back, and an edit cannot land without the entry that preserves
    /// what the memory used to say. The embedding is written after the commit
    /// (it is an index, not the record) — a failed re-embed therefore leaves
    /// the new text and its revision committed and only the vector stale,
    /// which is what `regenerate_embeddings` exists to repair. The reverse
    /// order used to lose the edit itself: the embed errors out, and the text
    /// the caller thought they saved was never written.
    pub fn update_node_content_with_revision(
        &self,
        id: &str,
        new_content: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        self.update_node_content_with_revision_as(id, new_content, reason, None)
    }

    /// [`Self::update_node_content_with_revision`] with the actor recorded too.
    ///
    /// A separate method rather than a fourth parameter on the existing one,
    /// because the existing signature is public API and every current caller
    /// passes no actor: widening it would break them for a field most of them
    /// cannot supply. The `smart_ingest` update paths — the ones a tool drives,
    /// and therefore the ones where an actor exists — call this.
    pub fn update_node_content_with_revision_as(
        &self,
        id: &str,
        new_content: &str,
        reason: Option<&str>,
        actor: Option<&str>,
    ) -> Result<()> {
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        let embedding = self.embed_text(new_content).map_err(|e| {
            StorageError::Init(format!(
                "refusing to update node {}: could not embed the new content ({})",
                id, e
            ))
        })?;

        let now = Utc::now();

        {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            let tx = super::helpers::begin_write_transaction(&mut writer)?;

            // Read the previous text inside the write transaction: reading it
            // through the reader pool first would let a concurrent edit land in
            // between, and the revision would then claim the wrong "before".
            let previous: Option<String> = tx
                .query_row(
                    "SELECT content FROM knowledge_nodes WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;
            let previous = previous.ok_or_else(|| StorageError::NotFound(id.to_string()))?;

            tx.execute(
                "UPDATE knowledge_nodes SET content = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_content, now.to_rfc3339(), id],
            )?;

            // `recorded_at` is absent from that SET list on purpose: it is the
            // record time of the memory, not of its latest wording. Moving it
            // here would make an edited memory look newly learned.
            Self::record_revision(
                &tx,
                id,
                RevisionKind::Edit,
                Some(previous.as_str()),
                Some(new_content),
                reason,
                actor,
            )?;

            tx.commit()?;
        }

        // `store_embedding_for_node` upserts both `node_embeddings` and the
        // HNSW entry (`VectorIndex::add` replaces a key it already holds), so
        // the memory is never absent from the index between the two writes.
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        self.store_embedding_for_node(id, &embedding)?;

        Ok(())
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Result<Option<KnowledgeNode>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare("SELECT * FROM knowledge_nodes WHERE id = ?1")?;

        let node = stmt.query_row(params![id], Self::row_to_node).optional()?;
        Ok(node)
    }

    /// Fetch multiple nodes in a single query (avoids N+1).
    pub fn get_nodes_bulk(&self, ids: &[String]) -> Result<Vec<KnowledgeNode>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let reader = self.acquire_reader()?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT * FROM knowledge_nodes WHERE id IN ({})",
            placeholders
        );
        let mut stmt = reader.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let nodes = stmt
            .query_map(params.as_slice(), Self::row_to_node)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(nodes)
    }

    /// Delete a node and evict its vector from the index.
    ///
    /// The eviction result is no longer discarded, and the sidecar is rewritten
    /// before returning. Previously the sidecar was only written by
    /// consolidation and by the startup rebuild, so a deleted UUID stayed in
    /// `vestige.hnsw` and — once unrelated ingests brought the row count back
    /// to what the meta recorded — could be loaded again on the next boot.
    ///
    /// The node's revisions go with it, in the same transaction. Deleting a
    /// memory while leaving its history behind would keep the erased text
    /// readable in `memory_revisions` after every other query agreed the node
    /// was gone — the failure GDPR erasure exists to prevent, reached through
    /// the plain delete path instead.
    pub fn delete_node(&self, id: &str) -> Result<bool> {
        let rows = {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            let tx = super::helpers::begin_write_transaction(&mut writer)?;
            Self::delete_revisions_for(&tx, id)?;
            let rows = tx.execute("DELETE FROM knowledge_nodes WHERE id = ?1", params![id])?;
            tx.commit()?;
            rows
        };

        // Clean up vector index to prevent stale search results
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        if rows > 0 {
            match self.vector_index.lock() {
                Ok(mut index) => {
                    if let Err(e) = index.remove(id) {
                        tracing::warn!(
                            node_id = %id,
                            error = %e,
                            "vector index eviction failed — the deleted memory can still be \
                             returned by semantic search until the index is rebuilt"
                        );
                    }
                }
                Err(_) => tracing::warn!(
                    node_id = %id,
                    "vector index lock poisoned — the deleted memory was not evicted"
                ),
            }

            // Persist immediately: leaving the rewrite to the next
            // consolidation is what let a deleted UUID survive in the sidecar.
            if let Err(e) = self.persist_vector_index() {
                tracing::warn!(
                    error = %e,
                    "could not rewrite the vector index sidecar after delete — \
                     next startup rebuilds from SQLite instead of the sidecar"
                );
            }
        }

        Ok(rows > 0)
    }

    /// Search with full-text search
    pub fn search(&self, query: &str, limit: i32) -> Result<Vec<KnowledgeNode>> {
        let sanitized_query = sanitize_fts5_query(query);

        let reader = self
            .reader
            .lock()
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
        self.get_all_nodes_filtered(limit, offset, None, None, None)
    }

    /// `get_all_nodes` with SQL-side filtering for `node_type`, `tag`,
    /// and `min_retention`.
    ///
    /// All three filters are optional. The tag filter uses `json_each` to
    /// scan the `tags` JSON array — fast for small tag arrays (typical:
    /// 0–5 elements) and correct for the storage format. We do **not** add
    /// an index on `tags` because tag membership queries hit at most a few
    /// rows per term; FTS5 already covers the keyword-search case.
    ///
    /// Pushdown matters because the dashboard caller previously did
    /// `get_all_nodes(LIMIT 50) → retain(filter)` in Rust, which silently
    /// returned 0–3 rows instead of 50 whenever the filter rejected the
    /// top-50 by created_at. (Audit 2026-05-22.)
    pub fn get_all_nodes_filtered(
        &self,
        limit: i32,
        offset: i32,
        node_type: Option<&str>,
        tag: Option<&str>,
        min_retention: Option<f64>,
    ) -> Result<Vec<KnowledgeNode>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;

        // Build the WHERE clause dynamically. Each branch appends both the
        // SQL fragment and the bound parameter so the ordering stays in
        // sync — rusqlite's positional params are easy to get wrong here.
        let mut sql = String::from("SELECT * FROM knowledge_nodes");
        let mut clauses: Vec<&'static str> = Vec::new();
        let mut params_dyn: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(nt) = node_type {
            clauses.push("node_type = ?");
            params_dyn.push(Box::new(nt.to_string()));
        }
        if let Some(t) = tag {
            // `tags` is JSON like `["foo","bar"]`. `json_each` flattens it
            // into a virtual row per element; EXISTS short-circuits on
            // the first match.
            clauses.push("EXISTS (SELECT 1 FROM json_each(knowledge_nodes.tags) WHERE value = ?)");
            params_dyn.push(Box::new(t.to_string()));
        }
        if let Some(min_ret) = min_retention {
            clauses.push("retention_strength >= ?");
            params_dyn.push(Box::new(min_ret));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        params_dyn.push(Box::new(limit));
        params_dyn.push(Box::new(offset));

        let mut stmt = reader.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = params_dyn.iter().map(|b| b.as_ref()).collect();
        let nodes = stmt.query_map(refs.as_slice(), Self::row_to_node)?;

        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }

    /// Count rows that `get_all_nodes_filtered` would emit if its
    /// `LIMIT` were removed. The dashboard uses this to render an
    /// honest "X of Y" — `page.len()` lied whenever the population
    /// exceeded the page size. (Audit 2026-05-22.)
    ///
    /// The WHERE clause MUST stay byte-for-byte identical to
    /// `get_all_nodes_filtered` (modulo SELECT/ORDER/LIMIT) — that's
    /// the invariant the test guards.
    pub fn count_nodes_filtered(
        &self,
        node_type: Option<&str>,
        tag: Option<&str>,
        min_retention: Option<f64>,
    ) -> Result<usize> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;

        let mut sql = String::from("SELECT COUNT(*) FROM knowledge_nodes");
        let mut clauses: Vec<&'static str> = Vec::new();
        let mut params_dyn: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(nt) = node_type {
            clauses.push("node_type = ?");
            params_dyn.push(Box::new(nt.to_string()));
        }
        if let Some(t) = tag {
            clauses.push("EXISTS (SELECT 1 FROM json_each(knowledge_nodes.tags) WHERE value = ?)");
            params_dyn.push(Box::new(t.to_string()));
        }
        if let Some(min_ret) = min_retention {
            clauses.push("retention_strength >= ?");
            params_dyn.push(Box::new(min_ret));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        let mut stmt = reader.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = params_dyn.iter().map(|b| b.as_ref()).collect();
        let count: i64 = stmt.query_row(refs.as_slice(), |row| row.get(0))?;
        Ok(count.max(0) as usize)
    }

    /// Find the existing Topic Hub node (Proposal A) for a given
    /// cluster signature, if any. Returns `Ok(None)` when no hub has
    /// been written for this cluster yet — the caller (`tools/dream`)
    /// uses this to decide between **insert** and **regenerate**.
    ///
    /// Backed by the partial index `idx_nodes_hub_signature` from
    /// migration v13, so the lookup is O(log N) even on dense bases.
    pub fn find_hub_by_signature(&self, signature: &str) -> Result<Option<KnowledgeNode>> {
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM knowledge_nodes
             WHERE node_type = 'hub'
               AND extra_json IS NOT NULL
               AND json_extract(extra_json, '$.hub.clusterSignature') = ?1
             LIMIT 1",
        )?;
        let node = stmt
            .query_row(params![signature], Self::row_to_node)
            .optional()?;
        Ok(node)
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
        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        match tag_filter {
            Some(tag) => {
                // Exact tag match via `json_each`, same pattern as
                // `get_all_nodes_filtered` / `count_nodes_filtered`. The old
                // `tags LIKE '%"<tag>%'` pattern matched tag *prefixes*
                // (`code` matched `codebase`, `work` matched `workshop`) and
                // let `%`/`_` inside the tag act as wildcards. Tags are stored
                // already lowercased by `normalize_tags`, so no `LOWER()` is
                // needed.
                let mut stmt = reader.prepare(
                    "SELECT * FROM knowledge_nodes
                     WHERE node_type = ?1
                     AND EXISTS (SELECT 1 FROM json_each(knowledge_nodes.tags) WHERE value = ?2)
                     ORDER BY retention_strength DESC, created_at DESC
                     LIMIT ?3",
                )?;
                let rows = stmt.query_map(params![node_type, tag, limit], Self::row_to_node)?;
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

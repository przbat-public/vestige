//! Smart ingest with Prediction Error Gating
//!
//! `smart_ingest` is the neuroscience-inspired insertion path that decides
//! whether new content should:
//! - **Create** a new memory (high prediction error — content is novel),
//! - **Update** an existing memory (low prediction error — variant of known content),
//! - **Supersede** a previously demoted/outdated memory (correction),
//! - **Merge** with several similar memories (mid-similarity cluster).
//!
//! The decision is delegated to
//! [`PredictionErrorGate`](crate::advanced::prediction_error::PredictionErrorGate);
//! this module handles the I/O side (embedding the new content, fetching
//! candidate embeddings, applying the gate's decision to the SQLite store,
//! and writing connection edges so memory evolution stays observable in the
//! graph view).
//!
//! Falls back to plain [`Storage::ingest`] when the embedding service is not
//! ready, returning `decision: "create"` with `prediction_error: 1.0` so
//! callers can detect the degraded path.

#![cfg(all(feature = "embeddings", feature = "vector-search"))]

use chrono::Utc;
use rusqlite::{OptionalExtension, params};

use super::Storage;
use super::records::RevisionKind;
use crate::memory::IngestInput;
use crate::storage::{
    ConnectionRecord, ContradictionReport, Result, SmartIngestResult, StorageError,
};

impl Storage {
    /// Smart ingest with Prediction Error Gating.
    ///
    /// Uses neuroscience-inspired prediction error to decide whether to:
    /// - Create a new memory (high prediction error)
    /// - Update an existing memory (low prediction error)
    /// - Supersede a demoted/outdated memory (correction)
    /// - Merge with several similar memories
    ///
    /// This solves the "bad vs good similar memory" problem.
    pub fn smart_ingest(&self, input: IngestInput) -> Result<SmartIngestResult> {
        use crate::advanced::prediction_error::{
            CandidateMemory, GateDecision, PredictionErrorGate, UpdateType,
        };

        // Who is writing, lifted out before `input` is consumed by the ingest
        // branches below. Every revision this call appends — the edit inside an
        // update, both halves of a supersede — carries it, so the timeline does
        // not answer "who" differently depending on which decision the gate took.
        let actor = input.actor.clone();

        if !self.embedding_service.is_ready() {
            let node = self.ingest(input)?;
            return Ok(SmartIngestResult {
                decision: "create".to_string(),
                node,
                superseded_id: None,
                similarity: None,
                prediction_error: Some(1.0),
                reason: "Embeddings not available, falling back to regular ingest".to_string(),
                contradiction: None,
                neighbor_ids: Vec::new(),
            });
        }

        let new_embedding = self
            .embedding_service
            .embed(&input.content)
            .map_err(|e| StorageError::Init(format!("Embedding failed: {}", e)))?;

        // Reuse the embedding we just computed instead of going through
        // `semantic_search_raw`, which would embed the exact same text a
        // second time (and a third+ if HyDE expansion fires). The neighbour
        // lookup here is for the prediction-error gate, not user-facing
        // semantic search, so we don't want HyDE either — `smart_ingest`
        // is asking "what is most similar to *this exact memory*", not
        // "what is most relevant to this query intent".
        let similar = self.semantic_search_by_embedding(&new_embedding.vector, 10)?;

        // Capture the nearest-neighbour IDs once so we can return them on
        // every decision branch. Lets MCP callers wire post-ingest cognitive
        // updates without firing a second `semantic_search_raw` against the
        // same embedding (the previous behaviour did two embed+search rounds
        // per call — once here, once in the MCP layer).
        let neighbor_ids: Vec<String> = similar.iter().map(|(id, _)| id.clone()).collect();

        let mut candidates: Vec<CandidateMemory> = Vec::new();
        for (node_id, _similarity) in similar.iter() {
            if let Some(node) = self.get_node(node_id)?
                && let Some(emb) = self.get_node_embedding(node_id)?
            {
                let was_demoted = node.retrieval_strength < 0.3;
                let was_promoted = node.retrieval_strength > 0.85;

                candidates.push(CandidateMemory {
                    id: node.id.clone(),
                    content: node.content.clone(),
                    embedding: emb,
                    retrieval_strength: node.retrieval_strength,
                    retention_strength: node.retention_strength,
                    tags: node.tags.clone(),
                    source: node.source.clone(),
                    was_demoted,
                    was_promoted,
                });
            }
        }

        let mut gate = PredictionErrorGate::new();
        let decision = gate.evaluate(&input.content, &new_embedding.vector, &candidates);

        match decision {
            GateDecision::Create {
                prediction_error,
                related_memory_ids,
                reason,
                ..
            } => {
                let node = self.ingest_with_embedding(input, &new_embedding)?;

                // Memory evolution (A-Mem pattern): when a new memory is created,
                // forge connections to related memories and give them a small
                // retroactive boost. This models how learning something new
                // enriches existing knowledge — related memories become more
                // accessible because a new retrieval cue now points to them.
                if !related_memory_ids.is_empty() {
                    let now = Utc::now();
                    for related_id in &related_memory_ids {
                        let conn = ConnectionRecord {
                            source_id: node.id.clone(),
                            target_id: related_id.clone(),
                            strength: 0.3 + (1.0 - prediction_error as f64) * 0.4,
                            link_type: "semantic".to_string(),
                            created_at: now,
                            last_activated: now,
                            activation_count: 1,
                        };
                        let _ = self.save_connection(&conn);

                        let writer = self
                            .writer
                            .lock()
                            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
                        writer.execute(
                            "UPDATE knowledge_nodes SET
                                retrieval_strength = MIN(1.0, retrieval_strength + 0.02),
                                last_accessed = ?1
                            WHERE id = ?2",
                            params![now.to_rfc3339(), related_id],
                        )?;
                    }
                }

                Ok(SmartIngestResult {
                    decision: "create".to_string(),
                    node,
                    superseded_id: None,
                    // Reported even though nothing was rewritten: the caller
                    // decides what to tell the writer, and "this is 83% similar
                    // to a memory you already have, stored separately" is the
                    // signal that the writer may have meant to edit instead.
                    // Withholding it made the near-duplicate advisory in the
                    // MCP layer unreachable on the one path that needs it.
                    similarity: Some(1.0 - prediction_error),
                    prediction_error: Some(prediction_error),
                    reason: if related_memory_ids.is_empty() {
                        format!("Created new memory: {:?}", reason)
                    } else {
                        format!(
                            "Created new memory: {:?}. Considered (not linked): {:?}",
                            reason, related_memory_ids
                        )
                    },
                    contradiction: None,
                    neighbor_ids: neighbor_ids.clone(),
                })
            }
            GateDecision::Update {
                target_id,
                similarity,
                update_type,
                prediction_error,
            } => match update_type {
                UpdateType::Reinforce => {
                    self.strengthen_on_access(&target_id)?;
                    let node = self
                        .get_node(&target_id)?
                        .ok_or_else(|| StorageError::NotFound(target_id.clone()))?;
                    Ok(SmartIngestResult {
                        decision: "reinforce".to_string(),
                        node,
                        superseded_id: None,
                        similarity: Some(similarity),
                        prediction_error: Some(prediction_error),
                        reason: "Content nearly identical - reinforced existing memory".to_string(),
                        contradiction: None,
                        neighbor_ids: neighbor_ids.clone(),
                    })
                }
                // Kept for callers that ask for an append explicitly, but no
                // automatic decision reaches it any more: this is the branch that
                // produced compound memories, because "similar" was read as "the
                // same claim" and the new text landed under the old heading. The
                // gate now returns `Create { DifferentDomain }` for similar
                // content, which stores it separately and links it.
                UpdateType::Merge | UpdateType::Append => {
                    let existing = self
                        .get_node(&target_id)?
                        .ok_or_else(|| StorageError::NotFound(target_id.clone()))?;

                    let merged_content = format!(
                        "{}\n\n[Updated {}]\n{}",
                        existing.content,
                        chrono::Utc::now().format("%Y-%m-%d"),
                        input.content
                    );

                    self.update_node_content_with_revision_as(
                        &target_id,
                        &merged_content,
                        Some("smart_ingest: merged with similar memory"),
                        actor.as_deref(),
                    )?;
                    // The text this update absorbed carries its own citations.
                    // The node's earlier anchors stay: they describe text the
                    // memory still holds in its history, and a verdict — not a
                    // deletion — is how a reader learns they no longer apply.
                    self.attach_code_anchors(&target_id, &input.anchors)?;
                    self.strengthen_on_access(&target_id)?;

                    let node = self
                        .get_node(&target_id)?
                        .ok_or_else(|| StorageError::NotFound(target_id.clone()))?;

                    Ok(SmartIngestResult {
                        decision: "update".to_string(),
                        node,
                        superseded_id: None,
                        similarity: Some(similarity),
                        prediction_error: Some(prediction_error),
                        reason: "Merged with existing similar memory".to_string(),
                        contradiction: None,
                        neighbor_ids: neighbor_ids.clone(),
                    })
                }
                UpdateType::Replace => {
                    self.update_node_content_with_revision_as(
                        &target_id,
                        &input.content,
                        Some("smart_ingest: replaced with new content"),
                        actor.as_deref(),
                    )?;
                    self.attach_code_anchors(&target_id, &input.anchors)?;
                    let node = self
                        .get_node(&target_id)?
                        .ok_or_else(|| StorageError::NotFound(target_id.clone()))?;

                    Ok(SmartIngestResult {
                        decision: "replace".to_string(),
                        node,
                        superseded_id: None,
                        similarity: Some(similarity),
                        prediction_error: Some(prediction_error),
                        reason: "Replaced existing memory with new content".to_string(),
                        contradiction: None,
                        neighbor_ids: neighbor_ids.clone(),
                    })
                }
                UpdateType::AddContext => {
                    let existing = self
                        .get_node(&target_id)?
                        .ok_or_else(|| StorageError::NotFound(target_id.clone()))?;

                    let merged_content =
                        format!("{}\n\n---\nContext: {}", existing.content, input.content);

                    self.update_node_content_with_revision_as(
                        &target_id,
                        &merged_content,
                        Some("smart_ingest: added as context"),
                        actor.as_deref(),
                    )?;
                    self.attach_code_anchors(&target_id, &input.anchors)?;
                    let node = self
                        .get_node(&target_id)?
                        .ok_or_else(|| StorageError::NotFound(target_id.clone()))?;

                    Ok(SmartIngestResult {
                        decision: "add_context".to_string(),
                        node,
                        superseded_id: None,
                        similarity: Some(similarity),
                        prediction_error: Some(prediction_error),
                        reason: "Added new content as context to existing memory".to_string(),
                        contradiction: None,
                        neighbor_ids: neighbor_ids.clone(),
                    })
                }
            },
            GateDecision::Supersede {
                old_memory_id,
                similarity,
                supersede_reason,
                prediction_error,
            } => {
                // Supersede changes the old memory's *standing* without touching
                // its text, so the revision is the only record of what replaced
                // what. Two rows rather than one, because the successor's id
                // cannot exist before its own INSERT: the first row records the
                // retirement (the text that was retired, and the validity
                // window it had — `unbounded` when it had none), the second
                // names the replacement. Collapsing them into one row would
                // force the first to either claim a successor that was never
                // stored, or drop the window transition.
                let replacement_reason =
                    format!("Superseded by new memory: {:?}", supersede_reason);

                // Rank the old memory down first (its own FSRS write), then
                // retire it and record why in one transaction.
                self.demote_memory(&old_memory_id)?;

                {
                    let mut writer = self
                        .writer
                        .lock()
                        .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
                    let tx = super::helpers::begin_write_transaction(&mut writer)?;

                    let (content, previous_valid_until): (String, Option<String>) = tx
                        .query_row(
                            "SELECT content, valid_until FROM knowledge_nodes WHERE id = ?1",
                            params![&old_memory_id],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()?
                        .ok_or_else(|| StorageError::NotFound(old_memory_id.clone()))?;

                    // Mark it temporally invalidated (Graphiti pattern). Same
                    // transaction as the revision: the two cannot disagree
                    // about whether the memory was actually superseded.
                    tx.execute(
                        "UPDATE knowledge_nodes SET valid_until = ?1 WHERE id = ?2",
                        params![Utc::now().to_rfc3339(), &old_memory_id],
                    )?;

                    Storage::record_revision(
                        &tx,
                        &old_memory_id,
                        RevisionKind::Supersede,
                        Some(content.as_str()),
                        Some(previous_valid_until.as_deref().unwrap_or("unbounded")),
                        Some(replacement_reason.as_str()),
                        actor.as_deref(),
                    )?;

                    tx.commit()?;
                }

                // The replacement is written after the old memory is retired,
                // so an ingest failure leaves one demoted memory rather than
                // losing the fact that a successor was promised.
                let node = self.ingest_with_embedding(input, &new_embedding)?;

                // Name the successor now that its id exists. A second revision
                // rather than an UPDATE of the first: the table is append-only,
                // and "the old memory was retired" and "here is what replaced
                // it" are two moments, each with its own record time.
                //
                // `old_content` is left empty here so the pair reads as a
                // sequence — the retirement row carries the text and the
                // window, this row carries the successor. Repeating the text
                // would make the second row look like a second supersede.
                {
                    let mut writer = self
                        .writer
                        .lock()
                        .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
                    let tx = super::helpers::begin_write_transaction(&mut writer)?;
                    Storage::record_revision(
                        &tx,
                        &old_memory_id,
                        RevisionKind::Supersede,
                        None,
                        Some(node.id.as_str()),
                        Some(replacement_reason.as_str()),
                        actor.as_deref(),
                    )?;
                    tx.commit()?;
                }

                Ok(SmartIngestResult {
                    decision: "supersede".to_string(),
                    node,
                    superseded_id: Some(old_memory_id),
                    similarity: Some(similarity),
                    prediction_error: Some(prediction_error),
                    reason: replacement_reason,
                    contradiction: None,
                    neighbor_ids: neighbor_ids.clone(),
                })
            }
            GateDecision::Merge {
                memory_ids,
                avg_similarity,
                strategy,
            } => {
                let node = self.ingest_with_embedding(input, &new_embedding)?;

                // Memory evolution: link new memory to all merge candidates
                let now = Utc::now();
                for mid in &memory_ids {
                    let conn = ConnectionRecord {
                        source_id: node.id.clone(),
                        target_id: mid.clone(),
                        strength: avg_similarity as f64,
                        link_type: "semantic".to_string(),
                        created_at: now,
                        last_activated: now,
                        activation_count: 1,
                    };
                    let _ = self.save_connection(&conn);
                }

                Ok(SmartIngestResult {
                    decision: "merge".to_string(),
                    node,
                    superseded_id: None,
                    similarity: Some(avg_similarity),
                    prediction_error: Some(1.0 - avg_similarity),
                    reason: format!(
                        "Created new memory linked to {} similar memories ({:?})",
                        memory_ids.len(),
                        strategy
                    ),
                    contradiction: None,
                    neighbor_ids,
                })
            }
            // A contradiction is reported, never enforced. The new memory is
            // stored and linked to the one it may deny; the older memory keeps its
            // text, its validity window and its standing, because retiring it would
            // write `valid_until` — a claim about the past — on the strength of a
            // lexical rule that marked 39 of 156 ordered pairs in a real
            // thirteen-memory store. Retirement stays explicit:
            // `temporal(action="invalidate")`, or a write carrying `supersedes`.
            GateDecision::Contradiction {
                existing_id,
                similarity,
                confidence,
                evidence,
            } => {
                let node = self.ingest_with_embedding(input, &new_embedding)?;

                let now = Utc::now();
                let conn = ConnectionRecord {
                    source_id: node.id.clone(),
                    target_id: existing_id.clone(),
                    strength: confidence as f64,
                    // The edge type the rest of the system already knows: the search
                    // pipeline penalises a pair joined by a `contradiction` edge so
                    // both claims stop surfacing as if they agreed, and dream writes
                    // the same kind. A second spelling ("contradicts") would have been
                    // invisible to that rule and left two names for one relation.
                    link_type: crate::memory::EdgeType::Contradiction.to_string(),
                    created_at: now,
                    last_activated: now,
                    activation_count: 1,
                };
                let _ = self.save_connection(&conn);

                Ok(SmartIngestResult {
                    decision: "create".to_string(),
                    node,
                    superseded_id: None,
                    similarity: Some(similarity),
                    prediction_error: Some(1.0 - similarity),
                    reason: format!(
                        "Created new memory. Possible contradiction with {} (confidence {:.2}) \
                         was reported, not acted on",
                        existing_id, confidence
                    ),
                    contradiction: Some(ContradictionReport {
                        existing_id,
                        similarity,
                        confidence,
                        evidence,
                        hint: "If this memory replaces the older one, retire it explicitly — \
                               temporal(action=\"invalidate\"), or a write passing `supersedes`. \
                               Nothing was retired automatically, so both read as current until \
                               you decide."
                            .to_string(),
                    }),
                    neighbor_ids: neighbor_ids.clone(),
                })
            }
            // The one decision that must not write. `PredictionErrorGate::evaluate`
            // never returns it — a reject is decided from the content alone, one
            // layer up, before this call — so this arm exists so that if a future
            // gate ever does produce one, the write path refuses instead of
            // silently falling through to an arm that stores it.
            GateDecision::Reject { reason, .. } => Err(StorageError::Rejected(reason)),
        }
    }
}

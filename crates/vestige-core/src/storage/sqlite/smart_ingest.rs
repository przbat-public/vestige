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
use rusqlite::params;

use super::Storage;
use crate::memory::IngestInput;
use crate::storage::{ConnectionRecord, Result, SmartIngestResult, StorageError};

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

        if !self.embedding_service.is_ready() {
            let node = self.ingest(input)?;
            return Ok(SmartIngestResult {
                decision: "create".to_string(),
                node,
                superseded_id: None,
                similarity: None,
                prediction_error: Some(1.0),
                reason: "Embeddings not available, falling back to regular ingest".to_string(),
            });
        }

        let new_embedding = self
            .embedding_service
            .embed(&input.content)
            .map_err(|e| StorageError::Init(format!("Embedding failed: {}", e)))?;

        let similar = self.semantic_search_raw(&input.content, 10)?;

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
                let node = self.ingest(input)?;

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
                    similarity: None,
                    prediction_error: Some(prediction_error),
                    reason: if related_memory_ids.is_empty() {
                        format!("Created new memory: {:?}", reason)
                    } else {
                        format!(
                            "Created new memory: {:?}. Considered (not linked): {:?}",
                            reason, related_memory_ids
                        )
                    },
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
                    })
                }
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

                    self.update_node_content(&target_id, &merged_content)?;
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
                    })
                }
                UpdateType::Replace => {
                    self.update_node_content(&target_id, &input.content)?;
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
                    })
                }
                UpdateType::AddContext => {
                    let existing = self
                        .get_node(&target_id)?
                        .ok_or_else(|| StorageError::NotFound(target_id.clone()))?;

                    let merged_content =
                        format!("{}\n\n---\nContext: {}", existing.content, input.content);

                    self.update_node_content(&target_id, &merged_content)?;
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
                    })
                }
            },
            GateDecision::Supersede {
                old_memory_id,
                similarity,
                supersede_reason,
                prediction_error,
            } => {
                // Demote the old memory and mark it as temporally invalidated (Graphiti pattern)
                self.demote_memory(&old_memory_id)?;
                {
                    let writer = self
                        .writer
                        .lock()
                        .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
                    writer.execute(
                        "UPDATE knowledge_nodes SET valid_until = ?1 WHERE id = ?2",
                        params![Utc::now().to_rfc3339(), old_memory_id],
                    )?;
                }

                let node = self.ingest(input)?;

                Ok(SmartIngestResult {
                    decision: "supersede".to_string(),
                    node,
                    superseded_id: Some(old_memory_id),
                    similarity: Some(similarity),
                    prediction_error: Some(prediction_error),
                    reason: format!("New memory supersedes old: {:?}", supersede_reason),
                })
            }
            GateDecision::Merge {
                memory_ids,
                avg_similarity,
                strategy,
            } => {
                let node = self.ingest(input)?;

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
                })
            }
        }
    }
}

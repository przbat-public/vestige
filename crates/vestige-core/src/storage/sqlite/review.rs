//! FSRS review and reinforcement.
//!
//! Everything that updates a node's "how strong is this memory right now"
//! signal lives here:
//!
//! - [`Storage::mark_reviewed`]: explicit FSRS review with a `Rating`. Runs
//!   the scheduler, writes new stability/difficulty, schedules the next
//!   review.
//! - [`Storage::strengthen_on_access`] / [`Storage::strengthen_batch_on_access`]:
//!   passive boost from being recalled (Testing Effect, Roediger &
//!   Karpicke 2006) with cross-neighbor spreading reinforcement.
//! - [`Storage::mark_memory_useful`], [`Storage::promote_memory`],
//!   [`Storage::demote_memory`]: user feedback that swings retrieval
//!   strength up or down.
//! - [`Storage::get_review_queue`], [`Storage::preview_review`]: read-side
//!   helpers for surfacing what's due and previewing FSRS outcomes.

use chrono::{Duration, Utc};
use rusqlite::params;

use crate::fsrs::{FSRSState, LearningState, Rating};
use crate::memory::KnowledgeNode;

use super::{Result, Storage, StorageError};

impl Storage {
    /// Mark a memory as reviewed
    pub fn mark_reviewed(&self, id: &str, rating: Rating) -> Result<KnowledgeNode> {
        let node = self
            .get_node(id)?
            .ok_or_else(|| StorageError::NotFound(id.to_string()))?;

        let learning_state = match node.reps {
            0 => LearningState::New,
            _ if node.lapses > 0 && node.reps == node.lapses => LearningState::Relearning,
            _ => LearningState::Review,
        };

        let current_state = FSRSState {
            difficulty: node.difficulty,
            stability: node.stability,
            state: learning_state,
            reps: node.reps,
            lapses: node.lapses,
            last_review: node.last_accessed,
            scheduled_days: 0,
        };

        let scheduler = self.scheduler.lock()
            .map_err(|_| StorageError::Init("Scheduler lock poisoned".into()))?;
        let elapsed_days = scheduler.days_since_review(&current_state.last_review);

        let sentiment_boost = if node.sentiment_magnitude > 0.0 {
            Some(node.sentiment_magnitude)
        } else {
            None
        };

        let result = scheduler
            .review(&current_state, rating, elapsed_days, sentiment_boost);
        drop(scheduler);

        let now = Utc::now();
        let next_review = now + Duration::days(result.interval as i64);

        let new_storage_strength = if rating != Rating::Again {
            node.storage_strength + 0.1
        } else {
            node.storage_strength + 0.3
        };

        let new_retrieval_strength = 1.0;
        let new_retention =
            (new_retrieval_strength * 0.7) + ((new_storage_strength / 10.0).min(1.0) * 0.3);

        {
            let writer = self.writer.lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "UPDATE knowledge_nodes SET
                    stability = ?1,
                    difficulty = ?2,
                    reps = ?3,
                    lapses = ?4,
                    learning_state = ?5,
                    storage_strength = ?6,
                    retrieval_strength = ?7,
                    retention_strength = ?8,
                    last_accessed = ?9,
                    updated_at = ?10,
                    next_review = ?11,
                    scheduled_days = ?12
                WHERE id = ?13",
                params![
                    result.state.stability,
                    result.state.difficulty,
                    result.state.reps,
                    result.state.lapses,
                    format!("{:?}", result.state.state).to_lowercase(),
                    new_storage_strength,
                    new_retrieval_strength,
                    new_retention,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    next_review.to_rfc3339(),
                    result.interval,
                    id,
                ],
            )?;
        }

        self.get_node(id)?
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    /// Passively strengthen a memory when it's accessed (recalled/searched).
    /// Implements the Testing Effect (Roediger & Karpicke 2006) + v1.4.0
    /// content-aware cross-memory reinforcement: semantically similar neighbors
    /// receive a diminished boost proportional to cosine similarity.
    pub fn strengthen_on_access(&self, id: &str) -> Result<()> {
        let now = Utc::now();

        // Primary boost on the accessed node
        {
            let writer = self.writer.lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "UPDATE knowledge_nodes SET
                    last_accessed = ?1,
                    retrieval_strength = MIN(1.0, retrieval_strength + 0.05),
                    retention_strength = MIN(1.0, retention_strength + 0.02),
                    times_retrieved = COALESCE(times_retrieved, 0) + 1,
                    utility_score = CASE
                        WHEN COALESCE(times_retrieved, 0) + 1 > 0
                        THEN CAST(COALESCE(times_useful, 0) AS REAL) / (COALESCE(times_retrieved, 0) + 1)
                        ELSE 0.0
                    END
                WHERE id = ?2",
                params![now.to_rfc3339(), id],
            )?;
        }

        // Log access for ACT-R activation computation
        let _ = self.log_access(id, "search_hit");

        // Content-aware cross-memory reinforcement: boost semantically similar neighbors
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        {
            if let Ok(Some(embedding)) = self.get_node_embedding(id) {
                let index = self
                    .vector_index
                    .lock()
                    .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;

                // Query top-6 similar (one will be self, so we get ~5 neighbors)
                let neighbors_result = index.search(&embedding, 6);
                drop(index);

                if let Ok(neighbors) = neighbors_result {
                    let writer = self.writer.lock()
                        .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
                    for (neighbor_id, similarity) in neighbors {
                        if neighbor_id == id || similarity < 0.7 {
                            continue;
                        }
                        // Diminished boost: 0.02 * similarity (max ~0.02)
                        let boost = 0.02 * similarity as f64;
                        let retention_boost = 0.008 * similarity as f64;
                        let _ = writer.execute(
                            "UPDATE knowledge_nodes SET
                                retrieval_strength = MIN(1.0, retrieval_strength + ?1),
                                retention_strength = MIN(1.0, retention_strength + ?2)
                            WHERE id = ?3",
                            params![boost, retention_boost, neighbor_id],
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Batch strengthen multiple memories on access.
    /// Runs primary boost for all IDs in a single transaction to avoid
    /// acquiring/releasing the writer lock N times.
    pub fn strengthen_batch_on_access(&self, ids: &[&str]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let now = Utc::now();
        let now_str = now.to_rfc3339();

        {
            let writer = self.writer.lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute_batch("BEGIN IMMEDIATE")?;
            let mut stmt = writer.prepare_cached(
                "UPDATE knowledge_nodes SET
                    last_accessed = ?1,
                    retrieval_strength = MIN(1.0, retrieval_strength + 0.05),
                    retention_strength = MIN(1.0, retention_strength + 0.02),
                    times_retrieved = COALESCE(times_retrieved, 0) + 1,
                    utility_score = CASE
                        WHEN COALESCE(times_retrieved, 0) + 1 > 0
                        THEN CAST(COALESCE(times_useful, 0) AS REAL) / (COALESCE(times_retrieved, 0) + 1)
                        ELSE 0.0
                    END
                WHERE id = ?2",
            )?;
            for id in ids {
                let _ = stmt.execute(params![now_str, id]);
            }
            drop(stmt);
            writer.execute_batch("COMMIT")?;
        }

        for id in ids {
            let _ = self.log_access(id, "search_hit");
        }

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        {
            for id in ids {
                if let Ok(Some(embedding)) = self.get_node_embedding(id) {
                    let index = self
                        .vector_index
                        .lock()
                        .map_err(|_| StorageError::Init("Vector index lock poisoned".to_string()))?;
                    let neighbors_result = index.search(&embedding, 6);
                    drop(index);

                    if let Ok(neighbors) = neighbors_result {
                        let writer = self.writer.lock()
                            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
                        for (neighbor_id, similarity) in neighbors {
                            if neighbor_id == *id || similarity < 0.7 {
                                continue;
                            }
                            let boost = 0.02 * similarity as f64;
                            let retention_boost = 0.008 * similarity as f64;
                            let _ = writer.execute(
                                "UPDATE knowledge_nodes SET
                                    retrieval_strength = MIN(1.0, retrieval_strength + ?1),
                                    retention_strength = MIN(1.0, retention_strength + ?2)
                                WHERE id = ?3",
                                params![boost, retention_boost, neighbor_id],
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Mark a memory as "useful" — called when a retrieved memory is subsequently
    /// referenced in a save or decision (MemRL-inspired utility tracking).
    ///
    /// Increments `times_useful` and recomputes `utility_score = times_useful / times_retrieved`.
    pub fn mark_memory_useful(&self, id: &str) -> Result<()> {
        let writer = self.writer.lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        writer.execute(
            "UPDATE knowledge_nodes SET
                times_useful = COALESCE(times_useful, 0) + 1,
                utility_score = CASE
                    WHEN COALESCE(times_retrieved, 0) > 0
                    THEN MIN(1.0, CAST(COALESCE(times_useful, 0) + 1 AS REAL) / COALESCE(times_retrieved, 0))
                    ELSE 1.0
                END
            WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Promote a memory (thumbs up) - used when a memory led to a good outcome
    /// Significantly boosts retrieval strength so it surfaces more often.
    /// v1.9.0: Also sets waking SWR tag for preferential dream replay.
    pub fn promote_memory(&self, id: &str) -> Result<KnowledgeNode> {
        let now = Utc::now();

        // Strong boost: +0.2 retrieval, +0.1 retention, +1 useful count
        {
            let writer = self.writer.lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "UPDATE knowledge_nodes SET
                    last_accessed = ?1,
                    retrieval_strength = MIN(1.0, retrieval_strength + 0.20),
                    retention_strength = MIN(1.0, retention_strength + 0.10),
                    stability = stability * 1.5,
                    times_useful = COALESCE(times_useful, 0) + 1
                WHERE id = ?2",
                params![now.to_rfc3339(), id],
            )?;
        }

        let _ = self.log_access(id, "promote");

        // v1.9.0: Set waking SWR tag for preferential dream replay
        let _ = self.set_waking_tag(id);

        self.get_node(id)?
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    /// Demote a memory (thumbs down) - used when a memory led to a bad outcome
    /// Significantly reduces retrieval strength so better alternatives surface
    /// Does NOT delete - the memory stays for reference but ranks lower
    pub fn demote_memory(&self, id: &str) -> Result<KnowledgeNode> {
        let now = Utc::now();

        // Strong penalty: -0.3 retrieval, -0.15 retention, halve stability
        {
            let writer = self.writer.lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "UPDATE knowledge_nodes SET
                    last_accessed = ?1,
                    retrieval_strength = MAX(0.05, retrieval_strength - 0.30),
                    retention_strength = MAX(0.05, retention_strength - 0.15),
                    stability = stability * 0.5
                WHERE id = ?2",
                params![now.to_rfc3339(), id],
            )?;
        }

        let _ = self.log_access(id, "demote");

        self.get_node(id)?
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    /// Get memories due for review
    pub fn get_review_queue(&self, limit: i32) -> Result<Vec<KnowledgeNode>> {
        let now = Utc::now().to_rfc3339();

        let reader = self.reader.lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
        let mut stmt = reader.prepare(
            "SELECT * FROM knowledge_nodes
             WHERE next_review <= ?1
             ORDER BY next_review ASC
             LIMIT ?2",
        )?;

        let nodes = stmt.query_map(params![now, limit], Self::row_to_node)?;

        let mut result = Vec::new();
        for node in nodes {
            result.push(node?);
        }
        Ok(result)
    }

    /// Preview FSRS review outcomes for all rating options
    pub fn preview_review(&self, id: &str) -> Result<crate::fsrs::PreviewResults> {
        let node = self
            .get_node(id)?
            .ok_or_else(|| StorageError::NotFound(id.to_string()))?;

        let learning_state = match node.reps {
            0 => LearningState::New,
            _ if node.lapses > 0 && node.reps == node.lapses => LearningState::Relearning,
            _ => LearningState::Review,
        };

        let current_state = FSRSState {
            difficulty: node.difficulty,
            stability: node.stability,
            state: learning_state,
            reps: node.reps,
            lapses: node.lapses,
            last_review: node.last_accessed,
            scheduled_days: 0,
        };

        let scheduler = self.scheduler.lock()
            .map_err(|_| StorageError::Init("Scheduler lock poisoned".into()))?;
        let elapsed_days = scheduler.days_since_review(&current_state.last_review);

        Ok(scheduler.preview_reviews(&current_state, elapsed_days))
    }
}

//! 17-step consolidation pipeline for [`super::Storage`].
//!
//! Single entry point — [`Storage::run_consolidation`] — composes every
//! background-maintenance subsystem into one ordered pass. The order
//! matters: decay must run before promotion (otherwise we promote
//! already-stale items), dedup must run after embeddings (otherwise
//! cosine similarity is undefined for new nodes), ACT-R activation must
//! run before w20 optimization (it feeds the access log used by the
//! optimizer), and retention-snapshot is the very last step so the
//! snapshot reflects everything else.
//!
//! The five private helpers in this file are intentionally not exposed
//! on the public `Storage` surface — they're only meaningful as steps
//! of `run_consolidation` and would invite misuse if called in
//! isolation:
//!
//! - `auto_dedup_consolidation` — episodic→semantic merge of
//!   cosine-similar memories.
//! - `compute_act_r_activations` — ACT-R base-level activation
//!   `B_i = ln(Σ t_j^(-d))` per node from `memory_access_log`.
//! - `prune_access_log` — 90-day rolling window over the access log.
//! - `optimize_w20_if_ready` — `FSRSOptimizer` golden-section search on
//!   real review history once we have ≥100 logged accesses.
//! - `generate_missing_embeddings` — backfills `has_embedding = 0` rows
//!   in batches of 1 000.

use chrono::{DateTime, Duration, Utc};
use rusqlite::params;
use uuid::Uuid;

use crate::memory::ConsolidationResult;

use super::{ConnectionRecord, InsightRecord, Result, Storage, StorageError};

impl Storage {
    /// Run full FSRS-6 consolidation cycle
    ///
    /// 17-step automatic consolidation:
    /// 1–7. Core FSRS-6 (decay, emotional promotion, embeddings, dedup, ACT-R, prune, w20)
    /// 8. DreamEngine 4-phase cycle
    /// 9. Memory Compression
    /// 10. Memory State Transitions
    /// 11. Importance Evolution
    /// 12. Connection Graph Maintenance
    /// 13–14. FTS5 + PRAGMA optimize
    /// 15–17. Autonomic (auto-promote, retention target GC, snapshot)
    pub fn run_consolidation(&self) -> Result<ConsolidationResult> {
        let start = std::time::Instant::now();

        // v1.5.0: Use SleepConsolidation for structured consolidation
        let sleep = crate::SleepConsolidation::new();

        // 1. Apply FSRS-6 decay with real formula + personalized w20
        let decay_applied = self.apply_decay()? as i64;

        // 2. Promote emotional memories via SleepConsolidation
        let mut promoted = 0i64;
        {
            let candidates: Vec<(String, f64, f64)> = {
                let reader = self
                    .reader
                    .lock()
                    .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
                reader
                    .prepare(
                        "SELECT id, sentiment_magnitude, storage_strength
                         FROM knowledge_nodes
                         WHERE storage_strength < 10.0",
                    )?
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .filter_map(|r| r.ok())
                    .collect()
            };

            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            for (id, sentiment_mag, storage_strength) in &candidates {
                if sleep.should_promote(*sentiment_mag, *storage_strength) {
                    let boosted = sleep.promotion_boost(*storage_strength);
                    writer.execute(
                        "UPDATE knowledge_nodes SET storage_strength = ?1 WHERE id = ?2",
                        params![boosted, id],
                    )?;
                    promoted += 1;
                }
            }
        }

        // 3. Generate missing embeddings
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        let embeddings_generated = self.generate_missing_embeddings()?;
        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        let embeddings_generated = 0i64;

        // 4. Auto-dedup: merge similar memories (episodic → semantic consolidation)
        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        let duplicates_merged = self.auto_dedup_consolidation().unwrap_or(0);
        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        let duplicates_merged = 0i64;

        // 5. Compute ACT-R activations from access history
        let activations_computed = self.compute_act_r_activations().unwrap_or(0);

        // 6. Prune old access log entries (keep 90 days)
        let _ = self.prune_access_log();

        // 7. Optimize w20 if enough usage data
        let w20_optimized = self.optimize_w20_if_ready().unwrap_or(None);

        // ====================================================================
        // v1.5.0: Extended consolidation steps 8-15
        // ====================================================================

        // 8. DreamEngine 4-phase cycle (NREM1 → NREM3 → REM → Integration)
        let mut _insights_generated = 0i64;
        {
            let recent = self.get_all_nodes(100, 0).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Consolidation: failed to load nodes for dream engine");
                vec![]
            });
            if recent.len() >= 5 {
                let engine = crate::consolidation::phases::DreamEngine::new();
                let mut emotional = crate::neuroscience::emotional_memory::EmotionalMemory::new();
                let importance = crate::neuroscience::importance_signals::ImportanceSignals::new();
                let mut synaptic =
                    crate::neuroscience::synaptic_tagging::SynapticTaggingSystem::new();

                let result = engine.run(&recent, &mut emotional, &importance, &mut synaptic);

                for insight in &result.insights {
                    let record = InsightRecord {
                        id: Uuid::new_v4().to_string(),
                        insight: insight.insight.clone(),
                        source_memories: insight.source_memory_ids.clone(),
                        confidence: insight.confidence,
                        novelty_score: insight.novelty,
                        insight_type: insight.insight_type.clone(),
                        generated_at: Utc::now(),
                        tags: vec![],
                        feedback: None,
                        applied_count: 0,
                    };
                    if let Err(e) = self.save_insight(&record) {
                        tracing::warn!(error = %e, insight = %insight.insight, "Failed to persist dream insight");
                    }
                }
                _insights_generated = result.insights.len() as i64;

                // Persist creative connections discovered during REM phase
                let now = Utc::now();
                for conn in &result.creative_connections {
                    let link_type = match conn.connection_type {
                        crate::consolidation::phases::CreativeConnectionType::CrossDomain => {
                            "semantic"
                        }
                        crate::consolidation::phases::CreativeConnectionType::Causal => "causal",
                        crate::consolidation::phases::CreativeConnectionType::Complementary => {
                            "complementary"
                        }
                        crate::consolidation::phases::CreativeConnectionType::Contradictory => {
                            "contradiction"
                        }
                    };
                    let record = ConnectionRecord {
                        source_id: conn.memory_a_id.clone(),
                        target_id: conn.memory_b_id.clone(),
                        strength: conn.confidence,
                        link_type: link_type.to_string(),
                        created_at: now,
                        last_activated: now,
                        activation_count: 1,
                    };
                    if let Err(e) = self.save_connection(&record) {
                        tracing::warn!(
                            error = %e,
                            source = %conn.memory_a_id,
                            target = %conn.memory_b_id,
                            "Failed to persist creative connection"
                        );
                    }
                }
            }
        }

        // 9. Memory Compression (old memories → summaries)
        let mut _memories_compressed = 0i64;
        {
            let mut compressor = crate::advanced::compression::MemoryCompressor::new();
            let all_nodes = self.get_all_nodes(500, 0).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Consolidation: failed to load nodes for compression");
                vec![]
            });
            let thirty_days_ago = Utc::now() - Duration::days(30);
            let old_memories: Vec<crate::advanced::compression::MemoryForCompression> = all_nodes
                .iter()
                .filter(|n| n.created_at < thirty_days_ago && n.retention_strength < 0.5)
                .map(|n| crate::advanced::compression::MemoryForCompression {
                    id: n.id.clone(),
                    content: n.content.clone(),
                    tags: n.tags.clone(),
                    created_at: n.created_at,
                    last_accessed: Some(n.last_accessed),
                    embedding: None,
                })
                .collect();
            if old_memories.len() >= 3 {
                let groups = compressor.find_compressible_groups(&old_memories);
                for group_ids in groups.iter().take(5) {
                    // Limit to 5 groups per consolidation
                    let group: Vec<_> = old_memories
                        .iter()
                        .filter(|m| group_ids.contains(&m.id))
                        .cloned()
                        .collect();
                    if let Some(_compressed) = compressor.compress(&group) {
                        _memories_compressed += group.len() as i64;
                    }
                }
            }
        }

        // 10. Memory State Transitions (Active→Dormant→Silent→Unavailable)
        let _state_transitions: i64;
        {
            let service = crate::neuroscience::memory_states::StateUpdateService::new();
            let all_nodes = self.get_all_nodes(500, 0).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Consolidation: failed to load nodes for state transitions");
                vec![]
            });
            let mut lifecycles: Vec<crate::neuroscience::memory_states::MemoryLifecycle> =
                all_nodes
                    .iter()
                    .map(|n| {
                        let mut lc = crate::neuroscience::memory_states::MemoryLifecycle::new();
                        lc.last_access = n.last_accessed;
                        lc.access_count = n.reps as u32;
                        lc.state = if n.retention_strength > 0.7 {
                            crate::neuroscience::memory_states::MemoryState::Active
                        } else if n.retention_strength > 0.3 {
                            crate::neuroscience::memory_states::MemoryState::Dormant
                        } else if n.retention_strength > 0.1 {
                            crate::neuroscience::memory_states::MemoryState::Silent
                        } else {
                            crate::neuroscience::memory_states::MemoryState::Unavailable
                        };
                        lc
                    })
                    .collect();
            let batch_result = service.batch_update(&mut lifecycles);
            _state_transitions = batch_result.total_transitions as i64;
        }

        // 11. Importance Evolution (decay stale importance)
        {
            let tracker = crate::advanced::importance::ImportanceTracker::new();
            tracker.apply_importance_decay();
        }

        // 12. Connection Graph Maintenance (decay + prune weak connections)
        let _connections_pruned = self.prune_weak_connections(0.05).unwrap_or(0) as i64;

        // 13. FTS5 index optimization — merge segments for faster keyword search
        // 14. Run PRAGMA optimize to refresh query planner statistics
        //     + best-effort INCREMENTAL_VACUUM (paired with V14 auto_vacuum)
        //     + best-effort HNSW sidecar persistence so the next process boot
        //       skips the per-row rebuild.
        {
            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            let _ = writer
                .execute_batch("INSERT INTO knowledge_fts(knowledge_fts) VALUES('optimize');");
            let _ = writer.execute_batch("PRAGMA optimize;");
            // Reclaim pages freed by deletes/dedup. Bounded at 1 024 pages so
            // a single consolidation never stalls the writer for minutes on
            // pathological DBs. With page_size=8192 that's ~8 MB per pass.
            let _ = writer.execute_batch("PRAGMA incremental_vacuum(1024);");
        }

        // 14b. Persist the HNSW sidecar (post v3.6). Failure is non-fatal:
        // the index is fully reconstructible from `node_embeddings`. We log
        // a warning and proceed.
        #[cfg(feature = "vector-search")]
        if let Err(e) = self.persist_vector_index() {
            tracing::warn!(
                error = %e,
                "vector index sidecar persist failed in consolidation — next boot will rebuild"
            );
        }

        // ====================================================================
        // v1.9.0: Autonomic features (15-17)
        // ====================================================================

        // 15. Auto-promote memories with 3+ accesses in 24h (frequency-dependent potentiation)
        let auto_promoted = self.auto_promote_frequent_access().unwrap_or(0);
        promoted += auto_promoted;

        // 16. Retention Target System — auto-GC if avg retention below target
        let mut gc_triggered = false;
        {
            let retention_target: f64 = std::env::var("VESTIGE_RETENTION_TARGET")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.8);

            let avg_retention = self.get_avg_retention().unwrap_or(1.0);
            let total = self.get_stats().map(|s| s.total_nodes).unwrap_or(0);
            let below_target = self.count_memories_below_retention(0.3).unwrap_or(0);

            if avg_retention < retention_target && below_target > 0 {
                let gc_count = self.gc_below_retention(0.3, 30).unwrap_or(0);
                if gc_count > 0 {
                    gc_triggered = true;
                    tracing::info!(
                        avg_retention = avg_retention,
                        target = retention_target,
                        gc_count = gc_count,
                        "Retention target auto-GC: removed {} low-retention memories",
                        gc_count
                    );
                }
            }

            // 17. Save retention snapshot for trend tracking
            let _ = self.save_retention_snapshot(avg_retention, total, below_target, gc_triggered);
        }

        let duration = start.elapsed().as_millis() as i64;

        // Record consolidation history
        {
            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            let _ = writer.execute(
                "INSERT INTO consolidation_history (completed_at, duration_ms, memories_replayed, duplicates_merged, activations_computed, w20_optimized)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    Utc::now().to_rfc3339(),
                    duration,
                    decay_applied,
                    duplicates_merged,
                    activations_computed,
                    w20_optimized,
                ],
            );
        }

        Ok(ConsolidationResult {
            nodes_processed: decay_applied,
            nodes_promoted: promoted,
            nodes_pruned: 0,
            decay_applied,
            duration_ms: duration,
            embeddings_generated,
            duplicates_merged,
            neighbors_reinforced: 0,
            activations_computed,
            w20_optimized,
        })
    }

    /// Auto-deduplicate similar memories during consolidation (episodic → semantic merge)
    ///
    /// Finds clusters with cosine similarity > 0.85, keeps the strongest node,
    /// appends unique content from weaker nodes, and deletes duplicates.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    fn auto_dedup_consolidation(&self) -> Result<i64> {
        let all_embeddings = self.get_all_embeddings()?;
        let n = all_embeddings.len();

        if !(2..=2000).contains(&n) {
            return Ok(0);
        }

        const SIMILARITY_THRESHOLD: f32 = 0.85;
        let mut merged_count = 0i64;
        let mut consumed: std::collections::HashSet<String> = std::collections::HashSet::new();

        for i in 0..n {
            if consumed.contains(&all_embeddings[i].0) {
                continue;
            }

            let mut cluster: Vec<(usize, f32)> = Vec::new();

            for j in (i + 1)..n {
                if consumed.contains(&all_embeddings[j].0) {
                    continue;
                }
                let sim = crate::embeddings::cosine_similarity(
                    &all_embeddings[i].1,
                    &all_embeddings[j].1,
                );
                if sim >= SIMILARITY_THRESHOLD {
                    cluster.push((j, sim));
                }
            }

            if cluster.is_empty() {
                continue;
            }

            // Find the strongest node (highest retention_strength)
            let anchor_id = &all_embeddings[i].0;
            let reader = self
                .reader
                .lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            let anchor_retention: f64 = reader
                .query_row(
                    "SELECT retention_strength FROM knowledge_nodes WHERE id = ?1",
                    params![anchor_id],
                    |row| row.get(0),
                )
                .unwrap_or(0.0);

            let mut best_idx = i;
            let mut best_retention = anchor_retention;

            for &(j, _) in &cluster {
                let dup_id = &all_embeddings[j].0;
                let dup_retention: f64 = reader
                    .query_row(
                        "SELECT retention_strength FROM knowledge_nodes WHERE id = ?1",
                        params![dup_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0.0);
                if dup_retention > best_retention {
                    best_retention = dup_retention;
                    best_idx = j;
                }
            }

            let best_id = all_embeddings[best_idx].0.clone();

            // Get keeper's content
            let keeper_content: String = reader
                .query_row(
                    "SELECT content FROM knowledge_nodes WHERE id = ?1",
                    params![best_id],
                    |row| row.get(0),
                )
                .unwrap_or_default();

            // Collect weak node IDs (all nodes in cluster except the keeper)
            let mut weak_ids: Vec<String> = Vec::new();
            if best_idx != i {
                weak_ids.push(anchor_id.clone());
            }
            for &(j, _) in &cluster {
                if j != best_idx {
                    weak_ids.push(all_embeddings[j].0.clone());
                }
            }

            // Merge unique content from weak nodes
            let mut merged_content = keeper_content.clone();
            for weak_id in &weak_ids {
                let weak_content: String = reader
                    .query_row(
                        "SELECT content FROM knowledge_nodes WHERE id = ?1",
                        params![weak_id],
                        |row| row.get(0),
                    )
                    .unwrap_or_default();

                let weak_trimmed = weak_content.trim();
                if !merged_content.contains(weak_trimmed) && weak_trimmed.len() > 20 {
                    merged_content.push_str("\n\n[MERGED] ");
                    merged_content.push_str(weak_trimmed);
                }
            }

            // Drop reader before taking writer locks in update/delete
            drop(reader);

            // Update keeper with merged content
            if merged_content != keeper_content {
                let _ = self.update_node_content(&best_id, &merged_content);
            }

            // Delete weak nodes
            for weak_id in &weak_ids {
                let _ = self.delete_node(weak_id);
                consumed.insert(weak_id.clone());
                merged_count += 1;
            }

            consumed.insert(best_id);
        }

        Ok(merged_count)
    }

    /// Compute ACT-R base-level activation for all nodes from access history.
    /// B_i = ln(Σ t_j^(-d)) where t_j = days since j-th access, d = 0.5
    fn compute_act_r_activations(&self) -> Result<i64> {
        const ACT_R_DECAY: f64 = 0.5;
        let now = Utc::now();

        let node_ids: Vec<String> = {
            let reader = self
                .reader
                .lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            reader
                .prepare("SELECT DISTINCT node_id FROM memory_access_log")?
                .query_map([], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect()
        };

        if node_ids.is_empty() {
            return Ok(0);
        }

        let mut count = 0i64;
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let tx = writer.transaction()?;

        for node_id in &node_ids {
            let timestamps: Vec<String> = tx
                .prepare(
                    "SELECT accessed_at FROM memory_access_log
                     WHERE node_id = ?1
                     ORDER BY accessed_at DESC
                     LIMIT 500",
                )?
                .query_map(params![node_id], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();

            if timestamps.is_empty() {
                continue;
            }

            let mut sum_decay = 0.0_f64;
            for ts_str in &timestamps {
                let accessed_at = DateTime::parse_from_rfc3339(ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(now);
                let days_since = (now - accessed_at).num_seconds() as f64 / 86400.0;
                let t = days_since.max(0.001);
                sum_decay += t.powf(-ACT_R_DECAY);
            }

            let activation = sum_decay.ln();

            tx.execute(
                "UPDATE knowledge_nodes SET activation = ?1 WHERE id = ?2",
                params![activation, node_id],
            )?;
            count += 1;
        }

        tx.commit()?;
        Ok(count)
    }

    /// Prune old access log entries (keep last 90 days)
    fn prune_access_log(&self) -> Result<i64> {
        let cutoff = (Utc::now() - Duration::days(90)).to_rfc3339();
        let writer = self
            .writer
            .lock()
            .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
        let deleted = writer.execute(
            "DELETE FROM memory_access_log WHERE accessed_at < ?1",
            params![cutoff],
        )? as i64;
        Ok(deleted)
    }

    /// Optimize personalized w20 (forgetting curve decay) if enough access data exists.
    /// Uses FSRSOptimizer golden section search on real retrieval history.
    fn optimize_w20_if_ready(&self) -> Result<Option<f64>> {
        use crate::fsrs::{FSRSOptimizer, ReviewLog};

        let reader = self
            .reader
            .lock()
            .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;

        let access_count: i64 = reader
            .query_row("SELECT COUNT(*) FROM memory_access_log", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        if access_count < 100 {
            return Ok(None);
        }

        let mut optimizer = FSRSOptimizer::new();

        let logs: Vec<(String, String, String)> = reader
            .prepare(
                "SELECT mal.node_id, mal.access_type, mal.accessed_at
                 FROM memory_access_log mal
                 ORDER BY mal.accessed_at ASC
                 LIMIT 1000",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        for (node_id, access_type, accessed_at) in &logs {
            // Get node state for stability/difficulty
            let node_state: Option<(f64, f64, String)> = reader
                .query_row(
                    "SELECT stability, difficulty, created_at FROM knowledge_nodes WHERE id = ?1",
                    params![node_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            if let Some((stability, difficulty, created_at)) = node_state {
                let ts = DateTime::parse_from_rfc3339(accessed_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let created = DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(ts);

                let rating = match access_type.as_str() {
                    "promote" => 4,
                    "search_hit" => 3,
                    "demote" => 1,
                    _ => 3,
                };

                let elapsed = (ts - created).num_seconds() as f64 / 86400.0;

                optimizer.add_review(ReviewLog {
                    timestamp: ts,
                    rating,
                    stability,
                    difficulty,
                    elapsed_days: elapsed.max(0.001),
                });
            }
        }

        drop(reader);

        if !optimizer.has_enough_data() {
            return Ok(None);
        }

        let optimized_w20 = optimizer.optimize_decay();

        // Persist via the canonical helper so the loader at next boot picks
        // it up. Also continue writing the legacy `w20` key for backward
        // compatibility with any external tooling that learned to read it
        // — the two rows are kept in sync until we drop the legacy key.
        self.save_personalized_w20(optimized_w20)?;
        {
            let writer = self
                .writer
                .lock()
                .map_err(|_| StorageError::Init("Writer lock poisoned".into()))?;
            writer.execute(
                "INSERT OR REPLACE INTO fsrs_config (key, value, updated_at)
                 VALUES ('w20', ?1, ?2)",
                params![optimized_w20, Utc::now().to_rfc3339()],
            )?;
        }

        // Apply immediately to the in-memory scheduler so subsequent reviews
        // in the same process use the personalized decay without waiting for
        // a restart. Best-effort: a swap failure logs and continues; the
        // optimized value still lands on disk for the next boot.
        if let Err(e) = self.apply_personalized_weights() {
            tracing::warn!(
                error = %e,
                "personalized w20 saved but could not be applied to running scheduler"
            );
        }

        tracing::info!(
            w20 = optimized_w20,
            "Personalized w20 optimized from access history"
        );

        Ok(Some(optimized_w20))
    }

    /// Generate missing embeddings
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    fn generate_missing_embeddings(&self) -> Result<i64> {
        if !self.embedding_service.is_ready()
            && let Err(e) = self.embedding_service.init()
        {
            tracing::warn!("Could not initialize embedding model: {}", e);
            return Ok(0);
        }

        // Backfill batch size. 1000 covers most personal databases in a single
        // consolidation cycle. For larger databases, run consolidation multiple
        // times or use the dedicated `regenerate_embeddings` MCP tool which has
        // no per-call cap.
        let nodes: Vec<(String, String)> = {
            let reader = self
                .reader
                .lock()
                .map_err(|_| StorageError::Init("Reader lock poisoned".into()))?;
            reader
                .prepare(
                    "SELECT id, content FROM knowledge_nodes
                     WHERE has_embedding = 0 OR has_embedding IS NULL
                     LIMIT 1000",
                )?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect()
        };

        let mut count = 0i64;

        for (id, content) in nodes {
            if let Err(e) = self.generate_embedding_for_node(&id, &content) {
                tracing::warn!("Failed to generate embedding for {}: {}", id, e);
            } else {
                count += 1;
            }
        }

        Ok(count)
    }
}

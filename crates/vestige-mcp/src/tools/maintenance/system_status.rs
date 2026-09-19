//! `system_status` tool — combined health check + stats + cognitive overview.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::{FSRSScheduler, Storage};

use crate::cognitive::CognitiveEngine;

pub fn system_status_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

/// Combined system status tool (merges health_check + stats, v1.7.0)
///
/// Returns system health status, full statistics, FSRS preview,
/// cognitive module health, state distribution, and actionable recommendations.
pub async fn execute_system_status(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    _args: Option<Value>,
) -> Result<Value, String> {
    // get_stats touches several tables; offload to the blocking pool.
    let storage_stats = storage.clone();
    let stats = tokio::task::spawn_blocking(move || storage_stats.get_stats())
        .await
        .map_err(|e| format!("get_stats task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    // === Health assessment ===
    let status = if stats.total_nodes == 0 {
        "empty"
    } else if stats.average_retention < 0.3 {
        "critical"
    } else if stats.average_retention < 0.5 {
        "degraded"
    } else {
        "healthy"
    };

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    let embedding_ready = storage.is_embedding_ready();

    let mut warnings = Vec::new();
    if stats.average_retention < 0.5 && stats.total_nodes > 0 {
        warnings.push("Low average retention - consider running consolidation");
    }
    if stats.nodes_due_for_review > 10 {
        warnings.push("Many memories are due for review");
    }
    if stats.total_nodes > 0 && stats.nodes_with_embeddings == 0 {
        warnings.push("No embeddings generated - semantic search unavailable");
    }
    if embedding_coverage < 50.0 && stats.total_nodes > 10 {
        warnings.push("Low embedding coverage - run consolidate to improve semantic search");
    }

    let mut recommendations = Vec::new();
    if status == "critical" {
        recommendations
            .push("CRITICAL: Many memories have very low retention. Review important memories.");
    }
    if stats.nodes_due_for_review > 5 {
        recommendations.push("Review due memories to strengthen retention.");
    }
    if stats.nodes_with_embeddings < stats.total_nodes {
        recommendations.push("Run 'consolidate' to generate missing embeddings.");
    }
    if stats.total_nodes > 100 && stats.average_retention < 0.7 {
        recommendations.push("Consider running periodic consolidation.");
    }
    if status == "healthy" && recommendations.is_empty() {
        recommendations.push("Memory system is healthy!");
    }

    // === State distribution ===
    let storage_nodes = storage.clone();
    let nodes = tokio::task::spawn_blocking(move || storage_nodes.get_all_nodes(500, 0))
        .await
        .map_err(|e| format!("get_all_nodes task panicked: {}", e))?
        .map_err(|e| e.to_string())?;
    let total = nodes.len();
    let (active, dormant, silent, unavailable) = if total > 0 {
        let mut a = 0usize;
        let mut d = 0usize;
        let mut s = 0usize;
        let mut u = 0usize;
        for node in &nodes {
            let accessibility = node.retention_strength * 0.5
                + node.retrieval_strength * 0.3
                + node.storage_strength * 0.2;
            if accessibility >= 0.7 {
                a += 1;
            } else if accessibility >= 0.4 {
                d += 1;
            } else if accessibility >= 0.1 {
                s += 1;
            } else {
                u += 1;
            }
        }
        (a, d, s, u)
    } else {
        (0, 0, 0, 0)
    };

    // === FSRS Preview ===
    let scheduler = FSRSScheduler::default();
    let fsrs_preview = if let Some(representative) = nodes.first() {
        let mut state = scheduler.new_card();
        state.difficulty = representative.difficulty;
        state.stability = representative.stability;
        state.reps = representative.reps;
        state.lapses = representative.lapses;
        state.last_review = representative.last_accessed;
        let elapsed = scheduler.days_since_review(&state.last_review);
        let preview = scheduler.preview_reviews(&state, elapsed);
        Some(serde_json::json!({
            "representativeMemoryId": representative.id,
            "elapsedDays": format!("{:.1}", elapsed),
            "intervalIfGood": preview.good.interval,
            "intervalIfEasy": preview.easy.interval,
            "intervalIfHard": preview.hard.interval,
            "currentRetrievability": format!("{:.3}", preview.good.retrievability),
        }))
    } else {
        None
    };

    // === Cognitive health + reranker readiness ===
    //
    // `rerankerReady`/`rerankerStatus` answer a question monitoring keeps
    // asking: are search results coming from the cross-encoder, or are we
    // still serving BM25 fallback? main.rs loads Jina v2 (~1.1GB) on a
    // background task after stdio handshake; during that warm-up the
    // reranker exists but `has_cross_encoder()` is false. We surface that
    // explicitly so the dashboard can show "warming…" instead of a green
    // tick that lies.
    //
    // tri-state status:
    // - "ready":    cross-encoder loaded, neural reranking active
    // - "warming":  `embeddings` feature on, model not yet loaded
    // - "disabled": built without the `embeddings` feature
    //
    // We probe under `try_lock` so a busy reranker (e.g. another search in
    // flight) doesn't stall status calls. If the lock is contended we
    // optimistically report `warming` — the next status call will reflect
    // truth and this matches dashboards' polling cadence.
    let (cognitive_health, reranker_ready, reranker_status) = if let Ok(cog) = cognitive.try_lock()
    {
        // `activation_network` is now its own lock — use try_read so a
        // contended status call doesn't stall behind a long-running
        // search/ingest path. `0` is the safe fallback.
        let activation_count = cog
            .activation_network
            .try_read()
            .map(|n| n.get_associations("_probe_").len())
            .unwrap_or(0);
        let prediction_accuracy = cog.predictive_memory.prediction_accuracy().unwrap_or(0.0);
        let scheduler_stats = cog.consolidation_scheduler.get_activity_stats();
        let health = Some(serde_json::json!({
            "activationNetworkSize": activation_count,
            "predictionAccuracy": format!("{:.2}", prediction_accuracy),
            "modulesActive": 28,
            "schedulerStats": {
                "totalEvents": scheduler_stats.total_events,
                "eventsPerMinute": scheduler_stats.events_per_minute,
                "isIdle": scheduler_stats.is_idle,
                "timeUntilNextConsolidation": format!("{:?}", cog.consolidation_scheduler.time_until_next()),
            },
        }));
        let ready = cog.reranker.has_cross_encoder();
        let status = if !cfg!(feature = "embeddings") {
            "disabled"
        } else if ready {
            "ready"
        } else {
            "warming"
        };
        (health, ready, status)
    } else {
        crate::cognitive::try_lock_metrics::record_miss("system_status");
        // Lock contention — assume warming rather than ready. False
        // negatives here are safe (a brief "warming" blip); a false
        // positive would mislead monitoring.
        let status = if cfg!(feature = "embeddings") {
            "warming"
        } else {
            "disabled"
        };
        (None, false, status)
    };

    // === try_lock skip metrics (operational visibility into cognitive contention) ===
    let try_lock_skips = crate::cognitive::try_lock_metrics::snapshot();

    // === Automation triggers (for conditional dream/backup/gc at session start) ===
    // Three sequential single-row SELECTs plus an optional COUNT. Cheap each,
    // but never run blocking SQLite on the reactor — batch them.
    let storage_auto = storage.clone();
    let total_for_fallback = stats.total_nodes;
    let (last_consolidation, last_dream, saves_since_last_dream) =
        tokio::task::spawn_blocking(move || {
            let last_consolidation = storage_auto.get_last_consolidation().ok().flatten();
            let last_dream = storage_auto.get_last_dream().ok().flatten();
            let saves_since_last_dream = match &last_dream {
                Some(dt) => storage_auto.count_memories_since(*dt).unwrap_or(0),
                None => total_for_fallback,
            };
            (last_consolidation, last_dream, saves_since_last_dream)
        })
        .await
        .map_err(|e| format!("automation triggers task panicked: {}", e))?;
    let last_backup = Storage::get_last_backup_timestamp();

    Ok(serde_json::json!({
        "tool": "system_status",
        // Health
        "status": status,
        "warnings": warnings,
        "recommendations": recommendations,
        "embeddingReady": embedding_ready,
        // Stats
        "totalMemories": stats.total_nodes,
        "dueForReview": stats.nodes_due_for_review,
        "averageRetention": stats.average_retention,
        "averageStorageStrength": stats.average_storage_strength,
        "averageRetrievalStrength": stats.average_retrieval_strength,
        "withEmbeddings": stats.nodes_with_embeddings,
        "embeddingCoverage": format!("{:.1}%", embedding_coverage),
        "embeddingModel": stats.embedding_model,
        "oldestMemory": stats.oldest_memory.map(|dt| dt.to_rfc3339()),
        "newestMemory": stats.newest_memory.map(|dt| dt.to_rfc3339()),
        // Distribution
        "stateDistribution": {
            "active": active,
            "dormant": dormant,
            "silent": silent,
            "unavailable": unavailable,
            "sampled": total,
        },
        // FSRS
        "fsrsPreview": fsrs_preview,
        // Cognitive
        "cognitiveHealth": cognitive_health,
        "rerankerReady": reranker_ready,
        "rerankerStatus": reranker_status,
        "tryLockSkips": try_lock_skips,
        // Automation triggers — Claude uses these to decide when to dream/backup/gc
        "automationTriggers": {
            "lastDreamTimestamp": last_dream.map(|dt| dt.to_rfc3339()),
            "savesSinceLastDream": saves_since_last_dream,
            "lastBackupTimestamp": last_backup.map(|dt| dt.to_rfc3339()),
            "lastConsolidationTimestamp": last_consolidation.map(|dt| dt.to_rfc3339()),
        },
    }))
}

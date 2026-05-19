//! Dashboard handlers — stats, health, retention distribution
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use std::collections::HashMap;

use super::super::state::AppState;
use super::super::wire::{
    DashboardLimitsDto, EndangeredMemoryDto, HealthCheckDto, HealthStatus, RetentionBucketDto,
    RetentionDistributionDto, SystemStatsDto,
};
use super::{log_err, log_join_err};

/// Get system stats — totals, averages, embedding coverage.
pub async fn get_stats(State(state): State<AppState>) -> Result<Json<SystemStatsDto>, StatusCode> {
    let storage = state.storage.clone();
    let stats = tokio::task::spawn_blocking(move || storage.get_stats())
        .await
        .map_err(log_join_err("get_stats task panicked"))?
        .map_err(log_err("storage operation"))?;

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(SystemStatsDto {
        total_memories: stats.total_nodes,
        due_for_review: stats.nodes_due_for_review,
        average_retention: stats.average_retention,
        average_storage_strength: stats.average_storage_strength,
        average_retrieval_strength: stats.average_retrieval_strength,
        with_embeddings: stats.nodes_with_embeddings,
        embedding_coverage,
        embedding_model: stats.embedding_model.unwrap_or_default(),
        oldest_memory: stats.oldest_memory.map(|dt| dt.to_rfc3339()),
        newest_memory: stats.newest_memory.map(|dt| dt.to_rfc3339()),
    }))
}

/// Health check — derives a single-word status from the average
/// retention. Mirrors the legacy threshold ladder
/// (empty → critical < 0.3 → degraded < 0.5 → healthy).
pub async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<HealthCheckDto>, StatusCode> {
    let storage = state.storage.clone();
    let stats = tokio::task::spawn_blocking(move || storage.get_stats())
        .await
        .map_err(log_join_err("get_stats task panicked"))?
        .map_err(log_err("storage operation"))?;

    let status = if stats.total_nodes == 0 {
        HealthStatus::Empty
    } else if stats.average_retention < 0.3 {
        HealthStatus::Critical
    } else if stats.average_retention < 0.5 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    };

    Ok(Json(HealthCheckDto {
        status,
        total_memories: stats.total_nodes,
        average_retention: stats.average_retention,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

/// Get retention distribution (for histogram + heatmap visualization).
pub async fn retention_distribution(
    State(state): State<AppState>,
) -> Result<Json<RetentionDistributionDto>, StatusCode> {
    // Cap at 1000 to prevent excessive memory usage on large databases.
    // Even capped, a full SELECT + per-row deserialization runs >10ms on
    // a warm DB, so dispatch to the blocking pool.
    let storage = state.storage.clone();
    let nodes = tokio::task::spawn_blocking(move || storage.get_all_nodes(1000, 0))
        .await
        .map_err(log_join_err("get_all_nodes task panicked"))?
        .map_err(log_err("storage operation"))?;

    let mut buckets = [0u32; 10];
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut endangered = Vec::new();

    for node in &nodes {
        let bucket = ((node.retention_strength * 10.0).floor() as usize).min(9);
        buckets[bucket] += 1;
        *by_type.entry(node.node_type.clone()).or_default() += 1;

        if node.retention_strength < 0.3 {
            endangered.push(EndangeredMemoryDto {
                id: node.id.clone(),
                content: node.content.chars().take(60).collect::<String>(),
                retention: node.retention_strength,
                node_type: node.node_type.clone(),
            });
        }
    }

    let distribution: Vec<RetentionBucketDto> = buckets
        .iter()
        .enumerate()
        .map(|(i, &count)| RetentionBucketDto {
            range: format!("{}-{}%", i * 10, (i + 1) * 10),
            count,
        })
        .collect();

    Ok(Json(RetentionDistributionDto {
        distribution,
        by_type,
        endangered,
        total: nodes.len(),
    }))
}

/// Expose the dashboard's compile-time limits at `GET /api/_meta/limits`.
///
/// The frontend fetches this once on boot — used to seed the graph
/// node-cap input, the search "load more" threshold, and the WebSocket
/// event ring buffer. Living next to the constants in
/// `wire::limits::DashboardLimitsDto::DEFAULT` keeps backend clamp
/// values and frontend defaults in lockstep.
pub async fn get_limits() -> Json<DashboardLimitsDto> {
    Json(DashboardLimitsDto::DEFAULT)
}

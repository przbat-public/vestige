//! Dashboard handlers — memory search
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;

use super::super::events::VestigeEvent;
use super::super::state::AppState;
use super::super::wire::{MemoryDto, SearchResultDto};
use super::{log_err, log_join_err};

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<i32>,
    pub min_retention: Option<f64>,
}

/// Search memories with hybrid search (BM25 + cosine, 0.3/0.7 weighting).
pub async fn search_memories(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResultDto>, StatusCode> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let start = std::time::Instant::now();

    // hybrid_search re-embeds the query (~150–500ms on cold cache) and
    // touches BM25 + HNSW indices, so it blocks longer than the 10ms
    // soft-limit Tokio sets for its async reactor threads.
    //
    // Goes through `crate::retrieval` so the documented no-embeddings build
    // answers this route on FTS5 instead of failing to compile.
    let storage = state.storage.clone();
    let query = params.q.clone();
    let (kw, sem) = vestige_core::default_hybrid_weights();
    let results = tokio::task::spawn_blocking(move || {
        crate::retrieval::hybrid_search(&storage, &query, limit, kw, sem)
    })
    .await
    .map_err(log_join_err("hybrid_search task panicked"))?
    .map_err(log_err("storage operation"))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let result_ids: Vec<String> = results.iter().map(|r| r.node.id.clone()).collect();

    state.emit(VestigeEvent::SearchPerformed {
        query: params.q.clone(),
        result_count: results.len(),
        result_ids,
        duration_ms,
        timestamp: Utc::now(),
    });

    let memories: Vec<MemoryDto> = results
        .into_iter()
        .filter(|r| {
            params
                .min_retention
                .is_none_or(|min| r.node.retention_strength >= min)
        })
        .map(|r| {
            let score = f64::from(r.combined_score);
            MemoryDto::from(&r.node)
                .into_list_view()
                .with_combined_score(score)
        })
        .collect();

    Ok(Json(SearchResultDto {
        query: params.q,
        total: memories.len(),
        duration_ms,
        results: memories,
    }))
}

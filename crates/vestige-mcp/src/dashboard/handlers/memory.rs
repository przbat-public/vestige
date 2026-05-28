//! Dashboard handlers — memory CRUD
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;

use super::super::events::VestigeEvent;
use super::super::state::AppState;
use super::super::wire::{
    MemoryDto, MemoryListResponseDto, MemoryStatusDto, MemoryUpdateResultDto,
};
use super::super::wire::memory::MemoryStatusAction;
use super::{log_err, log_join_err};

#[derive(Debug, Deserialize)]
pub struct MemoryListParams {
    pub q: Option<String>,
    // `alias = "type"` keeps older dashboard builds (and any external caller
    // that read the original endpoint) working after the v3.3.x filter rename.
    // The canonical name stays `node_type` to match the storage column.
    #[serde(alias = "type")]
    pub node_type: Option<String>,
    pub tag: Option<String>,
    pub min_retention: Option<f64>,
    pub sort: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

/// List memories with optional search.
///
/// Returns a `MemoryListResponseDto`. List view drops sentiment, validity
/// window, and timing fields via `into_list_view()` so the wire payload
/// stays compact for the table render path.
pub async fn list_memories(
    State(state): State<AppState>,
    Query(params): Query<MemoryListParams>,
) -> Result<Json<MemoryListResponseDto>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    if let Some(query) = params.q.as_ref().filter(|q| !q.trim().is_empty()) {
        // Hybrid search re-embeds the query (~150–500ms on cold cache) and
        // touches BM25 + HNSW indices. Run it on the blocking pool so other
        // dashboard requests don't queue up on the reactor thread.
        let storage = state.storage.clone();
        let q = query.clone();
        let (kw, sem) = vestige_core::default_hybrid_weights();
        let results =
            tokio::task::spawn_blocking(move || storage.hybrid_search(&q, limit, kw, sem))
                .await
                .map_err(log_join_err("hybrid_search task panicked"))?
                .map_err(log_err("storage operation"))?;

        let memories: Vec<MemoryDto> = results
            .into_iter()
            .filter(|r| {
                if let Some(min_ret) = params.min_retention {
                    r.node.retention_strength >= min_ret
                } else {
                    true
                }
            })
            .map(|r| {
                // combined_score is f32 in storage; widen to f64 to match
                // the JS Number representation on the wire (avoids a
                // precision-loss surprise on the dashboard side).
                let score = f64::from(r.combined_score);
                MemoryDto::from(&r.node)
                    .into_list_view()
                    .with_combined_score(score)
            })
            .collect();

        return Ok(Json(MemoryListResponseDto {
            total: memories.len(),
            memories,
        }));
    }

    // No search query — list all memories. We push the optional
    // `node_type`, `tag`, and `min_retention` filters into SQL so LIMIT is
    // applied to the filtered result, not to the unfiltered prefix.
    // Previously the handler did `get_all_nodes(LIMIT) → retain(...)`,
    // which under-returned whenever the LIMIT prefix happened to be
    // dominated by rows that the filter rejected. (Audit 2026-05-22.)
    //
    // `total` is the population matching the WHERE clause — separate
    // from the page size. Previously the handler returned `memories.len()`,
    // which capped the displayed total at the LIMIT and made pagination
    // impossible. (Audit 2026-05-22.)
    let storage = state.storage.clone();
    let node_type_filter = params.node_type.clone();
    let tag_filter = params.tag.clone();
    let min_ret_filter = params.min_retention;
    let (nodes, total) = tokio::task::spawn_blocking(move || -> vestige_core::Result<_> {
        let total = storage.count_nodes_filtered(
            node_type_filter.as_deref(),
            tag_filter.as_deref(),
            min_ret_filter,
        )?;
        let page = storage.get_all_nodes_filtered(
            limit,
            offset,
            node_type_filter.as_deref(),
            tag_filter.as_deref(),
            min_ret_filter,
        )?;
        Ok((page, total))
    })
    .await
    .map_err(log_join_err("list_memories task panicked"))?
    .map_err(log_err("storage operation"))?;

    let memories: Vec<MemoryDto> = nodes
        .iter()
        .map(|n| MemoryDto::from(n).into_list_view())
        .collect();

    Ok(Json(MemoryListResponseDto { total, memories }))
}

/// Get a single memory by ID — returns the full `MemoryDto` (sentiment,
/// validity window, last_accessed, next_review).
pub async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryDto>, StatusCode> {
    let node = state
        .storage
        .get_node(&id)
        .map_err(log_err("get memory"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(MemoryDto::from(&node)))
}

/// Delete a memory by ID. Returns `MemoryStatusDto { action: Deleted }`
/// so the dashboard can confirm the operation; retention is the value
/// the node had immediately before deletion (0.0 if unknown — we don't
/// re-fetch).
pub async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryStatusDto>, StatusCode> {
    // Capture retention BEFORE deletion so the response carries an
    // accurate "what was promoted away" value. Storage `get_node` is a
    // single SQL query; the cost is negligible compared to the actual
    // delete.
    let prior_retention = state
        .storage
        .get_node(&id)
        .map_err(log_err("get memory before delete"))?
        .map(|n| n.retention_strength)
        .unwrap_or(0.0);

    let deleted = state
        .storage
        .delete_node(&id)
        .map_err(log_err("storage operation"))?;

    if deleted {
        state.emit(VestigeEvent::MemoryDeleted {
            id: id.clone(),
            timestamp: chrono::Utc::now(),
        });
        Ok(Json(MemoryStatusDto {
            ok: true,
            id,
            retention_strength: prior_retention,
            action: MemoryStatusAction::Deleted,
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Promote a memory (retention bump). Returns the post-promote retention.
pub async fn promote_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryStatusDto>, StatusCode> {
    let node = state
        .storage
        .promote_memory(&id)
        .map_err(log_err("storage operation"))?;

    state.emit(VestigeEvent::MemoryPromoted {
        id: node.id.clone(),
        new_retention: node.retention_strength,
        timestamp: chrono::Utc::now(),
    });

    Ok(Json(MemoryStatusDto {
        ok: true,
        id: node.id,
        retention_strength: node.retention_strength,
        action: MemoryStatusAction::Promoted,
    }))
}

/// Demote a memory (retention drop). Mirror of `promote_memory`.
pub async fn demote_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryStatusDto>, StatusCode> {
    let node = state
        .storage
        .demote_memory(&id)
        .map_err(log_err("storage operation"))?;

    state.emit(VestigeEvent::MemoryDemoted {
        id: node.id.clone(),
        new_retention: node.retention_strength,
        timestamp: chrono::Utc::now(),
    });

    Ok(Json(MemoryStatusDto {
        ok: true,
        id: node.id,
        retention_strength: node.retention_strength,
        action: MemoryStatusAction::Demoted,
    }))
}

/// Body for `PATCH /api/memories/{id}` — both fields optional.
/// Either or both can be supplied. Empty body returns 400.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemoryBody {
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Body for `POST /api/smart_ingest`.
///
/// Mirrors the MCP `smart_ingest` tool's single-mode arguments. We accept
/// camelCase here to match the rest of the dashboard wire format and rename
/// to snake_case before handing the value to `tools::smart_ingest::execute`,
/// which expects the MCP-tool-native casing.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartIngestBody {
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    /// When true, bypass the prediction-error gate (similarity check) and
    /// always create a new node. Maps to `smart_ingest`'s `force_create`.
    #[serde(default)]
    pub force_create: bool,
}

/// `POST /api/smart_ingest` — create a memory from the dashboard.
///
/// Thin REST wrapper over `tools::smart_ingest::execute`, which handles the
/// full ingest pipeline (importance scoring, intent detection, preprocessing,
/// prediction-error gating, near-duplicate detection). The dashboard surfaces
/// the same affordances (`compound_content_warning`, `near_duplicate_warning`,
/// `decision`) the MCP tool returns, so users see exactly what the engine
/// decided to do with their input.
///
/// Emits `MemoryCreated` so other dashboard tabs and connected clients see
/// the new node without polling.
pub async fn smart_ingest_memory(
    State(state): State<AppState>,
    Json(body): Json<SmartIngestBody>,
) -> Result<Json<Value>, StatusCode> {
    let cognitive = state
        .cognitive
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    if body.content.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // The tool expects snake_case `force_create` and `node_type` — we
    // rebuild the args here rather than re-deriving Serialize on
    // `SmartIngestBody` so the dashboard contract stays decoupled from the
    // MCP tool's argument schema.
    let args = serde_json::json!({
        "content": body.content,
        "tags": body.tags,
        "node_type": body.node_type,
        "source": body.source.clone().or(Some("dashboard".to_string())),
        "force_create": body.force_create,
        "agent": "dashboard",
    });

    let result = crate::tools::smart_ingest::execute(&state.storage, cognitive, Some(args))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "smart_ingest failed");
            // Tool returns user-friendly errors for validation issues
            // (empty content, content too large, missing fields). Map them
            // to 400 so the dashboard surfaces them inline without forcing
            // the user to open a console.
            if e.contains("empty")
                || e.contains("too large")
                || e.contains("Missing")
                || e.contains("Invalid")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    // Emit MemoryCreated only on a real new-node decision. `update`,
    // `reinforce`, `merge`, etc. mutate an existing node — we want
    // MemoryUpdated semantics for those, but the tool doesn't expose
    // enough to differentiate cleanly. Conservative: emit Created for
    // create/supersede, Updated for the rest, when we have a node id.
    let decision = result["decision"].as_str().unwrap_or("");
    let node_id = result["nodeId"].as_str().unwrap_or("").to_string();
    if !node_id.is_empty() {
        let preview: String = body.content.chars().take(80).collect();
        let now = chrono::Utc::now();
        let node_type = body.node_type.clone().unwrap_or_else(|| "fact".to_string());
        if matches!(decision, "create" | "supersede") {
            state.emit(VestigeEvent::MemoryCreated {
                id: node_id,
                content_preview: preview,
                node_type,
                tags: body.tags.clone(),
                timestamp: now,
            });
        } else {
            state.emit(VestigeEvent::MemoryUpdated {
                id: node_id,
                content_preview: preview,
                field: "smart_ingest".to_string(),
                timestamp: now,
            });
        }
    }

    Ok(Json(result))
}

/// Edit a memory's content and/or tags. Returns `MemoryUpdateResultDto`
/// — the post-update memory plus a `field` discriminator
/// ("content" | "tags" | "content+tags") so the dashboard can label the
/// confirmation toast precisely.
///
/// Updating content schedules embedding regeneration server-side (see
/// `Storage::update_node_content`).
pub async fn update_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMemoryBody>,
) -> Result<Json<MemoryUpdateResultDto>, StatusCode> {
    if body.content.is_none() && body.tags.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Some(ref new_content) = body.content {
        let trimmed = new_content.trim();
        if trimmed.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        // update_node_content regenerates the embedding (~150–500ms), so
        // dispatch it to the blocking pool to keep the reactor free.
        let storage = state.storage.clone();
        let id_owned = id.clone();
        let trimmed_owned = trimmed.to_string();
        tokio::task::spawn_blocking(move || storage.update_node_content(&id_owned, &trimmed_owned))
            .await
            .map_err(log_join_err("update_node_content task panicked"))?
            .map_err(log_err("update_node_content"))?;
    }

    if let Some(ref new_tags) = body.tags {
        state
            .storage
            .update_node_tags(&id, new_tags)
            .map_err(log_err("update_node_tags"))?;
    }

    let node = state
        .storage
        .get_node(&id)
        .map_err(log_err("get_node after update"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    let preview: String = node.content.chars().take(80).collect();
    let field = match (body.content.is_some(), body.tags.is_some()) {
        (true, true) => "content+tags",
        (true, false) => "content",
        (false, true) => "tags",
        _ => "unknown",
    };
    state.emit(VestigeEvent::MemoryUpdated {
        id: node.id.clone(),
        content_preview: preview,
        field: field.to_string(),
        timestamp: chrono::Utc::now(),
    });

    let mut memory = MemoryDto::from(&node);
    // Drop validity window from the update response — the dashboard
    // edit surface only shows content + tags, never temporal validity.
    memory.valid_from = None;
    memory.valid_until = None;
    memory.next_review_at = None;
    memory.sentiment_score = None;
    memory.sentiment_magnitude = None;

    Ok(Json(MemoryUpdateResultDto {
        memory,
        field: field.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use std::sync::Arc;
    use tempfile::tempdir;
    use vestige_core::Storage;
    use vestige_core::memory::IngestInput;

    fn make_state() -> (AppState, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let storage = Arc::new(Storage::new(Some(db_path)).unwrap());
        let cognitive = Some(Arc::new(tokio::sync::Mutex::new(CognitiveEngine::new())));
        (AppState::new(storage, cognitive), dir)
    }

    fn seed(state: &AppState, n: usize, node_type: &str) {
        for i in 0..n {
            state
                .storage
                .ingest(IngestInput {
                    content: format!("memory {} {}", node_type, i),
                    node_type: node_type.to_string(),
                    tags: vec!["seed".to_string()],
                    ..Default::default()
                })
                .unwrap();
        }
    }

    /// The previous handler returned `memories.len()` as `total`, which
    /// silently capped the dashboard's "X of Y" at the page size.
    /// Now `total` reflects the population for the matching filter.
    #[tokio::test]
    async fn list_memories_total_reflects_population_not_page_size() {
        let (state, _dir) = make_state();
        seed(&state, 25, "fact");

        let resp = list_memories(
            State(state),
            Query(MemoryListParams {
                q: None,
                node_type: None,
                tag: None,
                min_retention: None,
                sort: None,
                limit: Some(10),
                offset: Some(0),
            }),
        )
        .await
        .expect("list should succeed");

        assert_eq!(resp.0.memories.len(), 10, "page respects LIMIT");
        assert_eq!(resp.0.total, 25, "total reflects the full population");
    }

    /// The total must be filter-aware. Asking for `node_type=note` against a
    /// 25-fact population must report 0, not 25.
    #[tokio::test]
    async fn list_memories_total_respects_filters() {
        let (state, _dir) = make_state();
        seed(&state, 25, "fact");
        seed(&state, 4, "note");

        let resp = list_memories(
            State(state),
            Query(MemoryListParams {
                q: None,
                node_type: Some("note".to_string()),
                tag: None,
                min_retention: None,
                sort: None,
                limit: Some(50),
                offset: Some(0),
            }),
        )
        .await
        .expect("list should succeed");

        assert_eq!(resp.0.memories.len(), 4);
        assert_eq!(resp.0.total, 4);
    }

    /// Page 2 of an offset-based pagination — total stays constant, memories
    /// shift to the next slice.
    #[tokio::test]
    async fn list_memories_paginates_with_offset() {
        let (state, _dir) = make_state();
        seed(&state, 15, "fact");

        let page1 = list_memories(
            State(state.clone()),
            Query(MemoryListParams {
                q: None,
                node_type: None,
                tag: None,
                min_retention: None,
                sort: None,
                limit: Some(5),
                offset: Some(0),
            }),
        )
        .await
        .expect("page1");
        let page2 = list_memories(
            State(state),
            Query(MemoryListParams {
                q: None,
                node_type: None,
                tag: None,
                min_retention: None,
                sort: None,
                limit: Some(5),
                offset: Some(5),
            }),
        )
        .await
        .expect("page2");

        assert_eq!(page1.0.total, 15);
        assert_eq!(page2.0.total, 15);
        assert_eq!(page1.0.memories.len(), 5);
        assert_eq!(page2.0.memories.len(), 5);
        // No overlap — different ids on every page slot.
        let p1_ids: std::collections::HashSet<_> =
            page1.0.memories.iter().map(|m| m.id.clone()).collect();
        let p2_ids: std::collections::HashSet<_> =
            page2.0.memories.iter().map(|m| m.id.clone()).collect();
        assert!(p1_ids.is_disjoint(&p2_ids), "pages must not overlap");
    }
}

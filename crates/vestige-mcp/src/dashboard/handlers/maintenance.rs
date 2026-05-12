//! Dashboard handlers — maintenance ops (GC, backup, dedup, regenerate)
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;

use super::super::state::AppState;

fn maintenance_err(tool: &'static str) -> impl Fn(String) -> StatusCode {
    move |e| {
        tracing::error!(tool = tool, error = %e, "maintenance handler failed");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// `POST /api/maintenance/regenerate-embeddings` — backfill or rebuild embeddings.
pub async fn maintenance_regenerate_embeddings(
    State(state): State<AppState>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, StatusCode> {
    let args = body.map(|Json(v)| v);
    crate::tools::maintenance::execute_regenerate_embeddings(&state.storage, args)
        .await
        .map(Json)
        .map_err(maintenance_err("regenerate_embeddings"))
}

/// `POST /api/maintenance/find-duplicates` — return clusters of near-duplicate memories.
pub async fn maintenance_find_duplicates(
    State(state): State<AppState>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, StatusCode> {
    let args = body.map(|Json(v)| v);
    crate::tools::dedup::execute(&state.storage, args)
        .await
        .map(Json)
        .map_err(maintenance_err("find_duplicates"))
}

/// `POST /api/maintenance/gc` — list (or delete, with dry_run=false) low-retention memories.
pub async fn maintenance_gc(
    State(state): State<AppState>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, StatusCode> {
    let args = body.map(|Json(v)| v);
    crate::tools::maintenance::execute_gc(&state.storage, args)
        .await
        .map(Json)
        .map_err(maintenance_err("gc"))
}

/// `POST /api/maintenance/backup` — write a SQLite snapshot to ~/.vestige/backups.
pub async fn maintenance_backup(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    crate::tools::maintenance::execute_backup(&state.storage, None)
        .await
        .map(Json)
        .map_err(maintenance_err("backup"))
}

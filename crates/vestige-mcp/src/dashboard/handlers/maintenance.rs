//! Dashboard handlers — maintenance ops (GC, backup, dedup, regenerate, erase)
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;

use super::super::state::AppState;
use super::super::wire::{EraseRequestDto, EraseResponseDto};
use crate::tools::erase::EraseError;

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

/// `POST /api/maintenance/backup` — write a SQLite snapshot into the Vestige data
/// directory (`…/com.vestige.core/backups`, owner-only permissions). The snapshot
/// path is returned in the response body.
pub async fn maintenance_backup(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    crate::tools::maintenance::execute_backup(&state.storage, None)
        .await
        .map(Json)
        .map_err(maintenance_err("backup"))
}

/// `POST /api/maintenance/erase` — GDPR Article 17 erasure.
///
/// Thin wrapper around [`crate::tools::erase`]: same core call
/// (`right_to_erasure` / `erase_by_tag`), same confirmation gate, so the REST
/// surface cannot drift from the MCP tool. Erasure is irreversible, hence the
/// same defaults — `dryRun` is true unless the body says otherwise, and the
/// destructive pass needs `confirmed: true`.
///
/// Status codes:
/// - 200 — the report (which is also the complete answer for a dry run),
/// - 400 — invalid arguments *or* a missing `confirmed` acknowledgement. A
///   refusal is the client's fault, not a server fault, so it must not be
///   reported as a 500: a 5xx here would look like "the erasure was attempted
///   and failed", which is precisely the ambiguity an Art. 17 request cannot
///   afford.
/// - 500 — storage failed; core runs the erasure in one transaction, so
///   nothing was deleted.
///
/// There is no 404 for an id that does not exist: erasure is idempotent and
/// the report (`matched: 0`) is a better answer than an error, because a
/// repeated Art. 17 request must succeed rather than fail once the data is
/// already gone.
pub async fn maintenance_erase(
    State(state): State<AppState>,
    Json(req): Json<EraseRequestDto>,
) -> Result<Json<EraseResponseDto>, StatusCode> {
    let args = serde_json::json!({
        "action": req.action,
        "id": req.id,
        "tag": req.tag,
        "dry_run": req.dry_run,
        "confirmed": req.confirmed,
    });

    let outcome = crate::tools::erase::execute_checked(&state.storage, Some(args))
        .await
        .map_err(|e| match e {
            EraseError::Refused(message) | EraseError::Invalid(message) => {
                tracing::warn!(error = %message, "erase request rejected");
                StatusCode::BAD_REQUEST
            }
            EraseError::Storage(message) => {
                tracing::error!(error = %message, "erase failed");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    Ok(Json(EraseResponseDto::from(&outcome)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, header};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tower::ServiceExt;
    use vestige_core::{IngestInput, Storage};

    /// Same port the router is normally built with, so the OriginGuard layer
    /// sees the same-origin Host it expects.
    const PORT: u16 = 3927;

    fn test_storage(path: &std::path::Path) -> Arc<Storage> {
        Arc::new(Storage::new(Some(path.to_path_buf())).unwrap())
    }

    fn ingest(storage: &Storage, content: &str, tags: &[&str]) -> String {
        storage
            .ingest(IngestInput {
                content: content.to_string(),
                node_type: "fact".to_string(),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                ..Default::default()
            })
            .unwrap()
            .id
    }

    async fn post_erase(
        storage: Arc<Storage>,
        body: serde_json::Value,
    ) -> axum::response::Response {
        let (app, _state): (Router, AppState) = crate::dashboard::build_router(storage, None, PORT);
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/maintenance/erase")
                .header(header::HOST, format!("127.0.0.1:{PORT}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn erase_route_refuses_without_confirmed_and_deletes_nothing() {
        let dir = TempDir::new().unwrap();
        let storage = test_storage(&dir.path().join("refuse.db"));
        let id = ingest(&storage, "the subject's memory", &["gdpr"]);

        let response = post_erase(
            storage.clone(),
            serde_json::json!({ "action": "memory", "id": id, "dryRun": false }),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "a missing acknowledgement is a client error, never a 5xx: \
             500 would read as 'the erasure was attempted and failed'"
        );
        assert!(
            storage.get_node(&id).unwrap().is_some(),
            "a refused erasure must leave the memory untouched"
        );
    }

    #[tokio::test]
    async fn erase_route_omitting_dry_run_reports_without_deleting() {
        let dir = TempDir::new().unwrap();
        let storage = test_storage(&dir.path().join("dry.db"));
        let id = ingest(&storage, "the subject's memory", &["gdpr"]);

        // No `dryRun` field at all — the wire default must be the safe one.
        let response = post_erase(
            storage.clone(),
            serde_json::json!({ "action": "tag", "tag": "gdpr" }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["dryRun"], true);
        assert_eq!(body["matched"], 1);
        assert_eq!(body["ids"], serde_json::json!([id]));
        assert_eq!(body["artifactsErased"], 0);
        assert!(
            body["message"]
                .as_str()
                .unwrap()
                .contains("would be erased"),
            "the receipt must say nothing happened: {body}"
        );
        assert!(storage.get_node(&id).unwrap().is_some());
    }

    #[tokio::test]
    async fn erase_route_returns_the_dto_when_confirmed() {
        let dir = TempDir::new().unwrap();
        let storage = test_storage(&dir.path().join("confirm.db"));
        let doomed = ingest(&storage, "erase me", &["gdpr"]);
        let kept = ingest(&storage, "erase me not", &["codebase"]);

        let response = post_erase(
            storage.clone(),
            serde_json::json!({
                "action": "tag", "tag": "gdpr", "dryRun": false, "confirmed": true
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["action"], "tag");
        assert_eq!(body["dryRun"], false);
        assert_eq!(body["matched"], 1);
        assert_eq!(body["idsTruncated"], false);
        assert!(
            body["artifactsErased"].as_i64().unwrap() >= 1,
            "the DTO must report the artifact count: {body}"
        );
        assert!(
            storage.get_node(&doomed).unwrap().is_none(),
            "a confirmed erasure must delete the memory"
        );
        assert!(
            storage.get_node(&kept).unwrap().is_some(),
            "'gdpr' must not touch a memory tagged 'codebase'"
        );
    }

    #[tokio::test]
    async fn erase_route_rejects_an_unknown_action() {
        let dir = TempDir::new().unwrap();
        let storage = test_storage(&dir.path().join("invalid.db"));

        let response = post_erase(
            storage,
            serde_json::json!({ "action": "everything", "confirmed": true }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

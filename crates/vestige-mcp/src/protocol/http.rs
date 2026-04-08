//! Streamable HTTP transport for MCP.
//!
//! Implements the MCP Streamable HTTP transport specification:
//! - `POST /mcp` — JSON-RPC endpoint (initialize, tools/call, etc.)
//! - `DELETE /mcp` — session cleanup
//!
//! Each client gets a per-session `McpServer` instance (owns `initialized` state).
//! Shared state (Storage, CognitiveEngine, event bus) is shared across sessions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, post};
use axum::{Json, Router};
use subtle::ConstantTimeEq;
use tokio::sync::{broadcast, Mutex, RwLock};
use tower::ServiceBuilder;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use crate::cognitive::CognitiveEngine;
use crate::protocol::types::JsonRpcRequest;
use crate::server::McpServer;
use vestige_core::Storage;
use crate::dashboard::events::VestigeEvent;

/// Maximum concurrent sessions.
const MAX_SESSIONS: usize = 100;

/// Sessions idle longer than this are reaped.
const SESSION_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How often the reaper task runs.
const REAPER_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Concurrency limit for the tower middleware.
const CONCURRENCY_LIMIT: usize = 50;

/// Maximum request body size (256 KB — JSON-RPC requests should be small).
const MAX_BODY_SIZE: usize = 256 * 1024;

/// A per-client session holding its own McpServer instance.
struct Session {
    server: McpServer,
    last_active: Instant,
}

/// Shared state cloned into every axum handler.
#[derive(Clone)]
pub struct HttpTransportState {
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<Session>>>>>,
    storage: Arc<Storage>,
    cognitive: Arc<Mutex<CognitiveEngine>>,
    event_tx: broadcast::Sender<VestigeEvent>,
    auth_token: String,
}

/// Start the HTTP MCP transport on `127.0.0.1:<port>`.
///
/// This function spawns a background tokio task and returns immediately.
pub async fn start_http_transport(
    storage: Arc<Storage>,
    cognitive: Arc<Mutex<CognitiveEngine>>,
    event_tx: broadcast::Sender<VestigeEvent>,
    auth_token: String,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = HttpTransportState {
        sessions: Arc::new(RwLock::new(HashMap::new())),
        storage,
        cognitive,
        event_tx,
        auth_token,
    };

    // Spawn session reaper
    {
        let sessions = Arc::clone(&state.sessions);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(REAPER_INTERVAL).await;
                let mut map = sessions.write().await;
                let before = map.len();
                map.retain(|_id, session| {
                    // Try to check last_active without blocking; skip if locked
                    match session.try_lock() {
                        Ok(s) => s.last_active.elapsed() < SESSION_TIMEOUT,
                        Err(_) => true, // in-use, keep
                    }
                });
                let removed = before - map.len();
                if removed > 0 {
                    info!("Session reaper: removed {} idle sessions ({} active)", removed, map.len());
                }
            }
        });
    }

    let app = Router::new()
        .route("/mcp", post(post_mcp))
        .route("/mcp", delete(delete_mcp))
        .layer(
            ServiceBuilder::new()
                .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
                .layer(ConcurrencyLimitLayer::new(CONCURRENCY_LIMIT))
                .layer(CorsLayer::permissive()),
        )
        .with_state(state);

    // Bind to localhost only — use VESTIGE_HTTP_BIND=0.0.0.0 for remote access
    let bind_addr: std::net::IpAddr = std::env::var("VESTIGE_HTTP_BIND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

    let addr = std::net::SocketAddr::from((bind_addr, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("HTTP MCP transport listening on http://{}/mcp", addr);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            warn!("HTTP transport error: {}", e);
        }
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate the `Authorization: Bearer <token>` header using constant-time
/// comparison to prevent timing side-channel attacks.
fn validate_auth(headers: &HeaderMap, expected: &str) -> Result<(), (StatusCode, &'static str)> {
    let header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header"))?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid Authorization scheme (expected Bearer)"))?;

    // Constant-time comparison: prevents timing side-channel attacks.
    // We first check lengths match (length itself is not secret since UUIDs
    // have a fixed public format), then compare bytes in constant time.
    let token_bytes = token.as_bytes();
    let expected_bytes = expected.as_bytes();

    if token_bytes.len() != expected_bytes.len()
        || token_bytes.ct_eq(expected_bytes).unwrap_u8() != 1
    {
        return Err((StatusCode::FORBIDDEN, "Invalid auth token"));
    }

    Ok(())
}

/// Extract and validate the `Mcp-Session-Id` header value.
///
/// Only accepts valid UUID v4 format (8-4-4-4-12 hex) to prevent header
/// injection and ensure session IDs match server-generated format.
fn session_id_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /mcp` — main JSON-RPC handler.
async fn post_mcp(
    State(state): State<HttpTransportState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    // Auth check
    if let Err((status, msg)) = validate_auth(&headers, &state.auth_token) {
        return (status, HeaderMap::new(), msg.to_string()).into_response();
    }

    let is_initialize = request.method == "initialize";

    if is_initialize {
        // ── New session ──
        // Take write lock immediately to avoid TOCTOU race on MAX_SESSIONS check.
        let mut sessions = state.sessions.write().await;
        if sessions.len() >= MAX_SESSIONS {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Too many active sessions",
            )
                .into_response();
        }

        let server = McpServer::new_with_events(
            Arc::clone(&state.storage),
            Arc::clone(&state.cognitive),
            state.event_tx.clone(),
        );

        let session_id = uuid::Uuid::new_v4().to_string();

        let session = Arc::new(Mutex::new(Session {
            server,
            last_active: Instant::now(),
        }));

        // Handle the initialize request
        let response = {
            let mut sess = session.lock().await;
            sess.server.handle_request(request).await
        };

        // Insert session while still holding write lock — atomic check-and-insert
        sessions.insert(session_id.clone(), session);
        drop(sessions);

        let session_header = match session_id.parse() {
            Ok(v) => v,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to encode session ID").into_response(),
        };

        match response {
            Some(resp) => {
                let mut resp_headers = HeaderMap::new();
                resp_headers.insert("mcp-session-id", session_header);
                (StatusCode::OK, resp_headers, Json(resp)).into_response()
            }
            None => {
                let mut resp_headers = HeaderMap::new();
                resp_headers.insert("mcp-session-id", session_header);
                (StatusCode::ACCEPTED, resp_headers).into_response()
            }
        }
    } else {
        // ── Existing session ──
        let session_id = match session_id_from_headers(&headers) {
            Some(id) => id,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    "Missing or invalid Mcp-Session-Id header",
                )
                    .into_response();
            }
        };

        let session = {
            let sessions = state.sessions.read().await;
            sessions.get(&session_id).cloned()
        };

        let session = match session {
            Some(s) => s,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "Session not found or expired",
                )
                    .into_response();
            }
        };

        let response = {
            let mut sess = session.lock().await;
            sess.last_active = Instant::now();
            sess.server.handle_request(request).await
        };

        let session_header = match session_id.parse() {
            Ok(v) => v,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to encode session ID").into_response(),
        };

        let mut resp_headers = HeaderMap::new();
        resp_headers.insert("mcp-session-id", session_header);

        match response {
            Some(resp) => (StatusCode::OK, resp_headers, Json(resp)).into_response(),
            None => (StatusCode::ACCEPTED, resp_headers).into_response(),
        }
    }
}

/// `DELETE /mcp` — explicit session cleanup.
async fn delete_mcp(
    State(state): State<HttpTransportState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_auth(&headers, &state.auth_token) {
        return (status, msg).into_response();
    }

    let session_id = match session_id_from_headers(&headers) {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, "Missing or invalid Mcp-Session-Id header").into_response(),
    };

    let mut sessions = state.sessions.write().await;
    if sessions.remove(&session_id).is_some() {
        info!("Session {} deleted via DELETE /mcp", &session_id[..8]);
        (StatusCode::OK, "Session deleted").into_response()
    } else {
        (StatusCode::NOT_FOUND, "Session not found").into_response()
    }
}

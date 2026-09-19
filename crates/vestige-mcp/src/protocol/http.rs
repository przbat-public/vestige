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

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, post};
use axum::{Json, Router};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, RwLock, broadcast};
use tower::ServiceBuilder;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use crate::cognitive::CognitiveEngine;
use crate::dashboard::events::VestigeEvent;
use crate::protocol::timeout::with_timeout;
use crate::protocol::types::JsonRpcRequest;
use crate::server::McpServer;
use vestige_core::Storage;

/// Maximum concurrent sessions.
const MAX_SESSIONS: usize = 100;

/// Sessions idle longer than this are reaped.
const SESSION_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How often the reaper task runs.
const REAPER_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Concurrency limit for the tower middleware.
///
/// Bounds the number of *in-flight* requests, which already protects the
/// process from request floods. A separate request-rate limit
/// (`tower::limit::RateLimitLayer`) is intentionally NOT applied here:
/// in tower 0.5 it does not implement `Clone`, which axum requires on every
/// router layer, and `tower_governor` would add a new dependency. If
/// `VESTIGE_HTTP_BIND` is non-localhost you should put a reverse proxy
/// (nginx, Caddy, Cloudflare) in front and rate-limit there per IP.
const CONCURRENCY_LIMIT: usize = 50;

/// Maximum request body size (256 KB — JSON-RPC requests should be small).
const MAX_BODY_SIZE: usize = 256 * 1024;

/// Default per-request budget for JSON-RPC handlers.
///
/// Tuned for the *slowest* tool we expect to run synchronously inside a
/// single request: `dream`, `consolidate`, `backup`, large `reflect`. A
/// nominal `search` or `get` finishes in <50 ms, so this budget is several
/// orders of magnitude above the steady-state SLO. The point is purely to
/// recover from runaway handlers — never to bound user-visible latency.
///
/// Override per-deployment via `VESTIGE_REQUEST_TIMEOUT_SECS` (env var).
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Resolve the request budget, honouring `VESTIGE_REQUEST_TIMEOUT_SECS`.
fn request_timeout() -> Duration {
    std::env::var("VESTIGE_REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_REQUEST_TIMEOUT)
}

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
    /// Origins a browser is allowed to call this transport from. Same list the CORS
    /// layer advertises, so the header check and the response headers cannot drift.
    allowed_origins: Arc<Vec<String>>,
    /// `Host` values that may address this transport. Loopback by default; a
    /// non-loopback bind has to name its hosts explicitly.
    allowed_hosts: Arc<Vec<String>>,
}

/// Reject requests a browser should never be able to make against a local memory store.
///
/// MCP's transport spec makes this a MUST: "Servers MUST validate the Origin header on
/// all incoming connections to prevent DNS rebinding attacks. If the Origin header is
/// present and invalid, servers MUST respond with HTTP 403 Forbidden." The CORS layer
/// that used to be the only Origin handling only sets response headers — it never
/// rejects the request, and the browser still executes it (a cross-origin POST is sent,
/// the page just cannot read the reply).
///
/// Requests without an `Origin` header are passed through: non-browser clients (the
/// inspector, curl, another process) do not send one, and DNS rebinding is a browser
/// attack. `Host` is checked for every request, which is the same attack from the other
/// side — a rebound name arrives in `Host`, not in `Origin`.
async fn guard_origin_and_host(
    State(state): State<HttpTransportState>,
    request: Request,
    next: Next,
) -> Response {
    match crate::protocol::origin_guard::evaluate(
        &state.allowed_origins,
        &state.allowed_hosts,
        request.headers(),
    ) {
        crate::protocol::origin_guard::Decision::Allow => next.run(request).await,
        crate::protocol::origin_guard::Decision::Reject(reason) => {
            warn!(
                host = request
                    .headers()
                    .get(header::HOST)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("<missing>"),
                origin = request
                    .headers()
                    .get(header::ORIGIN)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("<none>"),
                "Rejected request: {reason}"
            );
            crate::protocol::origin_guard::forbidden(reason)
        }
    }
}

/// Build the transport router.
///
/// Extracted from [`start_http_transport`] so tests can drive it directly
/// (`tower::ServiceExt::oneshot`) instead of binding a socket.
fn build_router(state: HttpTransportState) -> Router {
    let origins: Vec<axum::http::HeaderValue> = state
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    let bind = std::env::var("VESTIGE_HTTP_BIND").unwrap_or_default();
    if bind == "0.0.0.0" || bind == "::" {
        tracing::info!(
            bind_addr = %bind,
            "VESTIGE_HTTP_BIND is non-localhost — set VESTIGE_CORS_ORIGINS and \
             VESTIGE_ALLOWED_HOSTS for browser access from remote hosts"
        );
    }

    Router::new()
        .route("/mcp", post(post_mcp))
        .route("/mcp", delete(delete_mcp))
        .layer(
            ServiceBuilder::new()
                .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
                .layer(ConcurrencyLimitLayer::new(CONCURRENCY_LIMIT))
                .layer(
                    CorsLayer::new()
                        .allow_origin(origins)
                        .allow_methods([
                            axum::http::Method::POST,
                            axum::http::Method::DELETE,
                            axum::http::Method::OPTIONS,
                        ])
                        .allow_headers([
                            axum::http::header::CONTENT_TYPE,
                            axum::http::header::AUTHORIZATION,
                        ]),
                )
                // Outermost of the small stack: a request from an origin we do not answer
                // to is refused before it reaches the session machinery.
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    guard_origin_and_host,
                )),
        )
        .with_state(state)
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
        allowed_origins: Arc::new(crate::protocol::origin_guard::allowed_origins(port, &[])),
        allowed_hosts: Arc::new(crate::protocol::origin_guard::allowed_hosts(port)),
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
                    info!(
                        "Session reaper: removed {} idle sessions ({} active)",
                        removed,
                        map.len()
                    );
                }
            }
        });
    }

    let app = build_router(state);

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

    let token = header.strip_prefix("Bearer ").ok_or((
        StatusCode::UNAUTHORIZED,
        "Invalid Authorization scheme (expected Bearer)",
    ))?;

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
    let method_for_logging = request.method.clone();
    let budget = request_timeout();

    if is_initialize {
        // ── New session ──
        // Take write lock immediately to avoid TOCTOU race on MAX_SESSIONS check.
        let mut sessions = state.sessions.write().await;
        if sessions.len() >= MAX_SESSIONS {
            return (StatusCode::SERVICE_UNAVAILABLE, "Too many active sessions").into_response();
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

        // Handle the initialize request. Wrap in a hard timeout so a hung
        // initialize (e.g. embedding model still loading and a blocking
        // SQLite write contending) cannot pin a worker slot forever.
        let response = match with_timeout(budget, async {
            let mut sess = session.lock().await;
            sess.server.handle_request(request).await
        })
        .await
        {
            Ok(r) => r,
            Err(_elapsed) => {
                warn!(
                    method = %method_for_logging,
                    budget_secs = budget.as_secs(),
                    "Request exceeded budget — returning 504"
                );
                return (
                    StatusCode::GATEWAY_TIMEOUT,
                    HeaderMap::new(),
                    format!(
                        "Request '{}' exceeded {}s budget",
                        method_for_logging,
                        budget.as_secs()
                    ),
                )
                    .into_response();
            }
        };

        // Insert session while still holding write lock — atomic check-and-insert
        sessions.insert(session_id.clone(), session);
        drop(sessions);

        let session_header = match session_id.parse() {
            Ok(v) => v,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to encode session ID",
                )
                    .into_response();
            }
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
                return (StatusCode::NOT_FOUND, "Session not found or expired").into_response();
            }
        };

        let response = match with_timeout(budget, async {
            let mut sess = session.lock().await;
            sess.last_active = Instant::now();
            sess.server.handle_request(request).await
        })
        .await
        {
            Ok(r) => r,
            Err(_elapsed) => {
                warn!(
                    method = %method_for_logging,
                    budget_secs = budget.as_secs(),
                    session_prefix = %&session_id[..8],
                    "Request exceeded budget — returning 504"
                );
                return (
                    StatusCode::GATEWAY_TIMEOUT,
                    HeaderMap::new(),
                    format!(
                        "Request '{}' exceeded {}s budget",
                        method_for_logging,
                        budget.as_secs()
                    ),
                )
                    .into_response();
            }
        };

        let session_header = match session_id.parse() {
            Ok(v) => v,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to encode session ID",
                )
                    .into_response();
            }
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
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Missing or invalid Mcp-Session-Id header",
            )
                .into_response();
        }
    };

    let mut sessions = state.sessions.write().await;
    if sessions.remove(&session_id).is_some() {
        info!("Session {} deleted via DELETE /mcp", &session_id[..8]);
        (StatusCode::OK, "Session deleted").into_response()
    } else {
        (StatusCode::NOT_FOUND, "Session not found").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode as Code};
    use tower::ServiceExt;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn test_state(port: u16) -> HttpTransportState {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(Some(dir.path().join("http-test.db"))).unwrap());
        // Leak the temp dir for the lifetime of the test process: `Storage` holds the
        // connection, and the tests never look at the file again.
        std::mem::forget(dir);
        let (event_tx, _) = broadcast::channel(16);

        HttpTransportState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            storage,
            cognitive: Arc::new(Mutex::new(CognitiveEngine::new())),
            event_tx,
            auth_token: TOKEN.to_string(),
            allowed_origins: Arc::new(crate::protocol::origin_guard::allowed_origins(port, &[])),
            allowed_hosts: Arc::new(crate::protocol::origin_guard::allowed_hosts(port)),
        }
    }

    fn post(host: &str, origin: Option<&str>, token: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri("/mcp")
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "application/json");

        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }

        builder
            .body(Body::from(
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list",
                })
                .to_string(),
            ))
            .unwrap()
    }

    async fn status_of(request: Request<Body>) -> Code {
        let port = 3928;
        build_router(test_state(port))
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn foreign_origin_is_rejected_with_403() {
        // The transport spec makes this a MUST: a browser that has been tricked into
        // resolving an attacker-controlled name to loopback still sends the attacker's
        // Origin. CORS only hides the response — the request itself would still run.
        let status = status_of(post(
            "127.0.0.1:3928",
            Some("https://evil.example"),
            Some(TOKEN),
        ))
        .await;
        assert_eq!(status, Code::FORBIDDEN);
    }

    #[tokio::test]
    async fn loopback_origin_is_accepted() {
        // Reaches auth (the token is valid, so the guard let it through and the empty
        // session map produced the "no session" answer rather than a 403).
        let status = status_of(post(
            "127.0.0.1:3928",
            Some("http://127.0.0.1:3928"),
            Some(TOKEN),
        ))
        .await;
        assert_eq!(
            status,
            Code::BAD_REQUEST,
            "an allowed Origin must reach the session logic, not the guard"
        );
    }

    #[tokio::test]
    async fn request_without_origin_is_allowed() {
        // Non-browser clients (curl, the MCP inspector, another local process) send no
        // Origin, and DNS rebinding is a browser attack.
        let status = status_of(post("127.0.0.1:3928", None, Some(TOKEN))).await;
        assert_eq!(status, Code::BAD_REQUEST);
    }

    #[tokio::test]
    async fn foreign_host_is_rejected_with_403() {
        // The same attack from the other side: a rebound name arrives in `Host`.
        let status = status_of(post(
            "attacker.example:3928",
            Some("https://evil.example"),
            Some(TOKEN),
        ))
        .await;
        assert_eq!(status, Code::FORBIDDEN);
    }

    #[tokio::test]
    async fn missing_token_is_unauthorized() {
        let status = status_of(post("127.0.0.1:3928", None, None)).await;
        assert_eq!(status, Code::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_token_is_forbidden() {
        let status = status_of(post(
            "127.0.0.1:3928",
            None,
            Some("ffffffffffffffffffffffffffffffff"),
        ))
        .await;
        assert_eq!(status, Code::FORBIDDEN);
    }

    #[tokio::test]
    async fn session_id_of_the_wrong_shape_is_rejected() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/mcp")
            .header(header::HOST, "127.0.0.1:3928")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header("mcp-session-id", "not-a-uuid")
            .body(Body::from(
                serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })
                    .to_string(),
            ))
            .unwrap();
        let status = status_of(request).await;
        assert_eq!(
            status,
            Code::BAD_REQUEST,
            "a malformed session id must be rejected before it is used as a map key"
        );
    }

    #[test]
    fn defaults_cover_loopback_only() {
        let origins = crate::protocol::origin_guard::allowed_origins(3928, &[]);
        assert!(origins.iter().any(|o| o == "http://127.0.0.1:3928"));
        assert!(origins.iter().any(|o| o == "http://localhost:3928"));
        assert!(
            !origins.iter().any(|o| o.contains("evil")),
            "no wildcard, no remote origin by default"
        );

        let hosts = crate::protocol::origin_guard::allowed_hosts(3928);
        assert!(hosts.iter().any(|h| h == "127.0.0.1:3928"));
        assert!(hosts.iter().any(|h| h == "localhost:3928"));
        assert_eq!(
            hosts.len(),
            3,
            "loopback v4, v6 and localhost — nothing else"
        );
    }
}

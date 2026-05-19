//! Memory Web Dashboard
//!
//! Self-contained web UI at localhost:3927 for browsing, searching,
//! and managing Vestige memories. Auto-starts inside the MCP server process.
//!
//! v2.0: WebSocket real-time events, CognitiveEngine access, new API endpoints.

pub mod events;
pub mod handlers;
pub mod state;
pub mod static_files;
pub mod websocket;
pub mod wire;

use axum::Router;
use axum::routing::{delete, get, patch, post};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{info, warn};

use crate::cognitive::CognitiveEngine;
use state::AppState;
use vestige_core::Storage;

/// Build the axum router with all dashboard routes
pub fn build_router(
    storage: Arc<Storage>,
    cognitive: Option<Arc<Mutex<CognitiveEngine>>>,
    port: u16,
) -> (Router, AppState) {
    let state = AppState::new(storage, cognitive);
    build_router_inner(state, port)
}

/// Build the axum router sharing an external event broadcast channel.
pub fn build_router_with_event_tx(
    storage: Arc<Storage>,
    cognitive: Option<Arc<Mutex<CognitiveEngine>>>,
    event_tx: tokio::sync::broadcast::Sender<events::VestigeEvent>,
    port: u16,
) -> (Router, AppState) {
    let state = AppState::with_event_tx(storage, cognitive, event_tx);
    build_router_inner(state, port)
}

fn build_router_inner(state: AppState, port: u16) -> (Router, AppState) {
    #[allow(unused_mut)]
    let mut origins = vec![
        format!("http://127.0.0.1:{}", port)
            .parse::<axum::http::HeaderValue>()
            .expect("valid origin"),
        format!("http://localhost:{}", port)
            .parse::<axum::http::HeaderValue>()
            .expect("valid origin"),
    ];

    // Vite dev server — only in debug builds
    #[cfg(debug_assertions)]
    {
        origins.push(
            "http://localhost:5173"
                .parse::<axum::http::HeaderValue>()
                .expect("valid origin"),
        );
        origins.push(
            "http://127.0.0.1:5173"
                .parse::<axum::http::HeaderValue>()
                .expect("valid origin"),
        );
    }

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]);

    // Security: restrict WebSocket connections to localhost only (prevents cross-site WS hijacking)
    let csp_value = format!(
        "default-src 'self'; \
         script-src 'self' 'unsafe-inline'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' blob: data:; \
         connect-src 'self' ws://127.0.0.1:{port} ws://localhost:{port}; \
         font-src 'self' data:; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         form-action 'self';"
    );
    let csp = SetResponseHeaderLayer::overriding(
        axum::http::header::CONTENT_SECURITY_POLICY,
        axum::http::HeaderValue::from_str(&csp_value).expect("valid CSP header"),
    );

    // Additional security headers
    let x_frame_options = SetResponseHeaderLayer::overriding(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    let x_content_type_options = SetResponseHeaderLayer::overriding(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    let referrer_policy = SetResponseHeaderLayer::overriding(
        axum::http::HeaderName::from_static("referrer-policy"),
        axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    let permissions_policy = SetResponseHeaderLayer::overriding(
        axum::http::HeaderName::from_static("permissions-policy"),
        axum::http::HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );

    let router = Router::new()
        // React Dashboard v2.1 (embedded static build)
        .route("/dashboard", get(static_files::serve_dashboard_spa))
        .route(
            "/dashboard/{*path}",
            get(static_files::serve_dashboard_asset),
        )
        // Legacy embedded HTML (keep for backward compat)
        .route("/", get(handlers::serve_dashboard))
        .route("/graph", get(handlers::serve_graph))
        // WebSocket for real-time events
        .route("/ws", get(websocket::ws_handler))
        // Memory CRUD
        .route("/api/memories", get(handlers::list_memories))
        .route("/api/memories/{id}", get(handlers::get_memory))
        .route("/api/memories/{id}", delete(handlers::delete_memory))
        .route("/api/memories/{id}", patch(handlers::update_memory))
        .route("/api/smart_ingest", post(handlers::smart_ingest_memory))
        .route("/api/memories/{id}/promote", post(handlers::promote_memory))
        .route("/api/memories/{id}/demote", post(handlers::demote_memory))
        .route(
            "/api/memories/{id}/changelog",
            get(handlers::get_memory_changelog),
        )
        .route("/api/memories/{id}/review", post(handlers::review_memory))
        .route("/api/review/queue", get(handlers::get_review_queue))
        // Maintenance — thin REST wrappers around MCP tools, used by Settings UI
        .route(
            "/api/maintenance/regenerate-embeddings",
            post(handlers::maintenance_regenerate_embeddings),
        )
        .route(
            "/api/maintenance/find-duplicates",
            post(handlers::maintenance_find_duplicates),
        )
        .route("/api/maintenance/gc", post(handlers::maintenance_gc))
        .route(
            "/api/maintenance/backup",
            post(handlers::maintenance_backup),
        )
        // Search
        .route("/api/search", get(handlers::search_memories))
        // Stats & health
        .route("/api/stats", get(handlers::get_stats))
        .route("/api/health", get(handlers::health_check))
        // Timeline
        .route("/api/timeline", get(handlers::get_timeline))
        // Graph
        .route("/api/graph", get(handlers::get_graph))
        // Cognitive operations (v2.0)
        .route("/api/dream", post(handlers::trigger_dream))
        .route("/api/explore", post(handlers::explore_connections))
        .route("/api/predict", post(handlers::predict_memories))
        .route("/api/importance", post(handlers::score_importance))
        .route("/api/consolidate", post(handlers::trigger_consolidation))
        .route(
            "/api/retention-distribution",
            get(handlers::retention_distribution),
        )
        // Single source of truth for shared limits (graph node cap,
        // search defaults, WS event ring buffer). Frontend fetches once
        // on boot — see `useDashboardLimits.ts`.
        .route("/api/_meta/limits", get(handlers::get_limits))
        // Intentions (v2.0)
        .route(
            "/api/intentions",
            get(handlers::list_intentions).post(handlers::create_intention),
        )
        // Metacognitive tools (v2.1)
        .route("/api/reflect", post(handlers::trigger_reflect))
        .route("/api/temporal", post(handlers::query_temporal))
        .route("/api/confidence", post(handlers::query_confidence))
        // Deep reference reasoning (v3.2.1 cognitive engine, exposed v3.5)
        .route("/api/deep_reference", post(handlers::query_deep_reference))
        // Decision Matrix (v3.4 — Proposal C)
        .route("/api/decisions", get(handlers::list_decisions))
        // Insight Tier (v3.4 — Proposal B)
        .route("/api/insights", get(handlers::list_insights))
        // Topic Hubs (v3.5 — Proposal A)
        .route("/api/hubs", get(handlers::list_hubs))
        .layer(
            ServiceBuilder::new()
                .concurrency_limit(50)
                .layer(cors)
                .layer(csp)
                .layer(x_frame_options)
                .layer(x_content_type_options)
                .layer(referrer_policy)
                .layer(permissions_policy),
        )
        .with_state(state.clone());

    (router, state)
}

/// Start the dashboard HTTP server (blocking — use in CLI mode)
pub async fn start_dashboard(
    storage: Arc<Storage>,
    cognitive: Option<Arc<Mutex<CognitiveEngine>>>,
    port: u16,
    open_browser: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (app, _state) = build_router(storage, cognitive, port);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    info!("Dashboard starting at http://127.0.0.1:{}", port);

    if open_browser {
        let url = format!("http://127.0.0.1:{}", port);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = open::that(&url);
        });
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Start the dashboard as a background task (non-blocking — use in MCP server)
pub async fn start_background(
    storage: Arc<Storage>,
    cognitive: Option<Arc<Mutex<CognitiveEngine>>>,
    port: u16,
) -> Result<AppState, Box<dyn std::error::Error>> {
    let (app, state) = build_router(storage, cognitive, port);
    start_background_inner(app, state, port).await
}

/// Start the dashboard sharing an external event broadcast channel.
pub async fn start_background_with_event_tx(
    storage: Arc<Storage>,
    cognitive: Option<Arc<Mutex<CognitiveEngine>>>,
    event_tx: tokio::sync::broadcast::Sender<events::VestigeEvent>,
    port: u16,
) -> Result<AppState, Box<dyn std::error::Error>> {
    let (app, state) = build_router_with_event_tx(storage, cognitive, event_tx, port);
    start_background_inner(app, state, port).await
}

async fn start_background_inner(
    app: Router,
    state: AppState,
    port: u16,
) -> Result<AppState, Box<dyn std::error::Error>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!(
                "Dashboard could not bind to port {}: {} (MCP server continues without dashboard)",
                port, e
            );
            return Err(Box::new(e));
        }
    };

    info!(
        "Dashboard available at http://127.0.0.1:{} (WebSocket at ws://127.0.0.1:{}/ws)",
        port, port
    );

    let serve_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            warn!("Dashboard server error: {}", e);
        }
        drop(serve_state);
    });

    Ok(state)
}

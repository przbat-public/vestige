//! Vestige MCP Server v1.0 - Cognitive Memory for Claude
//!
//! A Rust MCP (Model Context Protocol) server that provides
//! Claude and other AI assistants with long-term memory capabilities
//! powered by 130 years of memory research.
//!
//! Core Features:
//! - FSRS-6 spaced repetition algorithm (21 parameters; see the upstream SRS
//!   benchmark at <https://github.com/open-spaced-repetition/srs-benchmark> for
//!   FSRS-vs-SM-2 comparisons — we do not requote a single percentage here)
//! - Bjork dual-strength memory model
//! - Local semantic embeddings (nomic-embed-text-v1.5, 384D Matryoshka, no external API)
//! - HNSW vector search via USearch (in-memory; persistence still pending)
//! - Hybrid search (BM25 + semantic + RRF fusion)
//!
//! Neuroscience Features:
//! - Synaptic Tagging & Capture (retroactive importance)
//! - Spreading Activation Networks (multi-hop associations)
//! - Hippocampal Indexing (two-phase retrieval)
//! - Memory States (active/dormant/silent/unavailable)
//! - Context-Dependent Memory (encoding specificity)
//! - Multi-Channel Importance Signals
//! - Predictive Retrieval
//! - Prospective Memory (intentions with triggers)
//!
//! Advanced Features:
//! - Memory Dreams (insight generation during consolidation)
//! - Memory Compression
//! - Reconsolidation (memories editable on retrieval)
//! - Memory Chains (reasoning paths)

use vestige_mcp::cognitive;
use vestige_mcp::protocol;
use vestige_mcp::server;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{Level, error, info, warn};
use tracing_subscriber::EnvFilter;

// Use vestige-core for the cognitive science engine
use vestige_core::Storage;

use protocol::stdio::StdioTransport;
use server::McpServer;

/// Parsed CLI configuration.
struct Config {
    data_dir: Option<PathBuf>,
    http_port: u16,
}

/// Parse command-line arguments into a `Config`.
/// Exits the process if `--help` or `--version` is requested.
fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut data_dir: Option<PathBuf> = None;
    let mut http_port: u16 = std::env::var("VESTIGE_HTTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3928);
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("Vestige MCP Server v{}", env!("CARGO_PKG_VERSION"));
                println!();
                println!("FSRS-6 powered AI memory server using the Model Context Protocol.");
                println!();
                println!("USAGE:");
                println!("    vestige-mcp [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!("    -h, --help              Print help information");
                println!("    -V, --version           Print version information");
                println!("    --data-dir <PATH>       Custom data directory");
                println!("    --http-port <PORT>      HTTP transport port (default: 3928)");
                println!();
                println!("ENVIRONMENT:");
                println!(
                    "    RUST_LOG                  Log level filter (e.g., debug, info, warn, error)"
                );
                println!(
                    "    VESTIGE_AUTH_TOKEN         Override the bearer token for HTTP transport"
                );
                println!("    VESTIGE_HTTP_PORT          HTTP transport port (default: 3928)");
                println!("    VESTIGE_DASHBOARD_PORT     Dashboard port (default: 3927)");
                println!();
                println!("EXAMPLES:");
                println!("    vestige-mcp");
                println!("    vestige-mcp --data-dir /custom/path");
                println!("    vestige-mcp --http-port 8080");
                println!("    RUST_LOG=debug vestige-mcp");
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("vestige-mcp {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--data-dir" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --data-dir requires a path argument");
                    eprintln!("Usage: vestige-mcp --data-dir <PATH>");
                    std::process::exit(1);
                }
                data_dir = Some(PathBuf::from(&args[i]));
            }
            arg if arg.starts_with("--data-dir=") => {
                // Safe: we just verified the prefix exists with starts_with
                let path = arg.strip_prefix("--data-dir=").unwrap_or("");
                if path.is_empty() {
                    eprintln!("error: --data-dir requires a path argument");
                    eprintln!("Usage: vestige-mcp --data-dir <PATH>");
                    std::process::exit(1);
                }
                data_dir = Some(PathBuf::from(path));
            }
            "--http-port" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --http-port requires a port number");
                    eprintln!("Usage: vestige-mcp --http-port <PORT>");
                    std::process::exit(1);
                }
                http_port = match args[i].parse() {
                    Ok(p) => p,
                    Err(_) => {
                        eprintln!("error: invalid port number '{}'", args[i]);
                        std::process::exit(1);
                    }
                };
            }
            arg if arg.starts_with("--http-port=") => {
                let val = arg.strip_prefix("--http-port=").unwrap_or("");
                http_port = match val.parse() {
                    Ok(p) => p,
                    Err(_) => {
                        eprintln!("error: invalid port number '{}'", val);
                        std::process::exit(1);
                    }
                };
            }
            arg => {
                eprintln!("error: unknown argument '{}'", arg);
                eprintln!("Usage: vestige-mcp [OPTIONS]");
                eprintln!("Try 'vestige-mcp --help' for more information.");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Config {
        data_dir,
        http_port,
    }
}

#[tokio::main]
async fn main() {
    // Parse CLI arguments first (before logging init, so --help/--version work cleanly)
    let config = parse_args();

    // Initialize logging to stderr (stdout is for JSON-RPC)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .with_writer(io::stderr)
        .with_target(false)
        .with_ansi(false)
        .init();

    info!(
        "Vestige MCP Server v{} starting...",
        env!("CARGO_PKG_VERSION")
    );

    // Optional OTLP exporter (opt-in via `--features telemetry`). Logged once
    // so operators can confirm whether spans will actually be shipped.
    let telemetry_status = vestige_mcp::telemetry::init(&vestige_mcp::telemetry::TelemetryConfig::from_env());
    match &telemetry_status {
        vestige_mcp::telemetry::TelemetryStatus::Initialized { endpoint } => {
            info!(otlp_endpoint = %endpoint, "telemetry exporter initialized");
        }
        vestige_mcp::telemetry::TelemetryStatus::Disabled => {
            info!(
                "telemetry feature compiled in but no endpoint set; export disabled"
            );
        }
        vestige_mcp::telemetry::TelemetryStatus::FeatureDisabled => {
            // Stay quiet by default — this is the most common production path
            // and we already log at higher levels of importance elsewhere.
        }
    }

    // Initialize storage with optional custom data directory
    let storage = match Storage::new(config.data_dir) {
        Ok(s) => {
            info!("Storage initialized successfully");

            // Try to initialize embeddings early and log any issues
            #[cfg(feature = "embeddings")]
            {
                if let Err(e) = s.init_embeddings() {
                    error!("Failed to initialize embedding service: {}", e);
                    error!("Smart ingest will fall back to regular ingest without deduplication");
                    error!(
                        "Hint: Check FASTEMBED_CACHE_PATH or ensure ~/.cache/vestige/fastembed is writable"
                    );
                } else {
                    info!("Embedding service initialized successfully");
                }
            }

            Arc::new(s)
        }
        Err(e) => {
            error!("Failed to initialize storage: {}", e);
            std::process::exit(1);
        }
    };

    // Spawn periodic auto-consolidation so FSRS-6 decay scores stay fresh.
    // Runs on startup (if needed) and then every N hours (default: 6).
    // Configurable via VESTIGE_CONSOLIDATION_INTERVAL_HOURS env var.
    {
        let storage_clone = storage.clone();
        tokio::spawn(async move {
            let interval_hours: u64 = std::env::var("VESTIGE_CONSOLIDATION_INTERVAL_HOURS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(6);
            let interval = std::time::Duration::from_secs(interval_hours * 3600);
            // Cap waits so we always re-check at least once an hour. This keeps
            // the loop responsive to wall-clock drift (e.g. laptop suspend/
            // resume) and lets us pick up a manually-triggered consolidation
            // recorded by another path without sleeping a full interval first.
            let max_wait = std::time::Duration::from_secs(3600);

            // Small delay so we don't block server startup / stdio handshake
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            loop {
                // Decide whether to run now and how long to sleep before the
                // next check, based on time-since-last-consolidation rather
                // than a fixed wall clock tick. Pre-v3.2.1 this loop always
                // slept `interval_hours` after a skip, which made the auto-
                // consolidation schedule drift by up to one interval on every
                // restart that landed within the window. (See AGENTS.md
                // changelog.)
                let storage_for_check = storage_clone.clone();
                let last_consolidation =
                    tokio::task::spawn_blocking(move || storage_for_check.get_last_consolidation())
                        .await
                        .unwrap_or_else(|e| {
                            warn!("get_last_consolidation task panicked: {}", e);
                            Ok(None)
                        });
                let (should_run, sleep_for) = match last_consolidation {
                    Ok(Some(last)) => {
                        let now = chrono::Utc::now();
                        let elapsed = now - last;
                        let interval_chrono = chrono::Duration::seconds(interval.as_secs() as i64);
                        if elapsed >= interval_chrono {
                            (true, interval)
                        } else {
                            let remaining = interval_chrono - elapsed;
                            // Saturating conversion: remaining is always >0 here.
                            let secs = remaining.num_seconds().max(60) as u64;
                            let sleep = std::time::Duration::from_secs(secs).min(max_wait);
                            info!(
                                last_consolidation = %last,
                                next_check_in_secs = sleep.as_secs(),
                                "Skipping auto-consolidation (last run was < {} hours ago)",
                                interval_hours
                            );
                            (false, sleep)
                        }
                    }
                    Ok(None) => {
                        info!("No previous consolidation found — running first auto-consolidation");
                        (true, interval)
                    }
                    Err(e) => {
                        warn!(
                            "Could not read consolidation history: {} — running anyway",
                            e
                        );
                        (true, interval)
                    }
                };

                if should_run {
                    let s = storage_clone.clone();
                    match tokio::task::spawn_blocking(move || s.run_consolidation()).await {
                        Ok(Ok(result)) => {
                            info!(
                                nodes_processed = result.nodes_processed,
                                decay_applied = result.decay_applied,
                                embeddings_generated = result.embeddings_generated,
                                duplicates_merged = result.duplicates_merged,
                                activations_computed = result.activations_computed,
                                duration_ms = result.duration_ms,
                                "Periodic auto-consolidation complete"
                            );
                        }
                        Ok(Err(e)) => {
                            warn!("Periodic auto-consolidation failed: {}", e);
                        }
                        Err(e) => {
                            warn!("Consolidation task panicked: {}", e);
                        }
                    }
                }

                tokio::time::sleep(sleep_for).await;
            }
        });
    }

    // Create cognitive engine (stateful neuroscience modules)
    let cognitive = Arc::new(Mutex::new(cognitive::CognitiveEngine::new()));
    // Hydrate cognitive modules from persisted connections on the blocking
    // pool — full table scans of connections and memory_nodes can take
    // hundreds of ms on large databases.
    {
        let cognitive_for_hydrate = cognitive.clone();
        let storage_for_hydrate = storage.clone();
        tokio::task::spawn_blocking(move || {
            let mut cog = cognitive_for_hydrate.blocking_lock();
            cog.hydrate(&storage_for_hydrate);
        })
        .await
        .expect("CognitiveEngine hydrate task panicked");
    }
    info!("CognitiveEngine initialized and hydrated");

    // Create shared event broadcast channel for dashboard <-> MCP tool events
    let (event_tx, _) =
        tokio::sync::broadcast::channel::<vestige_mcp::dashboard::events::VestigeEvent>(1024);

    // Spawn dashboard HTTP server alongside MCP server (now with CognitiveEngine access)
    {
        let dashboard_port = std::env::var("VESTIGE_DASHBOARD_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(3927);
        let dashboard_storage = Arc::clone(&storage);
        let dashboard_cognitive = Arc::clone(&cognitive);
        let dashboard_event_tx = event_tx.clone();
        tokio::spawn(async move {
            match vestige_mcp::dashboard::start_background_with_event_tx(
                dashboard_storage,
                Some(dashboard_cognitive),
                dashboard_event_tx,
                dashboard_port,
            )
            .await
            {
                Ok(_state) => {
                    info!("Dashboard started with WebSocket + CognitiveEngine + shared event bus");
                }
                Err(e) => {
                    warn!("Dashboard failed to start: {}", e);
                }
            }
        });
    }

    // Start HTTP MCP transport (Streamable HTTP for Claude.ai / remote clients)
    {
        let http_storage = Arc::clone(&storage);
        let http_cognitive = Arc::clone(&cognitive);
        let http_event_tx = event_tx.clone();
        let http_port = config.http_port;

        match protocol::auth::get_or_create_auth_token() {
            Ok(token) => {
                let bind =
                    std::env::var("VESTIGE_HTTP_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
                eprintln!("Vestige HTTP transport: http://{}:{}/mcp", bind, http_port);
                eprintln!("Auth token: {}...", &token[..8]);
                tokio::spawn(async move {
                    if let Err(e) = protocol::http::start_http_transport(
                        http_storage,
                        http_cognitive,
                        http_event_tx,
                        token,
                        http_port,
                    )
                    .await
                    {
                        warn!("HTTP transport failed to start: {}", e);
                    }
                });
            }
            Err(e) => {
                warn!(
                    "Could not create auth token, HTTP transport disabled: {}",
                    e
                );
            }
        }
    }

    // Load cross-encoder reranker in the background (downloads ~150MB on first run)
    #[cfg(feature = "embeddings")]
    {
        let cog_clone = Arc::clone(&cognitive);
        tokio::spawn(async move {
            // Small delay so we don't block the stdio handshake
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let mut cog = cog_clone.lock().await;
            cog.reranker.init_cross_encoder();
        });
    }

    // Create MCP server with shared event channel for dashboard broadcasts
    let server = McpServer::new_with_events(storage.clone(), cognitive, event_tx);

    // Create stdio transport
    let transport = StdioTransport::new();

    info!("Starting MCP server on stdio...");

    // Run the server with graceful shutdown on SIGINT/SIGTERM
    let shutdown_storage = Arc::clone(&storage);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Received shutdown signal — checkpointing WAL");
        let s = shutdown_storage;
        let _ = tokio::task::spawn_blocking(move || s.wal_checkpoint()).await;
        info!("WAL checkpoint complete — exiting");
        std::process::exit(0);
    });

    if let Err(e) = transport.run(server).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    info!("Vestige MCP Server shutting down");
}

//! Dashboard and serve commands — launch the web dashboard or HTTP MCP server.

use std::sync::Arc;

use colored::Colorize;
use vestige_core::Storage;
use vestige_mcp::cognitive::CognitiveEngine;

pub(super) fn run_dashboard(port: u16, open_browser: bool) -> anyhow::Result<()> {
    println!("{}", "=== Vestige Dashboard ===".cyan().bold());
    println!();
    println!(
        "Starting dashboard at {}...",
        format!("http://127.0.0.1:{}", port).cyan()
    );

    let storage = Storage::new(None)?;

    // Try to initialize embeddings for search support
    #[cfg(feature = "embeddings")]
    {
        if let Err(e) = storage.init_embeddings() {
            println!(
                "  {} Embeddings unavailable: {} (search will use keyword-only)",
                "!".yellow(),
                e
            );
        }
    }

    let storage = std::sync::Arc::new(storage);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        vestige_mcp::dashboard::start_dashboard(storage, None, port, open_browser)
            .await
            .map_err(|e| anyhow::anyhow!("Dashboard error: {}", e))
    })
}

/// Start standalone HTTP MCP server (no stdio transport)
pub(super) fn run_serve(
    port: u16,
    with_dashboard: bool,
    dashboard_port: u16,
) -> anyhow::Result<()> {
    println!("{}", "=== Vestige HTTP Server ===".cyan().bold());
    println!();

    let storage = Storage::new(None)?;

    #[cfg(feature = "embeddings")]
    {
        if let Err(e) = storage.init_embeddings() {
            println!(
                "  {} Embeddings unavailable: {} (search will use keyword-only)",
                "!".yellow(),
                e
            );
        }
    }

    let storage = Arc::new(storage);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cognitive = Arc::new(tokio::sync::Mutex::new(CognitiveEngine::new()));
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

        let (event_tx, _) =
            tokio::sync::broadcast::channel::<vestige_mcp::dashboard::events::VestigeEvent>(1024);

        // Optionally start dashboard
        if with_dashboard {
            let ds = Arc::clone(&storage);
            let dc = Arc::clone(&cognitive);
            let dtx = event_tx.clone();
            tokio::spawn(async move {
                match vestige_mcp::dashboard::start_background_with_event_tx(
                    ds,
                    Some(dc),
                    dtx,
                    dashboard_port,
                )
                .await
                {
                    Ok(_) => println!(
                        "  {} Dashboard: http://127.0.0.1:{}",
                        ">".cyan(),
                        dashboard_port
                    ),
                    Err(e) => eprintln!("  {} Dashboard failed: {}", "!".yellow(), e),
                }
            });
        }

        // Get auth token
        let token = vestige_mcp::protocol::auth::get_or_create_auth_token()
            .map_err(|e| anyhow::anyhow!("Failed to create auth token: {}", e))?;

        let bind = std::env::var("VESTIGE_HTTP_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
        println!(
            "  {} HTTP transport: http://{}:{}/mcp",
            ">".cyan(),
            bind,
            port
        );
        println!("  {} Auth token: {}...", ">".cyan(), &token[..8]);
        println!();
        println!("{}", "Press Ctrl+C to stop.".dimmed());

        // Start HTTP transport (blocks on the server, no stdio)
        vestige_mcp::protocol::http::start_http_transport(
            Arc::clone(&storage),
            Arc::clone(&cognitive),
            event_tx,
            token,
            port,
        )
        .await
        .map_err(|e| anyhow::anyhow!("HTTP transport failed: {}", e))?;

        // Keep the process alive (the HTTP server runs in a spawned task)
        tokio::signal::ctrl_c().await.ok();
        println!();
        println!("{}", "Shutting down...".dimmed());

        Ok(())
    })
}

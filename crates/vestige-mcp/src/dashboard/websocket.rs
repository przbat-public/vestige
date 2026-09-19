//! WebSocket handler for real-time event streaming.
//!
//! Clients connect to `/ws` and receive all VestigeEvents as JSON.
//! Also sends heartbeats every 5 seconds with system stats.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use super::events::VestigeEvent;
use super::state::AppState;

/// WebSocket upgrade handler — GET /ws
///
/// Origin is validated by [`crate::protocol::origin_guard::OriginGuard::check`],
/// the outermost layer of the dashboard router, so it runs before this handler:
/// only the exact origins from the shared allowlist (`http://127.0.0.1:{port}`,
/// `http://localhost:{port}`, `http://[::1]:{port}`) can reach an upgrade.
///
/// This handler used to repeat the check with a prefix match
/// (`origin.starts_with("http://127.0.0.1:")`), which accepted *any* port on
/// loopback — a weaker rule than CORS applied, and weaker than the guard now in
/// front of it. There is deliberately no second copy of the policy here: two
/// copies of a security check drift, which is how the dashboard got a hole the
/// transport did not have. A `WebSocketUpgrade` reached through any other router
/// must be wrapped in `OriginGuard::check` explicitly.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.max_frame_size(64 * 1024)
        .max_message_size(256 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state))
        .into_response()
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_rx: broadcast::Receiver<VestigeEvent> = state.subscribe();

    debug!("WebSocket client connected");

    // Send initial connection event
    let welcome = serde_json::json!({
        "type": "Connected",
        "data": {
            "version": env!("CARGO_PKG_VERSION"),
            "timestamp": Utc::now().to_rfc3339(),
        }
    });
    if sender
        .send(Message::Text(welcome.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    // Heartbeat interval
    let heartbeat_state = state.clone();
    let (heartbeat_tx, mut heartbeat_rx) = tokio::sync::mpsc::channel::<String>(16);

    // Heartbeat task
    let heartbeat_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let uptime = heartbeat_state.start_time.elapsed().as_secs();

            // Get live stats
            let (memory_count, avg_retention) = heartbeat_state
                .storage
                .get_stats()
                .map(|s| (s.total_nodes as usize, s.average_retention))
                .unwrap_or((0, 0.0));

            let event = VestigeEvent::Heartbeat {
                uptime_secs: uptime,
                memory_count,
                avg_retention,
                timestamp: Utc::now(),
            };

            if heartbeat_tx.send(event.to_json()).await.is_err() {
                break;
            }
        }
    });

    // Main loop: forward events + heartbeats to client, handle incoming messages
    loop {
        tokio::select! {
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        let json = event.to_json();
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("WebSocket client lagged, dropped {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Heartbeat
            Some(hb) = heartbeat_rx.recv() => {
                if sender.send(Message::Text(hb.into())).await.is_err() {
                    break;
                }
            }
            // Client message (ping/pong, close, or commands)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        // Future: handle client commands (trigger dream, etc.)
                        debug!("WebSocket received: {}", text);
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    heartbeat_handle.abort();
    debug!("WebSocket client disconnected");
}

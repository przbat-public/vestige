//! stdio Transport for MCP
//!
//! Handles JSON-RPC communication over stdin/stdout.
//! v1.9.2: Async tokio I/O with error resilience.
//! v3.5: read loop is no longer raced against a keep-alive timer (see below).

use std::io;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tracing::{debug, error, info, warn};

use super::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::server::McpServer;

/// Maximum consecutive I/O errors before giving up
const MAX_CONSECUTIVE_ERRORS: u32 = 5;

/// stdio Transport for MCP server
pub struct StdioTransport;

impl StdioTransport {
    pub fn new() -> Self {
        Self
    }

    /// Run the MCP server over stdin/stdout with error resilience.
    pub async fn run(self, server: McpServer) -> Result<(), io::Error> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        Self::run_with_io(server, BufReader::new(stdin), stdout).await
    }

    /// The transport loop over any reader/writer pair.
    ///
    /// Extracted from [`run`] so tests can drive a full frame in and out without
    /// spawning a subprocess.
    ///
    /// **There is deliberately no keep-alive timer in this loop.** An earlier
    /// revision wrapped the read in `tokio::select! { read_line(..), sleep(30s) }`
    /// to emit a `notifications/ping` heartbeat. Two things were wrong with that:
    ///
    /// 1. `AsyncBufReadExt::read_line` is *not* cancellation-safe — tokio
    ///    documents that if another branch of the `select!` completes first, the
    ///    bytes already read into the future's internal buffer are dropped. A
    ///    client that wrote half a frame and then paused lost that half, and the
    ///    request it belonged to was never executed.
    /// 2. MCP has no `notifications/ping` method. Liveness is client-driven: the
    ///    client sends `ping` *requests* (answered by `handle_request`) and
    ///    notices a dead server by the missing reply. A server that injects frames
    ///    nobody asked for pollutes the stream.
    ///
    /// Keeping the read un-raced removes both problems at the root. If a
    /// keep-alive is ever needed again, run it as a separate task that owns a
    /// clone of the writer — never as a competing branch on this reader.
    pub async fn run_with_io<R, W>(
        mut server: McpServer,
        mut reader: R,
        mut stdout: W,
    ) -> Result<(), io::Error>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut consecutive_errors: u32 = 0;
        let mut line_buf = String::new();

        loop {
            line_buf.clear();

            match reader.read_line(&mut line_buf).await {
                Ok(0) => {
                    // Clean EOF — stdin closed
                    info!("stdin closed (EOF), shutting down");
                    break;
                }
                Ok(_) => {
                    consecutive_errors = 0;
                    let line = line_buf.trim();

                    if line.is_empty() {
                        continue;
                    }

                    debug!("Received: {} bytes", line.len());

                    // Parse JSON-RPC request
                    let request: JsonRpcRequest = match serde_json::from_str(line) {
                        Ok(r) => r,
                        Err(e) => {
                            warn!("Failed to parse request: {}", e);
                            let error_response =
                                JsonRpcResponse::error(None, JsonRpcError::parse_error());
                            match serde_json::to_string(&error_response) {
                                Ok(response_json) => {
                                    let out = format!("{}\n", response_json);
                                    stdout.write_all(out.as_bytes()).await?;
                                    stdout.flush().await?;
                                }
                                Err(e) => {
                                    error!("Failed to serialize error response: {}", e);
                                    let fallback = "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}\n";
                                    let _ = stdout.write_all(fallback.as_bytes()).await;
                                    let _ = stdout.flush().await;
                                }
                            }
                            continue;
                        }
                    };

                    // Handle the request. `None` means the frame was a
                    // notification: JSON-RPC forbids answering those.
                    if let Some(response) = server.handle_request(request).await {
                        match serde_json::to_string(&response) {
                            Ok(response_json) => {
                                debug!("Sending: {} bytes", response_json.len());
                                let out = format!("{}\n", response_json);
                                stdout.write_all(out.as_bytes()).await?;
                                stdout.flush().await?;
                            }
                            Err(e) => {
                                error!("Failed to serialize response: {}", e);
                                let fallback = "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}\n";
                                let _ = stdout.write_all(fallback.as_bytes()).await;
                                let _ = stdout.flush().await;
                            }
                        }
                    }
                }
                Err(e) => {
                    consecutive_errors += 1;
                    warn!(
                        "I/O error reading stdin ({}/{}): {}",
                        consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                    );
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!(
                            "Too many consecutive I/O errors ({}), shutting down",
                            consecutive_errors
                        );
                        break;
                    }
                    // Brief pause before retrying
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        Ok(())
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, DuplexStream};
    use tokio::sync::Mutex;

    use crate::cognitive::CognitiveEngine;
    use vestige_core::Storage;

    async fn test_server() -> (McpServer, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(Some(dir.path().join("test.db"))).unwrap());
        let cognitive = Arc::new(Mutex::new(CognitiveEngine::new()));
        (McpServer::new(storage, cognitive), dir)
    }

    /// Read one newline-delimited frame from the transport.
    async fn next_frame(client_reader: &mut DuplexStream) -> String {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let n = tokio::time::timeout(Duration::from_secs(10), client_reader.read(&mut byte))
                .await
                .expect("transport must answer instead of going quiet")
                .expect("client reader cannot fail");
            if n == 0 {
                break;
            }
            if byte[0] == b'\n' {
                break;
            }
            buf.push(byte[0]);
        }
        String::from_utf8(buf).expect("transport speaks UTF-8")
    }

    /// Yield until the transport task has certainly been polled and parked on its
    /// read. Required before touching the paused clock: a `sleep` branch only gets
    /// registered (and can therefore only fire) once the loop has been polled.
    async fn park_transport() {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }

    /// A frame split across two writes, separated by an idle period longer than
    /// the old 30 s heartbeat interval, must still be assembled and executed.
    ///
    /// Regression for the `select!`-cancelled `read_line`: the heartbeat branch
    /// used to win the race during the pause and silently drop the bytes already
    /// buffered inside the read future.
    #[tokio::test(start_paused = true)]
    async fn frame_split_across_an_idle_period_is_still_executed() {
        let (server, _dir) = test_server().await;
        // bytes the test writes → transport's reader
        let (mut client_writer, transport_reader) = tokio::io::duplex(64 * 1024);
        // bytes the transport writes → client's reader
        let (transport_writer, mut client_reader) = tokio::io::duplex(64 * 1024);

        tokio::spawn(async move {
            let _ = StdioTransport::run_with_io(
                server,
                BufReader::new(transport_reader),
                transport_writer,
            )
            .await;
        });

        // First half of an `initialize` request — no trailing newline.
        client_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initia")
            .await
            .unwrap();
        client_writer.flush().await.unwrap();

        // Let the transport pick those bytes up and park on the incomplete line
        // BEFORE the clock moves.
        park_transport().await;

        // Idle longer than the 30 s heartbeat the transport used to emit.
        tokio::time::advance(Duration::from_secs(120)).await;
        park_transport().await;

        // Second half arrives later.
        client_writer.write_all(b"lize\"}\n").await.unwrap();
        client_writer.flush().await.unwrap();

        let frame = next_frame(&mut client_reader).await;
        let response: serde_json::Value =
            serde_json::from_str(&frame).expect("transport must emit a JSON-RPC frame");

        assert_eq!(
            response["id"], 1,
            "the assembled frame must be answered: {frame}"
        );
        assert_eq!(
            response["result"]["serverInfo"]["name"], "vestige",
            "the whole frame must have been parsed and executed, not truncated: {frame}"
        );
    }

    /// The transport must never write unprompted — no keep-alive notifications.
    ///
    /// Regression for `notifications/ping`, which is not an MCP method and used
    /// to be pushed to stdout every 30 s, including before `initialize`.
    #[tokio::test(start_paused = true)]
    async fn idle_transport_writes_nothing() {
        let (server, _dir) = test_server().await;
        // Holder for the writing end so the reader never sees EOF.
        let (_client_writer, transport_reader) = tokio::io::duplex(64 * 1024);
        let (transport_writer, mut client_reader) = tokio::io::duplex(64 * 1024);

        let handle = tokio::spawn(async move {
            let _ = StdioTransport::run_with_io(
                server,
                BufReader::new(transport_reader),
                transport_writer,
            )
            .await;
        });

        // Let the transport park on its read before the clock moves — otherwise
        // there is no registered timer for `advance` to fire.
        park_transport().await;

        // Ten minutes of silence.
        tokio::time::advance(Duration::from_secs(600)).await;
        park_transport().await;

        let mut buf = [0u8; 256];
        match client_reader.read(&mut buf).now_or_never() {
            // Pending = nothing was written. This is the expected outcome.
            None => {}
            Some(Ok(0)) => panic!("transport closed the stream while idle"),
            Some(Ok(n)) => panic!(
                "transport wrote {} unprompted byte(s): {}",
                n,
                String::from_utf8_lossy(&buf[..n])
            ),
            Some(Err(e)) => panic!("unexpected read error: {e}"),
        }

        handle.abort();
    }
}

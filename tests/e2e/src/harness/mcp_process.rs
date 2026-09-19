//! Real `vestige-mcp` stdio server harness.
//!
//! Spawns the **actual server binary** as a child process, speaks JSON-RPC 2.0
//! over its stdin/stdout pipes and exposes small helpers for driving the MCP
//! lifecycle (`initialize` → `tools/list` → `tools/call`).
//!
//! Why this exists: the E2E MCP targets used to assert on JSON literals written
//! inside the test file — the server never started, so a broken transport,
//! dispatcher or tool catalog could not fail them. Every assertion in those
//! targets now travels through this harness.
//!
//! ## Isolation / determinism
//!
//! * The database is a file inside a [`TempDir`], never the developer's store.
//! * `VESTIGE_TEST_MOCK_EMBEDDINGS=1` keeps the embedding layer offline (see
//!   `vestige_core::embeddings::mock_embedding`) — no ~547 MB ONNX download.
//! * `HF_HUB_OFFLINE=1` stops the background reranker load from reaching the
//!   network; a missing model degrades to the BM25 fallback, which is fine.
//! * Dashboard and HTTP transports bind ephemeral ports (`:0`), so parallel
//!   test targets can never collide on 3927/3928.
//! * `VESTIGE_AUTH_TOKEN` is supplied up front so the server never writes an
//!   auth-token file outside the temp directory.
//! * stderr goes to a file (not a pipe) so a chatty server can never deadlock
//!   on a full pipe buffer, and the tail is available for failure messages.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

/// How long to wait for a single server response before failing the test.
///
/// Generous on purpose: the first response includes database creation,
/// migrations and cognitive-engine hydration, and CI may run the binary in a
/// debug profile on a cold machine. Override with `VESTIGE_E2E_TIMEOUT_SECS`.
fn response_timeout() -> Duration {
    let secs = std::env::var("VESTIGE_E2E_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(60);
    Duration::from_secs(secs)
}

/// How long to wait for a clean exit after stdin is closed. If the server is
/// still busy loading an optional model in the background we kill it instead of
/// hanging the suite — all assertions have already run by that point.
const EXIT_GRACE: Duration = Duration::from_secs(10);

/// Environment variable pointing at an explicit `vestige-mcp` binary.
pub const SERVER_BIN_ENV: &str = "VESTIGE_MCP_BIN";

/// Workspace root, derived from this crate's manifest directory
/// (`<root>/tests/e2e`).
#[must_use]
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Explain, in a way a human can act on, why no server binary was found.
#[must_use]
pub fn missing_binary_help() -> String {
    format!(
        "no `vestige-mcp` binary found.\n\
         Build it first, e.g.:\n    \
         cargo build -p vestige-mcp            # debug (fast)\n    \
         cargo build --release -p vestige-mcp  # release (what CI builds)\n\
         or point {SERVER_BIN_ENV} at an existing binary."
    )
}

/// Locate the `vestige-mcp` executable.
///
/// Resolution order:
/// 1. `$VESTIGE_MCP_BIN` (explicit override — CI-friendly).
/// 2. `option_env!("CARGO_BIN_EXE_vestige-mcp")`. Cargo only sets this for
///    tests of the package that *owns* the binary, so in this crate it is
///    normally absent; it is kept first in case the target ever moves into
///    `vestige-mcp` itself.
/// 3. `$CARGO_TARGET_DIR/{debug,release}/vestige-mcp` and
///    `<workspace>/target/{debug,release}/vestige-mcp`, preferring whichever
///    exists and was modified most recently (a stale release build must not
///    shadow a freshly built debug one).
#[must_use]
pub fn find_server_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var(SERVER_BIN_ENV) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(path) = option_env!("CARGO_BIN_EXE_vestige-mcp") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|p| {
            if p.is_absolute() {
                p
            } else {
                workspace_root().join(p)
            }
        })
        .unwrap_or_else(|_| workspace_root().join("target"));

    ["debug", "release"]
        .iter()
        .map(|profile| target_dir.join(profile).join("vestige-mcp"))
        .filter(|candidate| candidate.is_file())
        .max_by_key(|candidate| candidate.metadata().and_then(|meta| meta.modified()).ok())
}

/// Start one `vestige-mcp` child process plus its stdout reader thread.
fn launch(
    binary: &Path,
    data_dir: &Path,
    stderr_path: &Path,
) -> Result<(Child, ChildStdin, Receiver<Option<String>>), String> {
    let db_path = data_dir.join("vestige.db");
    let stderr_file = File::create(stderr_path)
        .map_err(|e| format!("could not create {}: {e}", stderr_path.display()))?;

    let mut child = Command::new(binary)
        .arg("--data-dir")
        .arg(&db_path)
        .current_dir(data_dir)
        .env("VESTIGE_TEST_MOCK_EMBEDDINGS", "1")
        .env("HF_HUB_OFFLINE", "1")
        .env("FASTEMBED_CACHE_PATH", data_dir.join("fastembed-cache"))
        // Ephemeral ports: no collision with a developer's running server
        // or with other test targets running in parallel.
        .env("VESTIGE_DASHBOARD_PORT", "0")
        .env("VESTIGE_HTTP_PORT", "0")
        .env(
            "VESTIGE_AUTH_TOKEN",
            "vestige-e2e-test-token-0123456789abcdef",
        )
        .env("RUST_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", binary.display()))?;

    let stdin = child.stdin.take().ok_or("child stdin was not piped")?;
    let stdout = child.stdout.take().ok_or("child stdout was not piped")?;

    // Reader thread: the child writes one JSON document per line; hand each
    // line to the test thread over a channel so reads can time out.
    let (tx, responses) = channel::<Option<String>>();
    std::thread::Builder::new()
        .name("vestige-mcp-stdout-reader".to_string())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        if tx.send(Some(line)).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            // EOF: tell the test thread the pipe is closed.
            let _ = tx.send(None);
        })
        .map_err(|e| format!("could not start stdout reader thread: {e}"))?;

    Ok((child, stdin, responses))
}

/// A live `vestige-mcp` child process speaking MCP over stdio.
pub struct McpServerProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<Option<String>>,
    data_dir: TempDir,
    binary: PathBuf,
    stderr_path: PathBuf,
    next_id: i64,
    /// Server-initiated lines (notifications, heartbeat pings) seen so far.
    notifications: Vec<Value>,
}

impl McpServerProcess {
    /// Spawn the server, or return `None` with a loud message when the binary
    /// has not been built.
    ///
    /// Tests should use this (rather than [`Self::spawn`]) so a missing binary
    /// skips instead of failing: the MCP targets are run by CI after
    /// `cargo build --release --package vestige-mcp`, but a bare
    /// `cargo test -p vestige-e2e-tests --test mcp_protocol` does not build it.
    pub fn spawn_or_skip(test_name: &str) -> Option<Self> {
        match Self::spawn() {
            Ok(process) => Some(process),
            Err(reason) => {
                eprintln!(
                    "\n[SKIP] {test_name}: {reason}\n\
                     [SKIP] The MCP end-to-end assertions did NOT run. {}\n",
                    missing_binary_help()
                );
                None
            }
        }
    }

    /// Spawn the server over stdio with an isolated TempDir database.
    pub fn spawn() -> Result<Self, String> {
        let binary = find_server_binary().ok_or_else(missing_binary_help)?;

        let data_dir = TempDir::new().map_err(|e| format!("TempDir::new failed: {e}"))?;
        let stderr_path = data_dir.path().join("server.stderr.log");
        let (child, stdin, responses) = launch(&binary, data_dir.path(), &stderr_path)?;

        Ok(Self {
            child,
            stdin: Some(stdin),
            responses,
            data_dir,
            binary,
            stderr_path,
            next_id: 1,
            notifications: Vec::new(),
        })
    }

    /// Kill the current child and start a fresh one against the *same*
    /// database file — the only way to prove that what a tool call wrote
    /// survived a real server restart (rather than living in process memory).
    pub fn restart(&mut self) -> Result<(), String> {
        self.shutdown();
        let stderr_path = self
            .data_dir
            .path()
            .join(format!("server.stderr.{}.log", self.next_id));
        let (child, stdin, responses) = launch(&self.binary, self.data_dir.path(), &stderr_path)?;
        self.child = child;
        self.stdin = Some(stdin);
        self.responses = responses;
        self.stderr_path = stderr_path;
        self.next_id = 1;
        self.notifications.clear();
        Ok(())
    }

    /// Path to the isolated SQLite database backing this server.
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.path().join("vestige.db")
    }

    /// Binary under test (useful in assertion messages).
    #[must_use]
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Server-initiated frames (no matching request id) seen so far.
    ///
    /// The stdio transport heartbeats every 30 s, so a short test normally sees
    /// none — which is exactly why an empty vector proves the server did not
    /// answer `notifications/initialized`.
    #[must_use]
    pub fn notifications(&self) -> &[Value] {
        &self.notifications
    }

    /// Tail of the server's stderr log, for failure diagnostics.
    #[must_use]
    pub fn stderr_tail(&self) -> String {
        let Ok(contents) = std::fs::read_to_string(&self.stderr_path) else {
            return "<stderr log unreadable>".to_string();
        };
        let lines: Vec<&str> = contents.lines().collect();
        let start = lines.len().saturating_sub(30);
        lines[start..].join("\n")
    }

    /// Write one JSON-RPC frame to the server's stdin.
    fn send_value(&mut self, value: &Value) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "stdin already closed (server shut down)".to_string())?;
        let mut frame = value.to_string();
        frame.push('\n');
        stdin
            .write_all(frame.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| {
                format!(
                    "failed to write {} to server stdin: {e}\n--- server stderr ---\n{}",
                    frame.trim_end(),
                    self.stderr_tail()
                )
            })
    }

    /// Read the next line the server writes, with a timeout.
    fn read_raw_line(&mut self) -> Result<String, String> {
        match self.responses.recv_timeout(response_timeout()) {
            Ok(Some(line)) => Ok(line),
            Ok(None) => Err(format!(
                "server closed stdout before answering\n--- server stderr ---\n{}",
                self.stderr_tail()
            )),
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "timed out after {:?} waiting for a server response\n--- server stderr ---\n{}",
                response_timeout(),
                self.stderr_tail()
            )),
            Err(RecvTimeoutError::Disconnected) => {
                Err("stdout reader thread died before a response arrived".to_string())
            }
        }
    }

    /// Parse a line into JSON, failing loudly with the server's stderr tail.
    fn parse_frame(&self, line: &str) -> Result<Value, String> {
        serde_json::from_str(line).map_err(|e| {
            format!(
                "server wrote a non-JSON line: {e}\nline: {line}\n--- server stderr ---\n{}",
                self.stderr_tail()
            )
        })
    }

    /// Send a request and return the matching JSON-RPC envelope.
    ///
    /// Server-initiated notifications are skipped (and recorded) until the
    /// response carrying `id` arrives.
    pub fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let mut frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        });
        if let Some(params) = params {
            frame["params"] = params;
        }
        self.send_value(&frame)?;

        loop {
            let line = self.read_raw_line()?;
            let value = self.parse_frame(&line)?;
            if value["jsonrpc"] != json!("2.0") {
                return Err(format!("response is not JSON-RPC 2.0: {value}"));
            }
            if value.get("id") == Some(&json!(id)) {
                return Ok(value);
            }
            // No id (or an unrelated id): a server-initiated notification such
            // as the stdio heartbeat ping. Keep it for assertions and continue.
            self.notifications.push(value);
        }
    }

    /// Send a notification (a request without an `id`). The server MUST NOT
    /// answer it.
    pub fn notify(&mut self, method: &str, params: Option<Value>) -> Result<(), String> {
        let mut frame = json!({
            "jsonrpc": "2.0",
            "method": method,
        });
        if let Some(params) = params {
            frame["params"] = params;
        }
        self.send_value(&frame)
    }

    /// Write a raw, possibly malformed frame (for parse-error coverage).
    pub fn send_raw(&mut self, raw: &str) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "stdin already closed (server shut down)".to_string())?;
        let mut frame = raw.to_string();
        frame.push('\n');
        stdin
            .write_all(frame.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("failed to write raw frame: {e}"))
    }

    /// Read the next frame the server emits, whatever its id.
    pub fn read_frame(&mut self) -> Result<Value, String> {
        let line = self.read_raw_line()?;
        self.parse_frame(&line)
    }

    /// Full MCP handshake: `initialize` + `notifications/initialized`.
    ///
    /// Returns the `initialize` result so tests can assert on it.
    pub fn initialize(&mut self) -> Result<Value, String> {
        let response = self.request(
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "vestige-e2e", "version": "1.0.0" }
            })),
        )?;
        let result = response
            .get("result")
            .cloned()
            .ok_or_else(|| format!("initialize returned an error: {response}"))?;
        self.notify("notifications/initialized", None)?;
        Ok(result)
    }

    /// `tools/list` → the advertised tool array.
    pub fn tools_list(&mut self) -> Result<Vec<Value>, String> {
        let response = self.request("tools/list", None)?;
        response["result"]["tools"]
            .as_array()
            .cloned()
            .ok_or_else(|| format!("tools/list did not return a tools array: {response}"))
    }

    /// `tools/call` → the raw `CallToolResult` envelope.
    pub fn call_tool_raw(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let response = self.request(
            "tools/call",
            Some(json!({ "name": name, "arguments": arguments })),
        )?;
        response
            .get("result")
            .cloned()
            .ok_or_else(|| format!("tools/call '{name}' returned a JSON-RPC error: {response}"))
    }

    /// `tools/call` for a tool that is expected to succeed: returns the parsed
    /// JSON payload carried in `content[0].text`.
    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let envelope = self.call_tool_raw(name, arguments)?;

        if envelope["isError"] == json!(true) {
            return Err(format!(
                "tools/call '{name}' reported isError=true: {envelope}"
            ));
        }
        let content = envelope["content"]
            .as_array()
            .ok_or_else(|| format!("tools/call '{name}' returned no content array: {envelope}"))?;
        let first = content
            .first()
            .ok_or_else(|| format!("tools/call '{name}' returned empty content: {envelope}"))?;
        let text = first["text"]
            .as_str()
            .ok_or_else(|| format!("tools/call '{name}' content[0] has no text: {envelope}"))?;
        serde_json::from_str(text)
            .map_err(|e| format!("tools/call '{name}' returned non-JSON text ({e}): {text}"))
    }

    /// `tools/call` for a tool that is expected to fail: returns
    /// `(isError, parsed_or_raw_text)`.
    pub fn call_tool_expecting_error(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<(bool, String), String> {
        let envelope = self.call_tool_raw(name, arguments)?;
        let is_error = envelope["isError"] == json!(true);
        let text = envelope["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Ok((is_error, text))
    }

    /// Close stdin and wait for a clean exit (killing the child if it lingers).
    ///
    /// Closing stdin is what makes the stdio transport terminate; without it
    /// the server would wait forever for the next frame.
    pub fn shutdown(&mut self) -> Option<i32> {
        self.stdin.take();
        let deadline = Instant::now() + EXIT_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return status.code(),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    return None;
                }
            }
        }
    }
}

impl Drop for McpServerProcess {
    fn drop(&mut self) {
        // Never leak a server process, even when an assertion panics.
        self.stdin.take();
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            _ => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }
}

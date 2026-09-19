"""Minimal JSON-RPC-over-stdio client for the vestige-mcp server.

Protocol notes learned from the real server:

  * Framing is line-delimited JSON on stdout (not LSP Content-Length framing).
  * Order is: initialize -> notifications/initialized -> tools/call.
  * stdin MUST stay open for the lifetime of the session. Closing it ends the
    server mid-run.
  * Tool results arrive as `result.content[0].text`, a JSON document.

PORTING NOTE (see PORTING-NOTES.md for the full list of deviations)
-------------------------------------------------------------------
This client was ported from an upstream harness written against Vestige v2.x.
Two v2.x mechanisms it relied on no longer exist and are replaced here:

  1. `VESTIGE_DATA_DIR` -> gone. The current server takes the store location
     from `--data-dir <PATH>`, and that path is the SQLite DATABASE FILE, not
     a directory (`Storage::new(Some(p))` is passed straight to
     `Connection::open`). We therefore pass `--data-dir <db_dir>/vestige.db`
     and let the HNSW sidecars land next to it (`<db_dir>/vestige.hnsw`,
     `<db_dir>/vestige.hnsw.meta.json`, derived via `Path::with_extension`).
     A fresh `db_dir` per simulated user is what isolates users now -- there is
     no `scope` argument to isolate with.

  2. `VESTIGE_DASHBOARD_ENABLED=false` / `VESTIGE_HTTP_ENABLED=0` -> gone. The
     current server always tries to bind the dashboard (default 3927) and the
     HTTP transport (default 3928), so a benchmark run would collide with a
     developer's live instance. We therefore hand each child its own free
     loopback ports, and we set `VESTIGE_AUTH_TOKEN` to a throwaway value so
     the HTTP transport never touches the shared `auth_token` file in the
     global data directory.

WARMUP
------
Upstream blocked for a mandatory 45 s embedding warmup because its server
initialised embeddings asynchronously *after* `initialize` returned. The
current server initialises embeddings synchronously inside `main()` before the
stdio transport is created, so embeddings are provably ready by the time
`initialize` answers. What is still asynchronous is the cross-encoder reranker
(loaded in a background task ~1 s after startup, and it downloads ~150 MB on a
cold cache). We keep a warmup window by default -- now as a reranker/settling
margin rather than an embedding requirement -- and record it in the results
file. `--warmup 5` is fine for smoke runs; keep it large (300) the very first
time you run the server on a machine with a cold model cache.
"""
from __future__ import annotations

import json
import os
import pathlib
import queue
import socket
import subprocess
import threading
import time
from typing import Any, Dict, List, Optional

DEFAULT_WARMUP_SECONDS = 45.0


def mtime_utc(path: str) -> Optional[str]:
    """Build timestamp of a binary, so a stale build is visible in results."""
    try:
        import datetime

        ts = pathlib.Path(path).stat().st_mtime
        return datetime.datetime.fromtimestamp(ts, datetime.timezone.utc).isoformat(
            timespec="seconds"
        )
    except OSError:
        return None

#: Throwaway bearer token, long enough to satisfy the server's length warning.
#: Never used: the harness speaks stdio only. Its only job is to stop the
#: server from reading/creating the shared `auth_token` file.
_DUMMY_AUTH_TOKEN = "memconflict-harness-throwaway-token-0123456789abcdef"


class VestigeMCPError(RuntimeError):
    pass


def free_port() -> int:
    """Pick a free loopback TCP port.

    Small TOCTOU race against other processes, which is acceptable here: the
    dashboard/HTTP listeners are best-effort in the server (a failed bind logs
    a warning and does not stop the stdio transport).
    """
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class VestigeMCP:
    """One server process, backed by one fresh store under `db_dir`."""

    def __init__(
        self,
        binary: str,
        db_dir: str,
        warmup_seconds: float = DEFAULT_WARMUP_SECONDS,
        timeout: float = 300.0,
        extra_env: Optional[Dict[str, str]] = None,
    ) -> None:
        self.binary = str(pathlib.Path(binary).resolve())
        # Directory holding this run's database + HNSW sidecars. NOT the value
        # of --data-dir: that flag takes the database file path (see the module
        # docstring).
        self.db_dir = str(pathlib.Path(db_dir).resolve())
        self.db_path = str(pathlib.Path(self.db_dir) / "vestige.db")
        self.warmup_seconds = warmup_seconds
        self.timeout = timeout
        self._id = 0
        self._proc: Optional[subprocess.Popen] = None
        self._out: "queue.Queue[Optional[str]]" = queue.Queue()
        self._stderr_lines: List[str] = []
        self._extra_env = dict(extra_env or {})
        self.server_info: Dict[str, Any] = {}
        self.observed_warmup: Dict[str, Any] = {}
        self.command: List[str] = []
        self.ports: Dict[str, int] = {}

    # -- lifecycle ---------------------------------------------------------

    def __enter__(self) -> "VestigeMCP":
        self.start()
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def start(self) -> None:
        pathlib.Path(self.db_dir).mkdir(parents=True, exist_ok=True)
        self.ports = {"http": free_port(), "dashboard": free_port()}
        env = dict(os.environ)
        env["VESTIGE_AUTH_TOKEN"] = env.get("VESTIGE_AUTH_TOKEN") or _DUMMY_AUTH_TOKEN
        env["VESTIGE_HTTP_PORT"] = str(self.ports["http"])
        env["VESTIGE_DASHBOARD_PORT"] = str(self.ports["dashboard"])
        env.update(self._extra_env)

        self.command = [self.binary, "--data-dir", self.db_path]
        self._proc = subprocess.Popen(
            self.command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            text=True,
            bufsize=1,
        )
        threading.Thread(target=self._pump_stdout, daemon=True).start()
        threading.Thread(target=self._pump_stderr, daemon=True).start()

    def _pump_stdout(self) -> None:
        assert self._proc and self._proc.stdout
        for line in self._proc.stdout:
            line = line.strip()
            if line:
                self._out.put(line)
        self._out.put(None)

    def _pump_stderr(self) -> None:
        assert self._proc and self._proc.stderr
        for line in self._proc.stderr:
            self._stderr_lines.append(line.rstrip())

    def close(self) -> None:
        if self._proc is None:
            return
        try:
            if self._proc.stdin:
                self._proc.stdin.close()
            self._proc.terminate()
            self._proc.wait(timeout=15)
        except Exception:
            try:
                self._proc.kill()
            except Exception:
                pass
        self._proc = None

    # -- transport ---------------------------------------------------------

    def _send(self, payload: Dict[str, Any]) -> None:
        if not self._proc or not self._proc.stdin:
            raise VestigeMCPError("server not running")
        self._proc.stdin.write(json.dumps(payload) + "\n")
        self._proc.stdin.flush()

    def _await_id(self, want_id: int, timeout: float) -> Dict[str, Any]:
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise VestigeMCPError(f"timeout waiting for response id={want_id}")
            try:
                line = self._out.get(timeout=remaining)
            except queue.Empty:
                raise VestigeMCPError(f"timeout waiting for response id={want_id}")
            if line is None:
                tail = "\n".join(self._stderr_lines[-20:])
                raise VestigeMCPError(f"server exited early. stderr tail:\n{tail}")
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue  # non-JSON noise on stdout is ignored
            if msg.get("id") == want_id:
                return msg

    def request(self, method: str, params: Optional[Dict[str, Any]] = None,
                timeout: Optional[float] = None) -> Dict[str, Any]:
        self._id += 1
        rid = self._id
        self._send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
        msg = self._await_id(rid, timeout if timeout is not None else self.timeout)
        if "error" in msg:
            raise VestigeMCPError(f"{method} failed: {msg['error']}")
        return msg.get("result", {})

    def notify(self, method: str, params: Optional[Dict[str, Any]] = None) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params or {}})

    # -- MCP handshake -----------------------------------------------------

    def initialize(self) -> Dict[str, Any]:
        result = self.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "vestige-memconflict-harness", "version": "1.1"},
            },
            timeout=120.0,
        )
        self.server_info = result.get("serverInfo", {})
        self.notify("notifications/initialized")

        # Warmup window. See the module docstring: embeddings are already ready
        # (synchronous init), this covers the async reranker load.
        t0 = time.monotonic()
        time.sleep(self.warmup_seconds)
        self.observed_warmup = {
            "requested_seconds": self.warmup_seconds,
            "actual_seconds": round(time.monotonic() - t0, 2),
            "embedding_ready_signal": self.embedding_log_signal(),
            "reranker_signal": self.reranker_log_signal(),
        }
        return result

    def embedding_log_signal(self) -> Optional[str]:
        """Best-effort: surface any embedding-related server log line.

        Recorded in results so a reader can confirm the run did not silently
        fall back to keyword-only retrieval. The current server logs
        "Embedding service initialized successfully" before it opens the stdio
        transport, so this should never be None on a healthy run.
        """
        for line in reversed(self._stderr_lines):
            if "embedding service" in line.lower():
                return line[-300:]
        for line in reversed(self._stderr_lines):
            low = line.lower()
            if "embed" in low or "model" in low:
                return line[-300:]
        return None

    def reranker_log_signal(self) -> Optional[str]:
        """Best-effort: did the async reranker finish loading during warmup?"""
        for line in reversed(self._stderr_lines):
            low = line.lower()
            if "rerank" in low or "cross-encoder" in low or "cross encoder" in low:
                return line[-300:]
        return None

    def stderr_tail(self, n: int = 40) -> List[str]:
        return self._stderr_lines[-n:]

    def launch_provenance(self) -> Dict[str, Any]:
        """What this child process was actually launched with.

        Recorded into every results file. A measurement that names a binary but
        not the switches it ran under is not reproducible, and Vestige's
        behaviour is switch-dependent (`VESTIGE_NOMIC_PREFIXES` changes the
        embedding space, the consolidation interval changes background work).
        The throwaway auth token is deliberately NOT echoed.
        """
        interesting = {
            k: v for k, v in sorted(self._extra_env.items())
        }
        for key in ("VESTIGE_NOMIC_PREFIXES", "VESTIGE_CONSOLIDATION_INTERVAL_HOURS",
                    "VESTIGE_HTTP_PORT", "VESTIGE_DASHBOARD_PORT", "RUST_LOG"):
            if key in os.environ:
                interesting.setdefault(key, os.environ[key])
        return {
            "command": list(self.command),
            "ports": dict(self.ports),
            "binary_resolved": self.binary,
            "binary_mtime_utc": mtime_utc(self.binary),
            "env_switches": interesting,
        }

    # -- tools -------------------------------------------------------------

    def call_tool(self, name: str, arguments: Dict[str, Any],
                  timeout: Optional[float] = None) -> Any:
        result = self.request(
            "tools/call", {"name": name, "arguments": arguments}, timeout=timeout
        )
        content = result.get("content") or []
        for block in content:
            if block.get("type") == "text":
                text = block.get("text", "")
                try:
                    return json.loads(text)
                except json.JSONDecodeError:
                    return text
        return result

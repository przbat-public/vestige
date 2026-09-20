#!/usr/bin/env python3
"""dream-ab — an honest A/B of `dream` / `reflect` against the wave-5 measures.

The specification this harness enforces is
`docs/SELF-CONTAINED-MEMORY-DESIGN.md` §12, in particular:

* §12.2 — two arms on the same corpus, measure self-containment and use (not a
  benchmark score), a negative result is a result, and no conclusion from a
  single run: three runs, same direction, or the report says `unresolved`.
* §12.4 — never against the live store; the store path comes from
  `directories::ProjectDirs`, so a harness-owned `HOME` relocates it; control
  and treatment are the same copy; two control runs on two identical copies
  must produce identical reports or the harness reports the divergence instead
  of averaging it; every number in `RESULTS.md` carries the command that made
  it and a control column.

What it refuses to do (each one is an enumerated source of a dishonest result):

* It never opens the live store for writing. The source is read through a
  read-only SQLite connection and snapshotted with the backup API.
* It does not compare a store against itself: the isolation check proves, by
  perturbing a throw-away copy, that `HOME` really selects the copy, before any
  arm is measured.
* It does not call a `null` (never-checked) memory clean: `unchecked` is its own
  row and never enters the self-containment denominator.
* It does not fold `unchecked` anchors into resolvability; that rate is `null`
  when nothing was checkable, and the row prints `unmeasured`.
* It prints a denominator with every rate, and flags the runs in which the
  pass's own new rows changed the denominator.
* It marks the `useCounts` rows as confounded in this design: the pass reads
  the store it is being measured on, so its own internal retrievals raise the
  use numbers. That delta is measured and named, not presented as value.

Standard library only, like `benchmarks/memconflict/run.py`, and for the same
reason: a benchmark that needs a dependency resolver is a benchmark that stops
reproducing. The only external programs it runs are the built binaries
(`vestige` for the CLI surface / `vestige serve` for the MCP surface) and, for
supplementary evidence only, a read-only SQLite query.
"""

from __future__ import annotations

import argparse
import atexit
import hashlib
import json
import os
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from fractions import Fraction
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

#: Where `directories::ProjectDirs::from("com", "vestige", "core")` puts the
#: store on macOS. The harness only ever writes this path inside a temporary
#: `HOME`; the live path is read (read-only) for the snapshot and nothing else.
STORE_REL = Path("Library/Application Support/com.vestige.core")
STORE_NAME = "vestige.db"

#: The fastembed model cache is *not* under the store path, but it *is* under
#: `$HOME` (`~/Library/Caches/vestige.vestige/fastembed`). A relocated HOME
#: would make the server try to re-download ~550 MB of ONNX models, so the real
#: cache is pointed at explicitly. This changes no measured value; it keeps the
#: treatment server's embedding behaviour identical to a normal run's.
CACHE_REL = Path("Library/Caches/vestige.vestige/fastembed")

#: Tools the `pass` argument selects, and the arguments each is called with.
PASS_TOOLS = {
    "dream": ("dream", {"memory_count": 50}),
    "reflect": ("reflect", {"depth": "standard"}),
}

EXIT_OK = 0
EXIT_ERROR = 1
EXIT_NO_SURFACE = 3
EXIT_ISOLATION = 4

# ---------------------------------------------------------------------------
# narrate
# ---------------------------------------------------------------------------


def log(message: str = "") -> None:
    print(message, flush=True)


def step(message: str) -> None:
    print(f"[dream-ab] {message}", flush=True)


def die(message: str, code: int = EXIT_ERROR) -> "None":
    print(f"[dream-ab] FATAL: {message}", file=sys.stderr, flush=True)
    raise SystemExit(code)


# ---------------------------------------------------------------------------
# small helpers
# ---------------------------------------------------------------------------


def utcnow() -> datetime:
    return datetime.now(timezone.utc)


def utcnow_iso() -> str:
    return utcnow().isoformat()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def port_is_open(port: int, timeout: float = 0.4) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        return sock.connect_ex(("127.0.0.1", port)) == 0


def wait_for_port(port: int, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if port_is_open(port):
            return True
        time.sleep(0.25)
    return False


def wait_for_port_closed(port: int, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not port_is_open(port):
            return True
        time.sleep(0.25)
    return False


def parse_json_loose(text: str):
    """Parse JSON, tolerating log lines around it (CLI surfaces like to chat)."""
    text = text.strip()
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        start, end = text.find("{"), text.rfind("}")
        if start >= 0 and end > start:
            return json.loads(text[start : end + 1])
        raise


def find_quality_object(payload):
    """Locate the quality report inside whatever wrapper a surface used."""
    if isinstance(payload, dict):
        if "containment" in payload and "useCounts" in payload:
            return payload
        for key in ("quality", "result", "structuredContent", "data", "report"):
            if key in payload:
                found = find_quality_object(payload[key])
                if found is not None:
                    return found
    return None


def fraction_or_none(numerator, denominator):
    if denominator is None or numerator is None or denominator <= 0:
        return None
    return Fraction(int(numerator), int(denominator))


def as_fraction(value):
    """Accept an in-memory `Fraction`, a serialised `{num, den}` or a `"n/d"`."""
    if value is None:
        return None
    if isinstance(value, Fraction):
        return value
    if isinstance(value, dict):
        numerator, denominator = value.get("num"), value.get("den")
        if numerator is None or not denominator:
            return None
        return Fraction(int(numerator), int(denominator))
    try:
        return Fraction(str(value))
    except (ValueError, ZeroDivisionError):
        return None


def json_default(value):
    """`Fraction` is exact and does not survive `json.dumps` on its own."""
    if isinstance(value, Fraction):
        return {"num": value.numerator, "den": value.denominator, "float": float(value)}
    return str(value)


# ---------------------------------------------------------------------------
# the store: snapshot, copy, count
# ---------------------------------------------------------------------------


def sqlite_ro(path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True, timeout=30)


def store_row_counts(db_path: Path) -> dict:
    counts = {}
    try:
        with sqlite_ro(db_path) as con:
            for table in ("knowledge_nodes", "memory_access_log", "code_refs"):
                try:
                    counts[table] = con.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                except sqlite3.Error as exc:  # pragma: no cover - schema drift
                    counts[table] = None
                    counts[f"{table}_error"] = str(exc)
    except sqlite3.Error as exc:
        counts["error"] = str(exc)
    return counts


def snapshot_store(src: Path, dest: Path) -> dict:
    """Copy `src` to `dest` without ever opening `src` for writing.

    The SQLite backup API, not a file copy: the live store is in WAL mode and
    may be written by the user's own server while the harness runs, and a
    file-level copy of a hot WAL database can capture a torn page set. A copy
    that silently lost rows is exactly the "compared a store against itself"
    failure this harness is supposed to prevent, so the snapshot is verified
    before it is used.
    """
    dest.parent.mkdir(parents=True, exist_ok=True)
    for suffix in ("", "-wal", "-shm"):
        leftover = Path(str(dest) + suffix)
        if leftover.exists():
            leftover.unlink()

    method = "sqlite3 backup API (source opened mode=ro)"
    try:
        source = sqlite3.connect(f"file:{src}?mode=ro", uri=True, timeout=60)
        target = sqlite3.connect(str(dest))
        try:
            source.backup(target)
            integrity = target.execute("PRAGMA integrity_check").fetchone()[0]
            target.commit()
        finally:
            target.close()
            source.close()
    except sqlite3.Error as exc:
        # Fallback: copy the database and its WAL sidecars together, then
        # verify. Still read-only on the source.
        method = f"file copy of db+wal+shm (backup API failed: {exc})"
        shutil.copyfile(src, dest)
        for suffix in ("-wal", "-shm"):
            sidecar = Path(str(src) + suffix)
            if sidecar.exists():
                shutil.copyfile(sidecar, Path(str(dest) + suffix))
        try:
            with sqlite3.connect(str(dest)) as con:
                integrity = con.execute("PRAGMA integrity_check").fetchone()[0]
        except sqlite3.Error as inner:
            die(f"snapshot of {src} failed and the fallback copy is unreadable: {inner}")

    if str(integrity).lower() != "ok":
        die(f"snapshot of {src} failed PRAGMA integrity_check: {integrity}")

    counts = store_row_counts(dest)
    if not counts.get("knowledge_nodes"):
        die(f"snapshot of {src} has no rows in knowledge_nodes — refusing to measure an empty store")

    return {
        "sourcePath": str(src),
        "snapshotPath": str(dest),
        "method": method,
        "sha256": sha256_file(dest),
        "bytes": dest.stat().st_size,
        "integrityCheck": integrity,
        "rows": counts,
    }


def home_store_path(home: Path) -> Path:
    return home / STORE_REL / STORE_NAME


def prepare_home(root: Path, name: str, snapshot: Path) -> Path:
    """A fresh `HOME` whose store is a byte copy of the verified snapshot."""
    home = root / name / "home"
    dest = home_store_path(home)
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(snapshot, dest)
    return home


def store_env(home: Path, cache: Path | None) -> dict:
    env = dict(os.environ)
    env["HOME"] = str(home)
    env.setdefault("RUST_LOG", "warn")
    if cache is not None and cache.is_dir():
        env["FASTEMBED_CACHE_PATH"] = str(cache)
    return env


# ---------------------------------------------------------------------------
# quality surfaces
# ---------------------------------------------------------------------------


class QualityUnavailable(RuntimeError):
    pass


class CliQuality:
    """`vestige quality --json` — the surface §12.4 names for harnesses."""

    name = "cli"

    def __init__(self, binary: Path, cache: Path | None, timeout: float):
        self.binary = binary
        self.cache = cache
        self.timeout = timeout

    def command(self, since: str | None) -> list:
        cmd = [str(self.binary), "quality", "--json"]
        if since:
            cmd += ["--since", since]
        return cmd

    def read(self, home: Path, since: str | None) -> dict:
        cmd = self.command(since)
        proc = subprocess.run(
            cmd,
            env=store_env(home, self.cache),
            capture_output=True,
            text=True,
            timeout=self.timeout,
        )
        if proc.returncode != 0:
            raise QualityUnavailable(
                f"`{' '.join(cmd)}` exited {proc.returncode}: {proc.stderr.strip()[-400:]}"
            )
        try:
            payload = parse_json_loose(proc.stdout)
        except json.JSONDecodeError as exc:
            raise QualityUnavailable(f"CLI output is not JSON: {exc}") from exc
        report = find_quality_object(payload)
        if report is None:
            raise QualityUnavailable("CLI output has no quality object")
        return {
            "surface": self.name,
            "command": f"HOME={home} " + " ".join(cmd),
            "report": report,
            "stderr": proc.stderr.strip()[-2000:],
        }


class McpQuality:
    """`memory_health` over the server's HTTP MCP endpoint — the fallback."""

    name = "mcp"

    def __init__(self, binary: Path, cache: Path | None, timeout: float, warmup: float):
        self.binary = binary
        self.cache = cache
        self.timeout = timeout
        self.warmup = warmup
        self._counter = 0

    def command(self, since: str | None) -> str:
        suffix = f" (since={since})" if since else ""
        return f"<vestige serve>; tools/call memory_health{suffix}"

    def read(self, home: Path, since: str | None) -> dict:
        self._counter += 1
        port, dash = free_port(), free_port()
        server = Server(
            binary=self.binary,
            home=home,
            port=port,
            dashboard_port=dash,
            cache=self.cache,
            log_path=home.parent / f"quality-mcp-{self._counter}.log",
        )
        server.start()
        try:
            server.wait_ready(self.timeout)
            if self.warmup:
                time.sleep(min(self.warmup, 10))
            client = McpSession(port, server.auth_token(), timeout=self.timeout)
            client.initialize()
            client.notify("notifications/initialized")
            result = client.call_tool("memory_health", {"since": since} if since else {})
            report = find_quality_object(result.get("payload"))
            if report is None:
                raise QualityUnavailable("memory_health returned no quality object")
            return {
                "surface": self.name,
                "command": f"HOME={home} {self.binary} serve --port {port} ; tools/call memory_health",
                "report": report,
                "stderr": "",
            }
        finally:
            server.stop()


def detect_surface(cli: Path, cache: Path | None, timeout: float, warmup: float):
    """CLI first (it is the surface §12.4 names for harnesses), MCP second."""
    candidates = []
    if cli.exists():
        candidates.append(CliQuality(cli, cache, timeout))
        candidates.append(McpQuality(cli, cache, timeout, warmup))
    problems = []
    for candidate in candidates:
        probe = Path(tempfile.mkdtemp(prefix="dream-ab-surface-"))
        try:
            home = prepare_probe_home(probe)
            if isinstance(candidate, CliQuality):
                # An empty HOME is enough: the surface either answers with a
                # quality object or it does not exist yet.
                candidate.read(home, None)
            else:
                # The MCP surface needs a store to open; let the server create
                # its own empty one rather than handing it a stub schema.
                candidate.read(home, None)
            return candidate
        except (QualityUnavailable, OSError, subprocess.SubprocessError, sqlite3.Error) as exc:
            problems.append(f"{candidate.name}: {exc}")
        finally:
            shutil.rmtree(probe, ignore_errors=True)
    die(
        "no quality surface available — neither `vestige quality --json` nor a "
        "`memory_health` quality object answered.\n  " + "\n  ".join(problems),
        EXIT_NO_SURFACE,
    )


def prepare_probe_home(root: Path) -> Path:
    home = root / "home"
    home_store_path(home).parent.mkdir(parents=True, exist_ok=True)
    return home


# ---------------------------------------------------------------------------
# MCP over HTTP
# ---------------------------------------------------------------------------


class McpSession:
    def __init__(self, port: int, token: str, timeout: float):
        self.url = f"http://127.0.0.1:{port}/mcp"
        self.token = token
        self.timeout = timeout
        self.session_id = None
        self._id = 0

    def _post(self, payload: dict, expect_json: bool = True):
        body = json.dumps(payload).encode()
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            "Authorization": f"Bearer {self.token}",
        }
        if self.session_id:
            headers["mcp-session-id"] = self.session_id
        request = urllib.request.Request(self.url, data=body, headers=headers, method="POST")
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                text = response.read().decode()
                self.session_id = response.headers.get("mcp-session-id", self.session_id)
                status = response.status
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode()[:500]
            raise RuntimeError(f"HTTP {exc.code} from {self.url}: {detail}") from exc
        if not expect_json or not text.strip():
            return status, None
        return status, parse_json_loose(text)

    def initialize(self):
        self._id += 1
        _, response = self._post(
            {
                "jsonrpc": "2.0",
                "id": self._id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "dream-ab", "version": "1.0"},
                },
            }
        )
        if response is None or "error" in response:
            raise RuntimeError(f"initialize failed: {response}")
        return response.get("result")

    def notify(self, method: str, params: dict | None = None):
        payload = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            payload["params"] = params
        self._post(payload, expect_json=False)

    def call_tool(self, name: str, arguments: dict) -> dict:
        self._id += 1
        _, response = self._post(
            {
                "jsonrpc": "2.0",
                "id": self._id,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }
        )
        if response is None:
            raise RuntimeError(f"tools/call {name}: empty response")
        if "error" in response:
            raise RuntimeError(f"tools/call {name} failed: {response['error']}")
        result = response.get("result", {})
        payload = result.get("structuredContent")
        if payload is None:
            texts = [
                block.get("text", "")
                for block in result.get("content", [])
                if isinstance(block, dict) and block.get("type") == "text"
            ]
            joined = "\n".join(texts)
            try:
                payload = parse_json_loose(joined)
            except json.JSONDecodeError:
                payload = {"text": joined}
        return {
            "tool": name,
            "arguments": arguments,
            "isError": bool(result.get("isError")),
            "payload": payload,
        }


class Server:
    """A `vestige serve` process on a temporary HOME, killed on every exit path."""

    _live: list = []

    def __init__(
        self,
        binary: Path,
        home: Path,
        port: int,
        dashboard_port: int,
        cache: Path | None,
        log_path: Path,
    ):
        self.binary = binary
        self.home = home
        self.port = port
        self.dashboard_port = dashboard_port
        self.cache = cache
        self.log_path = log_path
        self.proc: subprocess.Popen | None = None

    def command(self) -> list:
        return [
            str(self.binary),
            "serve",
            "--port",
            str(self.port),
            "--dashboard-port",
            str(self.dashboard_port),
        ]

    def start(self):
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        self._log_handle = open(self.log_path, "w")
        env = store_env(self.home, self.cache)
        env["VESTIGE_HTTP_BIND"] = "127.0.0.1"
        self.proc = subprocess.Popen(
            self.command(),
            env=env,
            stdout=self._log_handle,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        Server._live.append(self)

    def wait_ready(self, timeout: float):
        if not wait_for_port(self.port, timeout):
            self.stop()
            tail = ""
            try:
                tail = self.log_path.read_text()[-2000:]
            except OSError:
                pass
            die(f"server on port {self.port} never became ready.\n{tail}")

    def auth_token(self) -> str:
        path = self.home / STORE_REL / "auth_token"
        for _ in range(40):
            if path.exists():
                token = path.read_text().strip()
                if token:
                    return token
            time.sleep(0.25)
        raise RuntimeError(f"no auth token appeared at {path}")

    def stop(self):
        proc = self.proc
        if proc is None:
            return
        try:
            if proc.poll() is None:
                try:
                    os.killpg(os.getpgid(proc.pid), signal.SIGINT)
                except (ProcessLookupError, PermissionError):
                    proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
                    except (ProcessLookupError, PermissionError):
                        proc.kill()
                    proc.wait(timeout=10)
        finally:
            self.proc = None
            try:
                self._log_handle.close()
            except Exception:
                pass
            if self in Server._live:
                Server._live.remove(self)
            if port_is_open(self.port):
                wait_for_port_closed(self.port, 5)


@atexit.register
def _kill_live_servers():  # pragma: no cover - safety net
    for server in list(Server._live):
        try:
            server.stop()
        except Exception:
            pass


# ---------------------------------------------------------------------------
# the pass
# ---------------------------------------------------------------------------


def run_pass(serve_binary: Path, home: Path, pass_kind: str, cache: Path | None, cfg) -> dict:
    port, dash = free_port(), free_port()
    server = Server(
        binary=serve_binary,
        home=home,
        port=port,
        dashboard_port=dash,
        cache=cache,
        log_path=home.parent / f"serve-{pass_kind}.log",
    )
    calls = []
    server_quality = None
    error = None
    started = utcnow_iso()
    server.start()
    try:
        server.wait_ready(cfg.start_timeout)
        if cfg.warmup:
            step(f"warmup {cfg.warmup:.0f}s before the pass (server port {port})")
            time.sleep(cfg.warmup)
        client = McpSession(port, server.auth_token(), timeout=cfg.request_timeout)
        client.initialize()
        client.notify("notifications/initialized")
        tools = [pass_kind] if pass_kind != "both" else ["dream", "reflect"]
        for tool in tools:
            name, arguments = PASS_TOOLS[tool]
            step(f"pass: tools/call {name} {arguments}")
            t0 = time.monotonic()
            result = client.call_tool(name, arguments)
            calls.append(
                {
                    "tool": name,
                    "arguments": arguments,
                    "isError": result["isError"],
                    "seconds": round(time.monotonic() - t0, 3),
                    "result": result["payload"],
                }
            )
        try:
            health = client.call_tool("memory_health", {})
            server_quality = find_quality_object(health.get("payload"))
        except Exception as exc:  # the store measures do not depend on this
            server_quality = {"unavailable": str(exc)}
    except Exception as exc:  # keep the server from leaking, report what happened
        error = str(exc)
    finally:
        finished = utcnow_iso()
        server.stop()

    return {
        "kind": pass_kind,
        "startedAt": started,
        "finishedAt": finished,
        "serverCommand": f"HOME={home} " + " ".join(server.command()),
        "port": port,
        "calls": calls,
        "serverProcessQuality": server_quality,
        "error": error,
        "logTail": _tail(server.log_path, 4000),
    }


def _tail(path: Path, limit: int) -> str:
    try:
        text = path.read_text(errors="replace")
    except OSError:
        return ""
    return text[-limit:]


# ---------------------------------------------------------------------------
# the wave-5 measures
# ---------------------------------------------------------------------------


def measure_records(report: dict) -> dict:
    """Every measure §12.1 names, as an exact fraction plus its denominator."""
    containment = report.get("containment", {}) or {}
    anchors = report.get("anchors", {}) or {}
    use = report.get("useCounts", {}) or {}
    process = report.get("process", {}) or {}

    clean = containment.get("clean")
    flagged = containment.get("flagged")
    unchecked = containment.get("unchecked")
    c_total = containment.get("total")
    fresh = anchors.get("fresh")
    stale = anchors.get("stale")
    orphaned = anchors.get("orphaned")
    a_unchecked = anchors.get("unchecked")
    a_total = anchors.get("total")
    retrieved = use.get("retrievedAtLeastOnce")
    accessed = use.get("everAccessed")
    u_total = use.get("total")

    def count(num, den):
        return {"numerator": num, "denominator": den, "value": fraction_or_none(num, den)}

    records = {}

    def put(mid, label, good, num, den, is_rate, note=""):
        value = fraction_or_none(num, den)
        records[mid] = {
            "id": mid,
            "label": label,
            "good": good,
            "isRate": is_rate,
            "numerator": num,
            "denominator": den,
            "value": value,
            "float": (float(value) if value is not None else None),
            "note": note,
        }

    checked = None
    if clean is not None and flagged is not None:
        checked = clean + flagged

    put(
        "containment.selfContainmentRate",
        "memory self-containment: clean / (clean + flagged)",
        "+",
        clean,
        checked,
        True,
        "unchecked (gate never ran) is excluded from the denominator and reported separately",
    )
    put("containment.clean", "clean memories, share of window", "+", clean, c_total, False)
    put("containment.flagged", "flagged memories, share of window", "-", flagged, c_total, False)
    put(
        "containment.unchecked",
        "never-checked memories (self_contained IS NULL), share of window",
        "-",
        unchecked,
        c_total,
        False,
        "not a synonym for clean",
    )
    put(
        "anchors.resolvabilityRate",
        "anchor resolvability: fresh / (fresh + stale + orphaned)",
        "+",
        fresh,
        (None if None in (fresh, stale, orphaned) else fresh + stale + orphaned),
        True,
        "unchecked anchors (no repo/revision) sit outside this denominator",
    )
    put("anchors.fresh", "fresh anchors, share of all anchors", "+", fresh, a_total, False)
    put("anchors.stale", "stale anchors, share of all anchors", "-", stale, a_total, False)
    put("anchors.orphaned", "orphaned anchors, share of all anchors", "-", orphaned, a_total, False)
    put(
        "anchors.unchecked",
        "uncheckable anchors, share of all anchors",
        "-",
        a_unchecked,
        a_total,
        False,
        "environment failure (no repository or revision), not code rot",
    )
    put(
        "use.retrievalRate",
        "retrievedAtLeastOnce / useCounts.total  [CONFOUNDED HERE]",
        "+",
        retrieved,
        u_total,
        True,
        "the pass reads the store it is measured on; its own search_hit rows land in this numerator",
    )
    put(
        "use.everAccessedRate",
        "everAccessed / useCounts.total  [CONFOUNDED HERE]",
        "+",
        accessed,
        u_total,
        True,
        "same confound as retrievalRate, plus any access_count the pass itself writes",
    )

    records["_meta"] = {
        "window": {
            "since": report.get("since"),
            "until": report.get("until"),
            "windowBasis": report.get("windowBasis"),
        },
        "flaggedByKind": report.get("flaggedByKind", []),
        "rateCrossCheck": _cross_check_rates(report, records),
        "process": {
            "rejected": process.get("rejected"),
            "flagged": process.get("flagged"),
            "rejectedByKind": process.get("rejectedByKind", []),
            "flaggedByKind": process.get("flaggedByKind", []),
            "comparable": False,
            "note": (
                "process.* counts the process that answered, not the window and not the store "
                "(§12.4). Each arm is read by a different process, so these numbers are not "
                "comparable across arms and carry no verdict."
            ),
        },
        "notes": report.get("notes", []),
    }
    return records


def _cross_check_rates(report: dict, records: dict) -> list:
    """If the surface also ships a rate, it must equal the counts' own rate.

    The harness computes every rate from the counts it was given, so that a
    rate can always be traced to `numerator/denominator`. A surface-supplied
    rate that disagrees is a finding, not something to silently prefer. The
    current payload carries them under `rates.*`; the earlier per-section
    spellings are still honoured so a stored report keeps rendering.
    """
    rates = report.get("rates") or {}

    def first_present(*candidates):
        for candidate in candidates:
            if candidate is not None:
                return candidate
        return None

    supplied = {
        "containment.selfContainmentRate": first_present(
            rates.get("selfContainment"),
            (report.get("containment") or {}).get("selfContainmentRate"),
        ),
        "anchors.resolvabilityRate": first_present(
            rates.get("anchorResolvability"),
            (report.get("anchors") or {}).get("resolvabilityRate"),
        ),
        "use.retrievalRate": first_present(
            rates.get("retrieval"),
            (report.get("useCounts") or {}).get("retrievalRate"),
        ),
    }
    findings = []
    for mid, value in supplied.items():
        if value is None:
            continue
        mine = records[mid]["float"]
        if mine is None or abs(float(value) - mine) > 1e-9:
            findings.append({"measure": mid, "surface": value, "fromCounts": mine})
    return findings


MEASURE_ORDER = [
    "containment.selfContainmentRate",
    "containment.clean",
    "containment.flagged",
    "containment.unchecked",
    "anchors.resolvabilityRate",
    "anchors.fresh",
    "anchors.stale",
    "anchors.orphaned",
    "anchors.unchecked",
    "use.retrievalRate",
    "use.everAccessedRate",
]


def effect(before_rec: dict, after_rec: dict, control_before: dict, control_after: dict):
    """(treatment after − treatment before) − (control after − control before).

    Exact rational arithmetic: these are counts and ratios of counts, so a float
    comparison would manufacture differences that are not in the data.
    """
    t0, t1 = as_fraction(before_rec["value"]), as_fraction(after_rec["value"])
    c0, c1 = as_fraction(control_before["value"]), as_fraction(control_after["value"])
    if None in (t0, t1, c0, c1):
        return None
    return (t1 - t0) - (c1 - c0)


def verdict_of(effects: list, good: str) -> str:
    """§12.2 rule 4: the same direction in all three runs, or `unresolved`."""
    if any(effect is None for effect in effects):
        return "unmeasured"
    signs = {0 if effect == 0 else (1 if effect > 0 else -1) for effect in effects}
    if signs == {0}:
        return "no effect"
    if len(signs) != 1:
        return "unresolved"
    sign = signs.pop()
    if good == "+":
        return "improved" if sign > 0 else "worse"
    return "worse" if sign > 0 else "improved"


def serialise_fraction(value) -> dict | None:
    if value is None:
        return None
    return {"num": value.numerator, "den": value.denominator, "float": float(value)}


# ---------------------------------------------------------------------------
# isolation
# ---------------------------------------------------------------------------


def isolation_probe(surface, snapshot: Path, work: Path, live_counts: dict) -> dict:
    """Prove `HOME` selects the copy before any arm is measured.

    Two probes, in this order, because the first one cannot pollute anything:

    1. **Delete probe.** A throw-away copy has one node removed; the quality
       report read with `HOME` pointing at it must show one node fewer than the
       snapshot. If it still shows the snapshot's count, the report is not
       coming from the copy and the harness stops — before it has written
       anything anywhere.
    2. **Canary probe.** With relocation proven, a canary memory is ingested
       through the CLI into the throw-away copy; the report must count exactly
       one more row. This is the write-path half of the same claim.
    """
    root = work / "isolation"
    report = {"checks": {}, "ok": False}

    pristine_home = prepare_home(root, "pristine", snapshot)
    baseline = surface.read(pristine_home, None)
    baseline_total = baseline["report"]["containment"]["total"]
    snapshot_rows = store_row_counts(snapshot)
    report["pristineTotal"] = baseline_total
    report["snapshotRows"] = snapshot_rows
    report["checks"]["reportTotalMatchesSnapshotRows"] = (
        snapshot_rows.get("knowledge_nodes") == baseline_total
    )

    perturbed_home = prepare_home(root, "perturbed", snapshot)
    perturbed_db = home_store_path(perturbed_home)
    with sqlite3.connect(str(perturbed_db)) as con:
        victim = con.execute("SELECT id FROM knowledge_nodes LIMIT 1").fetchone()
        if victim is None:
            die("the snapshot has no rows to perturb — cannot verify isolation")
        con.execute("DELETE FROM knowledge_nodes WHERE id = ?", (victim[0],))
        con.commit()
    perturbed = surface.read(perturbed_home, None)
    perturbed_total = perturbed["report"]["containment"]["total"]
    report["perturbedTotal"] = perturbed_total
    report["checks"]["homeSelectsTheCopy"] = (
        snapshot_rows.get("knowledge_nodes") is not None
        and perturbed_total == snapshot_rows["knowledge_nodes"] - 1
    )

    canary_home = prepare_home(root, "canary", snapshot)
    canary_text = f"dream-ab isolation canary {utcnow_iso()} — a harness-owned row, never a measurement"
    ingest = subprocess.run(
        [str(surface.binary), "ingest", canary_text, "--tags", "dream-ab-canary"],
        env=store_env(canary_home, surface.cache),
        capture_output=True,
        text=True,
        timeout=surface.timeout,
    )
    report["canaryIngestExit"] = ingest.returncode
    report["canaryIngestStderr"] = ingest.stderr.strip()[-500:]
    after = surface.read(canary_home, None)
    canary_total = after["report"]["containment"]["total"]
    report["canaryTotal"] = canary_total
    report["checks"]["canaryWriteLandedInTheCopy"] = canary_total == baseline_total + 1

    report["liveStoreBefore"] = live_counts
    report["ok"] = all(report["checks"].values())
    if not report["ok"]:
        die(
            "isolation check failed — refusing to measure. A report was produced by a store that "
            f"is not the harness's copy: {json.dumps(report['checks'])}",
            EXIT_ISOLATION,
        )
    return report


# ---------------------------------------------------------------------------
# supplementary evidence (not a measure)
# ---------------------------------------------------------------------------


def parse_timestamp(text: str):
    """Timestamps in this store are RFC 3339 with a variable number of fraction digits."""
    if text is None:
        return None
    candidate = str(text).strip().replace(" ", "T")
    if candidate.endswith("Z"):
        candidate = candidate[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError:
        return None
    return parsed if parsed.tzinfo else parsed.replace(tzinfo=timezone.utc)


def access_log_activity(db_path: Path, since_iso: str, until_iso: str) -> dict:
    """How many retrievals the pass itself performed, read straight from the log.

    Supplementary and clearly labelled: this is the harness looking at the
    store, not `memory_quality` answering. It exists to attach a number to the
    `useCounts` confound instead of only naming it.
    """
    result = {"queried": False, "window": {"since": since_iso, "until": until_iso}}
    start, end = parse_timestamp(since_iso), parse_timestamp(until_iso)
    if start is None or end is None:
        result["error"] = "unparseable pass window"
        return result
    try:
        with sqlite_ro(db_path) as con:
            rows = con.execute(
                "SELECT access_type, node_id, accessed_at FROM memory_access_log"
            ).fetchall()
        in_window = []
        for access_type, node_id, accessed_at in rows:
            stamp = parse_timestamp(accessed_at)
            if stamp is not None and start <= stamp <= end:
                in_window.append((access_type, node_id))
        tally = {}
        for access_type, node_id in in_window:
            bucket = tally.setdefault(access_type, {"rows": 0, "nodes": set()})
            bucket["rows"] += 1
            bucket["nodes"].add(node_id)
        result["queried"] = True
        result["byType"] = [
            {"accessType": kind, "rows": data["rows"], "distinctNodes": len(data["nodes"])}
            for kind, data in sorted(tally.items())
        ]
        result["totalRows"] = len(in_window)
    except sqlite3.Error as exc:
        result["error"] = str(exc)
    return result


# ---------------------------------------------------------------------------
# determinism
# ---------------------------------------------------------------------------

VOLATILE_REPORT_KEYS = ("until", "process")


def comparable_report(report: dict) -> dict:
    """The store measures only: `until` is a clock, `process` is a process."""
    return {
        key: value
        for key, value in report.items()
        if key not in VOLATILE_REPORT_KEYS
    }


def determinism_check(runs: list) -> dict:
    pairs = []
    ok = True
    for i in range(len(runs)):
        for j in range(i + 1, len(runs)):
            left = comparable_report(runs[i]["control"]["before"]["report"])
            right = comparable_report(runs[j]["control"]["before"]["report"])
            same = left == right
            ok = ok and same
            entry = {
                "leftRun": runs[i]["index"],
                "rightRun": runs[j]["index"],
                "identical": same,
            }
            if not same:
                entry["differences"] = _diff_reports(left, right)
            pairs.append(entry)
    within = []
    for run in runs:
        left = comparable_report(run["control"]["before"]["report"])
        right = comparable_report(run["control"]["after"]["report"])
        within.append({"run": run["index"], "identical": left == right})
    return {
        "comparedKeys": "everything in MemoryQualityReport except `until` and `process`",
        "excluded": {
            "until": "the wall clock at read time; it cannot be equal across runs",
            "process": "process-lifetime gate counters; every read is its own process",
        },
        "acrossRuns": pairs,
        "withinRunBeforeAfter": within,
        "ok": ok and all(entry["identical"] for entry in within),
        "conclusion": (
            "two control runs on two identical copies produced identical store measures"
            if ok and all(entry["identical"] for entry in within)
            else "NONDETERMINISM: identical copies did not produce identical reports — find the "
            "other source of variation before reading any delta as an effect"
        ),
    }


def _diff_reports(left, right, path="") -> list:
    differences = []
    if isinstance(left, dict) and isinstance(right, dict):
        for key in sorted(set(left) | set(right)):
            differences += _diff_reports(left.get(key), right.get(key), f"{path}.{key}")
    elif left != right:
        differences.append({"path": path.lstrip("."), "left": left, "right": right})
    return differences


# ---------------------------------------------------------------------------
# rendering
# ---------------------------------------------------------------------------


def fmt_value(record: dict) -> str:
    if record is None:
        return "—"
    value = as_fraction(record.get("value"))
    numerator, denominator = record.get("numerator"), record.get("denominator")
    if value is None or denominator in (None, 0):
        return "unmeasured (0 in the denominator)"
    if record.get("isRate"):
        return f"{float(value):.3f} ({numerator}/{denominator})"
    return f"{numerator}/{denominator}"


def render_markdown(document: dict, source: str) -> str:
    lines = []
    lines.append(f"<!-- generated by: python3 benchmarks/dream-ab/run.py --render-table {source} -->")
    lines.append("")
    lines.append(f"### Per-run values — pass: `{document['pass']}`")
    lines.append("")
    lines.append(
        "| measure | run | control before | control after | treatment before | treatment after | Δ(treatment) − Δ(control) |"
    )
    lines.append("| --- | --- | --- | --- | --- | --- | --- |")
    for run in document["runs"]:
        control_measures = run["control"]["before"]["measures"]
        control_after = run["control"]["after"]["measures"]
        treatment_measures = run["treatment"]["before"]["measures"]
        treatment_after = run["treatment"]["after"]["measures"]
        for mid in MEASURE_ORDER:
            control_delta = _delta_text(control_measures[mid], control_after[mid])
            treatment_delta = _delta_text(treatment_measures[mid], treatment_after[mid])
            lines.append(
                "| `{mid}` | {run} | {cb} | {ca} | {tb} | {ta} | treatment {td}, control {cd} |".format(
                    mid=mid,
                    run=run["index"],
                    cb=fmt_value(control_measures[mid]),
                    ca=fmt_value(control_after[mid]),
                    tb=fmt_value(treatment_measures[mid]),
                    ta=fmt_value(treatment_after[mid]),
                    td=treatment_delta,
                    cd=control_delta,
                )
            )
    lines.append("")
    lines.append("### Verdict per measure (§12.2 rule 4: three runs, same direction)")
    lines.append("")
    lines.append("| measure | counts as improvement | Δ run 1 | Δ run 2 | Δ run 3 | verdict |")
    lines.append("| --- | --- | --- | --- | --- | --- |")
    for mid in MEASURE_ORDER:
        entry = document["measures"][mid]
        cells = []
        for effect in entry["effects"]:
            cells.append("unmeasured" if effect is None else f"{effect['float']:+.4f}")
        verdict = f"**{entry['verdict']}**"
        if entry.get("confoundedInThisDesign"):
            verdict += " ⚠ confounded (see notes)"
        lines.append(
            "| `{}` | {} | {} | {} | {} | {} |".format(
                mid,
                "up" if entry["good"] == "+" else "down",
                cells[0] if cells else "—",
                cells[1] if len(cells) > 1 else "—",
                cells[2] if len(cells) > 2 else "—",
                verdict,
            )
        )
    lines.append("")
    if document.get("passFailures"):
        lines.append("### Pass failures (no verdict may be drawn from these runs)")
        lines.append("")
        for failure in document["passFailures"]:
            lines.append(f"- run {failure['run']}: {failure['error']}")
        lines.append("")
    determinism = document["determinism"]
    lines.append("### Determinism control")
    lines.append("")
    lines.append(f"- {determinism['conclusion']}")
    for entry in determinism["acrossRuns"]:
        lines.append(
            f"- control run {entry['leftRun']} vs control run {entry['rightRun']}: "
            f"{'identical' if entry['identical'] else 'DIFFERENT'}"
        )
    lines.append("")
    lines.append("### Run facts (denominators and the pass's own activity)")
    lines.append("")
    lines.append(
        "| run | window total: control | window total: treatment | pass call | call seconds | "
        "access-log rows written during the pass (supplementary read) |"
    )
    lines.append("| --- | --- | --- | --- | --- | --- |")
    for run in document["runs"]:
        control = run["denominatorInflation"]["control"]
        treatment = run["denominatorInflation"]["treatment"]
        calls = run["pass"]["calls"]
        call_text = ", ".join(f"`{c['tool']}` {'error' if c['isError'] else 'ok'}" for c in calls)
        seconds = ", ".join(f"{c['seconds']:.1f}" for c in calls)
        by_type = run["supplementary"]["accessLogDuringPass"].get("byType")
        if by_type is None:
            supplementary = "not readable"
        elif not by_type:
            supplementary = "0 rows"
        else:
            supplementary = ", ".join(
                f"{entry['accessType']}: {entry['rows']} rows on {entry['distinctNodes']} nodes"
                for entry in by_type
            )
        lines.append(
            f"| {run['index']} | {_fmt_total(control)} | {_fmt_total(treatment)} | {call_text} | "
            f"{seconds} | {supplementary} |"
        )
    lines.append("")
    return "\n".join(lines)


def _fmt_total(delta) -> str:
    if delta is None:
        return "unmeasured"
    if delta == 0:
        return "no new rows"
    return f"{delta:+d} rows"


def _delta_text(before: dict, after: dict) -> str:
    left, right = as_fraction(before.get("value")), as_fraction(after.get("value"))
    if left is None or right is None:
        return "unmeasured"
    delta = right - left
    if delta == 0:
        return "0"
    return f"{float(delta):+.4f}"


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------


def parse_args(argv=None):
    parser = argparse.ArgumentParser(
        description="Honest A/B of `dream` / `reflect` against the wave-5 quality measures "
        "(docs/SELF-CONTAINED-MEMORY-DESIGN.md §12).",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--source-store",
        type=Path,
        default=Path(os.path.expanduser("~")) / STORE_REL / STORE_NAME,
        help="store to snapshot. Opened read-only, never written; the arms run on copies.",
    )
    parser.add_argument(
        "--pass",
        dest="pass_kind",
        choices=["dream", "reflect", "both"],
        default="dream",
        help="which consolidation pass the treatment arm runs",
    )
    parser.add_argument("--runs", type=int, default=3, help="control/treatment pairs to run")
    parser.add_argument(
        "--json-out",
        type=Path,
        default=REPO_ROOT / "benchmarks" / "dream-ab" / "results",
        help="directory for the run JSON (and the generated markdown table)",
    )
    parser.add_argument(
        "--cli",
        type=Path,
        default=REPO_ROOT / "target" / "release" / "vestige",
        help="path to the `vestige` binary (CLI quality + `serve`)",
    )
    parser.add_argument(
        "--mcp-binary",
        type=Path,
        default=REPO_ROOT / "target" / "release" / "vestige-mcp",
        help="path to the `vestige-mcp` binary (recorded; used for the stdio surface if ever needed)",
    )
    parser.add_argument(
        "--quality-source",
        choices=["auto", "cli", "mcp"],
        default="auto",
        help="how to read the quality report; auto prefers the CLI §12.4 names for harnesses",
    )
    parser.add_argument("--since", default=None, help="window start passed through to the surface")
    parser.add_argument("--warmup", type=float, default=15.0, help="seconds to wait after server start")
    parser.add_argument("--start-timeout", type=float, default=180.0, help="server readiness timeout")
    parser.add_argument("--request-timeout", type=float, default=900.0, help="MCP request timeout")
    parser.add_argument(
        "--fastembed-cache",
        type=Path,
        default=Path(os.path.expanduser("~")) / CACHE_REL,
        help="model cache to expose to the relocated HOME (no re-download)",
    )
    parser.add_argument("--work-root", type=Path, default=Path("/tmp"), help="parent for temp work dirs")
    parser.add_argument("--keep-work", action="store_true", help="keep the temp working directory")
    parser.add_argument(
        "--label", default=None, help="label for the run artifacts (default: UTC timestamp)"
    )
    parser.add_argument(
        "--render-table",
        nargs="+",
        type=Path,
        default=None,
        help="do not run anything: re-render the markdown table from stored run JSON",
    )
    parser.add_argument(
        "--markdown-table",
        type=Path,
        default=None,
        help="where --render-table writes (default: stdout)",
    )
    return parser.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)

    if args.render_table:
        chunks = []
        for path in args.render_table:
            document = json.loads(path.read_text())
            chunks.append(render_markdown(document, str(path)))
        text = "\n".join(chunks)
        if args.markdown_table:
            args.markdown_table.write_text(text)
            log(f"wrote {args.markdown_table}")
        else:
            log(text)
        return EXIT_OK

    if not args.cli.exists():
        die(
            f"{args.cli} not found — build it first: cargo build --release -p vestige-mcp -p vestige-restore"
        )
    source = args.source_store.expanduser()
    if not source.exists():
        die(f"source store {source} not found")
    if args.runs < 1:
        die("--runs must be >= 1")

    stamp = args.label or utcnow().strftime("%Y%m%dT%H%M%SZ")
    work = Path(tempfile.mkdtemp(prefix=f"dream-ab-{stamp}-", dir=str(args.work_root)))
    keep = args.keep_work
    step(f"work dir: {work}{' (kept)' if keep else ''}")
    if not keep:
        atexit.register(lambda: shutil.rmtree(work, ignore_errors=True))

    started = utcnow_iso()
    live_counts_before = store_row_counts(source)
    live_mtime_before = source.stat().st_mtime
    step(f"snapshotting {source} (read-only)")
    snapshot = snapshot_store(source, work / "snapshot" / STORE_NAME)
    step(
        f"snapshot: {snapshot['rows'].get('knowledge_nodes')} knowledge_nodes, "
        f"sha256 {snapshot['sha256'][:12]}…, method: {snapshot['method']}"
    )
    live_mtime_after = source.stat().st_mtime

    cache = args.fastembed_cache if args.fastembed_cache.is_dir() else None
    if cache is None:
        step(f"warning: fastembed cache {args.fastembed_cache} not found; server may re-download models")

    surface = None
    if args.quality_source == "cli":
        surface = CliQuality(args.cli, cache, args.start_timeout)
    elif args.quality_source == "mcp":
        surface = McpQuality(args.cli, cache, args.start_timeout, args.warmup)
    else:
        surface = detect_surface(args.cli, cache, args.start_timeout, args.warmup)
    step(f"quality surface: {surface.name}")

    isolation = isolation_probe(surface, Path(snapshot["snapshotPath"]), work, live_counts_before)
    step(f"isolation checks passed: {json.dumps(isolation['checks'])}")

    runs = []
    for index in range(1, args.runs + 1):
        step(f"--- run {index}/{args.runs} ---")
        control_home = prepare_home(work / f"run{index}", "control", Path(snapshot["snapshotPath"]))
        treatment_home = prepare_home(
            work / f"run{index}", "treatment", Path(snapshot["snapshotPath"])
        )

        control_before = _with_measures(surface.read(control_home, args.since))
        treatment_before = _with_measures(surface.read(treatment_home, args.since))

        pass_result = run_pass(args.cli, treatment_home, args.pass_kind, cache, args)
        if pass_result["error"]:
            # A pass that died mid-flight is a finding, not a reason to throw
            # the run away: the after-state is still read and the raw delta is
            # still recorded, but no verdict may be drawn from it.
            step(f"WARNING: the pass failed in run {index}: {pass_result['error']}")
            step("the after-state is still measured, and every verdict is forced to `unresolved`")

        treatment_after = _with_measures(surface.read(treatment_home, args.since))
        control_after = _with_measures(surface.read(control_home, args.since))

        supplementary = access_log_activity(
            home_store_path(treatment_home),
            pass_result["startedAt"],
            pass_result["finishedAt"],
        )

        runs.append(
            {
                "index": index,
                "control": {"before": control_before, "after": control_after},
                "treatment": {"before": treatment_before, "after": treatment_after},
                "pass": pass_result,
                "supplementary": {"accessLogDuringPass": supplementary},
                "denominatorInflation": {
                    "treatment": _total_delta(treatment_before, treatment_after),
                    "control": _total_delta(control_before, control_after),
                },
            }
        )
        step(
            f"run {index}: window total treatment {_total(treatment_before)} -> "
            f"{_total(treatment_after)}; control {_total(control_before)} -> {_total(control_after)}"
        )

    measures = {}
    pass_failures = [
        {"run": run["index"], "error": run["pass"]["error"]}
        for run in runs
        if run["pass"]["error"]
    ]
    for mid in MEASURE_ORDER:
        effects = [
            effect(
                run["treatment"]["before"]["measures"][mid],
                run["treatment"]["after"]["measures"][mid],
                run["control"]["before"]["measures"][mid],
                run["control"]["after"]["measures"][mid],
            )
            for run in runs
        ]
        spec = runs[0]["control"]["before"]["measures"][mid]
        computed = verdict_of(effects, spec["good"])
        measures[mid] = {
            "label": spec["label"],
            "good": spec["good"],
            "isRate": spec["isRate"],
            "effects": [serialise_fraction(item) for item in effects],
            "verdict": "unresolved" if pass_failures else computed,
            "verdictFromDeltas": computed,
            "passFailures": pass_failures,
            "confoundedInThisDesign": mid.startswith("use."),
            "note": spec["note"],
        }

    determinism = determinism_check(runs)
    finished = utcnow_iso()
    live_counts_after = store_row_counts(source)
    live_mtime_final = source.stat().st_mtime

    document = {
        "harness": "benchmarks/dream-ab/run.py",
        "spec": "docs/SELF-CONTAINED-MEMORY-DESIGN.md §12 (12.2 rules, 12.4 contract)",
        "stamp": stamp,
        "startedAt": started,
        "finishedAt": finished,
        "pass": args.pass_kind,
        "runs": args.runs,
        "qualitySurface": surface.name,
        "since": args.since,
        "config": {
            "warmupSeconds": args.warmup,
            "requestTimeoutSeconds": args.request_timeout,
            "keepWork": keep,
            "workDir": str(work),
            "fastembedCache": str(cache) if cache else None,
            "cliBinary": str(args.cli),
            "mcpBinary": str(args.mcp_binary),
            "binarySha256": sha256_file(args.cli),
            "binaryMtime": datetime.fromtimestamp(args.cli.stat().st_mtime, timezone.utc).isoformat(),
        },
        "sourceStore": {
            **snapshot,
            "liveMtimeBefore": live_mtime_before,
            "liveMtimeAfterSnapshot": live_mtime_after,
            "liveRowsBefore": live_counts_before,
            "liveRowsAfter": live_counts_after,
            "liveMtimeFinal": live_mtime_final,
        },
        "isolation": isolation,
        "passFailures": pass_failures,
        "measures": measures,
        "determinism": determinism,
        "caveats": [
            "Every rate is printed with its denominator; a rate over 0 rows is `unmeasured`, not 0.",
            "`unchecked` (gate never ran) is never counted as clean, and uncheckable anchors are "
            "never folded into resolvability.",
            "`useCounts` are confounded in this design: the treatment pass retrieves from the store "
            "it is measured on, so its own search_hit rows and access_count bumps are inside the "
            "treatment delta. The control arm performs no reads at all. A positive use delta is not "
            "evidence of value.",
            "The treatment arm is `the pass run through the server`, per §12.4, so any inline "
            "consolidation the server performs while serving the call is inside the treatment delta.",
            "`process.*` gate counters describe the process that answered the read, not the store; "
            "they are not comparable across arms and carry no verdict.",
            "The pass's own new rows (insights, connections) change the window total; every rate "
            "that moves is reported next to the denominator it moved against.",
        ],
    }
    document["runs"] = [
        {
            "index": run["index"],
            "control": run["control"],
            "treatment": run["treatment"],
            "pass": {
                "kind": run["pass"]["kind"],
                "startedAt": run["pass"]["startedAt"],
                "finishedAt": run["pass"]["finishedAt"],
                "serverCommand": run["pass"]["serverCommand"],
                "calls": [
                    {
                        "tool": call["tool"],
                        "arguments": call["arguments"],
                        "isError": call["isError"],
                        "seconds": call["seconds"],
                    }
                    for call in run["pass"]["calls"]
                ],
            },
            "serverProcessQuality": run["pass"]["serverProcessQuality"],
            "supplementary": run["supplementary"],
            "denominatorInflation": run["denominatorInflation"],
        }
        for run in runs
    ]
    # The full tool payloads are large; keep them in a sidecar so the main JSON
    # stays readable while every raw answer is still on disk.
    payloads = {
        "stamp": stamp,
        "pass": args.pass_kind,
        "calls": [call for run in runs for call in run["pass"]["calls"]],
    }

    args.json_out.mkdir(parents=True, exist_ok=True)
    json_path = args.json_out / f"{stamp}-{args.pass_kind}.json"
    payload_path = args.json_out / f"{stamp}-{args.pass_kind}-payloads.json"
    table_path = args.json_out / f"{stamp}-{args.pass_kind}-table.md"
    json_path.write_text(json.dumps(document, indent=2, default=json_default))
    payload_path.write_text(json.dumps(payloads, indent=2, default=json_default))
    table_path.write_text(render_markdown(document, str(json_path)))

    step("")
    log(render_markdown(document, str(json_path)))
    step("")
    log(f"json:    {json_path}")
    log(f"payloads:{payload_path}")
    log(f"table:   {table_path}")
    log("")
    log("verdicts:")
    for mid in MEASURE_ORDER:
        log(f"  {mid:<36} {measures[mid]['verdict']}")
    log("")
    log(f"determinism: {determinism['conclusion']}")
    if not keep:
        log(f"work dir {work} removed")
    return EXIT_OK


def _with_measures(record: dict) -> dict:
    return {**record, "measures": measure_records(record["report"])}


def _total(record: dict):
    """The window total, which is every count measure's denominator."""
    return record["measures"]["containment.clean"]["denominator"]


def _total_delta(before: dict, after: dict):
    left, right = _total(before), _total(after)
    if left is None or right is None:
        return None
    return right - left


if __name__ == "__main__":
    raise SystemExit(main())

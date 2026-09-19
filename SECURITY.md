# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 3.4.x   | :white_check_mark: |
| 3.3.x   | :white_check_mark: (security fixes only) |
| < 3.3   | :x:                |

The workspace version lives in the root `[workspace.package]` block in `Cargo.toml`. Every crate inherits it (`version.workspace = true`) so there is exactly one number to bump.

## Reporting a Vulnerability

If you discover a security vulnerability in Vestige, please report it responsibly:

1. **DO NOT** open a public GitHub issue
2. Email the maintainer directly (see GitHub profile)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

You can expect a response within 48 hours.

## Security Model

### Trust Boundaries

Vestige is a **local MCP server** designed to run on your machine with your user permissions:

- **Trusted**: The MCP client (Claude Code, Cursor, Codex, Gemini CLI, …) that connects via stdio or loopback HTTP.
- **Untrusted**: Content passed through MCP tool arguments. Every tool validates its input before storage; FTS5 queries are sanitised; SQL is parameterised.

### What Vestige Does NOT Do

- ❌ Send your memories anywhere. The only outbound requests are first-use model downloads from Hugging Face performed by `fastembed`/`hf-hub` (Nomic Embed v1.5 for `embeddings`, Jina Reranker v2 for the cross-encoder); they land in the local model cache (`FASTEMBED_CACHE_PATH` or the platform cache dir), and `VESTIGE_TEST_MOCK_EMBEDDINGS=1` removes even that. After the download the binary is offline. The `telemetry` cargo feature would add an OTLP exporter, but it ships no exporter today — `telemetry::init` reports `FeatureDisabled` without the feature and `Disabled` with it — and it is off by default.
- ❌ Execute shell commands. There is no `std::process::Command` anywhere in the workspace; the single process spawn is `open::that` for the dashboard's `--open` flag, which hands a loopback URL to your platform opener and is never built from tool input.
- ❌ Send telemetry or analytics.
- ❌ Phone home to any server.

### File Access

The server touches exactly three places on disk:

1. **Its data directory** (`--data-dir`, else the `directories` location below): `vestige.db` + `-wal`/`-shm`, the `vestige.hnsw` sidecar and meta JSON, `auth_token`, `exports/` and `backups/`. Created `0700` / written `0600` on Unix (`create_private_file` and `restrict_to_owner` in `tools/maintenance/mod.rs`); `export`'s optional `path` argument is a bare filename written inside `<data-dir>/exports` — separators and `..` are rejected.
2. **The model cache** (see above).
3. **One caller-supplied file**: `restore`'s `path` argument opens the backup JSON or SQLite snapshot the operator names — that is the point of a restore, and it is the *only* tool argument that reaches an arbitrary filesystem path. The CLI's `restore --input` behaves the same way. No path is enumerated, globbed or discovered; the server reads that one file and nothing else.

Library-only code (`codebase::GitAnalyzer`, `codebase::watcher`) can open a git repository and watch a directory, but no MCP tool or dashboard route constructs either, so that surface is not reachable from a connected client today.

### Data Storage

All data is stored locally in SQLite. Default locations come from the [`directories`](https://docs.rs/directories) crate (`ProjectDirs::from("com", "vestige", "core")`):

| Platform | Location |
|----------|----------|
| macOS    | `~/Library/Application Support/com.vestige.core/vestige.db` |
| Linux    | `~/.local/share/core/vestige.db` (or `$XDG_DATA_HOME/core/`) |
| Windows  | `%APPDATA%\vestige\core\data\vestige.db` |

**Default**: data is stored in plaintext with owner-only permissions — the data directory is created `0700`, and the database file, exports, backups and auth token are `0600` on Unix. Override the directory with `--data-dir <PATH>`.

A process that is *asked* for encryption never falls back to plaintext silently: see below.

### Encryption at Rest

Database-level encryption is SQLCipher (the `encryption` cargo feature, mutually exclusive with `bundled-sqlite`):

```bash
cargo build --release -p vestige-mcp \
  --no-default-features \
  --features embeddings,vector-search,preprocessing,encryption
```

The policy is resolved once per open (`Storage::encryption_config` in `crates/vestige-core/src/storage/sqlite/init.rs`) and **fails closed**:

| Environment | Behaviour |
|---|---|
| `VESTIGE_ENCRYPTION_KEY` set to a non-empty value | The key is applied as the first statement on every connection. On a build *without* the `encryption` feature the process refuses to start rather than open a plaintext database. A whitespace-only value counts as unset. |
| `VESTIGE_REQUIRE_ENCRYPTION` truthy (`1`/`true`/`yes`/`on`) with no key | Refuses to start. This is how a unit file states "encrypted or nothing" without embedding key material. |
| Neither set | Plaintext, with a one-time warning per process naming both variables. This is the default. |
| A key is supplied but the file cannot be decrypted | Refuses to start and says the passphrase is wrong or the file is not a SQLCipher database — it never treats `SQLITE_NOTADB` as "corrupt, carry on", because that would create a fresh plaintext database beside the encrypted one. |

SQLCipher encrypts every database file including the WAL journal. As a simpler alternative, use OS-level encryption (FileVault, BitLocker, LUKS).

### HTTP Transport Hardening

The secondary HTTP MCP transport (port `3928`, configurable via `--http-port` or `VESTIGE_HTTP_PORT`) is opt-in for clients that cannot speak stdio. When enabled:

- Bound to `127.0.0.1` by default. Override with `VESTIGE_HTTP_BIND`.
- Every request requires a bearer token. Either set `VESTIGE_AUTH_TOKEN` yourself, or let the binary generate one on first start (printed to stderr — never logged anywhere else). Comparison uses [`subtle`](https://docs.rs/subtle) constant-time bytes to defeat timing side channels.
- A fresh `Mcp-Session-Id` is required after the initial `tools/list` handshake; sessions can be torn down with `DELETE /mcp`.

If you do not need the HTTP transport, leave the flag off — stdio is the primary path.

### Input Validation

All MCP tool inputs are validated before they reach storage:

- Content size limit: 1 MB max per memory (`smart_ingest` rejects longer content).
- Query length limit: there is **no configurable cap on the query string itself**. `VESTIGE_MAX_TOKEN_BUDGET` (default 100,000) clamps the token budget of the *response*, not the request. The real query-side bound is in `vestige_core::fts`: queries are truncated to 1,000 characters and to at most 32 terms before they reach FTS5.
- FTS5 handling: queries pass through `vestige_core::fts`, which drops boolean operators (`OR`/`AND`/`NOT`/`NEAR`), neutralises column targeting and unbalanced quotes, and enforces the character/term limits above.
- All SQL uses parameterised queries (`params![]` macro) — no string interpolation.
- `gc` defaults to `dry_run=true`; `dry_run=false` is rejected unless the call also passes `confirmed: true`. `restore` has **no dry-run mode** and requires `confirmed: true` on every call (`tools/restore.rs`), and `erase` follows the same pattern (`dry_run` defaults to `true`; the destructive pass needs `confirmed: true`).
- `restore`'s `path` is read as given (see [File Access](#file-access)); it is an operator-supplied path, not a sandboxed one. Treat it as "run this only with backups you trust".

### Dependencies

We use `cargo audit`, `cargo deny`, and Dependabot. Current status (3.4.0 release):

- **Vulnerabilities** (`cargo audit`): 0 known.
- **Unmaintained warnings**: documented in `deny.toml` with upstream tracking links and recheck dates. We do not ignore findings silently — every suppression is an audit trail.
- **Supply-chain CI**: `cargo audit` + `cargo deny check` run on every PR; transitive `rustls-webpki` advisories from 2026-Q1 were patched in 3.3.0.

## Security Checklist

- [x] No hardcoded secrets.
- [x] Parameterised SQL queries everywhere.
- [x] Constant-time bearer-token comparison on the HTTP transport.
- [x] Input validation on every tool.
- [x] No command injection vectors (no `Command::new`, no shell expansion).
- [x] No `unsafe` Rust in our own production code (the only `unsafe` is in `#[cfg(test)]` test helpers for `env::set_var`, which is `unsafe` on the Rust 2024 edition; transitive `unsafe` documented).
- [x] Dependencies audited automatically in CI.
- [x] SQLite WAL checkpoint on graceful shutdown so a `SIGINT` does not leave a dirty journal.

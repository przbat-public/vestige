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

- ❌ Make network requests, except the first-run model download from Hugging Face (Nomic Embed v1.5 + Jina Reranker v2). After that the binary is fully offline.
- ❌ Execute shell commands.
- ❌ Access files outside its `--data-dir` and the embedding model cache.
- ❌ Send telemetry or analytics.
- ❌ Phone home to any server.

### Data Storage

All data is stored locally in SQLite. Default locations come from the [`directories`](https://docs.rs/directories) crate (`ProjectDirs::from("com", "vestige", "core")`):

| Platform | Location |
|----------|----------|
| macOS    | `~/Library/Application Support/com.vestige.core/vestige.db` |
| Linux    | `~/.local/share/core/vestige.db` (or `$XDG_DATA_HOME/core/`) |
| Windows  | `%APPDATA%\vestige\core\data\vestige.db` |

**Default**: data is stored in plaintext with owner-only file permissions (`0600`). Override the directory with `--data-dir <PATH>`.

### Encryption at Rest

For database-level encryption, build with SQLCipher (mutually exclusive with `bundled-sqlite`):

```bash
cargo build --release -p vestige-mcp \
  --no-default-features \
  --features embeddings,vector-search,preprocessing,encryption
```

Set the `VESTIGE_ENCRYPTION_KEY` environment variable before starting the server. SQLCipher encrypts every database file including the WAL journal. As a simpler alternative, use OS-level encryption (FileVault, BitLocker, LUKS).

### HTTP Transport Hardening

The secondary HTTP MCP transport (port `3928`, configurable via `--http-port` or `VESTIGE_HTTP_PORT`) is opt-in for clients that cannot speak stdio. When enabled:

- Bound to `127.0.0.1` by default. Override with `VESTIGE_HTTP_BIND`.
- Every request requires a bearer token. Either set `VESTIGE_AUTH_TOKEN` yourself, or let the binary generate one on first start (printed to stderr — never logged anywhere else). Comparison uses [`subtle`](https://docs.rs/subtle) constant-time bytes to defeat timing side channels.
- A fresh `Mcp-Session-Id` is required after the initial `tools/list` handshake; sessions can be torn down with `DELETE /mcp`.

If you do not need the HTTP transport, leave the flag off — stdio is the primary path.

### Input Validation

All MCP tool inputs are validated before they reach storage:

- Content size limit: 1 MB max per memory.
- Query length limit: configurable via `VESTIGE_MAX_TOKEN_BUDGET`; tools default to safe ceilings.
- FTS5 queries pass through `vestige_core::fts` which strips injection patterns and enforces length limits.
- All SQL uses parameterised queries (`params![]` macro) — no string interpolation.
- The `restore` and `gc` tools default to `dry_run=true`. Destructive use requires explicit opt-in.

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

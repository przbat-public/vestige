# Configuration Reference

> Environment variables, CLI commands, and setup options.

---

## First-Run Network Requirement

Vestige downloads two models on first use:

- **Nomic Embed Text v1.5** (~130 MB) — embedding model
- **Jina Reranker v2 Base Multilingual** (~278 M params, ~600 MB) — cross-encoder reranker

**All subsequent runs are fully offline.**

### Model Cache Location

Models are cached by `fastembed` in platform-specific directories:

| Platform | Cache Location |
|----------|----------------|
| macOS    | `~/Library/Caches/vestige.vestige/fastembed` |
| Linux    | `~/.cache/vestige/fastembed` |
| Windows  | `%LOCALAPPDATA%\vestige\vestige\cache\fastembed` |

Override the cache path:

```bash
export FASTEMBED_CACHE_PATH="/custom/path"
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `VESTIGE_DATA_DIR` | Platform default (see [STORAGE.md](STORAGE.md)) | Custom database location |
| `VESTIGE_LOG_LEVEL` | `info` | Logging verbosity |
| `RUST_LOG` | — | Detailed tracing filter (e.g. `vestige_mcp=debug,vestige_core=info`) |
| `VESTIGE_DASHBOARD_PORT` | `3927` | Dashboard HTTP + WebSocket port |
| `VESTIGE_HTTP_BIND` | `127.0.0.1` | HTTP MCP transport bind address |
| `VESTIGE_HTTP_PORT` | `3928` | HTTP MCP transport port (overridden by `--http-port`) |
| `VESTIGE_AUTH_TOKEN` | auto-generated | Bearer token for the HTTP MCP transport (constant-time compared) |
| `VESTIGE_MAX_TOKEN_BUDGET` | tool default | Cap for `search` / `session_context` token budget |
| `VESTIGE_RETENTION_TARGET` | `0.85` | FSRS-6 retention target |
| `VESTIGE_CONSOLIDATION_INTERVAL_HOURS` | `6` | Background consolidation cadence |
| `VESTIGE_ENCRYPTION_KEY` | — | Required when built with the `encryption` feature (SQLCipher) |
| `VESTIGE_TEST_MOCK_EMBEDDINGS` | — | Use mock embeddings in tests (skips ONNX model download) |
| `FASTEMBED_CACHE_PATH` | platform default | Embedding model cache location |

---

## Command-Line Options

```bash
vestige-mcp --data-dir /custom/path     # Custom storage location
vestige-mcp --http-port 4000            # Override HTTP MCP transport port
vestige-mcp --help                       # Show all options
vestige-mcp --version                    # Print the workspace version
```

---

## CLI Commands

The `vestige` CLI (built alongside `vestige-mcp`) exposes maintenance operations without consuming MCP context window:

```bash
vestige stats                    # Memory statistics
vestige stats --tagging          # Retention distribution by tag
vestige stats --states           # Cognitive state breakdown
vestige health                   # System health check
vestige consolidate              # Run FSRS-6 consolidation now
vestige dashboard                # Open the 3D dashboard in your browser
vestige-restore <file.json>     # Restore from a JSON backup (separate binary)
```

`vestige-restore` is shipped as its own crate so it builds without the fastembed/USearch dependency tree.

---

## Claude Configuration

### Claude Code (one-liner)

```bash
claude mcp add vestige vestige-mcp -s user
```

### Claude Code (manual)

Add to `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp"
    }
  }
}
```

### Claude Desktop (macOS)

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp"
    }
  }
}
```

### Claude Desktop (Windows)

Add to `%APPDATA%\Claude\claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp"
    }
  }
}
```

---

## Custom Data Directory

For per-project or custom storage:

```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp",
      "args": ["--data-dir", "/path/to/custom/dir"]
    }
  }
}
```

See [Storage Modes](STORAGE.md) for global vs per-project vs multi-instance setups.

---

## Updating Vestige

**Latest from source:**

```bash
cd vestige
git pull
cargo build --release -p vestige-mcp
sudo install -m 0755 target/release/{vestige-mcp,vestige,vestige-restore} /usr/local/bin/
```

**Pin to a specific version:**

```bash
git checkout v3.4.0
cargo build --release -p vestige-mcp
```

**Check your installed version:**

```bash
vestige-mcp --version
```

---

## Build Variants

| Feature flags | What you get |
|---------------|--------------|
| _(default)_ | `embeddings` + `vector-search` + `preprocessing` |
| `metal` | Apple Silicon GPU acceleration for embeddings (fastembed Metal backend) |
| `encryption` | SQLCipher encryption at rest — requires `VESTIGE_ENCRYPTION_KEY`. Mutually exclusive with `bundled-sqlite` |
| `telemetry` | Compile-in OpenTelemetry/OTLP scaffolding (no-op until exporter is wired) |
| `--no-default-features` | Skips embeddings + vector search. Smallest binary, keyword-only search |

Example — encryption build:

```bash
cargo build --release -p vestige-mcp \
  --no-default-features \
  --features embeddings,vector-search,preprocessing,encryption
```

---

## Development

```bash
# Run tests with mock embeddings (no model download)
VESTIGE_TEST_MOCK_EMBEDDINGS=1 cargo test --workspace

# Run the binary with verbose tracing
RUST_LOG=vestige_mcp=debug,vestige_core=info cargo run --release -p vestige-mcp

# Metadata drift gate — runs in CI on every PR
./scripts/check-version-and-tools.sh

# ts-rs ↔ TypeScript parity gate — runs in CI when wire/ DTOs change
./scripts/check-generated-types.sh
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full development workflow.

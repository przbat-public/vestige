# Configuration Reference

> Environment variables, CLI commands, and setup options.

---

## First-Run Network Requirement

Vestige downloads two models on first use (~1.68 GB in total):

- **Nomic Embed Text v1.5** (~547 MB ONNX, unquantized) — embedding model
- **Jina Reranker v2 Base Multilingual** (~278 M params, ~1.11 GB ONNX) — cross-encoder reranker

Sizes are the measured `onnx/model.onnx` blobs in the fastembed cache. **All subsequent runs are fully offline.**

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

Custom database location is set with the `--data-dir` flag (see [Command-Line Options](#command-line-options)), not an environment variable. Logging level is controlled by `RUST_LOG`.

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Tracing filter (e.g. `vestige_mcp=debug,vestige_core=info`) |
| `VESTIGE_DASHBOARD_PORT` | `3927` | Dashboard HTTP + WebSocket port |
| `VESTIGE_HTTP_BIND` | `127.0.0.1` | HTTP MCP transport bind address |
| `VESTIGE_HTTP_PORT` | `3928` | HTTP MCP transport port (overridden by `--http-port`) |
| `VESTIGE_AUTH_TOKEN` | auto-generated | Bearer token for the HTTP MCP transport (constant-time compared) |
| `VESTIGE_MAX_TOKEN_BUDGET` | `100000` | Upper clamp for the `search` / `session_context` **response** token budget (requests are capped separately: `vestige_core::fts` truncates queries to 1,000 chars / 32 terms) |
| `VESTIGE_RETENTION_TARGET` | `0.8` | FSRS-6 retention target (read by consolidation reporting and `health`) |
| `VESTIGE_CONSOLIDATION_INTERVAL_HOURS` | `6` | Background consolidation cadence |
| `VESTIGE_NOMIC_PREFIXES` | **on** | Apply the Nomic model-card `search_query:`/`search_document:` task prefixes (embedding space v2). Any non-truthy value (`0`/`false`/`no`/`off`) selects the legacy raw regime, which is a *different vector space* — coupled with `regenerate_embeddings`, see [`.env.example`](../.env.example) |
| `VESTIGE_ENCRYPTION_KEY` | — | Required when built with the `encryption` feature (SQLCipher) |
| `VESTIGE_TEST_MOCK_EMBEDDINGS` | — | Use mock embeddings in tests (skips ONNX model download) |
| `FASTEMBED_CACHE_PATH` | platform default | Embedding model cache location |

### Advanced tuning

Niche knobs read at runtime. Defaults are sensible; change only with a reason.

| Variable | Default | Description |
|----------|---------|-------------|
| `VESTIGE_HYBRID_KEYWORD_WEIGHT` | `0.3` | BM25 channel weight in hybrid-search blending (clamped `[0,1]`) |
| `VESTIGE_HYBRID_SEMANTIC_WEIGHT` | `0.7` | Semantic (HNSW cosine) channel weight (clamped `[0,1]`) |
| `VESTIGE_NREM3_DOWNSCALE_FACTOR` | `0.90` | DreamEngine NREM3 synaptic-downscaling factor (clamped `(0,1]`) |
| `VESTIGE_REQUEST_TIMEOUT_SECS` | `300` | Per-request budget for the HTTP MCP transport |
| `VESTIGE_CORS_ORIGINS` | — | Comma-separated extra CORS origins for the HTTP MCP transport |
| `VESTIGE_HUB_SYNTHESIS` | heuristic | Set to `llm` to enable LLM-based topic-hub naming (otherwise heuristic) |
| `VESTIGE_OTLP_ENDPOINT` / `OTEL_EXPORTER_OTLP_ENDPOINT` | — | OTLP endpoint. No exporter is compiled into the binary today (the `telemetry` feature is an empty placeholder), so setting this produces a startup warning naming the ignored endpoint — nothing is exported. `VESTIGE_`-prefixed wins |
| `VESTIGE_MMR` | off | Enable Maximal-Marginal-Relevance diversity reordering of search results (helps multi-hop synthesis) |
| `VESTIGE_MMR_LAMBDA` | `0.7` | MMR relevance/diversity trade-off in `[0,1]` (1.0 = pure relevance) |
| `VESTIGE_LATE_INTERACTION` | off | Use the ColBERT late-interaction reranker instead of the Jina cross-encoder (requires the `late-interaction` build + `VESTIGE_COLBERT_MODEL_DIR`) |
| `VESTIGE_COLBERT_MODEL_DIR` | — | Directory holding ColBERTv2 `model.onnx` + `tokenizer.json` (only read with the `late-interaction` build) |

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
vestige erase --tag <tag> --dry-run   # GDPR Art. 17: preview an erasure
vestige erase --tag <tag> --confirm   # …and perform it (irreversible)
vestige dashboard                # Open the 3D dashboard in your browser
vestige-restore <file.json>     # Restore from a JSON backup (separate binary)
```

`vestige erase` takes exactly one target (`--id <uuid>` or `--tag <tag>`) and refuses without `--confirm`, mirroring the MCP `erase` tool and `POST /api/maintenance/erase`; it is the only path that also removes the memory's content history and derived data (see [AGENTS.md → erase](../AGENTS.md)).

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
| `late-interaction` | ColBERT token-level reranker via `ort` (adds `ort` + `tokenizers`). Inert unless `VESTIGE_LATE_INTERACTION` + `VESTIGE_COLBERT_MODEL_DIR` are set |
| `--no-default-features` | Skips embeddings + vector search + preprocessing. Smallest binary. Retrieval is FTS5 keyword-only: `search`, `session_context`, `reflect`, `temporal`, `deep_reference` and the dashboard search/explore routes all still answer, but without query embeddings, the HNSW index, the cross-encoder reranker, compound-query decomposition, MMR or the temporal recency/validity boost. The server logs that list once at startup |

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

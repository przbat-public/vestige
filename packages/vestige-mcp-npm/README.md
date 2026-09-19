# vestige-mcp-server

Vestige MCP Server - A synthetic hippocampus for AI assistants.

Built on memory research from Ebbinghaus (1885) to FSRS-6, Vestige provides biologically-inspired memory that decays, strengthens, and consolidates like the human mind.

## Installation

```bash
npm install -g vestige-mcp-server
```

This automatically downloads the correct binary for your platform (macOS, Linux, Windows) from GitHub releases.

### What gets installed

| Command | Description |
|---------|-------------|
| `vestige-mcp` | MCP server for Claude integration |
| `vestige` | CLI for stats, health checks, and maintenance |

### Verify installation

```bash
vestige health
```

## Usage with Claude Code

```bash
claude mcp add vestige vestige-mcp -s user
```

Then restart Claude.

## Usage with Claude Desktop

Add to your Claude Desktop configuration:

**macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp"
    }
  }
}
```

## CLI Commands

```bash
vestige stats          # Memory statistics
vestige stats --states # Cognitive state distribution
vestige health         # System health check
vestige consolidate    # Run memory maintenance cycle
```

## Features

- **FSRS-6 Algorithm**: State-of-the-art spaced repetition for optimal memory retention
- **Dual-Strength Memory**: Bjork & Bjork (1992) - Storage + Retrieval strength model
- **Synaptic Tagging**: Memories become important retroactively (Frey & Morris 1997)
- **Semantic Search**: Local embeddings via nomic-embed-text-v1.5 (384-dim Matryoshka truncation)
- **Local-First**: All data stays on your machine - no cloud, no API costs

## Storage & Memory

Vestige uses SQLite for storage. Your memories are stored on **disk**, not in RAM.

- **Database limit**: ~17.5 TB — SQLite caps a database at 4,294,967,294 pages and a Vestige database runs at a 4 KiB page size (measured on a live database: `PRAGMA page_size` → 4096, `PRAGMA max_page_count` → 4294967294; migration V7 requests `page_size = 8192`, which does not take effect once the connection is in WAL mode). SQLite's theoretical maximum is ~281 TB at 64 KiB pages.
- **RAM usage**: ~64MB cache (configurable)
- **Typical usage**: 1 million memories ≈ 1-2GB on disk (rough estimate — there is no in-repo storage benchmark behind it)

A heavy user creating 100 memories/day would reach ~365k memories after 10 years, i.e. ~0.4-0.7 GB at that same per-memory estimate.

## Embeddings

On first use, Vestige downloads two ONNX models: nomic-embed-text-v1.5 (~547 MB, unquantized) and the Jina Reranker v2 base multilingual cross-encoder (~1.11 GB) — ~1.68 GB in total. This is a one-time download and all subsequent operations are fully offline.

The model is cached in a platform-specific directory (macOS `~/Library/Caches/vestige.vestige/fastembed`, Linux `~/.cache/vestige/fastembed`). Override it with:

```bash
export FASTEMBED_CACHE_PATH="$HOME/.fastembed_cache"
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Log verbosity / tracing filter | `info` |
| `FASTEMBED_CACHE_PATH` | Embeddings model cache location | platform cache dir |
| `VESTIGE_DASHBOARD_PORT` | Dashboard HTTP + WebSocket port | `3927` |

Custom storage location is set with the `--data-dir <PATH>` flag, not an environment variable. See [docs/CONFIGURATION.md](https://github.com/samvallad33/vestige/blob/main/docs/CONFIGURATION.md) for the full list.

## Troubleshooting

### "Could not attach to MCP server vestige"

1. Verify binary exists: `which vestige-mcp`
2. Test directly: `vestige-mcp` (should wait for stdio input)
3. Check Claude logs: `~/Library/Logs/Claude/` (macOS)

### "vestige: command not found"

Reinstall the package:
```bash
npm install -g vestige-mcp-server
```

### Embeddings not downloading

The model downloads on first `ingest` or `search` operation. If Claude can't connect to the MCP server, no memory operations happen and no model downloads.

Fix the MCP connection first, then the model will download automatically.

## Supported Platforms

| Platform | Architecture |
|----------|--------------|
| macOS | ARM64 (Apple Silicon), x86_64 (Intel) |
| Linux | x86_64 |
| Windows | x86_64 |

## License

AGPL-3.0-only

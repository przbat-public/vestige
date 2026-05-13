# Vestige MCP Server

A Rust [Model Context Protocol](https://modelcontextprotocol.io) server that gives Claude, Cursor, Codex, Gemini CLI, Continue, and other MCP clients a long-term cognitive memory.

## Features

- **FSRS-6 spaced repetition** — 21-parameter personalized decay model.
- **Dual-strength memory** — storage strength + retrieval strength (Bjork & Bjork 1992).
- **Local semantic embeddings** — `nomic-embed-text-v1.5` (768d → 384d Matryoshka) via fastembed v5; no external API call leaves the host.
- **HNSW vector search** — USearch index, ~20× faster than FAISS at typical sizes.
- **Hybrid retrieval** — BM25 + semantic, fused with Reciprocal Rank Fusion (RRF), wrapped in an 8-stage cognitive pipeline (compound-query decomposition, Jina Reranker v2, temporal boosting, accessibility filtering, context matching, retrieval competition, spreading activation).
- **Content Intelligence Pipeline** — entity extraction, coreference rewriting, temporal anchoring, relation extraction, and provenance tracking before storage.
- **27 MCP tools** with `readOnlyHint` / `destructiveHint` annotations so clients can decide auto-approval per the MCP `2025-03-26` spec.
- **Cargo features**: `embeddings`, `vector-search`, `preprocessing` (default); optional `encryption`, `metal` (Apple Silicon GPU).

## Installation

```bash
cd /path/to/vestige/crates/vestige-mcp
cargo build --release
# binary at target/release/vestige-mcp
```

For a packaged install via `npm` or the `.mcpb` bundle, see the root `README.md`.

## Claude Desktop Configuration

Add to your Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "vestige": {
      "command": "/path/to/vestige-mcp"
    }
  }
}
```

To pin a custom database location, pass `--data-dir <PATH>` (or `--data-dir=<PATH>`) in the `args` array; without it the server uses the platform-default directory listed below.

## Available Tools (27)

The catalog is built in [`server/catalog.rs`](src/server/catalog.rs); every entry ships an `inputSchema`, a `title`, and behavior `annotations`. Every tool is local-only (`openWorldHint=false`).

### Core memory
| Tool | Actions | Annotations |
|------|---------|-------------|
| `search` | hybrid retrieval + 8-stage cognitive pipeline | read-only, idempotent |
| `smart_ingest` | single (`content`) or batch (`items`, max 20) ingestion with Prediction Error Gating + Content Intelligence Pipeline | mutating, can SUPERSEDE → destructive |
| `memory` | `get`, `get_batch` (≤20 ids), `state`, `promote`, `demote`, `edit`, `delete` | destructive (delete + edit live alongside reads) |
| `codebase` | `remember_pattern`, `remember_decision`, `get_context` | mutating, additive |
| `intention` | `set`, `check`, `update` (complete/snooze/cancel), `list` | mutating, additive |
| `session_context` | one-call session bootstrap (search + status + intentions + predictions + codebase) | read-only, idempotent |

### Cognitive engine
| Tool | What it does | Annotations |
|------|--------------|-------------|
| `dream` | replay recent memories, discover connections, synthesize insights, persist graph | mutating, additive |
| `explore_connections` | `chain` (reasoning paths) / `associations` (spreading activation) / `bridges` | read-only, idempotent |
| `predict` | proactive retrieval based on context + activity history | read-only, idempotent |
| `deep_reference` | full reasoning engine — hybrid retrieval + FSRS trust + intent classification + temporal supersession + contradiction analysis + dream-insight integration. `cross_reference` is a backward-compatible alias. | read-only, idempotent |

### Metacognitive (v3.1+)
| Tool | What it does | Annotations |
|------|--------------|-------------|
| `reflect` | self-examination — contradictions, gaps, stale decisions, overconfident memories, pattern clusters | read-only, idempotent |
| `temporal` | fact versioning — `current` / `expired` / `history` / `invalidate` | destructive (`invalidate`), idempotent |
| `confidence` | calibration — `score` / `audit` / `calibrate` | read-only, idempotent |

### Maintenance
| Tool | What it does | Annotations |
|------|--------------|-------------|
| `system_status` | combined health + stats + cognitive state + recommendations | read-only, idempotent |
| `consolidate` | run FSRS-6 decay + embedding generation + maintenance | mutating, idempotent |
| `memory_health` | retention dashboard (avg, distribution, trend) | read-only, idempotent |
| `memory_graph` | subgraph export for visualization (depth 1..=3, max 200 nodes) | read-only, idempotent |
| `memory_timeline` | chronological browse (default last 7 days) | read-only, idempotent |
| `memory_changelog` | audit trail of memory state transitions | read-only, idempotent |
| `importance_score` | 4-channel scoring — novelty / arousal / reward / attention | read-only, idempotent |
| `find_duplicates` | cosine-similarity duplicate clusters | read-only, idempotent |
| `split_memories` | locate compound multi-topic memories that should be split (default `dry_run=true`) | destructive when `dry_run=false` |
| `regenerate_embeddings` | backfill or rebuild embeddings (e.g. after model upgrade) | mutating, idempotent |
| `backup` | create a SQLite snapshot file | mutating, additive |
| `export` | JSON/JSONL dump (filter by tag/date) | read-only, idempotent |
| `gc` | garbage collect below a retention threshold (default `dry_run=true`) | destructive, idempotent |
| `restore` | restore from JSON backup (MCP wrapper / RecallResult / direct array formats) | destructive |

## Available Resources (11)

| URI | Description |
|-----|-------------|
| `memory://stats` | Current statistics |
| `memory://recent` | Recently added memories (last 10) |
| `memory://decaying` | Memories with low retention |
| `memory://due` | Memories scheduled for review today |
| `memory://insights` | Insights generated during consolidation |
| `memory://consolidation-log` | History of consolidation runs |
| `memory://intentions` | Active prospective intentions |
| `memory://intentions/due` | Triggered or overdue intentions |
| `codebase://structure` | Known codebases |
| `codebase://patterns` | Remembered code patterns |
| `codebase://decisions` | Architectural decisions |

## Example Usage

```
User: Remember that we picked FSRS-6 over SM-2 because it's 20-30% more efficient.

Claude: [calls codebase action="remember_decision"]
Decision recorded.

User: What did we decide about the spaced repetition algorithm?

Claude: [calls codebase action="get_context", codebase="vestige"]
We picked FSRS-6 over SM-2 — 20-30% more efficient.
```

## Data Storage

- Default database path is resolved via the [`directories`](https://docs.rs/directories) crate as `ProjectDirs::from("com", "vestige", "core").data_dir().join("vestige.db")`:
  - macOS: `~/Library/Application Support/com.vestige.core/vestige.db`
  - Linux: `~/.local/share/core/vestige.db` (`$XDG_DATA_HOME/core/` if set)
  - Windows: `%APPDATA%\vestige\core\data\vestige.db`
- Override with `--data-dir /path/to/dir` — the server creates `vestige.db` inside that directory.
- SQLite with FTS5 for keyword search; embeddings stored in a parallel table indexed by USearch HNSW.

For per-project, multi-instance, and shared setups, see [`docs/STORAGE.md`](../../docs/STORAGE.md).

## Protocol

- JSON-RPC 2.0 over stdio (primary transport) and HTTP (secondary).
- MCP protocol version: `2024-11-05` baseline; tools advertise `annotations` from the `2025-03-26` revision.
- Logging via `tracing` to stderr (stdout reserved for JSON-RPC).

## License

MIT

<div align="center">

# Vestige

### The cognitive engine that gives AI a brain.

[![Upstream](https://img.shields.io/badge/upstream-samvallad33%2Fvestige-blue)](https://github.com/samvallad33/vestige)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![MCP Compatible](https://img.shields.io/badge/MCP-compatible-green)](https://modelcontextprotocol.io)

**Your AI forgets everything between sessions. Vestige fixes that.**

Built on 130 years of memory research — FSRS-6 spaced repetition, prediction error gating, synaptic tagging, spreading activation, memory dreaming — all running in a single Rust binary with a 3D neural visualization dashboard. 100% local. Zero cloud.

[Quick Start](#quick-start) | [Dashboard](#-3d-memory-dashboard) | [How It Works](#-the-cognitive-science-stack) | [Tools](#-28-mcp-tools) | [Docs](docs/)

</div>

---

> **Fork notice** — This is an extended fork of [samvallad33/vestige](https://github.com/samvallad33/vestige) (v2.0.3, "Live Memory Materialization"). The upstream project created an impressive cognitive memory system for AI agents. This fork pushes it further in three directions:
>
> 1. **Deeper cognitive science** — metacognition, Bayesian confidence estimation, epistemic classification, proactive interference resolution, and 10 scientific validation tests mapped to published research (Ebbinghaus, Bjork & Bjork, Collins & Loftus, Anderson, Frey & Morris, Diekelmann & Born, Roediger & Karpicke).
> 2. **Production-grade dashboard** — full internationalization (EN/PL), light/dark mode, accessibility (WCAG patterns), component decomposition into a reusable UI library, semantic design tokens.
> 3. **Competitive feature parity** — features inspired by analysis of 12+ memory systems (MemGPT/Letta, Zep/Graphiti, Cognee, Mem0, LightMem, A-Mem, MemoryOS, and others), adapted and integrated into Vestige's Rust architecture.
>
> All original features, tools, and APIs remain fully compatible. See [CHANGELOG.md](CHANGELOG.md) for the full diff.

---

## What's New

> Full version history lives in [CHANGELOG.md](CHANGELOG.md). The highlights below cover the post-fork releases.

### v3.4.0 — Typed-Memory Dashboard

- **Four new pages** surface the typed-memory schema: `Decisions` (structured Decision Matrix), `Insights` (dream + reflect output with validate/dismiss workflow), `Hubs` (auto-detected topic clusters), `Reasoning` (the `deep_reference` engine — evidence, contradictions, supersession, evolution timeline).
- **End-to-end wire contract.** Every dashboard response is a ts-rs-generated DTO from Rust; the five highest-blast-radius endpoints re-validate at runtime via Zod. CI gate (`scripts/check-generated-types.sh`) fails any PR that edits a Rust DTO without committing the regenerated `.ts`.
- **Dream-cycle refactor.** `MemoryDreamer` split into six sub-modules (lifecycle, clustering, connections, contradictions, hubs, insights) so each phase output evolves independently. Dream API returns a per-phase breakdown.
- **Tutorial v2.** Twelve modular components — sticky TOC, glossary tooltips, mode toggle, interactive FSRS-6 curve, live memory example, quiz, guided tour, in-page search, "what's new" banner, progress tracking. Bilingual EN/PL.
- **Migrations v12 + v13** add `decisions`, `hubs`, `insights` tables plus tier/confidence columns. Forward-only and idempotent.

### v3.3.0 — Refactor Wave + LoCoMo Breakthrough

- **LoCoMo benchmark: 41% → 66.17%** (+25 pp). Jina Reranker v2 + turn-level chunking + typed-memory `turn_extracted` mode. Beats LangMem (58.10%), within 0.71 pp of Mem0.
- **`MemoryKind` typed memory** (`Raw` / `Semantic` / `Episodic` / `Procedural` / `Aggregate`) with subject/predicate/object triples and episodic timestamps. Existing memories default to `Raw` — behaviour-neutral until typed retrievers ship.
- **Storage decomposition.** `storage/sqlite/` split into per-concern submodules (nodes, states, history, intentions, maintenance, embeddings, review, consolidation, search, graph, gdpr, temporal, smart_ingest, insights, records, stats).
- **Supply-chain sweep.** Bare `openssl` eliminated; `rustls-webpki` RUSTSEC-2026-0049/0098/0099/0104 patched; `cargo deny` + Dependabot + ARM Linux release target.

### v3.2.0 — Content Intelligence Pipeline

Every memory ingested through `smart_ingest` is automatically enriched:

- **Entity extraction** — URLs, emails, file paths, monetary values, proper nouns → auto-tags (`entity:john-smith`).
- **Coreference rewriting** — "He said X" → "John said X" (makes memories self-contained for better recall).
- **Temporal anchoring** — "by next Friday" resolves to absolute `valid_until`; "starting Monday" → `valid_from`.
- **Relation extraction** — SVO triples → knowledge graph edges (feed spreading activation from day one).
- **Provenance tracking** — session ID, agent, derivation chain, preprocessing artefacts on every memory.

All local heuristic/regex — zero model downloads, sub-millisecond latency.

**Compound query decomposition** — `"auth security; infrastructure costs"` or `"Who worked on FSRS? And dream consolidation?"` are split on semicolons, question chains, and conjunctions, searched independently, then merged via max-score dedup.

### v3.1.0 — Metacognitive Expansion

- **`reflect`** — deliberate self-examination. Detects contradictions, knowledge gaps, stale decisions, overconfident memories, pattern clusters. Flavell (1979), Schön (1983), Nelson & Narens (1990).
- **`temporal`** — bi-temporal fact versioning (Snodgrass 1999). Query valid-now / expired / history / invalidate.
- **`confidence`** — heuristic multi-dimensional scoring (encoding / retrieval / temporal / evidence). `calibrate` is a retention-based consistency check between opinions and facts, **not** Brier-score calibration.
- Eighteen cognitive journey tests mapped to published research (Bjork, Roediger & Karpicke, Collins & Loftus, Brown & Kulik, …).

### Inherited from v3.0.0 — Cognitive Expansion

- Metacognition layer — the search pipeline monitors its own quality, tracks hit/miss rates, detects knowledge gaps.
- Bayesian confidence with credible intervals via Beta distribution.
- Epistemic classification — memories tagged as facts, experiences, observations, or opinions.
- Proactive interference resolution — fan-effect penalty for competing memories (Anderson 1974).
- Memory evolution (A-Mem) — new memories auto-discover and link to related existing memories.
- Context compression (LightMem) — key-sentence extraction for token budget compliance.
- Privacy governance — `right_to_erasure()` for GDPR-style complete removal.
- Ten scientific validation tests, each mapped to published research.

### Inherited from upstream v2.0.3

- React 19 + Vite 6 + Three.js 3D neural graph with WebSocket events.
- Jina Reranker v2 Base Multilingual (278M params).
- Triple hybrid search (BM25 + semantic + RRF).
- HyDE query expansion, FSRS-6 spaced repetition.
- Command palette, PWA support, FSRS decay visualisation.

---

## Quick Start

> **Pre-built binaries for this fork are not yet published.** Use "Build from source" below. Pre-built `samvallad33/vestige` releases are upstream v2.0.3 — they do **not** contain the v3.x cognitive expansion, content intelligence pipeline, or the React dashboard. Use them only if you want to try the upstream baseline.

```bash
# 1. Build from source (requires Rust 1.91+)

cd vestige
cargo build --release -p vestige-mcp
# Optional: enable Metal GPU acceleration on Apple Silicon
cargo build --release -p vestige-mcp --features metal

# Install built binaries
install -m 0755 target/release/{vestige-mcp,vestige,vestige-restore} /usr/local/bin/

# 2. Connect to your AI assistant (Claude Code shown; see "Works Everywhere" below)
claude mcp add vestige vestige-mcp -s user

# 3. Test it
# "Remember that I prefer TypeScript over JavaScript"
# ...new session...
# "What are my coding preferences?"
# → "You prefer TypeScript over JavaScript."
```

<details>
<summary>Upstream pre-built binaries (v2.0.3 baseline only)</summary>

> These are the upstream releases. They do not include any v3.x features from this fork. Use only if you specifically want the upstream baseline.

**macOS (Apple Silicon):**
```bash
curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-aarch64-apple-darwin.tar.gz | tar -xz
sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/
```

**macOS (Intel):**
```bash
curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-x86_64-apple-darwin.tar.gz | tar -xz
sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/
```

**Linux (x86_64):**
```bash
curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-x86_64-unknown-linux-gnu.tar.gz | tar -xz
sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/
```

**Windows / npm:** see upstream [releases](https://github.com/samvallad33/vestige/releases/latest) and the `vestige-mcp-server` npm package.
</details>

---

## Works Everywhere

Vestige speaks MCP — the universal protocol for AI tools. One brain, every IDE.

| IDE | Setup |
|-----|-------|
| **Claude Code** | `claude mcp add vestige vestige-mcp -s user` |
| **Claude Desktop** | [2-min setup](docs/CONFIGURATION.md#claude-desktop-macos) |
| **Xcode 26.3** | [Integration guide](docs/integrations/xcode.md) |
| **Cursor** | [Integration guide](docs/integrations/cursor.md) |
| **VS Code (Copilot)** | [Integration guide](docs/integrations/vscode.md) |
| **JetBrains** | [Integration guide](docs/integrations/jetbrains.md) |
| **Windsurf** | [Integration guide](docs/integrations/windsurf.md) |

---

## 🧠 3D Memory Dashboard

Vestige v2.0 ships with a real-time 3D visualization of your AI's memory. Every memory is a glowing node in 3D space. Watch connections form, memories pulse when accessed, and the entire graph come alive during dream consolidation.

**Features:**
- Force-directed 3D graph with 1000+ nodes at 60fps
- Bloom post-processing for cinematic neural network aesthetic
- Real-time WebSocket events: memories pulse on access, burst on creation, fade on decay
- Dream visualization: graph enters purple dream mode, replayed memories light up sequentially
- FSRS retention curves: see predicted memory decay at 1d, 7d, 30d
- Command palette (`Cmd+K`), keyboard shortcuts, responsive mobile layout
- Installable as PWA for quick access

**Tech:** React 19 + Vite 6 + React Router 7 + Three.js + Tailwind CSS 4 + i18next + WebSocket

The dashboard runs automatically at `http://localhost:3927/dashboard` when the MCP server starts.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  React Dashboard (apps/dashboard)                   │
│  React 19 · Vite 6 · React Router 7 · Three.js      │
│  Tailwind 4 · i18next (EN/PL) · WebSocket           │
│  Light/Dark Mode · a11y · 16 pages                  │
├─────────────────────────────────────────────────────┤
│  Axum HTTP + WebSocket Server (port 3927)           │
│  39 REST routes · ts-rs/Zod wire contract · WS bus  │
├─────────────────────────────────────────────────────┤
│  MCP Server (stdio JSON-RPC + HTTP on :3928)        │
│  28 tools · 11 resources · per-session instances    │
├─────────────────────────────────────────────────────┤
│  Cognitive Engine                                   │
│  ┌──────────┐ ┌────────────┐ ┌───────────────┐      │
│  │ FSRS-6   │ │ Spreading  │ │ Prediction    │      │
│  │ Scheduler│ │ Activation │ │ Error Gating  │      │
│  └──────────┘ └────────────┘ └───────────────┘      │
│  ┌──────────┐ ┌────────────┐ ┌───────────────┐      │
│  │ Dream    │ │ Synaptic   │ │ Hippocampal   │      │
│  │ Engine   │ │ Tagging    │ │ Index         │      │
│  └──────────┘ └────────────┘ └───────────────┘      │
│  ┌──────────┐ ┌────────────┐ ┌───────────────┐      │
│  │ Meta-    │ │ Bayesian   │ │ Epistemic     │      │
│  │ cognition│ │ Confidence │ │ Separation    │      │
│  └──────────┘ └────────────┘ └───────────────┘      │
├─────────────────────────────────────────────────────┤
│  Storage Layer                                      │
│  SQLite + FTS5 · USearch HNSW · Nomic Embed v1.5    │
│  Jina Reranker v2 · RRF · Active Forgetting         │
│  Migrations v1–v13 · WAL · optional SQLCipher       │
└─────────────────────────────────────────────────────┘
```

---

## Why Not Just Use RAG?

RAG is a dumb bucket. Vestige is an active organ.

| | RAG / Vector Store | Vestige |
|---|---|---|
| **Storage** | Store everything | **Prediction Error Gating** — only stores what's surprising or new |
| **Retrieval** | Nearest-neighbor | **8-stage pipeline** — compound-query decomposition + triple hybrid (BM25 + semantic + RRF) + Jina v2 reranking + spreading activation |
| **Decay** | Nothing expires | **FSRS-6** — memories fade naturally, context stays lean |
| **Duplicates** | Manual dedup | **Self-healing** — auto-merges "likes dark mode" + "prefers dark themes" |
| **Importance** | All equal | **4-channel scoring** — novelty, arousal, reward, attention |
| **Sleep** | No consolidation | **Memory dreaming** — replays, connects, synthesizes insights |
| **Health** | No visibility | **Retention dashboard** — distributions, trends, recommendations |
| **Visualization** | None | **3D neural graph** — real-time WebSocket-powered Three.js |
| **Privacy** | Usually cloud | **100% local** — your data never leaves your machine |

---

## 🔬 The Cognitive Science Stack

This isn't a key-value store with an embedding model bolted on. Vestige implements real neuroscience:

**Prediction Error Gating** — The hippocampal bouncer. When new information arrives, Vestige compares it against existing memories. Redundant? Merged. Contradictory? Superseded. Novel? Stored with high synaptic tag priority.

**FSRS-6 Spaced Repetition** — 21 parameters governing the mathematics of forgetting. Frequently-used memories stay strong. Unused memories naturally decay. Your context window stays clean.

**HyDE Query Expansion** *(v2.0)* — Template-based Hypothetical Document Embeddings. Expands queries into 3-5 semantic variants, embeds all variants, and searches with the centroid embedding for dramatically better recall on conceptual queries.

**Synaptic Tagging** — A memory that seemed trivial this morning can be retroactively tagged as critical tonight. Based on [Frey & Morris, 1997](https://doi.org/10.1038/385533a0).

**Spreading Activation** — Search for "auth bug" and find the related JWT library update from last week. Memories form a graph, not a flat list. Based on [Collins & Loftus, 1975](https://doi.org/10.1037/0033-295X.82.6.407).

**Dual-Strength Model** — Every memory has storage strength (encoding quality) and retrieval strength (accessibility). A deeply stored memory can be temporarily hard to retrieve — just like real forgetting. Based on [Bjork & Bjork, 1992](https://doi.org/10.1016/S0079-7421(08)60016-9).

**Memory Dreaming** — Like sleep consolidation. Replays recent memories to discover hidden connections, strengthen important patterns, and synthesize insights. Dream-discovered connections persist to a graph database. Inspired by [Diekelmann & Born 2010](https://doi.org/10.1038/nrn2762) (memory consolidation during sleep) and an [Active Dreaming Memory preprint on engrXiv](https://engrxiv.org/preprint/download/5919/9826/8234) — note that the preprint is not peer-reviewed, treat as a design reference rather than a citation of established result.

**Waking SWR Tagging** — Promoted memories get sharp-wave ripple tags for preferential replay during dream consolidation. 70/30 tagged-to-random ratio. Based on [Buzsaki, 2015](https://doi.org/10.1038/nn.3963).

**Autonomic Regulation** — Self-regulating memory health. Auto-promotes frequently accessed memories. Auto-GCs low-retention memories. Consolidation triggers on 6h staleness or 2h active use.

[Full science documentation ->](docs/SCIENCE.md)

---

## 🛠 28 MCP Tools

The canonical catalog lives in [`crates/vestige-mcp/src/server/catalog.rs`](crates/vestige-mcp/src/server/catalog.rs). CI fails any PR that drifts.

### Context Packets
| Tool | What It Does |
|------|-------------|
| `session_context` | **One-call session init** — replaces 5 calls with token-budgeted context, automation triggers, expandable IDs |

### Core Memory
| Tool | What It Does |
|------|-------------|
| `search` | 8-stage cognitive search — compound query decomposition + triple hybrid (BM25 + semantic + RRF) + Jina v2 reranking + temporal + competition + spreading activation |
| `smart_ingest` | Intelligent storage with CREATE/UPDATE/SUPERSEDE via Prediction Error Gating. Runs the Content Intelligence Pipeline (entity extraction, coreference, temporal anchoring, relation extraction, provenance). Batch mode for session-end saves |
| `memory` | Get, batch get (up to 20 ids in one call), edit, delete, check state, promote (thumbs up), demote (thumbs down) |
| `codebase` | Remember code patterns and architectural decisions per-project. `remember_decision_v2` writes a structured Decision Matrix (question + choices + criteria + 1–5 scoring + optional `validUntil`) — `reflect` auto-flags decisions whose window has passed |
| `intention` | Prospective memory — "remind me to X when Y happens" |

### Cognitive Engine
| Tool | What It Does |
|------|-------------|
| `dream` | Memory consolidation — replays memories, discovers connections, synthesizes insights, persists graph |
| `explore_connections` | Graph traversal — reasoning chains, associations, bridges between memories, **`causal_chain` walks only persisted causal edges for `why?` questions** |
| `predict` | Proactive retrieval — predicts what you'll need next based on context and activity |
| `precompute_for_context` | **Sleep-time compute** — pre-fetch + summarize a topic before you ask, store as `precomputed_summary` with TTL (1–168h) and `topic:<slug>` tag. Next search hits a warm cache |

### Cognitive Reasoning (v3.2)
| Tool | What It Does |
|------|-------------|
| `deep_reference` | Reasoning engine across memories: hybrid retrieval, FSRS-6 trust scoring, intent classification, temporal supersession, contradiction analysis, dream-insight integration, structured synthesis. `cross_reference` is a backward-compatible alias. |

### Metacognitive (v3.1)
| Tool | What It Does |
|------|-------------|
| `reflect` | Self-examination — contradictions, knowledge gaps, stale decisions, overconfident memories, pattern clusters |
| `temporal` | Temporal fact versioning — current, expired, history, invalidate. Tracks fact evolution over time |
| `confidence` | Confidence scoring — multi-dimensional (encoding, retrieval, temporal, evidence). Audit and calibrate |

### Autonomic
| Tool | What It Does |
|------|-------------|
| `memory_health` | Retention dashboard — distribution, trends, recommendations |
| `memory_graph` | Knowledge graph export — force-directed layout, up to 200 nodes |

### Scoring & Dedup
| Tool | What It Does |
|------|-------------|
| `importance_score` | 4-channel neuroscience scoring (novelty, arousal, reward, attention) |
| `find_duplicates` | Detect and merge redundant memories via cosine similarity |

### Maintenance
| Tool | What It Does |
|------|-------------|
| `system_status` | Combined health + stats + cognitive state + recommendations |
| `consolidate` | Run FSRS-6 decay cycle (also auto-runs every 6 hours) |
| `memory_timeline` | Browse chronologically, grouped by day |
| `memory_changelog` | Audit trail of state transitions |
| `split_memories` | Find compound/multi-topic memories that should be split into atomic pieces |
| `regenerate_embeddings` | Backfill or rebuild embeddings (e.g. after a model upgrade or a long offline stretch) |
| `backup` / `export` / `gc` | Database backup, JSON export, garbage collection |
| `restore` | Restore from JSON backup |

---

## Make Your AI Use Vestige Automatically

Add this to your agent instructions file — `AGENTS.md` (the canonical cross-tool standard, picked up by Codex, Cursor, Copilot, Claude Code, Cline, Gemini CLI, Continue, Zed, JetBrains Junie, Devin, and others), or the tool-specific equivalent (`CLAUDE.md`, `GEMINI.md`, `.cursor/rules/*.mdc`, `.github/copilot-instructions.md`):

```markdown
## Memory

At the start of every session:
1. Call `session_context` with topic keywords (one MCP call replaces 5)
2. Save bug fixes, decisions, and patterns proactively via `smart_ingest`
3. Create reminders via `intention` when the user mentions deadlines
4. Promote memories that turned out useful, demote ones that misled
```

| You Say | AI Does |
|---------|---------|
| "Remember this" | Saves immediately |
| "I prefer..." / "I always..." | Saves as preference |
| "Remind me..." | Creates a future trigger |
| "This is important" | Saves + promotes |

[Full agent-instructions templates ->](docs/CLAUDE-SETUP.md)

---

## Technical Details

| Metric | Value |
|--------|-------|
| **Language** | Rust 2024 edition (MSRV 1.91) |
| **Codebase** | 1,080+ tests + 10 scientific validation + 18 cognitive journey tests |
| **Binary size** | ~20MB |
| **Embeddings** | Nomic Embed Text v1.5 (768D → 384D Matryoshka, 8192 context) |
| **Vector search** | USearch HNSW (in-memory ANN, single-thread reads via mutex) |
| **Reranker** | Jina Reranker v2 Base Multilingual (278M params) |
| **Search** | Triple hybrid scoring (BM25 + semantic + RRF) + metacognition + interference resolution |
| **Storage** | SQLite + FTS5 (optional SQLCipher encryption) |
| **Dashboard** | React 19 + Vite 6 + React Router 7 + Three.js + Tailwind CSS 4 + i18next |
| **Locales** | English, Polish (extensible) |
| **Themes** | Light + Dark (oklch design tokens, `prefers-color-scheme` aware) |
| **Transport** | MCP stdio (JSON-RPC 2.0) + optional HTTP MCP (port 3928) + WebSocket |
| **MCP tools** | 28 (4 unified + smart_ingest + 2 temporal + 7 maintenance + 2 dedup + 4 cognitive + restore + session_context + 2 autonomic + 3 metacognitive + deep_reference) |
| **Cognitive modules** | Stateful neuroscience + advanced + search families. Canonical list in `cognitive.rs::CognitiveEngine` |
| **First run** | Downloads embedding + reranker models (~1.3GB), then fully offline |
| **Platforms** | macOS (ARM/Intel), Linux (x86_64), Windows |

### Optional Features

```bash
# Metal GPU acceleration (Apple Silicon — faster embedding inference)
cargo build --release -p vestige-mcp --features metal

# SQLCipher encryption
cargo build --release -p vestige-mcp --no-default-features --features encryption,embeddings,vector-search
```

---

## CLI

```bash
vestige stats                    # Memory statistics
vestige stats --tagging          # Retention distribution
vestige stats --states           # Cognitive state breakdown
vestige health                   # System health check
vestige consolidate              # Run memory maintenance
vestige restore <file>           # Restore from backup
vestige dashboard                # Open 3D dashboard in browser
```

---

## Documentation

| Document | Contents |
|----------|----------|
| [FAQ](docs/FAQ.md) | 30+ common questions answered |
| [Science](docs/SCIENCE.md) | The neuroscience behind every feature |
| [Storage Modes](docs/STORAGE.md) | Global, per-project, multi-instance |
| [Agent Instructions](AGENTS.md) | Canonical operational policy for AI coding agents (CLAUDE.md and GEMINI.md are symlinks) |
| [Setup Templates](docs/CLAUDE-SETUP.md) | User-side templates for proactive memory (Claude Code, Cursor, etc.) |
| [Configuration](docs/CONFIGURATION.md) | CLI commands, environment variables |
| [Integrations](docs/integrations/) | Xcode, Cursor, VS Code, JetBrains, Windsurf |
| [Changelog](CHANGELOG.md) | Version history |

---

## Troubleshooting

<details>
<summary>"Command not found" after installation</summary>

Ensure `vestige-mcp` is in your PATH:
```bash
which vestige-mcp
# Or use the full path:
claude mcp add vestige /usr/local/bin/vestige-mcp -s user
```
</details>

<details>
<summary>Embedding model download fails</summary>

First run downloads ~130MB from Hugging Face. If behind a proxy:
```bash
export HTTPS_PROXY=your-proxy:port
```

Cache: macOS `~/Library/Caches/vestige.vestige/fastembed` | Linux `~/.cache/vestige/fastembed`
</details>

<details>
<summary>Dashboard not loading</summary>

The dashboard starts automatically on port 3927 when the MCP server runs. Check:
```bash
curl http://localhost:3927/api/health
# Should return {"status":"healthy",...}
```
</details>

[More troubleshooting ->](docs/FAQ.md#troubleshooting)

---

## Contributing

Issues and PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

AGPL-3.0 — free to use, modify, and self-host. If you offer Vestige as a network service, you must open-source your modifications.

---

<p align="center">
  <i>Originally built by <a href="https://github.com/samvallad33">@samvallad33</a></i><br>
  <sub>28 tools · typed-memory dashboard · content intelligence pipeline · scientific validation · i18n · light/dark mode · one binary</sub>
</p>

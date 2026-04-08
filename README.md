<div align="center">

# Vestige

### The cognitive engine that gives AI a brain.

[![Upstream](https://img.shields.io/badge/upstream-samvallad33%2Fvestige-blue)](https://github.com/samvallad33/vestige)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![MCP Compatible](https://img.shields.io/badge/MCP-compatible-green)](https://modelcontextprotocol.io)

**Your AI forgets everything between sessions. Vestige fixes that.**

Built on 130 years of memory research — FSRS-6 spaced repetition, prediction error gating, synaptic tagging, spreading activation, memory dreaming — all running in a single Rust binary with a 3D neural visualization dashboard. 100% local. Zero cloud.

[Quick Start](#quick-start) | [Dashboard](#-3d-memory-dashboard) | [How It Works](#-the-cognitive-science-stack) | [Tools](#-24-mcp-tools) | [Docs](docs/)

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

## What's New in v3.1.0 "Metacognitive Expansion"

### Metacognitive Tools (new in v3.1)
- **`reflect`** — deliberate self-examination of memories. Detects contradictions, knowledge gaps, stale decisions, overconfident memories, pattern clusters. Based on Flavell (1979), Schön (1983), Nelson & Narens (1990)
- **`temporal`** — temporal fact versioning. Query valid-now, expired, historical facts. Mark facts as no longer valid. Based on bi-temporal theory (Snodgrass 1999) and Graphiti temporal knowledge graphs
- **`confidence`** — multi-dimensional confidence scoring. Evaluate encoding, retrieval, temporal, and evidence strength. Audit poorly-calibrated memories. Based on Kahneman (2011), Tetlock (2015)
- **Dashboard integration** — Self-Reflection and Confidence Audit accessible from the Settings page
- **18 cognitive journey tests** — each mapped to published research (Bjork, Roediger & Karpicke, Collins & Loftus, Brown & Kulik, and more)

### Inherited from v3.0.0 "Cognitive Expansion"
- **Metacognition layer** — the search pipeline monitors its own quality, tracks hit/miss rates, detects knowledge gaps
- **Bayesian confidence** — access-pattern-based confidence with credible intervals via Beta distribution
- **Epistemic classification** — memories classified as facts, experiences, observations, or opinions
- **Proactive interference resolution** — fan-effect penalty for competing memories (Anderson, 1974)
- **Memory evolution** (A-Mem) — new memories auto-discover and link to related existing memories
- **Context compression** (LightMem) — key sentence extraction for token budget compliance
- **Privacy governance** — `right_to_erasure()` for GDPR-style complete removal
- **10 scientific validation tests** — each mapped to published research
- **Tutorial page** — comprehensive guide (analogies, lifecycle, science, FAQ, glossary) written for beginners, in English and Polish

### Dashboard
- **Internationalization** — full EN + PL via i18next
- **Light / dark mode** — oklch-based semantic design tokens with `prefers-color-scheme` detection
- **Accessibility** — skip-to-content, route announcer, `aria-live`, keyboard navigation, reduced motion
- **UI component library** — Button, Card, Badge, ProgressBar, SearchInput, EmptyState, LoadingSpinner, StatCard
- **Component decomposition** — focused sub-components across all pages
- **10 pages** — Graph, Memories, Timeline, Feed, Explore, Intentions, Stats, Settings, Tutorial, Not Found

### Inherited from upstream v2.0.3
- React 19 + Vite 6 + Three.js 3D neural graph with WebSocket events
- Jina Reranker v2 Base Multilingual (278M params)
- Triple hybrid search (BM25 + semantic + RRF)
- HyDE query expansion, FSRS-6 spaced repetition, 29 cognitive modules
- Command palette, PWA support, FSRS decay visualization

---

## Quick Start

```bash
# 1. Install (macOS Apple Silicon)
curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-aarch64-apple-darwin.tar.gz | tar -xz
sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/

# 2. Connect to Claude Code
claude mcp add vestige vestige-mcp -s user

# 3. Test it
# "Remember that I prefer TypeScript over JavaScript"
# ...new session...
# "What are my coding preferences?"
# → "You prefer TypeScript over JavaScript."
```

<details>
<summary>Other platforms & install methods</summary>

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

**Windows:** Download from [Releases](https://github.com/samvallad33/vestige/releases/latest)

**npm:**
```bash
npm install -g vestige-mcp-server
```

**Build from source (requires Rust 1.91+):**
```bash
git clone https://github.com/samvallad33/vestige && cd vestige
cargo build --release -p vestige-mcp
# Optional: enable Metal GPU acceleration on Apple Silicon
cargo build --release -p vestige-mcp --features metal
```
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
│  React 19 · Vite 6 · Three.js 3D Graph · i18next    │
│  Light/Dark Mode · a11y · EN/PL · WebSocket         │
├─────────────────────────────────────────────────────┤
│  Axum HTTP + WebSocket Server (port 3927)           │
│  18 REST endpoints · WS event broadcast             │
├─────────────────────────────────────────────────────┤
│  MCP Server (stdio JSON-RPC)                        │
│  24 tools · 29 cognitive modules                    │
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
└─────────────────────────────────────────────────────┘
```

---

## Why Not Just Use RAG?

RAG is a dumb bucket. Vestige is an active organ.

| | RAG / Vector Store | Vestige |
|---|---|---|
| **Storage** | Store everything | **Prediction Error Gating** — only stores what's surprising or new |
| **Retrieval** | Nearest-neighbor | **7-stage pipeline** — triple hybrid (BM25 + semantic + RRF) + Jina v2 reranking + spreading activation |
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

**Memory Dreaming** — Like sleep consolidation. Replays recent memories to discover hidden connections, strengthen important patterns, and synthesize insights. Dream-discovered connections persist to a graph database. Based on the [Active Dreaming Memory](https://engrxiv.org/preprint/download/5919/9826/8234) framework.

**Waking SWR Tagging** — Promoted memories get sharp-wave ripple tags for preferential replay during dream consolidation. 70/30 tagged-to-random ratio. Based on [Buzsaki, 2015](https://doi.org/10.1038/nn.3963).

**Autonomic Regulation** — Self-regulating memory health. Auto-promotes frequently accessed memories. Auto-GCs low-retention memories. Consolidation triggers on 6h staleness or 2h active use.

[Full science documentation ->](docs/SCIENCE.md)

---

## 🛠 24 MCP Tools

### Context Packets
| Tool | What It Does |
|------|-------------|
| `session_context` | **One-call session init** — replaces 5 calls with token-budgeted context, automation triggers, expandable IDs |

### Core Memory
| Tool | What It Does |
|------|-------------|
| `search` | 7-stage cognitive search — triple hybrid (BM25 + semantic + RRF) + Jina v2 reranking + temporal + competition + spreading activation |
| `smart_ingest` | Intelligent storage with CREATE/UPDATE/SUPERSEDE via Prediction Error Gating. Batch mode for session-end saves |
| `memory` | Get, delete, check state, promote (thumbs up), demote (thumbs down) |
| `codebase` | Remember code patterns and architectural decisions per-project |
| `intention` | Prospective memory — "remind me to X when Y happens" |

### Cognitive Engine
| Tool | What It Does |
|------|-------------|
| `dream` | Memory consolidation — replays memories, discovers connections, synthesizes insights, persists graph |
| `explore_connections` | Graph traversal — reasoning chains, associations, bridges between memories |
| `predict` | Proactive retrieval — predicts what you'll need next based on context and activity |

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
| `backup` / `export` / `gc` | Database backup, JSON export, garbage collection |
| `restore` | Restore from JSON backup |

---

## Make Your AI Use Vestige Automatically

Add this to your `CLAUDE.md`:

```markdown
## Memory

At the start of every session:
1. Search Vestige for user preferences and project context
2. Save bug fixes, decisions, and patterns without being asked
3. Create reminders when the user mentions deadlines
```

| You Say | AI Does |
|---------|---------|
| "Remember this" | Saves immediately |
| "I prefer..." / "I always..." | Saves as preference |
| "Remind me..." | Creates a future trigger |
| "This is important" | Saves + promotes |

[Full CLAUDE.md templates ->](docs/CLAUDE-SETUP.md)

---

## Technical Details

| Metric | Value |
|--------|-------|
| **Language** | Rust 2024 edition (MSRV 1.91) |
| **Codebase** | 1,080+ tests + 10 scientific validation + 18 cognitive journey tests |
| **Binary size** | ~20MB |
| **Embeddings** | Nomic Embed Text v1.5 (768D → 384D Matryoshka, 8192 context) |
| **Vector search** | USearch HNSW (20x faster than FAISS) |
| **Reranker** | Jina Reranker v2 Base Multilingual (278M params) |
| **Search** | Triple hybrid scoring (BM25 + semantic + RRF) + metacognition + interference resolution |
| **Storage** | SQLite + FTS5 (optional SQLCipher encryption) |
| **Dashboard** | React 19 + Vite 6 + React Router 7 + Three.js + Tailwind CSS 4 + i18next |
| **Locales** | English, Polish (extensible) |
| **Themes** | Light + Dark (oklch design tokens, `prefers-color-scheme` aware) |
| **Transport** | MCP stdio (JSON-RPC 2.0) + WebSocket |
| **MCP tools** | 24 (core memory, cognitive, metacognitive, autonomic, maintenance) |
| **Cognitive modules** | 29 stateful + metacognition + Bayesian confidence + epistemic separation |
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
| [CLAUDE.md Setup](docs/CLAUDE-SETUP.md) | Templates for proactive memory |
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

Cache: macOS `~/Library/Caches/com.vestige.core/fastembed` | Linux `~/.cache/vestige/fastembed`
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
  <i>Extended by <a href="https://github.com/przemyslawbatte">@przemyslawbatte</a></i><br>
  <sub>24 tools · 29 cognitive modules · scientific validation · i18n · light/dark mode · one binary</sub>
</p>

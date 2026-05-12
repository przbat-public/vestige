# Vestige — Architecture Reference

Complete technical reference for Vestige's system architecture. For operational tool usage, see [CLAUDE.md](CLAUDE.md).

---

## System Overview

Vestige is a single Rust binary (`vestige-mcp`) that runs three concurrent subsystems:

```
┌───────────────────────────────────────────────────────────┐
│                    vestige-mcp binary                     │
├───────────────┬───────────────┬───────────────────────────┤
│  Stdio MCP    │  HTTP MCP     │  Dashboard Server         │
│  (primary)    │  (port 3928)  │  (port 3927)              │
│  JSON-RPC     │  JSON-RPC     │  Axum REST + WS + SPA     │
│  stdin/stdout │  POST /mcp    │  React 19 + Three.js      │
├───────────────┴───────────────┴───────────────────────────┤
│  McpServer — 27 tools, event emission                     │
├───────────────────────────────────────────────────────────┤
│  CognitiveEngine (Arc<Mutex<_>>)                          │
│  29 modules: FSRS-6, spreading activation, dreaming, ...  │
├───────────────────────────────────────────────────────────┤
│  Storage (Arc<_>)                                         │
│  SQLite WAL + FTS5 + USearch HNSW + Nomic Embed v1.5      │
└───────────────────────────────────────────────────────────┘
```

**Startup sequence** (`main.rs`):
1. Parse CLI args (`--data-dir`, `--http-port`)
2. `Storage::new` + optional embeddings init (Nomic v1.5 384D + Jina Reranker v2 278M)
3. Spawn background consolidation loop (default 6h interval)
4. `CognitiveEngine::new` + `hydrate`
5. Create `broadcast::channel<VestigeEvent>(1024)` — shared event bus
6. Start Dashboard server (port 3927) with REST API + WebSocket + static SPA
7. Start HTTP MCP transport (port 3928) with per-session `McpServer` instances
8. Start Stdio MCP transport (primary) — line-delimited JSON-RPC on stdin/stdout
9. SIGINT handler — WAL checkpoint then exit

---

## Directory Layout

```
vestige/
├── crates/
│   ├── vestige-core/          # Cognitive engine, storage, embeddings, search, FSRS
│   │   └── src/
│   │       ├── storage/       # SQLite (sqlite/ submodules: nodes, states, history,
│   │       │                  # intentions, maintenance, embeddings, review, consolidation,
│   │       │                  # search, graph, gdpr, temporal, smart_ingest, insights,
│   │       │                  # records, stats), migrations v1-v11, WAL, FTS5
│   │       ├── memory/        # Node types, FSRS strength, temporal, typed-memory MemoryKind
│   │       ├── fsrs/          # Algorithm, scheduler, optimizer
│   │       ├── embeddings/    # Nomic v1.5 local ONNX, hybrid, code embeddings
│   │       ├── preprocessing/  # Content intelligence pipeline (entities, coref, temporal, relations, provenance)
│   │       ├── search/        # Hybrid, vector, keyword (BM25), reranker, temporal, HyDE, decompose
│   │       ├── neuroscience/  # Spreading activation, memory states, hippocampal index,
│   │       │                  # synaptic tagging, importance signals, predictive retrieval,
│   │       │                  # emotional memory, context memory, prospective memory
│   │       ├── advanced/      # Dreams, reconsolidation, prediction error, intent detection,
│   │       │                  # importance, compression, chains, cross-project, adaptive embedding,
│   │       │                  # speculative retrieval
│   │       ├── consolidation/ # Phases, sleep
│   │       ├── codebase/      # Code patterns, git, relationships, watcher
│   │       └── fts.rs         # FTS5 query sanitization
│   └── vestige-mcp/           # MCP server binary + dashboard embedding
│       └── src/
│           ├── main.rs        # CLI, init, startup sequence
│           ├── server.rs      # McpServer — JSON-RPC dispatch, tool routing, event emission
│           ├── server/        # catalog.rs — canonical 27-tool/11-resource list (b15 split)
│           ├── cognitive.rs   # CognitiveEngine wrapper
│           ├── protocol/      # stdio.rs, http.rs, messages.rs, types.rs, auth.rs
│           ├── dashboard/     # mod.rs (Axum router), handlers/ (memory, search, graph,
│           │                  # history, intentions, maintenance, review, cognitive,
│           │                  # metacognitive, observability, pages), websocket.rs,
│           │                  # events.rs, state.rs, static_files.rs
│           ├── tools/         # One file per MCP tool (27 tools)
│           ├── resources/     # MCP resources (memory.rs, codebase.rs)
│           └── bin/           # cli.rs (vestige CLI). vestige-restore is its own crate.
├── apps/
│   └── dashboard/             # React 19 + Vite 6 + React Router 7 + Three.js
│       ├── src/
│       │   ├── main.tsx       # Entry point
│       │   ├── App.tsx        # React Router setup
│       │   ├── app.css        # Global styles (Tailwind 4)
│       │   ├── pages/         # 10 pages: Graph, Memories, Timeline, Feed,
│       │   │                  # Explore, Intentions, Stats, Settings, Tutorial, NotFound
│       │   ├── components/    # Layout, Graph3D, PipelineVisualizer,
│       │   │                  # TimeSlider, RetentionCurve
│       │   ├── stores/        # api.ts, websocket.ts (Zustand)
│       │   ├── types/         # Shared TypeScript types
│       │   └── graph/         # Three.js graph engine (9 files)
│       ├── build/             # Production build (embedded in Rust binary)
│       └── vite.config.ts     # Aliases: @ → src, @graph → src/graph
├── tests/e2e/                 # E2E test crate (cognitive, journeys, extreme, mcp)
├── packages/                  # NPM distribution packages
├── docs/                      # FAQ, science, storage, integrations
└── .github/                   # CI workflows (test.yml, release.yml)
```

> **Dashboard layout note:** `apps/dashboard/src/lib/` holds framework-agnostic TypeScript helpers (`utils.ts`, `i18n.ts`, `concurrency.ts`) shared across pages and components. React-specific code lives in `src/graph/`, `src/components/`, and `src/stores/`. Do not remove `src/lib/` — it is actively imported via the `@/lib/...` alias.

---

## Cognitive Engine

### Search Pipeline (8 stages)

0. **Decompose (new)** — compound queries split into sub-queries (semicolons, question chains, conjunctions), searched independently, merged via max-score dedup. +43% MRR on compound queries (ACL 2025)
1. **Overfetch** — 3x results from triple hybrid search (BM25 keyword + semantic vector + Reciprocal Rank Fusion)
   - Embeddings: nomic-embed-text-v1.5, 768D → 384D Matryoshka truncation, 8K token context
   - Vector index: USearch HNSW
2. **Rerank** — Cross-encoder rescoring (Jina Reranker v2 Base Multilingual, 278M params)
3. **Temporal** — Recency + validity window boosting (85% relevance + 15% temporal)
4. **Accessibility** — FSRS-6 retention filter (Active ≥0.7, Dormant ≥0.4, Silent ≥0.1)
5. **Context** — Tulving 1973 encoding specificity (topic overlap → +30% boost)
6. **Competition** — Anderson 1994 retrieval-induced forgetting (winners strengthen, competitors weaken)
7. **Activation** — Spreading activation side effects + predictive model + reconsolidation marking

### Ingest Pipeline

- **Preprocessing (new):** Content Intelligence Pipeline enriches content before storage:
  1. Entity extraction (regex: URLs, emails, file paths, monetary, proper nouns → `entity:` auto-tags)
  2. Coreference rewriting ("He said" → "John said" — single unambiguous referent only)
  3. Temporal anchoring ("by next Friday" → `valid_until` via `natural-date-rs`)
  4. Relation extraction (SVO triples: "John manages Auth Team" → knowledge graph edges)
  5. Provenance assembly (session_id, agent, derivation chain, preprocessing artifacts)
- **Pre:** 4-channel importance scoring (novelty/arousal/reward/attention) + intent detection → auto-tag
- **Store:** Prediction Error Gating — similarity >0.92 → UPDATE, 0.75-0.92 → UPDATE/SUPERSEDE, <0.75 → CREATE
- **Post:** Synaptic tagging (Frey & Morris 1997, 9h backward + 2h forward) + hippocampal indexing + relation edge creation + cross-project recording
- **Active forgetting:** Contradiction detection via `has_negation_divergence` — conflicting memories flagged
- **Prospective indexing:** New memories pre-indexed for anticipated future retrieval patterns

### FSRS-6 (Spaced Repetition)

- Retrievability: `R = (1 + factor × t / S)^(-w20)` — 21 trained parameters
- Dual-strength model (Bjork & Bjork 1992): storage strength (grows) + retrieval strength (decays)
- Accessibility = retention×0.5 + retrieval×0.3 + storage×0.2

### 29 Cognitive Modules

**Neuroscience (16):**
ActivationNetwork (Collins & Loftus 1975), SynapticTaggingSystem (Frey & Morris 1997), HippocampalIndex (Teyler & Rudy 2007), ContextMatcher (Tulving 1973), AccessibilityCalculator, CompetitionManager (Anderson 1994), StateUpdateService, ImportanceSignals, NoveltySignal, ArousalSignal, RewardSignal, AttentionSignal, EmotionalMemory (Brown & Kulik 1977), PredictiveMemory, ProspectiveMemory, IntentionParser

**Advanced (11):**
ImportanceTracker, ReconsolidationManager (Nader — 5min labile window), IntentDetector (9 intent types), ActivityTracker, MemoryDreamer (5-stage consolidation), MemoryChainBuilder (A*-like), MemoryCompressor (30-day min age), CrossProjectLearner (6 pattern types), AdaptiveEmbedder, SpeculativeRetriever (6 trigger types), ConsolidationScheduler

**Search (2):** Reranker, TemporalSearcher

### Memory States

- **Active** (retention ≥ 0.7) — easily retrievable
- **Dormant** (≥ 0.4) — retrievable with effort
- **Silent** (≥ 0.1) — difficult, needs cues
- **Unavailable** (< 0.1) — needs reinforcement

### Connection Types

semantic, temporal, causal, spatial, part_of, user_defined — each with strength (0-1), activation_count, timestamps

---

## Storage Layer

**`crates/vestige-core/src/storage/`**

| Component | Details |
|-----------|---------|
| **SQLite** | WAL mode, reader/writer connection split, PRAGMA optimizations |
| **FTS5** | Full-text search with porter tokenizer, page_size tuning |
| **Migrations** | Versions 1-11 (FSRS, embeddings, neuroscience tables, graph/scopes, FSRS-6 upgrade, dream history, FTS5, autonomic fields, emotional/temporal hierarchy, V10 provenance tracking, V11 typed memory `MemoryKind`) |
| **Vector index** | USearch HNSW, feature-gated behind `vector-search` |
| **Embeddings** | Nomic Embed v1.5 via fastembed (local ONNX), feature-gated behind `embeddings` |
| **FTS sanitization** | `fts.rs` — strips injection patterns, length limits, always available |

---

## MCP Server Transports

### Stdio (primary)
`protocol/stdio.rs` — Async line-delimited JSON-RPC on stdin/stdout. Logging goes to stderr. Heartbeat on stdout. This is the transport used by Claude Code, Cursor, and other MCP clients.

### HTTP (secondary)
`protocol/http.rs` — Streamable HTTP MCP on port 3928 (configurable via `--http-port` / `VESTIGE_HTTP_PORT`).
- `POST /mcp` — JSON-RPC requests. First call creates a session; subsequent calls require `Mcp-Session-Id` header.
- `DELETE /mcp` — Session teardown.
- Auth: Bearer token from `VESTIGE_AUTH_TOKEN` env var or auto-generated.
- Per-session `McpServer` instances sharing `Arc<Storage>` and `Arc<Mutex<CognitiveEngine>>`.

---

## Dashboard Server

**Port 3927** (configurable via `VESTIGE_DASHBOARD_PORT`). Built with Axum.

### REST API Endpoints (28 operations across 26 paths)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/memories` | List memories (paginated) |
| GET | `/api/memories/{id}` | Get single memory |
| DELETE | `/api/memories/{id}` | Delete memory |
| PATCH | `/api/memories/{id}` | Update memory content/tags (preserves FSRS state) |
| POST | `/api/memories/{id}/promote` | Promote memory (thumbs up) |
| POST | `/api/memories/{id}/demote` | Demote memory (thumbs down, no delete) |
| GET | `/api/memories/{id}/changelog` | Per-memory state-transition audit trail |
| POST | `/api/memories/{id}/review` | Record an FSRS review rating |
| GET | `/api/review/queue` | Items due for review |
| POST | `/api/maintenance/regenerate-embeddings` | Backfill/rebuild embeddings |
| POST | `/api/maintenance/find-duplicates` | Cluster near-duplicates by cosine |
| POST | `/api/maintenance/gc` | Garbage-collect low-retention memories |
| POST | `/api/maintenance/backup` | SQLite backup (WAL checkpoint + copy) |
| GET | `/api/search?q=...` | Search memories |
| GET | `/api/stats` | System statistics |
| GET | `/api/health` | Health check |
| GET | `/api/timeline` | Timeline data |
| GET | `/api/graph` | Graph data (nodes + edges) |
| POST | `/api/dream` | Trigger dream consolidation |
| POST | `/api/explore` | Explore connections |
| POST | `/api/predict` | Proactive prediction |
| POST | `/api/importance` | Score importance |
| POST | `/api/consolidate` | Run FSRS consolidation |
| GET | `/api/retention-distribution` | FSRS retention buckets |
| GET | `/api/intentions` | List intentions |
| POST | `/api/intentions` | Create intention |
| POST | `/api/reflect` | Metacognitive self-reflection (v3.1) |
| POST | `/api/temporal` | Temporal fact versioning (v3.1) |
| POST | `/api/confidence` | Confidence scoring and audit (v3.1) |

### Static SPA
- `GET /dashboard` and `GET /dashboard/{*path}` — Embedded React build via `include_dir!`
- Serves `index.html` for all dashboard routes (SPA fallback)

### WebSocket

`GET /ws` — Upgrades to WebSocket. Subscribes to the `VestigeEvent` broadcast channel.

**Wire protocol:**
1. **Welcome message** — JSON with `"type": "Connected"`, version, timestamp
2. **Event stream** — Each `VestigeEvent` serialized as `{"type": "...", "data": {...}}`
3. **Heartbeats** — Every 5 seconds: uptime, memory count, average retention

**Event types** (`dashboard/events.rs`):

| Event | Triggered by |
|-------|-------------|
| `MemoryCreated` | smart_ingest (create) |
| `MemoryUpdated` | smart_ingest (update/supersede), memory edit |
| `MemoryDeleted` | memory delete |
| `MemoryPromoted` | memory promote |
| `MemoryDemoted` | memory demote |
| `SearchPerformed` | search |
| `DreamStarted` | dream begin |
| `DreamProgress` | dream phase updates |
| `DreamCompleted` | dream finished |
| `ConsolidationStarted` | consolidation begin |
| `ConsolidationCompleted` | consolidation finished |
| `RetentionDecayed` | FSRS decay cycle |
| `ConnectionDiscovered` | dream/explore finds new connection |
| `ActivationSpread` | spreading activation propagation |
| `ImportanceScored` | importance_score |
| `Heartbeat` | periodic (5s) |

---

## Dashboard Frontend

**Stack:** React 19, Vite 6, React Router 7, Three.js, Tailwind CSS 4, TypeScript

### Pages (10)

| Page | Route | Description |
|------|-------|-------------|
| `GraphPage` | `/graph` | 3D neural graph with node selection, detail panel, explore connections |
| `MemoriesPage` | `/memories` | Searchable memory list with FSRS state indicators |
| `TimelinePage` | `/timeline` | Chronological memory browse grouped by day |
| `FeedPage` | `/feed` | Real-time event feed via WebSocket |
| `ExplorePage` | `/explore` | Connection explorer (associations, chains, bridges) + importance scorer |
| `IntentionsPage` | `/intentions` | Prospective memory management |
| `StatsPage` | `/stats` | System statistics, retention distribution, pipeline visualizer |
| `SettingsPage` | `/settings` | Configuration, backup/restore, maintenance, metacognitive tools (reflect, confidence) |
| `TutorialPage` | `/tutorial` | Beginner-friendly guide with analogies, lifecycle, science, FAQ, glossary (EN/PL) |
| `NotFoundPage` | `*` | 404 fallback |

### Three.js Graph Engine

`apps/dashboard/src/graph/` — 9 modules powering the 3D visualization:

| Module | Responsibility |
|--------|---------------|
| `scene.ts` | Scene setup, camera, WebGLRenderer, OrbitControls, EffectComposer, bloom pass, lights, raycaster |
| `nodes.ts` | `NodeManager` — mesh creation, glow sprites, labels, materialize/dissolve animations, type-based colors |
| `edges.ts` | `EdgeManager` — line geometry, grow/dissolve animations |
| `force-sim.ts` | `ForceSimulation` — repulsion, attraction, centering, damping, velocity limits |
| `effects.ts` | `EffectManager` — pulse, spawn burst, rainbow burst, ripple wave, implosion, shockwave, connection flash |
| `particles.ts` | `ParticleSystem` — star field background + neural particle clouds |
| `dream-mode.ts` | `DreamMode` — visual transition between normal and dream states (bloom, fog, nebula, chromatic, vignette) |
| `events.ts` | Maps `VestigeEvent` types to graph mutations (add/remove/update nodes/edges), spawn positions, FIFO cap |
| `temporal.ts` | Date filtering for timeline slider, opacity near cutoff, `retentionAtDate` FSRS decay helper |

---

## Advanced Techniques

### Cross-Project Intelligence
CrossProjectLearner tracks patterns across ALL projects (ErrorHandling, AsyncConcurrency, Testing, Architecture, Performance, Security). Patterns learned in one project become available in all projects.

### Reconsolidation Window
After any memory is accessed, it enters a 5-minute "labile" state where modifications are enhanced (Nader). The system handles this automatically.

### Synaptic Tagging (Retroactive Importance)
Memories encoded in the last 9 hours can be retroactively promoted when something important happens (Frey & Morris 1997). Related memories from the past 9 hours get importance boosts automatically.

### Dream Consolidation
5-stage cycle: Replay → Cross-reference → Strengthen → Prune → Transfer. Uses Waking SWR tagging (70% tagged + 30% random). Generates insights by cross-referencing recent memories with older knowledge. Check `insights_generated` in dream results.

### Metacognitive Tools (v3.1)

Three tools that go beyond automatic consolidation to enable deliberate self-examination:

| Tool | Purpose | Scientific Basis |
|------|---------|-----------------|
| `reflect` | Active self-examination: contradictions, gaps, stale decisions, pattern clusters | Flavell 1979 (metacognition), Schön 1983 (reflection-in-action), Nelson & Narens 1990 (metamemory) |
| `temporal` | Temporal fact versioning: current, expired, history, invalidation | Snodgrass 1999 (bi-temporal), Graphiti/Zep 2024 (temporal knowledge graphs) |
| `confidence` | Multi-dimensional confidence scoring: encoding, retrieval, temporal, evidence | Kahneman 2011 (dual process), Tetlock 2015 (superforecasting), Mercier & Sperber 2017 (argumentative theory) |

Exposed both as MCP tools (stdio/HTTP) and dashboard REST API endpoints (`/api/reflect`, `/api/temporal`, `/api/confidence`).

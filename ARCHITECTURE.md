# Vestige — Architecture Reference

Complete technical reference for Vestige's system architecture. For operational tool usage, see [AGENTS.md](AGENTS.md) (the canonical agent instructions; `CLAUDE.md` and `GEMINI.md` are symlinks to the same file).

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
│  McpServer — 29 tools, event emission                     │
├───────────────────────────────────────────────────────────┤
│  CognitiveEngine (Arc<Mutex<_>>)                          │
│  Stateful cognitive modules: FSRS-6, spreading            │
│  activation, dreaming, synaptic tagging, …                │
├───────────────────────────────────────────────────────────┤
│  Storage (Arc<_>)                                         │
│  SQLite WAL + FTS5 + USearch HNSW + Nomic Embed v1.5      │
│  Jina Reranker v2 Base Multilingual cross-encoder         │
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
│   │       │                  # intentions, maintenance, embeddings, fsrs_personalization,
│   │       │                  # review, consolidation, search, graph, gdpr, temporal,
│   │       │                  # smart_ingest, insights, records, stats),
│   │       │                  # migrations v1–v18, WAL, FTS5
│   │       ├── memory/        # Node types, FSRS strength, temporal, typed-memory MemoryKind
│   │       ├── fsrs/          # Algorithm, scheduler, optimizer
│   │       ├── embeddings/    # Nomic v1.5 local ONNX, hybrid, code embeddings
│   │       ├── preprocessing/ # Content intelligence pipeline (entities, coref, temporal, relations, provenance)
│   │       ├── nlp/           # Pluggable detectors: ContradictionDetector, OpinionDetector,
│   │       │                  # FutureRelevanceDetector. NegEx scope, EN/PL lexicons,
│   │       │                  # eval harness, baseline tests.
│   │       ├── search/        # Hybrid, vector, keyword (BM25), reranker, temporal, HyDE, decompose
│   │       ├── neuroscience/  # Spreading activation, memory states, hippocampal index,
│   │       │                  # synaptic tagging, importance signals, predictive retrieval,
│   │       │                  # emotional memory, context memory, prospective memory
│   │       ├── advanced/      # Dreams (6 sub-modules: lifecycle, clustering, connections,
│   │       │                  # contradictions, hubs, insights), reconsolidation,
│   │       │                  # prediction error, intent detection, importance, compression,
│   │       │                  # chains, cross-project, adaptive embedding, speculative retrieval
│   │       ├── consolidation/ # Phases, sleep
│   │       ├── codebase/      # Code patterns, git, relationships, watcher
│   │       └── fts.rs         # FTS5 query sanitization
│   ├── vestige-mcp/           # MCP server binary + dashboard embedding
│   │   └── src/
│   │       ├── main.rs        # CLI, init, startup sequence
│   │       ├── lib.rs         # Public crate surface
│   │       ├── server.rs      # McpServer — JSON-RPC dispatch, tool routing, event emission
│   │       ├── server/        # catalog.rs — canonical 28-tool / 11-resource list
│   │       │                  # (b15 split, drift-guarded by check-version-and-tools.sh)
│   │       ├── cognitive.rs   # CognitiveEngine wrapper
│   │       ├── telemetry.rs   # OTLP scaffolding behind `telemetry` feature (no-op default)
│   │       ├── protocol/      # stdio.rs, http.rs, messages.rs, auth.rs, timeout.rs
│   │       ├── dashboard/     # mod.rs (Axum router), wire/ (ts-rs DTOs — the contract),
│   │       │                  # handlers/ (memory, search, graph, history, intentions,
│   │       │                  # maintenance, review, cognitive, metacognitive,
│   │       │                  # observability, decisions, hubs, insights, pages),
│   │       │                  # websocket.rs, events.rs, state.rs, static_files.rs
│   │       ├── tools/         # One file (or submodule) per MCP tool (29 tools)
│   │       ├── resources/     # MCP resources (memory.rs, codebase.rs)
│   │       └── bin/           # cli/ (vestige CLI). vestige-restore is its own crate.
│   └── vestige-restore/       # Standalone restore binary (no fastembed/USearch deps)
├── apps/
│   └── dashboard/             # React 19 + Vite 6 + React Router 7 + Three.js
│       ├── src/
│       │   ├── main.tsx       # Entry point
│       │   ├── App.tsx        # React Router setup (16 user-facing routes + 404 fallback)
│       │   ├── app.css        # Global styles (Tailwind 4, OKLCH design tokens)
│       │   ├── pages/         # 16 user-facing pages + NotFound — see "Pages" section below
│       │   ├── components/    # Layout, Graph3D, PipelineVisualizer, TimeSlider,
│       │   │                  # DecisionCard, HubCard, InsightCard, ConfirmDialog,
│       │   │                  # GraphHelpOverlay, RetentionCurve, MaintenancePanel, …
│       │   ├── stores/        # api.ts, websocket.ts, toast.tsx, telemetry.ts,
│       │   │                  # confirm.ts, dialogs.ts (Zustand)
│       │   ├── types/         # generated/ (ts-rs from Rust DTOs) + runtime.ts (Zod) + index.ts
│       │   ├── hooks/         # useMemoryMutations, use-dashboard-limits, use-graph-keyboard,
│       │   │                  # use-multi-select, use-selection-history, …
│       │   ├── i18n/          # en.json, pl.json (i18next)
│       │   └── graph/         # Three.js graph engine — see "Three.js Graph Engine" below
│       ├── build/             # Production build (embedded in Rust binary via include_dir!)
│       └── vite.config.ts     # Aliases: @ → src, @graph → src/graph
├── tests/e2e/                 # E2E test crate (cognitive, journeys, extreme, mcp protocol)
├── benchmarks/locomo/         # LoCoMo benchmark harness (vestige-locomo-bench)
├── packages/                  # NPM distribution + .mcpb bundle
├── docs/                      # FAQ, science, storage, integrations, design RFCs
└── .github/                   # CI workflows (test.yml, release.yml, supply-chain.yml)
```

> **Dashboard layout note:** `apps/dashboard/src/lib/` holds framework-agnostic TypeScript helpers (`utils.ts`, `i18n.ts`, `concurrency.ts`) shared across pages and components. React-specific code lives in `src/graph/`, `src/components/`, and `src/stores/`. Do not remove `src/lib/` — it is actively imported via the `@/lib/...` alias.
>
> **Wire contract:** `apps/dashboard/src/types/generated/` is **auto-generated** from `crates/vestige-mcp/src/dashboard/wire/` via ts-rs. Never edit by hand. The CI gate `scripts/check-generated-types.sh` fails any PR that changes a Rust DTO without committing the regenerated `.ts` companion. See [AGENTS.md → "Adding a Type-Safe Dashboard Endpoint"](AGENTS.md#adding-a-type-safe-dashboard-endpoint).

---

## Cognitive Engine

### Search Pipeline (8 stages)

0. **Decompose** — compound queries split into sub-queries (semicolons, question chains, conjunctions), searched independently, merged via max-score dedup. Mitigates the single-embedding-per-multi-topic-query failure mode; no formal effect-size benchmark on production traffic.
1. **Overfetch** — 3x results from triple hybrid search (BM25 keyword + semantic vector + Reciprocal Rank Fusion)
   - Embeddings: nomic-embed-text-v1.5, 768D → 384D Matryoshka, 8K token context. The reduction is the model card's order — `layer_norm` over the full 768D, *then* the slice, *then* L2 — and documents/queries carry the mandated `search_document:`/`search_query:` task prefixes by default (`VESTIGE_NOMIC_PREFIXES=0` selects the legacy raw regime). Both the recipe and the prefix regime change the numbers, so the module versions them (`EMBEDDING_SPACE_VERSION`, currently 2) and every stored vector records its space in `knowledge_nodes.embedding_model` (`nomic-embed-text-v1.5+v2+prefix`); a store holding an older version must be re-embedded with `regenerate_embeddings` (`force: true`)
   - Vector index: USearch HNSW
2. **Rerank** — Cross-encoder rescoring (Jina Reranker v2 Base Multilingual, 278M params)
3. **Temporal** — Recency + validity window boosting (85% relevance + 15% temporal)
4. **Accessibility** — FSRS-6 retention filter. This stage buckets on raw `retention_strength` (`search_unified/pipeline/scoring.rs`): >0.7 Active, >0.3 Dormant, >0.1 Silent. Note the 0.3 cut-off for Dormant — `memory(action="state")` classifies on the accessibility composite with 0.4 instead (see [Memory States](#memory-states)).
5. **Context** — Tulving 1973 encoding specificity (topic overlap via `context_topics` → up to +30% boost)
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

### Cognitive Modules

`CognitiveEngine` holds the stateful modules; the engine is shared as `Arc<Mutex<_>>` across MCP and dashboard call sites. Modules group into three families:

**Neuroscience:**
ActivationNetwork (Collins & Loftus 1975), SynapticTaggingSystem (Frey & Morris 1997), HippocampalIndex (Teyler & Rudy 2007), ContextMatcher (Tulving 1973), AccessibilityCalculator, CompetitionManager (Anderson 1994), StateUpdateService, ImportanceSignals (Novelty / Arousal / Reward / Attention sub-signals), EmotionalMemory (Brown & Kulik 1977), PredictiveMemory, ProspectiveMemory, IntentionParser.

**Advanced:**
ImportanceTracker, ReconsolidationManager (Nader — 5-minute labile window), IntentDetector, ActivityTracker, DreamEngine (canonical 4-phase NREM1 → NREM3 → REM → Integration cycle), MemoryDreamer (6 sub-phases — connections, clustering, contradictions, insights, hubs, strengthen/compress — insight/hub/contradiction synthesis), MemoryChainBuilder, MemoryCompressor, CrossProjectLearner, AdaptiveEmbedder, SpeculativeRetriever, ConsolidationScheduler (inline replay-style FSRS consolidation), PredictionErrorGate.

**Search:** Reranker (Jina Reranker v2), TemporalSearcher, CompoundQueryDecomposer.

> The exact module count drifts across releases as sub-modules split or merge — the canonical list lives in `crates/vestige-mcp/src/cognitive.rs::CognitiveEngine`. The CI metadata gate verifies the **MCP tool count** (28); module counts are intentionally not pinned.

### Memory States

Two classifiers bucket memories into the same four states, and they do **not** currently share thresholds. This is a known divergence, documented rather than hidden:

**1. Accessibility composite** — the canonical classifier behind `memory(action="state")`, the dashboard and `tools/memory_unified/helpers.rs` (`ACCESSIBILITY_ACTIVE = 0.7`, `ACCESSIBILITY_DORMANT = 0.4`, `ACCESSIBILITY_SILENT = 0.1`). It buckets the weighted composite `accessibility = 0.5 × retention + 0.3 × retrieval + 0.2 × storage`:

- **Active** (accessibility ≥ 0.7) — easily retrievable
- **Dormant** (≥ 0.4) — retrievable with effort
- **Silent** (≥ 0.1) — difficult, needs cues
- **Unavailable** (< 0.1) — needs reinforcement

**2. Raw retention** — the search pipeline (`tools/search_unified/pipeline/scoring.rs`) and consolidation snapshots (`storage/sqlite/consolidation.rs`) bucket on `retention_strength` alone with a **0.3** Dormant cut-off (>0.7 Active, >0.3 Dormant, >0.1 Silent). A memory with `retention_strength = 0.35` therefore reads as Dormant in `memory(action="state")` but as Silent in search scoring and consolidation stats. Unifying the two thresholds is tracked as a follow-up; until then, quote the classifier you mean.

### Connection Types

semantic, temporal, causal, spatial, part_of, user_defined — each with strength (0-1), activation_count, timestamps

---

## Storage Layer

**`crates/vestige-core/src/storage/`**

| Component | Details |
|-----------|---------|
| **SQLite** | WAL mode, reader/writer connection split, PRAGMA optimizations |
| **FTS5** | Full-text search with porter tokenizer, page_size tuning |
| **Migrations** | Versions 1–15. FSRS, embeddings, neuroscience tables, graph/scopes, FSRS-6 upgrade, dream history, FTS5, autonomic fields, emotional/temporal hierarchy, V10 provenance tracking, V11 typed memory `MemoryKind`, V12 typed-memory tables (`decisions`, `hubs`, `insights` + foreign-key indexes), V13 tier/confidence columns on `insights` and `decisions`, V14 `auto_vacuum=INCREMENTAL` (reclaims deleted pages without a full `VACUUM`; runs outside a transaction, see `migrations/runner.rs`), V15 FTS5 tokenizer `porter unicode61 remove_diacritics 2` (accent folding, non-ASCII tokens). All forward-only and idempotent. |
| **Vector index** | USearch HNSW, feature-gated behind `vector-search` |
| **Embeddings** | Nomic Embed v1.5 via fastembed (local ONNX), feature-gated behind `embeddings`. Embedding space is versioned (`crates/vestige-core/src/embeddings/local.rs`: `EMBEDDING_SPACE_VERSION`, `embedding_space_fingerprint()`, `embedding_model_tag()`); version 2 = LayerNorm→slice→L2 plus task prefixes on by default. The HNSW sidecar meta does **not** yet include the space fingerprint — it validates `(COUNT(*), MAX(created_at))` only — so an upgraded binary can reload a sidecar built from the previous space until the store is re-embedded |
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

### REST API Endpoints

42 route registrations total (`dashboard/mod.rs`): 36 of them sit under `/api/*` and cover **34 distinct REST paths** (`/api/memories/{id}` is registered three times — GET/DELETE/PATCH), plus 4 page/SPA routes (`/dashboard`, `/dashboard/{*path}`, `/`, `/graph`), 1 WebSocket route (`/ws`) and 1 Prometheus scrape endpoint (`/metrics`). Every response body is a `Json<T>` of a ts-rs DTO from `dashboard/wire/`, except `/metrics`, which is Prometheus text exposition v0.0.4; the dashboard re-validates the five highest-blast-radius endpoints with Zod at runtime.

**Memory CRUD**

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/memories` | List memories (paginated, filterable) |
| GET | `/api/memories/{id}` | Get single memory |
| DELETE | `/api/memories/{id}` | Delete memory |
| PATCH | `/api/memories/{id}` | Update memory content/tags (preserves FSRS state) |
| POST | `/api/smart_ingest` | Smart ingest with Prediction Error Gating |
| POST | `/api/memories/{id}/promote` | Promote memory (thumbs up) |
| POST | `/api/memories/{id}/demote` | Demote memory (thumbs down — does NOT delete) |
| GET  | `/api/memories/{id}/changelog` | Per-memory state-transition audit trail |
| POST | `/api/memories/{id}/review` | Record an FSRS review rating |
| GET  | `/api/review/queue` | Items due for review |

**Search & Discovery**

| Method | Path | Description |
|--------|------|-------------|
| GET  | `/api/search?q=...` | 8-stage cognitive search |
| GET  | `/api/timeline` | Timeline data |
| GET  | `/api/graph` | Graph data (nodes + edges) |
| POST | `/api/explore` | Explore connections (chain, associations, bridges, causal_chain) |
| POST | `/api/predict` | Proactive prediction |
| GET  | `/api/retention-distribution` | FSRS retention buckets |

**Typed Memory (v3.4)**

| Method | Path | Description |
|--------|------|-------------|
| GET  | `/api/decisions` | Decision Matrix entries (v2 structured decisions) |
| GET  | `/api/insights` | Tentative insights from dream + reflect |
| GET  | `/api/hubs` | Auto-detected topic clusters |
| POST | `/api/deep_reference` | Cognitive reasoning engine (cross_reference alias) |

**Intentions**

| Method | Path | Description |
|--------|------|-------------|
| GET  | `/api/intentions` | List intentions |
| POST | `/api/intentions` | Create intention |

**Cognitive Engine**

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/dream` | Trigger dream consolidation (per-phase breakdown) |
| POST | `/api/consolidate` | Run FSRS consolidation cycle |
| POST | `/api/importance` | Score importance (4-channel model) |

**Metacognitive (v3.1)**

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/reflect` | Metacognitive self-reflection |
| POST | `/api/temporal` | Temporal fact versioning |
| POST | `/api/confidence` | Confidence scoring and audit |

**Maintenance**

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/maintenance/regenerate-embeddings` | Backfill/rebuild embeddings |
| POST | `/api/maintenance/find-duplicates` | Cluster near-duplicates by cosine |
| POST | `/api/maintenance/gc` | Garbage-collect low-retention memories |
| POST | `/api/maintenance/backup` | SQLite backup (WAL checkpoint + copy) |
| POST | `/api/maintenance/erase` | GDPR Art. 17 erasure of one memory (`action: "memory", id`) or an exact tag (`action: "tag", tag`); dry-run by default, destructive pass requires `confirmed: true` |

**Observability & Meta**

| Method | Path | Description |
|--------|------|-------------|
| GET  | `/api/stats` | System statistics |
| GET  | `/api/health` | Health check — 200 for `healthy`/`degraded`/`empty`, **503 for `critical`** (average retention < 0.3), with the same `HealthCheckDto` body either way |
| GET  | `/api/_meta/limits` | Dashboard caps (page sizes, search timeouts, fetch budgets) |
| GET  | `/metrics` | Prometheus text exposition v0.0.4: store gauges, per-stage `try_lock` skip counters, WS subscribers, uptime |

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

### Pages (16 user-facing + 404 fallback)

| Page | Route | Description |
|------|-------|-------------|
| `GraphPage` | `/graph` | 3D neural graph with node selection, detail panel, explore connections, keyboard navigation |
| `MemoriesPage` | `/memories` | Searchable memory list with FSRS state indicators, multi-select |
| `ReviewPage` | `/review` | FSRS review queue — rate memories due for retention |
| `BriefingPage` | `/briefing` | Session-start briefing — recent activity, due intentions, decaying memories |
| `TimelinePage` | `/timeline` | Chronological memory browse grouped by day |
| `FeedPage` | `/feed` | Real-time event feed via WebSocket |
| `ExplorePage` | `/explore` | Connection explorer (associations, chains, bridges, causal chains) + importance scorer |
| `ReasoningPage` | `/reasoning` | `deep_reference` cognitive reasoning engine — evidence, contradictions, supersession, evolution timeline |
| `DecisionsPage` | `/decisions` | Decision Matrix entries (v2 structured decisions: question, choices, criteria, 1–5 scoring, validUntil) |
| `InsightsPage` | `/insights` | Tentative insights emitted by dream cycles and `reflect` runs (validate/dismiss workflow) |
| `HubsPage` | `/hubs` | Auto-detected topic clusters — high in-degree concepts in the knowledge graph |
| `IntentionsPage` | `/intentions` | Prospective memory management |
| `TemporalPage` | `/temporal` | Bi-temporal fact versioning UI — current / expired / history / invalidate |
| `StatsPage` | `/stats` | System statistics, retention distribution, pipeline visualizer |
| `SettingsPage` | `/settings` | Configuration, backup/restore, maintenance, metacognitive tools (reflect, confidence) |
| `TutorialPage` | `/tutorial` | Beginner-friendly guide — 12 modular components, interactive FSRS curve, glossary tooltips, EN/PL |
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
The `dream` tool and `/api/dream` run two engines over the selected memory set (70% Waking-SWR-tagged + 30% random):
- **DreamEngine** — the canonical 4-phase NREM-grounded cycle: NREM1 triage → NREM3 consolidation (synaptic downscaling, tunable via `VESTIGE_NREM3_DOWNSCALE_FACTOR`) → REM creative pairing → Integration (Diekelmann & Born 2010, Stickgold & Walker 2013, Tononi & Cirelli 2006).
- **MemoryDreamer** — runs alongside for backward-compatible synthesis across the same set: 6 sub-phases (connections → clustering → contradictions → insights → hubs → strengthen/compress). Emits insights, Topic-Hub candidates, and contradiction flags. Check `insights_generated` in dream results.

A separate **ConsolidationScheduler** drives the inline replay-style FSRS consolidation (replay → cross-reference → strengthen → prune → transfer) — distinct from the dream tool; see `advanced/dreams/`.

### Metacognitive Tools (v3.1)

Three tools that go beyond automatic consolidation to enable deliberate self-examination:

| Tool | Purpose | Scientific Basis |
|------|---------|-----------------|
| `reflect` | Active self-examination: contradictions, gaps, stale decisions, pattern clusters | Flavell 1979 (metacognition), Schön 1983 (reflection-in-action), Nelson & Narens 1990 (metamemory) |
| `temporal` | Temporal fact versioning: current, expired, history, invalidation | Snodgrass 1999 (bi-temporal), Graphiti/Zep 2024 (temporal knowledge graphs) |
| `confidence` | Heuristic multi-dimensional confidence scoring (encoding/retrieval/temporal/evidence); `calibrate` does a retention-based consistency check rather than Brier-style calibration | Inspired by Kahneman 2011 (dual process), Tetlock 2015 (superforecasting), Mercier & Sperber 2017 (argumentative theory); implementation is a weighted heuristic, **not** a calibration procedure |

Exposed both as MCP tools (stdio/HTTP) and dashboard REST API endpoints (`/api/reflect`, `/api/temporal`, `/api/confidence`).

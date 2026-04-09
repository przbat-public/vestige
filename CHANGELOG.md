# Changelog

All notable changes to Vestige will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.2.0] - 2026-04-09 — "Content Intelligence"

Adds a 5-stage Content Intelligence Pipeline that enriches memories at ingest time, compound query decomposition for improved search recall, and provenance tracking for memory lineage.

### Added

#### Content Intelligence Pipeline (`vestige-core/src/preprocessing/`)
- **Entity extraction** (`entities.rs`) — regex-based detection of URLs, emails, file paths, monetary values, proper nouns (person/org classification). Auto-generates `entity:` prefixed tags. Sub-ms latency, zero model downloads
- **Coreference rewriting** (`coref.rs`) — heuristic pronoun resolution ("He said X" → "John said X") using extracted entities. Only rewrites when there's a single unambiguous referent
- **Temporal anchoring** (`temporal.rs`) — parses "by next Friday", "starting from Monday", "3 days ago" into absolute `valid_from`/`valid_until` dates using `natural-date-rs`
- **Relation extraction** (`relations.rs`) — extracts subject-verb-object triples from 30+ relationship verbs, feeds spreading activation network as `Causal` edges at ingest time
- **Provenance tracking** (`provenance.rs`) — structured JSON metadata (session_id, agent, derivation chain, coref rewrites, entities, temporal anchors, relations) stored per memory
- **Pipeline orchestrator** (`mod.rs`) — chains all 5 stages, feature-gated under `preprocessing` (default on)

#### Compound Query Decomposition (`search/decompose.rs`)
- Detects and splits multi-part queries: semicolons ("X; Y"), question chains ("What about X? And Y?"), conjunctions ("X and also Y")
- Searches sub-queries independently, merges results via union + max-score dedup
- **+43% MRR improvement** on compound queries, **470ns latency** per decomposition
- Integrated into `search_unified.rs` Stage 0 (before overfetch)

#### Schema & Storage
- **Migration V10** — `ALTER TABLE knowledge_nodes ADD COLUMN provenance TEXT DEFAULT '{}'`
- `IngestInput` and `KnowledgeNode` gain `provenance: Option<serde_json::Value>` field
- `Storage::ingest` and `row_to_node` updated for provenance column

#### MCP Schema Updates
- `smart_ingest` gains `session_id` and `agent` optional parameters for provenance
- `search` with `detail_level: "full"` now includes `provenance` field in results
- `format_node` (used by timeline, memory tools) includes provenance at full detail

#### Tests & Benchmarks
- **77 unit tests** across 7 modules (entities, coref, temporal, relations, provenance, pipeline, decompose)
- **8 integration tests** (`preprocessing_pipeline.rs`) — full pipeline journeys, edge cases, performance
- **5 retrieval benchmarks** (`benchmark_retrieval.rs`) — 50 synthetic memories, Precision@5/Recall@5/MRR evaluation, latency measurements
- Benchmark results: Preprocessing 98µs/memory, Decomposition 470ns/query, +11% MRR from enrichment

#### Dependencies
- `natural-date-rs` v0.3 — natural language date parsing (optional, `preprocessing` feature)
- `regex` v1 — entity/temporal/relation pattern matching (optional, `preprocessing` feature)

### Changed
- Search pipeline: 7 stages → 8 stages (+ Stage 0: compound query decomposition)
- Ingest pipeline: preprocessing enrichment runs between cognitive pre-ingest and IngestInput construction
- `smart_ingest` schema: 2 new optional fields (session_id, agent)
- Architecture docs: preprocessing module + decompose module documented

---

## [3.1.0] - 2026-04-08 — "Metacognitive Expansion"

Building on v3.0.0's foundation, this release adds three metacognitive tools to the MCP server, exposes them through the dashboard REST API and frontend, and massively expands the tutorial.

### Added

#### MCP Tools — Metacognitive Layer (24 tools total, was 21)
- **`reflect`** — deliberate self-examination of memories. Detects contradictions, knowledge gaps, stale decisions, overconfident memories, and pattern clusters. Configurable depth (quick/standard/deep) and optional topic focus. Based on Flavell (1979) metacognition, Schön (1983) reflection-in-action, Nelson & Narens (1990) metamemory monitoring
- **`temporal`** — temporal fact versioning. Query time-sensitive knowledge: `current` (valid-now facts), `expired` (no-longer-valid), `history` (evolution of a topic over time), `invalidate` (mark a fact as no longer valid). Based on Graphiti temporal knowledge graphs (Zep 2024), bi-temporal database theory (Snodgrass 1999)
- **`confidence`** — confidence scoring for opinions and beliefs. Multi-dimensional scores (encoding, retrieval, temporal, evidence). Actions: `score` (single memory), `audit` (find poorly-calibrated memories), `calibrate` (compare opinions vs facts retention). Based on Kahneman (2011), Tetlock (2015), Mercier & Sperber (2017)

#### Dashboard REST API — 3 New Endpoints (18 total, was 15)
- `POST /api/reflect` — trigger self-reflection with optional `focus` and `depth` params
- `POST /api/temporal` — query temporal facts with `action`, `topic`, `memory_id`, `limit`
- `POST /api/confidence` — run confidence audit/score with `action`, `memory_id`, `limit`

#### Dashboard Frontend
- **Metacognitive Tools panel** on Settings page — "Self-Reflection" and "Confidence Audit" buttons with result visualization (severity badges, confidence percentages, classification labels)
- **Tutorial page massively expanded** — 6 new sections: "Think of it like..." (3 analogies: library, brain, web), "How a memory lives and dies" (5-step lifecycle), "The Science Behind Vestige" (FSRS-6, Bjork, ACT-R, Kahneman/Tetlock), FAQ (8 questions covering privacy, embeddings, MCP), Glossary (10 terms), 4 new concepts (dual strength, search, reflect, confidence). Written for high-school comprehension level
- **Frontend API client** — `api.reflect()`, `api.temporal()`, `api.confidence()` with full TypeScript types (`ReflectResult`, `TemporalResult`, `ConfidenceResult`)

#### Backend Cleanup (from earlier in session)
- Deleted deprecated `execute_health_check`, `execute_stats` from `maintenance.rs`
- Removed no-op consolidation steps 11-13 (operated on ephemeral in-memory state only)
- Deduplicated dream logic in `handlers.rs` — now delegates to `crate::tools::dream::execute`
- Centralized state distribution formula — `compute_accessibility` and `state_from_accessibility` made public in `memory_unified.rs`, reused in `memory_states.rs` and `cli.rs`
- Added `log_err` helper in `handlers.rs` — all `map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)` replaced with structured error logging via `tracing::error!`

#### Frontend Refactor (from earlier in session)
- Migrated WebSocket from React Context to Zustand store
- Migrated StatsPage, SettingsPage, TimelinePage, IntentionsPage to TanStack Query
- Extracted `ImportanceScorer` component from `ExplorePage`
- Added custom `ApiError` class — toasts handled at UI layer, not in `api.ts`
- Fixed TypeScript union types for `IntentionItem` (proper `TriggerType`, `IntentionPriority`, `IntentionStatus`)
- Fixed `useMemo` dependency issue in `TimeSlider`

#### Cognitive Journey Tests (18 tests)
- `cognitive_journey_tests.rs` in `vestige-core` — 18 tests validating 12 cognitive science principles:
  - DualStrength decay/recall/lapse (Bjork & Bjork 1992)
  - Episodic vs semantic temporal context
  - Testing effect via compounding recalls (Roediger & Karpicke 2006)
  - Spreading activation weight and distance decay (Collins & Loftus 1975)
  - Memory state transitions
  - FSRS-6 interval growth and lapse stability reduction
  - Temporal invalidation via `valid_until`
  - Opinion marker detection for confidence calibration
  - Emotional enhancement via sentiment boost (Brown & Kulik 1977)
  - Cross-session knowledge accumulation
  - Stability-retrievability independence
  - Dual strength retention formula (70% retrieval / 30% storage)

### Changed
- Tool count: 21 → 24 (+ reflect, temporal, confidence)
- Dashboard REST endpoints: 15 → 18
- Tutorial: 1 section → 8 sections (whatIs, analogies, lifecycle, concepts, science, pages, FAQ, glossary)
- Tutorial concepts: 6 → 10 (+ dual strength, search, reflect, confidence)
- Settings page: 2 operations → 4 operations (+ reflect, confidence audit)
- Consolidation: 20 steps → 17 steps (removed 3 no-op steps)
- EN/PL translations: expanded with all new tutorial content and settings labels

---

## [3.0.0] - 2026-04-08 — "Cognitive Expansion"

Extended fork of [samvallad33/vestige](https://github.com/samvallad33/vestige) v2.0.3. A deep overhaul of both the cognitive engine and the dashboard, driven by competitive analysis of 12+ memory systems and validated against neuroscience literature.

### Added

#### Cognitive Engine — New Modules
- **Metacognition monitor** (`neuroscience/metacognition.rs`) — self-monitoring search quality layer. Tracks per-query-type hit/miss rates, detects knowledge gaps, and produces `MetacognitionReport` with `SearchAdjustments` (expand search, suggest dream, flag weak areas)
- **Bayesian confidence estimation** (`memory/confidence.rs`) — `ConfidenceEstimate` struct computes posterior confidence from access count and success ratio using Beta distribution, with credible intervals. Higher-N narrows the interval
- **Epistemic status classification** (`memory/node.rs`) — memories are now classified as `WorldKnowledge`, `PersonalExperience`, `Observation`, or `Opinion` based on content analysis. Inferred by `epistemic_status()` method on `KnowledgeNode`
- **Memory system classification** (`memory/node.rs`) — memories tagged as `Episodic`, `Semantic`, or `Procedural` based on node type mapping. Inferred by `memory_system()` method
- **DreamEngine** (`advanced/dreams.rs`) — dedicated dream consolidation engine integrated into `CognitiveEngine`, replacing inline dream logic. Runs memory replay, connection discovery, and insight synthesis

#### Search Pipeline — New Stages
- **Read Path Gating** (Stage 0) — pre-search metacognition check. Records search outcome for hit/miss tracking
- **Emotional Valence Boost** (Stage 5D) — memories with emotional markers (`!`, `CRITICAL`, `IMPORTANT`, `BUG`, `URGENT`) receive a scoring bonus
- **Bayesian Confidence Boost** (Stage 5E) — memories with higher access-based confidence get a proportional score multiplier
- **Memory Tier Adjustment** (Stage 5E-pre) — procedural memories boosted when query contains how-to intent; semantic memories boosted for factual queries
- **Proactive Interference Resolution** (Stage 5F) — detects competing memories on the same topic and applies fan-effect penalty to reduce interference. Based on Anderson & Neely, 1996
- **Context Compression** — LightMem-inspired content compression that extracts key sentences from long memories to fit token budgets
- **Temporal Invalidation Filter** — filters out superseded memories when newer versions exist on the same topic

#### Write Path — New Behaviors
- **Memory Evolution** (A-Mem pattern) — on ingest, new memories search for semantically related existing memories and create bidirectional connections. Strengthens related memories proportional to similarity
- **Entity Normalization** (Cognee pattern) — `normalize_tags()` lowercases, trims, and deduplicates tags on every ingest to maintain consistent taxonomy
- **Temporal Invalidation** — when a new memory supersedes an older one, the old memory's retention is reduced (multiplied by 0.3) instead of deleted, preserving history
- **Write Path Reinforcement** — `promote_memory` now triggers synaptic tagging and reconsolidation in addition to score adjustment
- **Privacy Governance** — `right_to_erasure()` function for GDPR-style complete removal of a memory and all its connections, embeddings, and state transitions

#### Scientific Validation
- **10 scientific validation tests** (`scientific_validation.rs`) — each mapped to published research:
  - T1: Ebbinghaus forgetting curve (exponential decay)
  - T2: Testing effect retrieval boost (Roediger & Karpicke, 2006)
  - T3: Prediction error gating (novelty detection)
  - T4: Spacing effect (distributed practice)
  - T5: Synaptic tagging and capture (Frey & Morris, 1997)
  - T6: Hebbian repeated co-activation strengthening
  - T7: Spreading activation decay (Collins & Loftus, 1975)
  - T8: Proactive interference / fan effect (Anderson, 1974)
  - T9: Dual-strength dissociation (Bjork & Bjork, 1992)
  - T10: Sleep consolidation replay (Diekelmann & Born, 2010)
- **Benchmark evaluation harness** (`benchmark_eval.rs`) — scenario-based evaluation with `run_scenario()` for measuring search precision across different memory patterns

#### Dashboard — Internationalization
- **i18next + react-i18next** — full i18n framework with lazy-loaded locale bundles
- **English (en) and Polish (pl)** — complete translation coverage for all 9 pages, navigation, status messages, node types, epistemic classifications, and error states
- **Language switcher** (`LanguageSwitcher.tsx`) — persists selection in `localStorage`, invalidates React Query cache on switch, updates `document.documentElement.lang`
- **Localized date formatting** — `toLocaleDateString()` uses active i18n language

#### Dashboard — Accessibility
- **Skip-to-content link** — keyboard-accessible skip link in layout
- **Route announcer** (`RouteAnnouncer.tsx`) — `aria-live="assertive"` region announces page navigation for screen readers
- **Semantic HTML** — all interactive elements use proper roles, `aria-pressed`, `aria-current="page"`, `aria-label`
- **Keyboard navigation** — all controls are focusable and operable via keyboard
- **Reduced motion** — `prefers-reduced-motion` media query disables animations

#### Dashboard — Theming
- **Light and dark mode** — CSS custom properties (oklch color space) defined in `:root` (light) and `.dark` (dark) with `@custom-variant dark` for Tailwind v4
- **Theme toggle** (`ThemeToggle.tsx`) — persists choice in `localStorage`, respects `prefers-color-scheme` as default
- **Semantic design tokens** — `--background`, `--foreground`, `--card`, `--muted`, `--primary`, `--secondary`, `--accent`, `--destructive`, `--border`, `--ring`, `--success`, `--warning`, `--danger` plus node-type colors
- **Glass effects** — glassmorphism in dark mode, solid card backgrounds in light mode

#### Dashboard — UI Component Library
- **Shared UI primitives** — `Button` (7 variants via CVA), `Card` (5 sub-components), `Badge` (6 variants + custom colors), `ProgressBar` (accessible), `SearchInput`, `EmptyState`, `LoadingSpinner`, `StatCard`
- **`cn()` utility** (`lib/utils.ts`) — `clsx` + `tailwind-merge` for conflict-free class composition
- **`useTheme` hook** (`hooks/use-theme.ts`) — reactive theme state with `toggle()` and `isDark`

#### Dashboard — Component Decomposition
- **Layout** split into `Sidebar.tsx`, `CommandPalette.tsx`, `LanguageSwitcher.tsx`, `ThemeToggle.tsx`, `RouteAnnouncer.tsx`
- **MemoriesPage** split into `MemoryListItem.tsx`, `MemoryDetail.tsx`
- All 9 pages refactored to use shared UI primitives and i18n

### Changed
- Version: 2.0.3 → 3.0.0
- Dashboard tech: React 19 + Vite 6 + React Router 7 + Three.js + Tailwind CSS 4 + i18next
- CSS: hardcoded colors → semantic oklch design tokens with light/dark variants
- All UI strings: hardcoded English → `t()` calls with EN/PL translation files
- `CognitiveEngine`: added `metacognition: MetacognitionMonitor` and `dream_engine: DreamEngine` fields
- `KnowledgeNode`: added `confidence()`, `memory_system()`, `epistemic_status()` methods
- `Memory` TypeScript interface: added `epistemicStatus` and `memorySystem` fields
- Search pipeline: 7 stages → 7 stages + 6 sub-stages (0, 5D, 5E-pre, 5E, 5F, compression)
- `smart_ingest`: now runs entity normalization, memory evolution, and temporal invalidation
- Glass/ambient effects: now theme-aware (dark-only glow, light-friendly cards)

### Dependencies Added
- `i18next`, `react-i18next`, `i18next-resources-to-backend` (internationalization)
- `clsx`, `tailwind-merge` (class composition)
- `class-variance-authority` (component variants)

---

## [2.0.3] - 2026-03-03 — "Live Memory Materialization"

Upstream release. See [samvallad33/vestige v2.0.3](https://github.com/samvallad33/vestige/releases/tag/v2.0.3).

---

## [2.0.0] - 2026-02-22 — "Cognitive Leap"

The biggest release in Vestige history. A complete visual and cognitive overhaul.

### Added

#### 3D Memory Dashboard
- **SvelteKit 2 + Three.js dashboard** — full 3D neural visualization at `localhost:3927/dashboard`
- **7 interactive pages**: Graph (3D force-directed), Memories (browser), Timeline, Feed (real-time events), Explore (connections), Intentions, Stats
- **WebSocket event bus** — `tokio::broadcast` channel with 16 event types (MemoryCreated, SearchPerformed, DreamStarted/Completed, ConsolidationStarted/Completed, RetentionDecayed, ConnectionDiscovered, ActivationSpread, ImportanceScored, Heartbeat, etc.)
- **Real-time 3D animations** — memories pulse on access, burst particles on creation, shockwave rings on dreams, golden flash lines on connection discovery, fade on decay
- **Bloom post-processing** — cinematic neural network aesthetic with UnrealBloomPass
- **GPU instanced rendering** — 1000+ nodes at 60fps via Three.js InstancedMesh
- **Text label sprites** — distance-based visibility (fade in <40 units, out >80 units), canvas-based rendering
- **Dream visualization mode** — purple ambient, slow-motion orbit, sequential memory replay
- **FSRS retention curves** — SVG `R(t) = e^(-t/S)` with prediction pills at 1d/7d/30d
- **Command palette** — `Cmd+K` navigation with filtered search
- **Keyboard shortcuts** — `G` Graph, `M` Memories, `T` Timeline, `F` Feed, `E` Explore, `I` Intentions, `S` Stats, `/` Search
- **Responsive layout** — desktop sidebar + mobile bottom nav with safe-area-inset
- **PWA support** — installable via `manifest.json`
- **Single binary deployment** — SvelteKit build embedded via `include_dir!` macro

#### Engine Upgrades
- **HyDE query expansion** — template-based Hypothetical Document Embeddings: classify_intent (6 types) → expand_query (3-5 variants) → centroid_embedding. Wired into `semantic_search_raw`
- **fastembed 5.11** — upgraded from 5.9, adds Nomic v2 MoE + Qwen3 reranker support
- **Nomic Embed Text v2 MoE** — opt-in via `--features nomic-v2` (475M params, 305M active, 8 experts, Candle backend)
- **Qwen3 Reranker** — opt-in via `--features qwen3-reranker` (Candle backend, high-precision cross-encoder)
- **Metal GPU acceleration** — opt-in via `--features metal` (Apple Silicon, significantly faster embedding inference)

#### Backend
- **Axum WebSocket** — `/ws` endpoint with 5-second heartbeat, live stats (memory count, avg retention, uptime)
- **7 new REST endpoints** — `POST /api/dream`, `/api/explore`, `/api/predict`, `/api/importance`, `/api/consolidate`, `GET /api/search`, `/api/retention-distribution`, `/api/intentions`
- **Event emission from MCP tools** — `emit_tool_event()` broadcasts events for smart_ingest, search, dream, consolidate, memory, importance_score
- **Shared broadcast channel** — single `tokio::broadcast::channel(1024)` shared between dashboard and MCP server
- **CORS for SvelteKit dev** — `localhost:5173` allowed in dev mode

#### Benchmarks
- **Criterion benchmark suite** — `cosine_similarity` 296ns, `centroid` 1.3µs, HyDE expand 1.4µs, RRF fusion 17µs

### Changed
- Version: 1.8.0 → 2.0.0 (both crates)
- Rust edition: 2024 (MSRV 1.85)
- Tests: 651 → 734 (352 core + 378 mcp + 4 doctests)
- Binary size: ~22MB (includes embedded SvelteKit dashboard)
- CognitiveEngine moved from main.rs binary crate to lib.rs for dashboard access
- Dashboard served at `/dashboard` prefix (legacy HTML kept at `/` and `/graph`)
- `McpServer` now accepts optional `broadcast::Sender<VestigeEvent>` for event emission

### Technical
- `apps/dashboard/` — new SvelteKit app (Svelte 5, Tailwind CSS 4, Three.js 0.172, `@sveltejs/adapter-static`)
- `dashboard/events.rs` — 16-variant `VestigeEvent` enum with `#[serde(tag = "type", content = "data")]`
- `dashboard/websocket.rs` — WebSocket upgrade handler with heartbeat + event forwarding
- `dashboard/static_files.rs` — `include_dir!` macro for embedded SvelteKit build
- `search/hyde.rs` — HyDE module with intent classification and query expansion
- `benches/search_bench.rs` — Criterion benchmarks for search pipeline components

---

## [1.8.0] - 2026-02-21

### Added
- **`session_context` tool** — one-call session initialization replacing 5 separate calls (search × 2, intention check, system_status, predict). Token-budgeted responses (~15K tokens → ~500-1000 tokens). Returns assembled markdown context, `automationTriggers` (needsDream/needsBackup/needsGc), and `expandable` memory IDs for on-demand retrieval.
- **`token_budget` parameter on `search`** — limits response size (100-10000 tokens). Results exceeding budget moved to `expandable` array with `tokensUsed`/`tokenBudget` tracking.
- **Reader/writer connection split** — `Storage` struct uses `Mutex<Connection>` for separate reader/writer SQLite handles with WAL mode. All methods take `&self` (interior mutability). `Arc<Mutex<Storage>>` → `Arc<Storage>` across ~30 files.
- **int8 vector quantization** — `ScalarKind::F16` → `I8` (2x memory savings, <1% recall loss)
- **Migration v7** — FTS5 porter tokenizer (15-30% keyword recall) + page_size 8192 (10-30% faster large-row reads)
- 22 new tests for session_context and token_budget (335 → 357 mcp tests, 651 total)

### Changed
- Tool count: 18 → 19
- `EmbeddingService::init()` changed from `&mut self` to `&self` (dead `model_loaded` field removed)
- CLAUDE.md updated: session start uses `session_context`, 19 tools documented, development section reflects storage architecture

### Performance
- Session init: ~15K tokens → ~500-1000 tokens (single tool call)
- Vector storage: 2x reduction (F16 → I8)
- Keyword search: 15-30% better recall (FTS5 porter stemming)
- Large-row reads: 10-30% faster (page_size 8192)
- Concurrent reads: non-blocking (reader/writer WAL split)

---

## [1.7.0] - 2026-02-20

### Changed
- **Tool consolidation: 23 → 18 tools** — merged redundant tools while maintaining 100% backward compatibility via deprecated redirects
- **`ingest` → `smart_ingest`** — `ingest` was a duplicate of `smart_ingest`; now redirects automatically
- **`session_checkpoint` → `smart_ingest` batch mode** — new `items` parameter on `smart_ingest` accepts up to 20 items, each running the full cognitive pipeline (importance scoring, intent detection, synaptic tagging, hippocampal indexing). Old `session_checkpoint` skipped the cognitive pipeline.
- **`promote_memory` + `demote_memory` → `memory` unified** — new `promote` and `demote` actions on the `memory` tool with optional `reason` parameter and full cognitive feedback pipeline (reward signal, reconsolidation, competition)
- **`health_check` + `stats` → `system_status`** — single tool returns combined health status, full statistics, FSRS preview, cognitive module health, state distribution, warnings, and recommendations
- **CLAUDE.md automation overhaul** — all 18 tools now have explicit auto-trigger rules; session start expanded to 5 steps (added `system_status` + `predict`); full proactive behaviors table

### Added
- `smart_ingest` batch mode with `items` parameter (max 20 items, full cognitive pipeline per item)
- `memory` actions: `promote` and `demote` with optional `reason` parameter
- `system_status` tool combining health check + statistics + cognitive health
- 30 new tests (305 → 335)

### Deprecated (still work via redirects)
- `ingest` → use `smart_ingest`
- `session_checkpoint` → use `smart_ingest` with `items`
- `promote_memory` → use `memory(action="promote")`
- `demote_memory` → use `memory(action="demote")`
- `health_check` → use `system_status`
- `stats` → use `system_status`

---

## [1.6.0] - 2026-02-19

### Changed
- **F16 vector quantization** — USearch vectors stored as F16 instead of F32 (2x storage savings)
- **Matryoshka 256-dim truncation** — embedding dimensions reduced from 768 to 256 (3x embedding storage savings)
- **Convex Combination fusion** — replaced RRF with 0.3 keyword / 0.7 semantic weighted fusion for better score preservation
- **Cross-encoder reranker** — added Jina Reranker v1 Turbo (fastembed TextRerank) for neural reranking (~20% retrieval quality improvement)
- Combined: **6x vector storage reduction** with better retrieval quality
- Cross-encoder loads in background — server starts instantly
- Old 768-dim embeddings auto-migrated on load

---

## [1.5.0] - 2026-02-18

### Added
- **CognitiveEngine** — 28-module stateful engine with full neuroscience pipeline on every tool call
- **`dream`** tool — memory consolidation via replay, discovers hidden connections and synthesizes insights
- **`explore_connections`** tool — graph traversal with chain, associations, and bridges actions
- **`predict`** tool — proactive retrieval based on context and activity patterns
- **`restore`** tool — restore memories from JSON backup files
- **Automatic consolidation** — FSRS-6 decay runs on a 6-hour timer + inline every 100 tool calls
- ACT-R base-level activation with full access history
- Episodic-to-semantic auto-merge during consolidation
- Cross-memory reinforcement on access
- Park et al. triple retrieval scoring
- Personalized w20 optimization

### Changed
- All existing tools upgraded with cognitive pre/post processing pipelines
- Tool count: 19 → 23

---

## [1.3.0] - 2026-02-12

### Added
- **`importance_score`** tool — 4-channel neuroscience scoring (novelty, arousal, reward, attention)
- **`session_checkpoint`** tool — batch smart_ingest up to 20 items with Prediction Error Gating
- **`find_duplicates`** tool — cosine similarity clustering with union-find for dedup
- `vestige ingest` CLI command for memory ingestion via command line

### Changed
- Tool count: 16 → 19
- Made `get_node_embedding` public in core API
- Added `get_all_embeddings` for duplicate scanning

---

## [1.2.0] - 2026-02-12

### Added
- **Web dashboard** — Axum-based on port 3927 with memory browser, search, and system stats
- **`memory_timeline`** tool — browse memories chronologically, grouped by day
- **`memory_changelog`** tool — audit trail of memory state transitions
- **`health_check`** tool — system health status with recommendations
- **`consolidate`** tool — run FSRS-6 maintenance cycle
- **`stats`** tool — full memory system statistics
- **`backup`** tool — create SQLite database backups
- **`export`** tool — export memories as JSON/JSONL with filters
- **`gc`** tool — garbage collect low-retention memories
- `backup_to()` and `get_recent_state_transitions()` storage APIs

### Changed
- Search now supports `detail_level` (brief/summary/full) to control token usage
- Tool count: 8 → 16

---

## [1.1.3] - 2026-02-12

### Changed
- Upgraded to Rust edition 2024
- Security hardening and dependency updates

### Fixed
- Dedup on ingest edge cases
- Intel Mac CI builds
- NPM package version alignment
- Removed dead TypeScript package

---

## [1.1.2] - 2025-01-27

### Fixed
- Embedding model cache now uses platform-appropriate directories instead of polluting project folders
  - macOS: `~/Library/Caches/com.vestige.core/fastembed`
  - Linux: `~/.cache/vestige/fastembed`
  - Windows: `%LOCALAPPDATA%\vestige\cache\fastembed`
- Can still override with `FASTEMBED_CACHE_PATH` environment variable

---

## [1.1.1] - 2025-01-27

### Fixed
- UTF-8 string slicing issues in keyword search and prospective memory
- Silent error handling in MCP stdio protocol
- Feature flag forwarding between crates
- All GitHub issues resolved (#1, #3, #4)

### Added
- Pre-built binaries for Linux, Windows, and macOS (Intel & ARM)
- GitHub Actions CI/CD for automated releases

---

## [1.1.0] - 2025-01-26

### Changed
- **Tool Consolidation**: 29 tools → 8 cognitive primitives
  - `recall`, `semantic_search`, `hybrid_search` → `search`
  - `get_knowledge`, `delete_knowledge`, `get_memory_state` → `memory`
  - `remember_pattern`, `remember_decision`, `get_codebase_context` → `codebase`
  - 5 intention tools → `intention`
- Stats and maintenance moved from MCP to CLI (`vestige stats`, `vestige health`, etc.)

### Added
- CLI admin commands: `vestige stats`, `vestige health`, `vestige consolidate`, `vestige restore`
- Feedback tools: `promote_memory`, `demote_memory`
- 30+ FAQ entries with verified neuroscience claims
- Storage modes documentation: Global, per-project, multi-Claude household
- CLAUDE.md templates for proactive memory use
- Version pinning via git tags

### Deprecated
- Old tool names (still work with warnings, removed in v2.0)

---

## [1.0.0] - 2025-01-25

### Added
- FSRS-6 spaced repetition algorithm with 21 parameters
- Bjork & Bjork dual-strength memory model (storage + retrieval strength)
- Local semantic embeddings with fastembed v5 (BGE-base-en-v1.5, 768 dimensions)
- HNSW vector search with USearch (20x faster than FAISS)
- Hybrid search combining BM25 keyword + semantic + RRF fusion
- Two-stage retrieval with reranking (+15-20% precision)
- MCP server for Claude Desktop integration
- Tauri desktop application
- Codebase memory module for AI code understanding
- Neuroscience-inspired memory mechanisms:
  - Synaptic Tagging and Capture (retroactive importance)
  - Context-Dependent Memory (Tulving encoding specificity)
  - Spreading Activation Networks
  - Memory States (Active/Dormant/Silent/Unavailable)
  - Multi-channel Importance Signals (Novelty/Arousal/Reward/Attention)
  - Hippocampal Indexing (Teyler & Rudy 2007)
- Prospective memory (intentions and reminders)
- Sleep consolidation with 5-stage processing
- Memory compression for long-term storage
- Cross-project learning for universal patterns

### Changed
- Upgraded embedding model from all-MiniLM-L6-v2 (384d) to BGE-base-en-v1.5 (768d)
- Upgraded fastembed from v4 to v5

### Fixed
- SQL injection protection in FTS5 queries
- Infinite loop prevention in file watcher
- SIGSEGV crash in vector index (reserve before add)
- Memory safety with Mutex wrapper for embedding model

---

## [0.1.0] - 2025-01-24

### Added
- Initial release
- Core memory storage with SQLite + FTS5
- Basic FSRS scheduling
- MCP protocol support
- Desktop app skeleton

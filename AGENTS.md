# Vestige — Cognitive Memory System

> **Canonical agent instructions.** This file is the source of truth for all AI
> coding agents working on this repository (Codex, Cursor, GitHub Copilot,
> Claude Code, Cline, Gemini CLI, Continue, Zed, JetBrains Junie, Devin, etc.).
> `CLAUDE.md` and `GEMINI.md` are symlinks to this file so tool-specific
> loaders pick up the same content.

Vestige is your long-term memory. Use it automatically. Use it aggressively. Retrieve context silently — do not announce memory operations to the user.

---

## MCP Invocation

**Server:** `user-vestige`

All tools are called via `CallMcpTool` with:
- `server`: `"user-vestige"`
- `toolName`: tool name from sections below
- `arguments`: `{ ... }` as documented

If the MCP server is errored or unreachable, follow the fallback in `vestige-shared-brain.mdc`.

---

## Session Start Protocol

Every conversation, before responding to the user:

```
CallMcpTool → server: "user-vestige", toolName: "session_context"
arguments: {
  queries: ["user preferences", "[current project] context"],
  context: { codebase: "[project]", topics: ["[current topics]"] },
  token_budget: 2000
}
```

Then check `automationTriggers` from response:
- `needsDream` → call `dream`
- `needsBackup` → call `backup`
- `needsGc` → call `gc` with `dry_run: true`, then review

`session_context` returns exactly those three triggers in `automationTriggers`. The memory-count and saves-based rules need one extra call — `system_status` reports both fields:

- `totalMemories` > 700 → call `find_duplicates`
- `automationTriggers.savesSinceLastDream` > 100 → call `reflect` (metacognitive check)

> **Fallback:** If `session_context` unavailable: `search` × 2 → `intention` check → `system_status` → `predict`.

---

## What Runs Automatically vs What You Must Trigger

### Automatic (server handles it)
- **FSRS-6 consolidation** — background loop every 6h (configurable via `VESTIGE_CONSOLIDATION_INTERVAL_HOURS`)
- **Inline consolidation** — after tool calls, `ConsolidationScheduler` decides when to run mini-consolidation
- **Reconsolidation window expiry** — 5-minute labile windows auto-expire
- **Nothing is ever deleted automatically.** Consolidation only *reports* low-retention candidates; `gc_below_retention` is never called from an automatic path. GC happens only when you call `gc` (dry-run first, `confirmed: true` for the destructive pass).

### You must trigger (via automationTriggers)
| Trigger | Condition | Tool to call |
|---------|-----------|-------------|
| `needsDream` | >24h since last dream OR >50 saves | `dream` |
| `needsBackup` | >7 days since last backup | `backup` |
| `needsGc` | status is "degraded" or "critical" | `gc` with `dry_run: true` |
| High memory count | >700 memories | `find_duplicates` |

### You should trigger periodically (no automation signal)
| Tool | When | Why |
|------|------|-----|
| `reflect` | Weekly, or after large knowledge ingestion | Finds contradictions, gaps, stale decisions |
| `confidence` audit | Monthly, or when memories seem unreliable | Finds poorly-calibrated opinions |
| `temporal` current | When working with time-sensitive facts | Filters out superseded information |
| `temporal` invalidate | When a stored fact becomes outdated | Marks old version as expired |

---

## Tool Reference

### session_context — One-Call Initialization
```json
{ "queries": ["user preferences", "project context"],
  "context": { "codebase": "project-name", "topics": ["react", "rust"], "file": "src/main.rs" },
  "token_budget": 2000, "include_status": true,
  "include_intentions": true, "include_predictions": true }
```
Returns: markdown context + `automationTriggers` + `expandable` IDs for on-demand retrieval.

### smart_ingest — Save Anything

**CRITICAL: Atomic Memory Rule**

Every memory must contain **exactly ONE** fact, decision, event, or insight. Compound content degrades search recall by 40-60%. The server will return a `compound_content_warning` if it detects multi-topic content — you MUST split and re-ingest when you see it.

**When to split into batch:**
- Content has multiple unrelated facts → separate `items` entries
- Conversation summary covers several topics → one memory per topic
- Bug fix touched multiple files for different reasons → one memory per root cause
- Session-end save has >3 insights → batch with one insight per item

**Good (atomic):**
```json
{ "content": "Vestige search uses triple hybrid scoring: BM25 + semantic + RRF. The default weights are 0.3/0.7 for BM25/semantic.", "tags": ["vestige", "architecture"], "node_type": "fact" }
```

**Bad (compound — will trigger warning):**
```json
{ "content": "Today we fixed the search bug, also John prefers dark mode, and the deployment deadline is Friday. Oh and we decided to use PostgreSQL instead of MySQL.", "tags": ["session-end"], "node_type": "fact" }
```

**Correct fix — use batch:**
```json
{ "items": [
  { "content": "Search bug: hybrid_search returned 0 results when query contained semicolons. Root cause: FTS5 treated ; as statement separator. Fix: escape semicolons in query preprocessing.", "tags": ["bug-fix", "vestige"], "node_type": "fact" },
  { "content": "John prefers dark mode in all tools and editors.", "tags": ["preference", "person"], "node_type": "person" },
  { "content": "Deployment deadline: Friday 2026-04-11.", "tags": ["deadline"], "node_type": "event" },
  { "content": "Decision: use PostgreSQL over MySQL. Rationale: better JSON support, NOTIFY/LISTEN for real-time, team familiarity.", "tags": ["decision", "database"], "node_type": "decision" }
] }
```

**Single:**
```json
{ "content": "What to remember", "tags": ["tag1", "tag2"],
  "node_type": "fact", "source": "optional reference", "forceCreate": false,
  "session_id": "conversation-uuid", "agent": "cursor" }
```
**Batch (up to 20 items):**
```json
{ "items": [
  { "content": "Item 1", "tags": ["session-end"], "node_type": "fact" },
  { "content": "Item 2", "tags": ["bug-fix"], "node_type": "fact" }
], "session_id": "conversation-uuid", "agent": "cursor" }
```
Node types: `fact` | `concept` | `event` | `person` | `place` | `note` | `pattern` | `decision`

**Content Intelligence Pipeline:** Every `smart_ingest` call automatically preprocesses content before storage:
1. **Entity extraction** — URLs, emails, file paths, monetary values, proper nouns → auto-tags (`entity:john-smith`)
2. **Coreference rewriting** — "He said X" → "John said X" (self-contained memories improve search recall)
3. **Temporal anchoring** — "by next Friday" → absolute `valid_until` date; "starting from Monday" → `valid_from`
4. **Relation extraction** — "John manages Auth Team" → knowledge graph edge (feeds spreading activation)
5. **Provenance tracking** — session_id, agent, derivation chain, preprocessing artifacts stored as JSON metadata

All local heuristic/regex, zero model downloads, sub-millisecond latency.

### search — 8-Stage Cognitive Search
```json
{ "query": "search query", "limit": 10, "min_retention": 0.0,
  "min_similarity": 0.5, "detail_level": "summary",
  "context_topics": ["rust", "debugging"], "token_budget": 3000,
  "retrieval_mode": "balanced" }
```
Every search strengthens the memories it finds (Testing Effect).

**Retrieval modes:** `precise` (top results only, fast, skips activation/competition), `balanced` (full 8-stage cognitive pipeline, default), `exhaustive` (maximum recall with 5x overfetch, deep graph traversal, no competition suppression).

**Compound query decomposition:** Queries containing semicolons, question chains, or conjunctions are automatically split into sub-queries, searched independently, and merged (union, max-score dedup). Useful for multi-topic questions like `"Who worked on FSRS? And dream consolidation?"`. Effect size on real workloads has not been formally benchmarked outside the internal LoCoMo subset; treat any improvement as workload-dependent.

**Provenance in results:** Use `detail_level: "full"` to include provenance metadata (session, agent, entities, relations, temporal anchors) in search results.

### memory — Read, Edit, Delete, Promote, Demote, Review, Batch Get
```json
{ "action": "get", "id": "uuid" }
{ "action": "get_batch", "ids": ["uuid-1", "uuid-2", "uuid-3"] }
{ "action": "edit", "id": "uuid", "content": "updated text" }
{ "action": "delete", "id": "uuid", "confirmed": true }
{ "action": "promote", "id": "uuid", "reason": "was helpful" }
{ "action": "demote", "id": "uuid", "reason": "was wrong" }
{ "action": "state", "id": "uuid" }
{ "action": "review", "id": "uuid", "rating": "good" }
```
`get_batch` retrieves up to 20 memories in one call (saves round-trips for expandable IDs from search).
Promote/demote adjusts ranking, does NOT delete. Demoted memories rank lower; alternatives surface instead.
**`delete` is destructive and requires `confirmed: true`** — a call without it is refused, so a model that
reaches for `delete` by mistake cannot remove a memory.
`review` records a spaced-repetition review of a due memory; `rating` is one of
`again` | `hard` | `good` | `easy` and defaults to `good`. `mark_reviewed` still works as a
backward-compatible alias for `review`.

### codebase — Code Patterns & Architectural Decisions
```json
{ "action": "remember_pattern", "name": "Pattern Name",
  "description": "How it works", "files": ["src/file.rs"], "codebase": "project-name" }
{ "action": "remember_decision", "decision": "What was decided",
  "rationale": "Why", "alternatives": ["A", "B"], "files": ["src/file.rs"], "codebase": "project-name" }
{ "action": "get_context", "codebase": "project-name", "limit": 10 }
```

### intention — Prospective Memory (Reminders)
```json
{ "action": "set", "description": "What to do",
  "trigger": { "type": "context", "topic": "authentication" }, "priority": "high" }
{ "action": "set", "description": "Deploy by Friday",
  "trigger": { "type": "time", "at": "2026-03-07T17:00:00Z" },
  "deadline": "2026-03-07T17:00:00Z" }
{ "action": "check", "context": { "codebase": "vestige", "topics": ["testing"] } }
{ "action": "update", "id": "uuid", "status": "complete" }
{ "action": "list", "filter_status": "active" }
```

### dream — Memory Consolidation
```json
{ "memory_count": 50 }
```
4-phase cycle: NREM1 triage → NREM3 consolidation → REM creative → Integration. Returns insights, connections, stats.

### reflect — Metacognitive Self-Examination (v3.1)
```json
{ "focus": "optional topic to focus on", "depth": "standard" }
```
Depth: `quick` (fast scan) | `standard` (default) | `deep` (thorough).
Detects: contradictions, knowledge gaps, stale decisions, overconfident memories, pattern clusters.
Unlike `dream` (unconscious consolidation), `reflect` is deliberate self-examination.

**When to use:** After large knowledge ingestion, when memories seem inconsistent, periodically (weekly).

### temporal — Temporal Fact Versioning (v3.1)
```json
{ "action": "current", "topic": "deployment process", "limit": 10 }
{ "action": "expired", "topic": "API endpoints", "limit": 10 }
{ "action": "history", "topic": "database schema", "limit": 20 }
{ "action": "invalidate", "memory_id": "uuid" }
```
Actions: `current` (valid-now facts), `expired` (no-longer-valid), `history` (evolution over time), `invalidate` (mark as superseded).

**When to use:** When facts change (API versions, team members, processes). Use `invalidate` when you discover a stored fact is outdated.

### confidence — Confidence Scoring (v3.1)
```json
{ "action": "score", "memory_id": "uuid" }
{ "action": "audit", "limit": 20 }
{ "action": "calibrate", "limit": 20 }
```
Actions: `score` (evaluate single memory), `audit` (find poorly-calibrated memories), `calibrate` (compare opinions vs facts).
Returns multi-dimensional scores: encoding, retrieval, temporal, evidence.

**When to use:** `audit` periodically to find unreliable memories. `score` when you're unsure about a retrieved memory's reliability.

### explore_connections — Graph Traversal
```json
{ "action": "associations", "from": "uuid", "limit": 10 }
{ "action": "chain", "from": "uuid-A", "to": "uuid-B" }
{ "action": "bridges", "from": "uuid-A", "to": "uuid-B" }
```

### predict — Proactive Retrieval
```json
{ "context": { "codebase": "vestige", "current_file": "src/main.rs",
  "current_topics": ["error handling", "rust"] } }
```

### precompute_for_context — Sleep-Time Compute
```json
{ "topic": "OAuth2 token refresh", "top_k": 5, "ttl_hours": 24 }
```
Pre-fetches and summarizes the top-K memories for an upcoming topic **before** you ask, storing the digest as a `precomputed_summary` memory tagged `precomputed` + `topic:<slug>` with `valid_until = now + ttl_hours`. `topic` is required; `top_k` is clamped to 1–20 (default 5) and `ttl_hours` to 1–168 (default 24). Call it at session start ("I expect to work on X today") or session end; the next `search` / `session_context` / `deep_reference` call hits the warm cache. Mutating but additive — each call mints a fresh memory and older summaries expire through `temporal`.

### deep_reference — Cognitive Reasoning Engine
```json
{ "query": "Why did we choose PostgreSQL over MySQL?", "depth": 20 }
```
Full reasoning across memories: FSRS-6 trust scoring, intent classification (FactCheck/Timeline/RootCause/Comparison/Synthesis), temporal supersession, contradiction detection, dream insight integration. Returns: recommended answer, evidence, contradictions, superseded memories, evolution timeline.
`cross_reference` is a backward-compatible alias.

### importance_score — Should I Save This?
```json
{ "content": "Content to evaluate", "context_topics": ["debugging"], "project": "vestige" }
```
Composite > 0.6 = save it (documentation heuristic — the tool returns the score and enforces no threshold).

### find_duplicates, memory_timeline, memory_changelog, memory_health, memory_graph
```json
find_duplicates: { "similarity_threshold": 0.80, "limit": 20, "tags": ["bug-fix"] }
memory_timeline: { "start": "2026-02-01", "end": "2026-03-01", "node_type": "decision", "tags": ["vestige"], "limit": 50 }
memory_changelog: { "memory_id": "uuid", "limit": 20 }  // or { "start": "2026-03-01", "limit": 20 }
memory_health: {}
memory_graph: { "query": "search term", "depth": 2, "max_nodes": 50 }
```

### split_memories — Find & Fix Compound Memories
```json
{ "min_length": 300, "limit": 20, "dry_run": true }
```
Scans all memories for compound/multi-topic content. Returns a list with splitting suggestions.
- `dry_run: true` (default) — report only, no deletions
- `dry_run: false` — delete compound memories after reporting (you must re-ingest as atomic items)

**Workflow:** Run `split_memories` → for each compound memory, read full content with `memory(action="get")` → split into atomic facts → `smart_ingest` as batch → `memory(action="delete", confirmed: true)` the original.

### Maintenance Tools
```json
system_status: {}
consolidate: {}
backup: {}
export: { "format": "json", "tags": ["bug-fix"], "since": "2026-01-01" }
gc: { "min_retention": 0.1, "dry_run": true }                      // dry_run:false additionally requires "confirmed": true
restore: { "path": "/path/to/backup.json", "confirmed": true }     // no dry-run mode — confirmed is mandatory on every call
erase: { "action": "memory", "id": "uuid", "dry_run": true }       // GDPR Art. 17; "action":"tag" + "tag" for bulk. dry_run:false additionally requires "confirmed": true
```

### erase — GDPR Article 17 Erasure
```json
{ "action": "memory", "id": "uuid" }
{ "action": "tag", "tag": "gdpr-subject", "dry_run": false, "confirmed": true }
```
Hard-deletes the memory (or every memory carrying that exact tag — `code` does not match `codebase`) **and** the data derived from it: connections, embeddings, access log, state history, and every insight whose `source_memories` named it. `memory(action="delete")` and `gc` do not do this.

**Always dry-run first** — the call is irreversible. `dry_run` defaults to `true` and reports the exact count plus up to 100 ids; the destructive pass additionally requires `confirmed: true`. Same operation is available as `POST /api/maintenance/erase` and `vestige erase --id|--tag [--dry-run] [--confirm]`.

---

## Mandatory Save Gates

**You MUST NOT proceed past a save gate without executing the save.**

| Gate | Trigger | Action |
|------|---------|--------|
| **BUG_FIX** | After any error is resolved | `smart_ingest` with content: `"BUG FIX: [what broke]\nRoot cause: [why it broke]\nLesson: [the rule to apply next time]"`, tags: `["bug-fix", "project"]`, node_type: `"fact"`. File paths and line numbers stay **out of the content** — a stored path breaks silently in both directions (it keeps resolving to an old copy after a version bump, and vanishes without a word when the target is deleted), and only `code_refs`/`source` may carry them. `Solution: [the diff]` is not a memory: the diff is in the repository. The lesson is the part that is nowhere else. |
| **DECISION** | After any architectural/design choice | `codebase` with action: `"remember_decision"`, stating the question and the future decision it settles. To replace an earlier decision, pass `supersedes: [ids]` and, when the answer expires, `validUntil` — never overwrite it. A decision that silently disappears takes the record of what we believed with it. |
| **CODE_CHANGE** | After >20 lines or new pattern | `codebase` with action: `"remember_pattern"` — the rule the pattern encodes (when to use it, when not to), not the diff. Touched files go in `files`, never in `description`. |
| **SESSION_END** | Before stopping or compaction | `smart_ingest` batch with tags: `["session-end"]` |
| **FACT_CHANGED** | When a previously stored fact becomes outdated | `temporal` with action: `"invalidate"`, then `smart_ingest` with the corrected fact |

### Reading the self-containedness gate

Every write is checked against one question: will this still be readable by someone who was not in the conversation that produced it? There are three answers, and each needs a different response.

| Response | Meaning | What to do |
|---|---|---|
| no `self_contained` key | The gate found nothing. | Nothing. |
| `self_contained: { requiresContext: true, findings: [...] }` | **Written, and flagged.** The memory exists and is searchable, but it leans on something it does not carry. | Rewrite it: each finding names the `kind` (which rule fired), the `span` (the exact text) and a `hint` (what to write instead). Name the antecedent of a pronoun, say what "the fix" was, replace "next week" with a date. Do **not** delete it — silent loss is worse than a flagged entry. Note the marker records the gate's verdict at write time: editing the memory does not clear it, so a rewritten memory stays listed until the gate is re-run on it. |
| `decision: "reject"`, `stored: false` | **Nothing was written.** The content is something the repository already owns — a code block, a directory tree, copied source, a coverage figure, a version number — or there was nothing in it. | Read `reason` and `findings`, then save the lesson or the decision it was meant to carry. Do not retry the same text. If that knowledge genuinely is not in the repository yet, put it there first: then it does not need to be remembered. Nothing fixable is ever rejected — a fixable memory is written and flagged. |

---

## Trigger Words — Auto-Save

| User Says | Action |
|-----------|--------|
| "Remember this" / "Don't forget" | `smart_ingest` immediately |
| "I always..." / "I never..." / "I prefer..." | Save as preference |
| "This is important" | `smart_ingest` + `memory(action="promote")` |
| "Remind me..." / "Next time..." | `intention(action="set")` |

---

## Memory Hygiene

- **One fact per memory.** If you're writing "also" or "additionally" in a memory, stop and split it. Use batch mode.
- **Promote** when user confirms helpful, solution worked, info was accurate.
- **Demote** when user corrects mistake, info was wrong, led to bad outcome.
- **Never save:** secrets, API keys, passwords, temporary debugging state, trivial info.
- **State the future decision this changes, or do not save.** If you cannot name the decision a memory would alter, it is a description — and descriptions of the code belong in the repository, where they cannot go stale unnoticed. Prediction Error Gating removes duplicates; it cannot tell that a memory was never worth having.
- **If `compound_content_warning` appears in response:** delete the compound memory, split content into atomic pieces, re-ingest as batch.
- **If `self_contained` appears in response or `decision` is `"reject"`:** see *Reading the self-containedness gate* above — rewrite a flagged memory, replace a refused one.

---

## Token Budget Strategy

- Quick lookups: `token_budget: 500`
- Normal context: `token_budget: 2000`
- Deep context: `token_budget: 3000-5000`
- Detail levels: `brief` (scan) | `summary` (default) | `full` (debug)
- Overflow goes to `expandable` — retrieve with `memory(action="get")`

---

## Development

- **Crate:** `vestige-mcp` v3.4.0, Rust 2024 edition, MSRV 1.91
- **Tools:** 29 MCP tools (core memory, cognitive, metacognitive, autonomic, maintenance, deep_reference). Canonical list lives in `vestige-mcp/src/server/catalog.rs::build_tools_list`; `scripts/check-version-and-tools.sh` fails the build if this number, `catalog.rs` and `README.md`/`ARCHITECTURE.md` disagree.
- **Tests:** workspace `cargo test` runs the full unit + E2E + cognitive + journey + extreme + MCP protocol + scientific-validation suites; exact pass count drifts with each release, so check the latest CI log instead of hard-coding it here
- **Build:** `cargo build --release -p vestige-mcp` (features: `embeddings` + `vector-search` + `preprocessing`)
- **Build (no embeddings):** `cargo build --release -p vestige-mcp --no-default-features`
- **Preprocessing:** entity extraction, coreference rewriting, temporal anchoring, relation extraction — all local regex/heuristic, zero model downloads. Feature-gated under `preprocessing` (default on).
- **NLP detectors:** `vestige_core::nlp` exposes three traits — `ContradictionDetector`, `OpinionDetector`, `FutureRelevanceDetector` — and **three factory functions** that return the process-wide default implementation: `default_contradiction_detector()`, `default_opinion_detector()`, `default_future_relevance_detector()`. Call sites (`PredictionErrorGate::detect_contradiction`, `DreamEngine::categorize_memory`, `tools::confidence::is_opinion`) MUST go through the factories — never construct `HeuristicXDetector` directly. To plug an ONNX or LLM backend behind a feature flag, edit `nlp::mod.rs` (one file) instead of every call site. Defaults today are pure-Rust heuristics (NegEx scope + EN/PL lexicons + custom language detection). Per-signal confidence weights are `pub const` (e.g. `NEGATION_SCOPE_CONFIDENCE = 0.85`) so the eval baseline can pin them. Eval harness lives in `nlp/eval/` with hand-curated EN+PL datasets; baselines enforced by `tests/nlp_baseline.rs` (`cargo test -p vestige-core --test nlp_baseline -- --nocapture`). Contradiction detector returns `negative` when *either* side is empty/whitespace (regression: empty `old` previously triggered the asymmetric fallback on every `new` containing a negation trigger).
- **Bench:** `cargo bench -p vestige-core`
- **Architecture:** `McpServer` → `Arc<Storage>` + `Arc<Mutex<CognitiveEngine>>`
- **Storage:** SQLite WAL mode, `Mutex<Connection>` reader/writer split, FTS5 full-text search. Implementation split across `storage/sqlite/` per concern (nodes, states, history, intentions, maintenance, embeddings, review, consolidation, search, graph, gdpr, temporal, smart_ingest, insights, records, stats). Migrations v1–v19.
- **Embeddings:** nomic-embed-text-v1.5 (768D → 384D Matryoshka, 8K context) via fastembed (local ONNX, no API; ~547 MB download). The reduction is the model card's exact order — `layer_norm` over the full 768 dims → slice to 384 → L2 — and the card-mandated `search_document:`/`search_query:` task prefixes are applied on the default path. Do not reorder those steps, and do not slice before normalising: the two orders select different directions in the 384-dim space.
- **Embedding space version:** `vestige_core::embeddings::EMBEDDING_SPACE_VERSION` (currently `2`) identifies the vectors the embedding function produces; `embedding_space_fingerprint()` is the derived string a cache must key on, and `embedding_model_tag()` is the per-memory tag in `knowledge_nodes.embedding_model` (`nomic-embed-text-v1.5+v2+prefix`, or `...+v2` in the legacy raw regime). Bump the version whenever the same text would produce a different vector. v1 = slice→L2 with optional prefixes (every earlier build); v2 = LayerNorm before the slice plus prefixes on by default, so every v1 vector is in a different space.
- **Re-embedding:** `regenerate_embeddings` with `force: true` re-embeds every memory from its content, and is the only supported migration after an embedding-space change; until it runs, old and new vectors coexist and retrieval degrades silently. The HNSW sidecar is NOT version-aware: its meta validates only `(COUNT(*), MAX(created_at))` (`Storage::embeddings_fingerprint`, `crates/vestige-core/src/storage/sqlite/init.rs`), so an upgraded binary reloads a pre-v2 sidecar unless the store has been re-embedded.
- **Reranker:** Jina Reranker v2 Base Multilingual (278M params, ~1.11 GB ONNX download) cross-encoder
- **Search:** Compound query decomposition + Triple hybrid scoring (BM25 + semantic + RRF), active forgetting, prospective indexing
- **Vector index:** USearch HNSW (in-memory ANN; persisted to a `vestige.hnsw` sidecar + meta JSON and loaded on startup via a row-count-validated fast path, with rebuild-from-SQLite fallback when the sidecar is missing or stale)
- **Binaries:** `vestige-mcp` (MCP server), `vestige` (CLI), `vestige-restore`
- **Dashboard:** React 19 + Vite 6 + React Router 7 + Three.js + Tailwind 4 + i18next (EN/PL), embedded at `/dashboard`
- **Dashboard API:** 42 route registrations — 34 distinct REST API paths (36 registrations, because `/api/memories/{id}` registers GET, DELETE and PATCH) plus 4 page/SPA routes (`/dashboard`, `/dashboard/{*path}`, `/`, `/graph`), 1 WebSocket (`/ws`) and 1 Prometheus scrape endpoint (`/metrics`); the arithmetic is 36 + 4 + 1 + 1 = 42. Includes `/api/reflect`, `/api/temporal`, `/api/confidence`, `/api/decisions`, `/api/hubs`, `/api/insights`, `/api/_meta/limits`, `/api/maintenance/erase`. Handlers split per domain under `dashboard/handlers/` (memory, search, graph, history, intentions, maintenance, review, cognitive, metacognitive, observability, decisions, hubs, insights, pages).
- **Env vars:** `VESTIGE_DASHBOARD_PORT` (default 3927), `VESTIGE_CONSOLIDATION_INTERVAL_HOURS` (default 6), `RUST_LOG`, `VESTIGE_NOMIC_PREFIXES` (default **on** — apply the Nomic `search_query:`/`search_document:` task prefixes the model card mandates; set it to any non-truthy value (`0`/`false`/`no`/`off`) for the legacy raw regime. Either direction changes the embedding space, so it is coupled with `regenerate_embeddings` — see `.env.example`)

For cognitive architecture details, see [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Adding a Type-Safe Dashboard Endpoint

The dashboard ↔ backend contract is enforced end-to-end: ts-rs generates
TypeScript declarations from Rust DTOs, Zod re-validates the five
highest-blast-radius responses at runtime, and a CI gate fails any PR
that edits a Rust DTO without committing the regenerated `.ts` file.

### Where things live

```
crates/vestige-mcp/src/dashboard/
├── wire/                 # DTOs (this is the contract — single source of truth)
│   ├── memory.rs         # MemoryDto, MemoryListResponseDto, …
│   ├── graph.rs
│   ├── limits.rs         # DashboardLimitsDto + the parity tests
│   └── …
├── handlers/             # Convert domain → DTO → Json(...)
└── events.rs             # VestigeEvent (WS discriminated union, also ts-rs)

apps/dashboard/src/types/
├── generated/*.ts        # AUTO-GENERATED — do not edit by hand
├── runtime.ts            # Zod schemas for critical endpoints
└── index.ts              # Public facade: aliases, color tables, helpers

scripts/check-generated-types.sh   # CI gate
.cargo/config.toml                 # ts-rs export dir + bigint→number
```

### Workflow for a new endpoint

1. **Define the DTO** in `crates/vestige-mcp/src/dashboard/wire/<domain>.rs`:
   ```rust
   #[derive(Debug, Clone, Serialize, TS)]
   #[serde(rename_all = "camelCase")]
   #[ts(export, export_to = "MyResponseDto.ts", rename_all = "camelCase")]
   pub struct MyResponseDto {
       pub id: String,
       #[serde(skip_serializing_if = "Option::is_none")]
       #[ts(optional)]
       pub note: Option<String>,
   }
   ```
   - Re-export from `wire/mod.rs`.
   - Optional fields: `Option<T>` + `skip_serializing_if` + `#[ts(optional)]` → `field?: T` in TS.
   - Enums with discriminators: `#[serde(tag = "type", content = "data")]` — ts-rs emits a discriminated union.
   - Add `From<&DomainType> for MyResponseDto` so the handler stays one line.

2. **Wire the handler** in `dashboard/handlers/<domain>.rs`:
   ```rust
   pub async fn my_endpoint(State(state): State<AppState>)
       -> Result<Json<MyResponseDto>, StatusCode> { … }
   ```

3. **Register the route** in `dashboard/mod.rs`.

4. **Regenerate TypeScript**:
   ```sh
   cargo test -p vestige-mcp --lib dashboard
   ```
   This writes `apps/dashboard/src/types/generated/MyResponseDto.ts`.

5. **Add to the barrel export**: append `export * from "./MyResponseDto";` to `apps/dashboard/src/types/generated/index.ts`.

6. **(Optional) Alias for legacy callers** in `apps/dashboard/src/types/index.ts`:
   ```ts
   export type MyResponse = MyResponseDto;
   ```

7. **(For high-blast-radius endpoints) Add Zod runtime validation** in `apps/dashboard/src/types/runtime.ts`:
   ```ts
   const mySchema = z.object({ id: z.string(), note: z.string().optional() });
   type _Check = AssertSubset<z.infer<typeof mySchema>, MyResponseDto>;
   export const wire = { ..., my: (v: unknown) => assertWire("/my", mySchema, v) };
   ```
   Then in `stores/api.ts`: `wire.my(await fetcher<unknown>("/my"))`.

8. **Verify**:
   ```sh
   ./scripts/check-generated-types.sh   # parity gate
   cd apps/dashboard && pnpm ci         # tsc + biome + vitest
   ```

### Common pitfalls

- **Editing a Rust DTO without regenerating.** The CI gate catches it, but locally you'll get confusing TypeScript errors first. Run the regen test before debugging the dashboard.
- **Adding a field on the wire that the dashboard didn't expect.** TypeScript stays happy (extra fields are allowed), but Zod will reject — that's by design. Add the field to the schema in `runtime.ts`.
- **Returning `Json<Value>` from a new handler.** Resist the urge — the whole point is no more `serde_json::json!()` macros in the handler layer. If the upstream tool returns `Value` (`tools::reflect::execute` and friends), parse through a DTO via `parse_or_502` (see `dashboard::handlers::metacognitive`).
- **Hard-coding limits.** The dashboard learns its caps from `GET /api/_meta/limits`. New limits go in `wire::limits::DashboardLimitsDto::DEFAULT` plus a parity test, never in component-local `const`.

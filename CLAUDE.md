# Vestige — Cognitive Memory System

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
- `totalMemories` > 700 → call `find_duplicates`
- `savesSinceLastDream` > 100 → call `reflect` (metacognitive check)

> **Fallback:** If `session_context` unavailable: `search` × 2 → `intention` check → `system_status` → `predict`.

---

## What Runs Automatically vs What You Must Trigger

### Automatic (server handles it)
- **FSRS-6 consolidation** — background loop every 6h (configurable via `VESTIGE_CONSOLIDATION_INTERVAL_HOURS`)
- **Inline consolidation** — after tool calls, `ConsolidationScheduler` decides when to run mini-consolidation
- **Reconsolidation window expiry** — 5-minute labile windows auto-expire

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
{ "content": "Vestige search uses triple hybrid scoring: BM25 + semantic + RRF. The weights are 0.4/0.6 for BM25/semantic.", "tags": ["vestige", "architecture"], "node_type": "fact" }
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

**Compound query decomposition:** Queries containing semicolons, question chains, or conjunctions are automatically split into sub-queries, searched independently, and merged (union, max-score dedup). Improves MRR by +43% on multi-topic queries.

**Provenance in results:** Use `detail_level: "full"` to include provenance metadata (session, agent, entities, relations, temporal anchors) in search results.

### memory — Read, Edit, Delete, Promote, Demote, Batch Get
```json
{ "action": "get", "id": "uuid" }
{ "action": "get_batch", "ids": ["uuid-1", "uuid-2", "uuid-3"] }
{ "action": "edit", "id": "uuid", "content": "updated text" }
{ "action": "delete", "id": "uuid" }
{ "action": "promote", "id": "uuid", "reason": "was helpful" }
{ "action": "demote", "id": "uuid", "reason": "was wrong" }
{ "action": "state", "id": "uuid" }
```
`get_batch` retrieves up to 20 memories in one call (saves round-trips for expandable IDs from search).
Promote/demote adjusts ranking, does NOT delete. Demoted memories rank lower; alternatives surface instead.

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
Composite > 0.6 = save it.

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

**Workflow:** Run `split_memories` → for each compound memory, read full content with `memory(action="get")` → split into atomic facts → `smart_ingest` as batch → `memory(action="delete")` the original.

### Maintenance Tools
```json
system_status: {}
consolidate: {}
backup: {}
export: { "format": "json", "tags": ["bug-fix"], "since": "2026-01-01" }
gc: { "min_retention": 0.1, "dry_run": true }
restore: { "path": "/path/to/backup.json" }
```

---

## Mandatory Save Gates

**You MUST NOT proceed past a save gate without executing the save.**

| Gate | Trigger | Action |
|------|---------|--------|
| **BUG_FIX** | After any error is resolved | `smart_ingest` with content: `"BUG FIX: [error]\nRoot cause: [why]\nSolution: [fix]\nFiles: [paths]"`, tags: `["bug-fix", "project"]`, node_type: `"fact"` |
| **DECISION** | After any architectural/design choice | `codebase` with action: `"remember_decision"` |
| **CODE_CHANGE** | After >20 lines or new pattern | `codebase` with action: `"remember_pattern"` |
| **SESSION_END** | Before stopping or compaction | `smart_ingest` batch with tags: `["session-end"]` |
| **FACT_CHANGED** | When a previously stored fact becomes outdated | `temporal` with action: `"invalidate"`, then `smart_ingest` with the corrected fact |

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
- **When in doubt, save.** Prediction Error Gating handles dedup. Lost knowledge is permanent.
- **If `compound_content_warning` appears in response:** delete the compound memory, split content into atomic pieces, re-ingest as batch.

---

## Token Budget Strategy

- Quick lookups: `token_budget: 500`
- Normal context: `token_budget: 2000`
- Deep context: `token_budget: 3000-5000`
- Detail levels: `brief` (scan) | `summary` (default) | `full` (debug)
- Overflow goes to `expandable` — retrieve with `memory(action="get")`

---

## Development

- **Crate:** `vestige-mcp` v3.2.1, Rust 2024 edition, MSRV 1.91
- **Tools:** 27 MCP tools (core memory, cognitive, metacognitive, autonomic, maintenance, deep_reference). Canonical list lives in `vestige-mcp/src/server/catalog.rs::build_tools_list`.
- **Tests:** 1,080+ (unit + E2E + cognitive + journey + extreme) + 18 cognitive journey + 10 scientific validation
- **Build:** `cargo build --release -p vestige-mcp` (features: `embeddings` + `vector-search` + `preprocessing`)
- **Build (no embeddings):** `cargo build --release -p vestige-mcp --no-default-features`
- **Preprocessing:** entity extraction, coreference rewriting, temporal anchoring, relation extraction — all local regex/heuristic, zero model downloads. Feature-gated under `preprocessing` (default on).
- **Bench:** `cargo bench -p vestige-core`
- **Architecture:** `McpServer` → `Arc<Storage>` + `Arc<Mutex<CognitiveEngine>>`
- **Storage:** SQLite WAL mode, `Mutex<Connection>` reader/writer split, FTS5 full-text search. Implementation split across `storage/sqlite/` per concern (nodes, states, history, intentions, maintenance, embeddings, review, consolidation, search, graph, gdpr, temporal, smart_ingest, insights, records, stats). Migrations v1–v11.
- **Embeddings:** nomic-embed-text-v1.5 (768D → 384D Matryoshka truncation, 8K context) via fastembed (local ONNX, no API)
- **Reranker:** Jina Reranker v2 Base Multilingual (278M params) cross-encoder
- **Search:** Compound query decomposition + Triple hybrid scoring (BM25 + semantic + RRF), active forgetting, prospective indexing
- **Vector index:** USearch HNSW (20x faster than FAISS)
- **Binaries:** `vestige-mcp` (MCP server), `vestige` (CLI), `vestige-restore`
- **Dashboard:** React 19 + Vite 6 + React Router 7 + Three.js + Tailwind 4 + i18next (EN/PL), embedded at `/dashboard`
- **Dashboard API:** 28 REST operations across 26 paths (including `/api/reflect`, `/api/temporal`, `/api/confidence`). Handlers split per domain under `dashboard/handlers/` (memory, search, graph, history, intentions, maintenance, review, cognitive, metacognitive, observability, pages).
- **Env vars:** `VESTIGE_DASHBOARD_PORT` (default 3927), `VESTIGE_CONSOLIDATION_INTERVAL_HOURS` (default 6), `RUST_LOG`

For cognitive architecture details, see [ARCHITECTURE.md](ARCHITECTURE.md).

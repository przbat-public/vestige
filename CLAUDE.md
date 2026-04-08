# Vestige v3.1.0 — Cognitive Memory System

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

> **Fallback:** If `session_context` unavailable: `search` × 2 → `intention` check → `system_status` → `predict`.

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
**Single:**
```json
{ "content": "What to remember", "tags": ["tag1", "tag2"],
  "node_type": "fact", "source": "optional reference", "forceCreate": false }
```
**Batch (up to 20 items):**
```json
{ "items": [
  { "content": "Item 1", "tags": ["session-end"], "node_type": "fact" },
  { "content": "Item 2", "tags": ["bug-fix"], "node_type": "fact" }
] }
```
Node types: `fact` | `concept` | `event` | `person` | `place` | `note` | `pattern` | `decision`

### search — 7-Stage Cognitive Search
```json
{ "query": "search query", "limit": 10, "min_retention": 0.0,
  "min_similarity": 0.5, "detail_level": "summary",
  "context_topics": ["rust", "debugging"], "token_budget": 3000 }
```
Every search strengthens the memories it finds (Testing Effect).

### memory — Read, Edit, Delete, Promote, Demote
```json
{ "action": "get", "id": "uuid" }
{ "action": "edit", "id": "uuid", "content": "updated text" }
{ "action": "delete", "id": "uuid" }
{ "action": "promote", "id": "uuid", "reason": "was helpful" }
{ "action": "demote", "id": "uuid", "reason": "was wrong" }
{ "action": "state", "id": "uuid" }
```
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

- **Promote** when user confirms helpful, solution worked, info was accurate.
- **Demote** when user corrects mistake, info was wrong, led to bad outcome.
- **Never save:** secrets, API keys, passwords, temporary debugging state, trivial info.
- **When in doubt, save.** Prediction Error Gating handles dedup. Lost knowledge is permanent.

---

## Token Budget Strategy

- Quick lookups: `token_budget: 500`
- Normal context: `token_budget: 2000`
- Deep context: `token_budget: 3000-5000`
- Detail levels: `brief` (scan) | `summary` (default) | `full` (debug)
- Overflow goes to `expandable` — retrieve with `memory(action="get")`

---

## Development

- **Crate:** `vestige-mcp` v3.1.0, Rust 2024 edition, MSRV 1.91
- **Tests:** 1,080+ (unit + E2E + cognitive + journey + extreme), zero warnings
- **Build:** `cargo build --release -p vestige-mcp` (features: `embeddings` + `vector-search`)
- **Build (no embeddings):** `cargo build --release -p vestige-mcp --no-default-features`
- **Bench:** `cargo bench -p vestige-core`
- **Architecture:** `McpServer` → `Arc<Storage>` + `Arc<Mutex<CognitiveEngine>>`
- **Storage:** SQLite WAL mode, `Mutex<Connection>` reader/writer split, FTS5 full-text search
- **Embeddings:** nomic-embed-text-v1.5 (768D → 384D Matryoshka truncation, 8K context) via fastembed (local ONNX, no API)
- **Reranker:** Jina Reranker v2 Base Multilingual (278M params) cross-encoder
- **Search:** Triple hybrid scoring (BM25 + semantic + RRF), active forgetting, prospective indexing
- **Vector index:** USearch HNSW (20x faster than FAISS)
- **Binaries:** `vestige-mcp` (MCP server), `vestige` (CLI), `vestige-restore`
- **Dashboard:** React 19 + Vite 6 + React Router 7 + Three.js + Tailwind 4, embedded at `/dashboard`
- **Env vars:** `VESTIGE_DASHBOARD_PORT` (default 3927), `VESTIGE_CONSOLIDATION_INTERVAL_HOURS` (default 6), `RUST_LOG`

For cognitive architecture details, see [ARCHITECTURE.md](ARCHITECTURE.md).

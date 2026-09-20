# Docs ↔ dashboard ↔ backend consistency audit — 2026-09-20

**Scope.** `docs/**`, the root prose (`README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `SECURITY.md`),
the agent-facing files at the root (`CLAUDE.md`/`GEMINI.md` are symlinks to `AGENTS.md`;
`CLAUDE.md.template` is a copy-paste template, not a symlink), `packages/*/README.md`,
`crates/vestige-mcp/README.md`, `apps/dashboard/README.md`, `.env.example`, and the dashboard
frontend (`apps/dashboard/src/**`) against the write-gate / record-time / erasure work of 2026-09-19/20.

**Method.** Every backend behaviour was verified in source before being used as the standard; every
doc claim below was read at the line cited. The two background streams (`crates/vestige-core/**`,
`crates/vestige-mcp/src/tools/smart_ingest/**`) were read-only: no file under those paths was written.

## 0. Ground truth verified in the code

| Behaviour | Evidence |
|---|---|
| V17: `knowledge_nodes.recorded_at` + append-only `memory_revisions` (kind ∈ create/edit/supersede/invalidate/quarantine, `old_content`, `new_content`, `reason`, `actor`), backfilled `recorded_at = created_at` | `crates/vestige-core/src/storage/migrations/sql.rs:797-820`; node field `crates/vestige-core/src/memory/node.rs:202` |
| `recordedAt` is on MCP **search** results only | `crates/vestige-mcp/src/tools/search_unified/format.rs:34,63,93` |
| V18: `self_contained` INTEGER (NULL = gate never ran, 1 = passed, 0 = flagged) + `self_contained_findings`, not backfilled | `sql.rs:840-848` |
| Every write is gated; refusal is flat `success:false, decision:"reject", stored:false, reason, guidance, findings[{kind,span,hint}]` and writes nothing | `crates/vestige-mcp/src/tools/smart_ingest/execute.rs:167-169`; `smart_ingest/self_contained.rs:173-198` |
| Flagged (not refused) write attaches `self_contained` **only when flagged**: `{ok, requiresContext, rejected, rejectReason, findings[]}` | `execute.rs:297-299,381-383,442-444`; `self_contained.rs:149-161` |
| Batch: per-item `status:"rejected"/decision:"reject"/stored:false` and a summary `rejected` count | `smart_ingest/batch.rs:52-71,162-170,353-356` |
| **Correction to the brief:** empty content in *single* mode is a transport tool error, not a reject response; only *batch* items get `stored:false` for empty content | `execute.rs:52-59`; `batch.rs:60-71` |
| Edits append a revision instead of overwriting | `crates/vestige-core/src/storage/sqlite/nodes.rs:137,386`; `revisions.rs:47` |
| Revisions have **no reader outside core**: `get_memory_revisions`/`get_latest_revision` are called only from core tests | `revisions.rs:95,117`; `grep -rn` over `crates/` shows no `vestige-mcp` caller |
| Erasure (tool `erase`, `POST /api/maintenance/erase`, `vestige erase --id|--tag`) also deletes content history and every derived table | `crates/vestige-mcp/src/server/catalog.rs:264-272`; `dashboard/mod.rs:193`; `bin/cli/main.rs:97-110,173-178`; `storage/sqlite/gdpr.rs:1-20,81-85` |
| `/metrics`; `/api/health` → 503 only for `critical`; `memory(action="delete")` needs `confirmed:true`; `gc(dry_run:false)` needs `confirmed:true` | `dashboard/mod.rs:203`; `dashboard/handlers/observability.rs:99-110`; `tools/memory_unified/actions.rs:135-142`; `tools/maintenance/gc.rs:74-83` |
| Unknown tool → `-32602`, unknown resource → `-32002` | `server/dispatch.rs:243-262`; `server/tests.rs:488-507` |
| 29 tools; 42 `.route(` registrations; migrations v1–v18 | `catalog.rs:139-454` (counted); `dashboard/mod.rs:151-247`; `scripts/check-version-and-tools.sh:125,176-180` |

---

## 1. Mismatches

### 1.1 Root docs — **report only** (another change is already touching these files)

| file:line | what it says | what the code does | exact fix |
|---|---|---|---|
| `README.md:15` | `[Tools](#-28-mcp-tools)` | the heading it points at is `README.md:262` `## 🛠 29 MCP Tools`, so the anchor is `#-29-mcp-tools` and the current link is dead | `[Tools](#-29-mcp-tools)` |
| `README.md:446` | `curl http://localhost:3927/api/health` / `# Should return {"status":"healthy",...}` | `observability.rs:99-110`: `critical` (avg retention < 0.3) answers **503** with the same body; an operator following this troubleshooting note reads 503 as "dashboard broken" | append: `# 200 = healthy/degraded/empty; 503 = critical (avg retention < 0.3) — the body carries the same health JSON either way` |
| `README.md:323` | erase "also removes the derived data: connections, embeddings, access log, state history and every insight derived from the memory" | `gdpr.rs:81-85` also deletes `memory_revisions` — the one table that stores the memory's text a second time | insert `content history (memory_revisions),` into that list |
| `ARCHITECTURE.md:173` | "The CI metadata gate verifies the **MCP tool count** (28)" | `scripts/check-version-and-tools.sh:125` pins **29**, and `ARCHITECTURE.md:20` already says 29 — the file contradicts itself; the gate passes because it only greps for the right number existing somewhere | `(29)` |
| `ARCHITECTURE.md:52-55` | sqlite submodule list: "nodes, states, history, intentions, maintenance, embeddings, fsrs_personalization, review, consolidation, search, graph, gdpr, temporal, smart_ingest, insights, records, stats" | `crates/vestige-core/src/storage/sqlite/revisions.rs` (V17's content history) is missing from the enumeration | add `revisions,` (and note `snapshot_restore` if the list is meant to be exhaustive) |
| `ARCHITECTURE.md:141-151` | "Ingest Pipeline" runs Preprocessing → Pre → Store(Prediction Error Gating) → Post, with no gate stage | every write first passes the self-containedness gate and can be **refused** (`execute.rs:156-169`), and the verdict is persisted (`execute.rs:200-201`); the refusal outcome is absent from the architecture description entirely | insert before `- **Pre:**`: `- **Gate:** self-containedness check — content the repository already owns is refused (\`decision: "reject"\`, nothing written); a memory that needs the conversation is stored and flagged (\`knowledge_nodes.self_contained=0\` + \`self_contained_findings\`)` |
| `CONTRIBUTING.md:182` | "`server/catalog.rs` \| Canonical list of MCP tools (28) and resources (11)" | 29 tools (`catalog.rs:139`, `check-version-and-tools.sh:125`) | `(29)` |
| `CONTRIBUTING.md:173` | `storage/sqlite/` per-concern list | omits `revisions` (V17) | add `revisions,` |
| `CONTRIBUTING.md:232-233` | "Update the tool count in `README.md`, `ARCHITECTURE.md`, `crates/vestige-mcp/README.md`, and `AGENTS.md`" | correct as a procedure, but the file it names is in fact stale (see 1.3); the tool-count check in `scripts/check-version-and-tools.sh:125-140` covers only `README.md` and `ARCHITECTURE.md`, and `CONTRIBUTING.md` is checked only for the migration range (`:156-168`) | leave the procedure; consider extending the gate to cover `CONTRIBUTING.md` and `crates/vestige-mcp/README.md` |
| `SECURITY.md:103-107` | destructive-gate paragraph lists `gc`, `restore`, `erase` | `memory(action="delete")` now also requires `confirmed:true` (`actions.rs:135-142`), and `erase` also purges content history (`gdpr.rs:81-85`) — both absent | add: `` `memory(action="delete")` requires `confirmed: true` (`tools/memory_unified/actions.rs`); `erase` additionally removes the memory's content history (`memory_revisions`) `` |
| `CLAUDE.md.template:40-41` | bug-fix memory body contains `Solution: [how we fixed it]` and `Files: [affected files]` | `AGENTS.md:318` now says file paths/line numbers stay **out of the content** and `Solution: [the diff]` is not a memory; `crates/vestige-mcp/src/tools/smart_ingest/schema.rs:26` repeats the same rule to callers | replace with `Lesson: [the rule to apply next time]"` and delete the `Files:` line |
| `CLAUDE.md.template:118` | "1. Run `health_check` — check overall system status" | `health_check` was replaced by `system_status` (`server/catalog.rs:208`) and survives only as a deprecated alias; the 29-tool catalog advertises `system_status` | `system_status` |
| `CLAUDE.md.template:108-140` | maintenance section has no erasure guidance, no `confirmed:true`, no flagged/refused-save handling | `erase` is a first-class tool/route/CLI command; refuses without `confirmed:true` | add an `erase` line mirroring `AGENTS.md:326-333` |

### 1.2 `docs/**` — mismatches found; ✅ = fixed by me (see §3)

| file:line | what it says | what the code does | exact fix |
|---|---|---|---|
| `docs/CONFIGURATION.md:88-96` ✅ | CLI list: stats/health/consolidate/dashboard/restore — no `erase` | `vestige erase --id\|--tag [--dry-run] [--confirm]` exists (`bin/cli/main.rs:97-110`) and is documented in `README.md:393-394` | added the two `vestige erase` lines + a paragraph naming the confirmation gate and the content-history blast radius (now `:94-100`) |
| `docs/CLAUDE-SETUP.md:29` ✅ | "`needsGc` → call `gc` with `dry_run: true`, review, then delete" | the destructive pass is refused without `confirmed:true` (`tools/maintenance/gc.rs:74-83`) | "…review, then run it again with `confirmed: true` to actually delete" |
| `docs/CLAUDE-SETUP.md:69` ✅ | `- Content: "BUG FIX: [error message] \| Root cause: [why] \| Solution: [how]"` | contradicts `AGENTS.md:318`: paths/diffs stay out of content, the durable part is the lesson | rewritten to `Lesson: [the rule to apply next time]` + a line pointing paths at `source` |
| `docs/CLAUDE-SETUP.md:108-112` ✅ | "MEMORY HYGIENE: promote / demote / never save" — no write-gate guidance | every write returns either a flag or a refusal (`self_contained.rs:149-197`); the canonical rule is "state the future decision this changes" (`AGENTS.md:353`) | added that rule and a three-outcome block for the response |
| `docs/FAQ.md:243-254` ✅ | "When you call `smart_ingest`, Vestige doesn't just blindly add memories: 1. Compares … 2. Decides" | the refusal/flag gate runs **before** the similarity comparison (`execute.rs:156-169`) | prepended step 0 ("Refuses first") with the reject/flag outcomes (now `:245`) |
| `docs/SCIENCE.md:25` ✅ | "When you call `smart_ingest`, Vestige compares new content against existing memories" | same as above; `docs/SCIENCE.md` is what `README.md:404` links as "the neuroscience behind every feature" | prepended the gate paragraph and "Only then does `smart_ingest` compare…" |
| `docs/integrations/windsurf.md:118` ✅ | "Vestige uses 19 tools" | 29 (`catalog.rs:139`, gate `:125`) | `29` |
| `docs/integrations/xcode.md:54` ✅ | "you should see `vestige` listed with 19 tools" | 29 | `29` |
| `docs/blog/xcode-memory.md:47` ✅ | "you'll see 23 Vestige tools loaded" | 29 — and the same post says 29 at `:86` | `29` |
| `docs/integrations/xcode.md:90` | "the agent … identifies it as wrong, and deletes it autonomously" | `memory(action="delete")` is now refused without `confirmed:true` (`actions.rs:135-142`); full removal is `erase` | "…marks it wrong and demotes it; deleting it needs explicit `confirmed: true`, and erasing it (with its derived data) is the `erase` tool" |
| `docs/integrations/codex.md:43` | "After solving bugs: call `smart_ingest` to save the root cause and fix." | `AGENTS.md:318-320` and the tool schema (`smart_ingest/schema.rs:22-30`) require the *lesson*; a diff/fix is not a memory | "save the root cause and the lesson (paths in `source`, not in the content)" |
| `docs/STORAGE.md:176-187` | "Direct SQL Access" example queries cover `knowledge_nodes` only | V17 added `recorded_at` and `memory_revisions`; `recorded_at` is the honest "when was this saved" column (it is not refreshed by search, unlike `last_accessed`) | add e.g. `SELECT recorded_at, content FROM knowledge_nodes ORDER BY recorded_at DESC LIMIT 10;` and `SELECT kind, recorded_at FROM memory_revisions WHERE node_id = '…' ORDER BY id DESC;` |
| `docs/STORAGE.md:119-129` | "Data Safety" section: backup guidance, "Vestige is not designed for… compliance guarantees" | no mention that a complete erasure path exists (three surfaces) | add one sentence pointing at `erase` / `POST /api/maintenance/erase` / `vestige erase` |
| `docs/FAQ.md` (whole file) | no mention of `erase`, `confirmed`, `self_contained` or `reject` anywhere | all four are user-visible | add a FAQ entry: "How do I delete everything about one subject?" → `erase` with dry-run first, and the fact it purges content history + derived data |
| `docs/CONFIGURATION.md` / `README.md` / `SECURITY.md` | `/metrics` is documented **only** in `ARCHITECTURE.md:229,306` | `GET /metrics` is live (`dashboard/mod.rs:203`, `observability.rs:112-130`): store gauges, per-stage `try_lock` skip counters, WS subscribers, uptime | add one line to `README.md`'s dashboard section and/or a "Monitoring" note to `docs/CONFIGURATION.md` |

### 1.3 Packages & crate READMEs — report only (outside my writable set)

| file:line | what it says | what the code does | exact fix |
|---|---|---|---|
| `crates/vestige-mcp/README.md:42` | `## Available Tools (28)` | 29 tools; `erase` is in the catalog (`catalog.rs:265-272`) but the table at `:46-88` jumps from `gc` (`:87`) to `restore` (`:88`) | `(29)` + an `erase` row |
| `crates/vestige-mcp/README.md:49,54,63,68` | `search`, `session_context`, `deep_reference`, `reflect` annotated "read-only, idempotent" | all four are `retrieval_mutating`: `read_only_hint=false`, `idempotent_hint=false` (`catalog.rs:151,343,370,396`; the helper at `catalog.rs:73-84` explains why: retrieval strengthens FSRS state) | `mutating (retrieval strengthens FSRS state), not idempotent` |
| `crates/vestige-mcp/README.md:86` | `export` "read-only, idempotent" | `mutating("Export memories", false)` — it writes a file to disk; `catalog.rs:501` has a test pinning that these five are **not** advertised read-only | `mutating` |
| `packages/vestige-mcp-npm/README.md:53-60` | CLI block lists stats/health/consolidate, no `erase` | `vestige erase` exists (`bin/cli/main.rs:97-110`) | add `vestige erase --id <uuid> \| --tag <tag> [--dry-run] [--confirm]` |

### 1.4 Frozen or record material — **no action recommended**

- `docs/launch/*` is frozen by its own README (`docs/launch/README.md:3-11`: "intentionally are *not* kept in lockstep with the codebase"), so the stale counts are policy: `blog-post.md:45` "19 MCP tools", `blog-post.md:112` "Memories are never hard-deleted" (false since `erase` exists), `show-hn.md:156,360,421,486` "21 tools", `demo-script.md:108` "Twenty-eight tools" (while `:386` says 29). If the freeze is kept, the only worthwhile change is a one-line footnote on `blog-post.md:112`.
- `docs/review/STATUS.md` is pinned to `811f40c → a2dfbec` (its `:3`) yet reads as a current closure status, and its own header (`:11`) says the metadata gate reports **29 narzędzi / 42 trasy** while the body still says: `:49,51,52,159,215` "28 narzędzi"; `:83,120` "erasure … not reachable from any interface" (now reachable: tool + REST + CLI); `:126` "`memory(action="delete")` nadal nie wymaga potwierdzenia" (it does now); `:127,281` "prefix `LIKE` … still there / DO ZROBIENIA before exposing erasure" (exact `json_each` match now, `gdpr.rs:165-176`). Either re-run the closure round or add a dated note at the top.
- `docs/review/REVIEW-2026-09-19.md` is pinned to commit `811f40c` (`:3`) — historical, no action.
- `docs/review/attachments/*` (other than `memory-quality-*.md`) are research records, e.g. `REPORT-long-lived-local-mcp-memory-server.md:3,545` says "~28 MCP tools".
- `docs/SELF-CONTAINED-MEMORY-DESIGN.md` (excluded from editing as a research record) is the pre-implementation spec: its `:78` says the migration range becomes **v1–v17** and `:122` says "`GateDecision` knows only Create/Update/Supersede/Merge … there is no way to say 'not worth storing'". Both were true when written; V18 (`sql.rs:840-848`) and the `Reject` variant (`smart_ingest/self_contained.rs:173-198`) supersede them. Do not patch a dated design record.

---

## 2. Frontend

### 2.1 Received (or reachable) but not shown

The dashboard's DTO layer is the choke point: `MemoryDto` (`crates/vestige-mcp/src/dashboard/wire/memory.rs:30-89`) is built by `From<&KnowledgeNode>` at `wire/memory.rs:138-169`, which copies `created_at`/`updated_at`/`last_accessed` but **drops `recorded_at`** even though `KnowledgeNode.recorded_at` exists (`crates/vestige-core/src/memory/node.rs:202`). Every list/search/detail row goes through it (`handlers/memory.rs:124`, `handlers/search.rs:66-71`), and `apps/dashboard/src/types/generated/MemoryDto.ts` has no such field. `grep -rn "recordedAt\|recorded_at" apps/dashboard/src` = 0 code hits; `MemoryMetadataFooter.tsx:62-73` renders Created / Last updated / Last accessed only. **So record time is not displayable today without a backend DTO change** (plus `apps/dashboard/src/types/runtime.ts:67-89` and the generated `.ts`).

The **raw write response is already received**: `handlers/memory.rs:301-349` returns the tool's `Value` verbatim (`Ok(Json(result))`), so a dashboard save gets `decision`, `stored`, `reason`, `guidance`, `findings[{kind,span,hint}]` and, when flagged, `self_contained{ok,requiresContext,rejected,rejectReason,findings[]}`. The frontend never sees them because the response type is hand-written at `apps/dashboard/src/stores/api.ts:131-148` and omits `stored`/`self_contained`/`guidance`/`findings`; `grep selfContained|self_contained apps/dashboard/src` = 0 hits.

Also unused: `EraseRequestDto`/`EraseResponseDto` are generated and re-exported (`types/generated/index.ts:32-33`) but no runtime file imports them — `api.maintenance` (`stores/api.ts:153-203`) has no `erase`, and `MaintenancePanel.tsx` has no erasure section. `HealthCheckDto.totalMemories`/`averageRetention` (`wire/observability.rs:46-47`) and `SystemStatsDto.averageRetrievalStrength`/`oldestMemory`/`newestMemory` are received and never rendered. `/metrics` has no frontend reference at all.

### 2.2 Shown, and now wrong

1. **A rejected write is reported as a success — `apps/dashboard/src/components/memories/AddMemoryDialog.tsx:112-119`.** `if (decision === 'create' || decision === 'supersede') … else { toast(t('addMemory.toastMerged', { decision }), 'info') }` then `reset(); onClose();`. A refusal arrives as **HTTP 200** (`handlers/memory.rs:349`), `decision` is `"reject"`, so the user is told *"Smart-ingest decided to reject — see dialog for details."* (`i18n/en.json:495`), the dialog closes and the text is discarded — while nothing was written. The inline alert at `:262` is keyed on `decision === 'create'` for success colour and `i18n/en.json:502-510` has no `decision.reject` key. Fix: branch first on `result.stored === false || result.decision === 'reject'`, render `reason`/`guidance`/`findings`, keep the dialog open, and never `reset()` on a refusal.
2. **The destructive GC button always fails — `apps/dashboard/src/components/MaintenancePanel.tsx:82-83`** sends `api.maintenance.gc({ dry_run: false, min_retention: 0.1 })`; `tools/maintenance/gc.rs:78` refuses without `confirmed: true`, `handlers/maintenance.rs:15-20` maps the error string to **500**, so after the themed confirm dialog (`:103-111`) the user always gets a generic error toast. Fix: pass `confirmed: true` once the confirm resolves (and widen the `gc` param type at `stores/api.ts:189`), or add a dedicated route.
3. **"Edit history" is a mislabel — `i18n/en.json:307`** (`pl.json:347` "Historia zmian") titles a panel that only lists *state transitions* (`MemoryChangelogPanel.tsx:49`; its own empty state at `en.json:308` says "No state transitions recorded yet."; backend `handlers/history.rs:37` calls `get_state_transitions`). Since V17 real content history exists (`memory_revisions`) but has no DTO (nothing matching `Revision` in `dashboard/wire/**`) and no endpoint. Either relabel to "State history" or add `GET /api/memories/{id}/revisions`.
4. **Tutorial copy describes an unconditional save — `i18n/en.json:876-877`** ("…it sends it to Vestige. Vestige analyzes it … and **stores it with full strength**"), `pl.json:959` likewise. The server may refuse or flag it. Similarly `en.json:1029` (`pl.json:1111`) answers "Can Vestige delete my memories without asking?" without mentioning erasure, and `en.json:980` describes Settings without it.
5. **"What's new" banner is behind — `components/tutorial/changelog.ts:20-32`** stops at 3.3.0 while the crate is 3.4.0 (`Cargo.toml:15`), and no `tutorial.whatsNew.v3_4.*` keys exist, so neither the write gate nor erasure is announced.

### 2.3 Explicitly **nothing to change**

- **Health-status handling is already correct**: `stores/api.ts:207-209` accepts `[200, 503]` with a comment explaining the 503-means-critical contract, and `StatsPage.tsx:41-46` maps all four statuses. The sidebar connection dot is WebSocket-driven, not `/api/health`-driven.
- **No stale counts in the UI**: greps for `29`, `42`, `migration`, `V17|V18` found no source hits; the limits list matches `DashboardLimitsDto::DEFAULT` and is fetched live.
- **Delete flows are safe as written**: single delete uses a 5 s undo window (`hooks/useMemoryMutations.ts:31,175-200`) and bulk delete a themed alertdialog. Not sending `confirmed: true` is correct for the dashboard because `DELETE /api/memories/{id}` (`handlers/memory.rs:149-183`) calls `storage.delete_node` directly and has no gate — the `confirmed` requirement belongs to the MCP `memory(action="delete")` path, which the dashboard never calls. This REST/MCP asymmetry is a **backend** observation, not a frontend defect.
- `apps/dashboard/README.md` makes no claim about any changed behaviour — no edit needed.

---

## 3. What I fixed myself

All under `docs/**` (the exclusions were respected; `AGENTS.md`, `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `SECURITY.md`, `crates/**`, `packages/**` were left untouched):

1. `docs/CONFIGURATION.md:94-100` — added `vestige erase --tag <tag> --dry-run` / `--confirm` to the CLI block and a paragraph noting the single-target rule, the confirmation gate, and that it is the only path that also removes content history.
2. `docs/CLAUDE-SETUP.md:29` — GC trigger now says the destructive pass needs `confirmed: true`.
3. `docs/CLAUDE-SETUP.md:69-70` — bug-fix template now matches the `AGENTS.md:318` save gate (`Lesson:`, paths kept out of content).
4. `docs/CLAUDE-SETUP.md:114-119` — added the "state the future decision this changes" hygiene rule and a three-outcome block for reading the write response.
5. `docs/FAQ.md:245` — the Prediction Error Gating answer now opens with the refusal/flag step.
6. `docs/SCIENCE.md:25-27` — same gate paragraph, so "Vestige compares new content" is no longer stated unconditionally.
7. `docs/integrations/windsurf.md:118`, `docs/integrations/xcode.md:54`, `docs/blog/xcode-memory.md:47` — tool count 19/19/23 → 29.

Deliberately **not** edited, because the wording would have to be invented or the file is a record: every row in §1.1 (report-only by instruction), §1.3, the erasure FAQ entry, the `docs/STORAGE.md` additions, the `/metrics` documentation, `docs/review/STATUS.md`, `docs/launch/**`, and `docs/SELF-CONTAINED-MEMORY-DESIGN.md`.

---

## 4. Could not determine

- **Why the dashboard's REST `DELETE /api/memories/{id}` deliberately bypasses the `confirmed: true` gate** that the MCP tool enforces (`handlers/memory.rs:164-167` vs `actions.rs:135-142`). No comment states an intent; I report the asymmetry rather than guess whether it is a bug or a decision.
- **Whether a revision-read surface is planned.** `get_memory_revisions` (`revisions.rs:95`) has no caller outside core tests, and no `Revision` DTO exists in `dashboard/wire/**`; `docs/SELF-CONTAINED-MEMORY-DESIGN.md` is silent on how content history should be read. Not enough evidence to call it an oversight.
- **`docs/review/STATUS.md`'s status.** It is pinned to a commit range yet reads as current; I could not tell whether it is maintained or closed, so I did not edit it.
- **`docs/launch/**` intent for the one factually false line** (`blog-post.md:112` "Memories are never hard-deleted"). The freeze notice covers stale numbers explicitly but not a false capability claim; flagged for the owner.
- **Whether `/metrics` is meant to be user-facing.** It is absent from every doc except `ARCHITECTURE.md`; the route comment (`dashboard/mod.rs:199-202`) says it is "the process talking to the monitoring system", which may make the omission deliberate.
- **Dashboard copy in Polish.** I checked `pl.json` for the four frontend items above (tutorial birth `:959`, FAQ a1 `:1111`, changelog title `:347`) but did not audit the rest of `pl.json` string-by-string for the same claims.

---

## Root docs — resolved

**Added 2026-09-20 by the follow-up docs pass.** Every row below was re-verified in source before the edit, not taken from §1.1/§1.3 on trust. Line numbers are the *pre-edit* ones this audit cites. Behaviour changed again while this pass ran — see the closing note.

### `crates/vestige-mcp/README.md` — the security-relevant one

The core defect was worse than "stale text": the table annotated five tools `read-only, idempotent` while the catalog marks them `readOnlyHint:false`. MCP clients use those hints for **auto-approval**, so the document contradicted the contract a client is supposed to trust.

| Mismatch | Change |
|---|---|
| `:42` `## Available Tools (28)` | → `(29)`; the documented name set is now byte-identical to the catalog's `tool(` entries (diffed programmatically: 29 vs 29, no extras, no omissions) |
| no `erase` row (table jumped `gc` → `restore`) | added an `erase` row between them: exact-tag semantics, the derived-data list **including content history (`memory_revisions`)**, the `dry_run`/`confirmed` gate, `destructive, idempotent` |
| `:49` `search` "read-only, idempotent" | → `mutating (retrieval strengthens FSRS state), not idempotent` |
| `:54` `session_context` "read-only, idempotent" | → same |
| `:63` `deep_reference` "read-only, idempotent" | → same |
| `:68` `reflect` "read-only, idempotent" | → same |
| `:86` `export` "read-only, idempotent" | → `mutating (writes a file to disk), not idempotent` |
| — | added a paragraph under the heading stating the rule the five share, so the correction is not just five isolated cells |

**Verification.** A script extracted each tool's `ToolAnnotations` helper from `catalog.rs` (`read_only_safe` / `retrieval_mutating` / `mutating(idem)` / `destructive(idem)`) and compared it cell-by-cell against the README — 29/29 rows now agree. This caught one the audit did *not* list: `export` is `mutating("Export memories", false)`, i.e. **not** idempotent, so the audit's suggested fix (`mutating`) was incomplete. `temporal` was re-checked and is correct as written (`destructive(…, true)`).

### Root prose

| Mismatch | Change |
|---|---|
| `README.md:15` dead `[Tools](#-28-mcp-tools)` | → `#-29-mcp-tools` |
| `README.md:446` `# Should return {"status":"healthy",...}` | → `# 200 = healthy/degraded/empty; 503 = critical (avg retention < 0.3) — the body carries the same health JSON either way` |
| `README.md:323` erase derived-data list | inserted `content history (memory_revisions)` |
| `README.md:276` `memory` row (not in the audit) | added: `delete` is permanent and needs `confirmed: true` — a call without it is refused |
| `ARCHITECTURE.md:173` gate verifies tool count `(28)` | → `(29)` |
| `ARCHITECTURE.md:52-55` sqlite submodules omit `revisions` | added `revisions` plus the modules the list was also missing (`tags`, `connections`, `snapshot_restore`) |
| `ARCHITECTURE.md:141-151` ingest pipeline has no gate stage | inserted a `**Gate:**` stage before `**Pre:**`, naming the refusal (`decision:"reject"`, nothing written) and the flag (`self_contained=0` + `self_contained_findings`) |
| `CONTRIBUTING.md:182` tools `(28)` | → `(29)` |
| `CONTRIBUTING.md:173` sqlite submodule list | added `revisions`, `tags`, `connections`, `snapshot_restore` |
| `SECURITY.md:103-107` gate paragraph | added the `memory(action="delete")` requirement **and** what it does *not* purge. The second half departs from the audit's wording on purpose: `delete_node` deletes only `knowledge_nodes`, and CASCADE removes state/embeddings/edges/access-log — but `memory_revisions` and `insights.source_memories` have no FK, so they are what actually survive. Saying "erase additionally removes content history" alone would still have let a reader believe a delete was complete. |
| `CLAUDE.md.template:40-41` `Solution:` + `Files:` | → `Lesson: [the rule to apply next time]`, `Files:` deleted, plus the rule that paths live in `source`/`code_refs` |
| `CLAUDE.md.template:118` `health_check` | → `system_status` (noting `health_check` survives as a deprecated alias) |
| `CLAUDE.md.template:108-140` no erasure / no gate guidance | added an **Erasure (GDPR Art. 17)** block (three surfaces, exact-tag semantics, derived data, dry-run first) and a **Reading a Save Response** block (unflagged / flagged / `decision:"reject"`) |
| `CLAUDE.md.template:94` `promote_memory` (not in the audit) | → `memory(action="promote")`; `promote_memory` is a deprecated alias (`server/deprecated.rs:51`) and is not in the 29-tool catalog |

### `packages/*/README.md`

One mismatch, as in §1.3 — no others exist: `grep -nE '[0-9]+ ?(MCP )?tools|migration|v1[0-9]|route' packages/*/README.md` returns exactly one hit, and it is the already-corrected SQLite page-size note.

| Mismatch | Change |
|---|---|
| `packages/vestige-mcp-npm/README.md:53-60` CLI block | added `vestige erase --id <uuid> \| --tag <tag> [--dry-run] [--confirm]` with the gate and the derived-data note; the install table row now reads "stats, health checks, maintenance, and GDPR erasure" |

The brief for this pass expected four package mismatches; the code says one. `packages/vestige-mcpb/README.md` (63 lines) makes no tool-count or save-behaviour claim, and `packages/vestige-init/` ships no README at all. Reported rather than padded.

### `docs/review/STATUS.md`

Conclusions were **not** rewritten. A new `## 0. Adendum — co domknięto po \`a2dfbec\`` section was inserted after the status legend (Polish, matching the document), with a table mapping each stale entry to its current state and evidence, and an explicit note that sections 1–13 describe the `a2dfbec` snapshot and that verifier ratings there are historical.

| Entry | Now |
|---|---|
| `:83`, `:120` erasure unreachable | implemented — three surfaces (`server/catalog.rs:266-274`, `dashboard/mod.rs:199`, `bin/cli/main.rs:98-113,173-178`), `22038070` |
| `:126` `memory(action="delete")` ungated | implemented — `tools/memory_unified/actions.rs:142-147`, `edeac324`. Confirmed by source archaeology, not by the commit message: `git show a2dfbec:crates/vestige-mcp/src/tools/memory_unified/*.rs \| grep confirmed` is empty, so the entry was accurate when written |
| `:84`, `:127`, `:281` prefix `LIKE` tag matching | implemented — exact `json_each` match at `storage/sqlite/gdpr.rs:177`, `b06f35c0`; the "DO ZROBIENIA before exposing erasure" precondition is satisfied |
| `:83`, `:127` erase/backup notes | content history is now deleted too (`gdpr.rs:81-85`); batch is transactional (`gdpr.rs:161-185`) |
| `:49, :51, :52, :159, :215` "28 narzędzi" | not rewritten; the addendum records that the catalog now has 29 and that `mark_reviewed` became `memory(action="review")` with a back-compat alias |
| `:128` delete does not clean the HNSW sidecar | still open — recorded as still open in the addendum, with the FK analysis explaining why `memory_revisions`/`insights` survive `delete` |

### Note for whoever reads this next: the migration range moved again

While this pass was running, an uncommitted **V19** (`code_refs`) appeared in `crates/vestige-core/src/storage/migrations/registry.rs:98-103`, so every "v1–vN" claim in these files was corrected to **v1–v19**, not to the v1–v18 this audit's §0 assumed. `./scripts/check-version-and-tools.sh` is what caught it.

### Out of scope, reported

- **`AGENTS.md` still advertises `v1–v18`** (and `CLAUDE.md`/`GEMINI.md` are symlinks to it). It is the one file the metadata gate still fails on. It was left untouched here by instruction; the parent agent owns it.
- The audit's §1.2 items (`docs/STORAGE.md`, the erasure FAQ entry, `/metrics` documentation) were not in this pass's writable set either, and remain as the audit left them.


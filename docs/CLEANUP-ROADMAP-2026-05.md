# Cleanup roadmap — May 2026

**Status:** Outdated — most items dropped after re-audit on May 14, 2026
**Author:** May 14, 2026
**Updated:** May 14, 2026 — see "Audit corrections" section below

## Audit corrections (May 14, 2026)

The May 13 audit that produced this roadmap was based on **naïve `grep` counts and outdated assumptions**. The May 14 re-audit, this time reading the actual code, contradicted four of five tickets:

| # | Original ticket | What the re-audit found | Status |
|---|---|---|---|
| 1 | AGENTS.md | Already in repo | ✅ done (unchanged) |
| 2 | `outputSchema` on 5 MCP tools | `catalog.rs:17-23` documents an explicit maintainer decision to **not** publish `outputSchema` until tools migrate to dual-format `text + structuredContent`. Adding it now would advertise a contract Vestige doesn't honor and break strict clients. | ❌ **dropped** |
| 3a | Split `dream.rs` (>800 LOC) | The file at the path I named doesn't exist; the actual hot-spot is `vestige-core/src/advanced/dreams/dreamer.rs` (745 LOC). | ✅ **shipped May 14** — split into `dreamer.rs` (183) + 5 phase modules, 29/29 tests pass |
| 3b | Split `search.rs` (>700 LOC) | Already split: `vestige-mcp/src/tools/search_unified/` and `vestige-core/src/search/` are both directories. The largest single file in either tree (`search/vector.rs`, 525 LOC) is below the splitting threshold. | ❌ **moot** |
| 4 | `.unwrap()` pilot in `dream.rs` (24 calls) | All 24 `.unwrap()` calls are inside the `#[cfg(test)] mod tests` block — `unwrap()` in tests is idiomatic. **Zero production unwraps** in `tools/dream.rs`. | ❌ **moot** |
| 5 | `.unwrap()` cleanup across remaining MCP (~150 calls) | A grep that filters `#[cfg(test)]` blocks finds **zero production unwraps** in `crates/vestige-mcp/src/tools/*.rs` and `crates/vestige-mcp/src/dashboard/handlers/*.rs`. The MCP layer is already error-handling-clean. | ❌ **moot** |

### Methodological lesson

The original roadmap counted unwraps with `rg '\.unwrap\(\)' | wc -l` and ranked by raw count, which mixed test code and production code into a single number. Always exclude `#[cfg(test)]` blocks before claiming "N production unwraps" — the difference between 261 and 0 is the difference between a real cleanup ticket and a phantom one.

The original sizing tables (preserved below for context) are kept as a record of how the audit went wrong, not as guidance.

---

## Original (pre-correction) sequence — kept for posterity

The pre-correction plan was:

| # | Category | Risk | Effort | Why this order |
|---|---|---|---|---|
| 1 | ~~AGENTS.md~~ | none | done | Already in repo |
| 2 | ~~`outputSchema` on 5 high-value tools~~ | ~~low~~ | ~~S~~ | Rejected by maintainer (see audit corrections) |
| 3 | ~~Split `dream.rs` and `search.rs`~~ | low | S | Only `dreamer.rs` needed splitting; shipped May 14 |
| 4 | ~~`.unwrap()` reduction — `dream.rs` pilot~~ | medium | M | All 24 unwraps were in tests — no work to do |
| 5 | ~~`.unwrap()` reduction — remaining ~150 calls in MCP~~ | medium | L | Zero production unwraps found — no work to do |

## Ticket 2 — `outputSchema` on 5 high-value MCP tools

### Scope

Add `outputSchema` to:

- `search`
- `memory.get`
- `smart_ingest`
- `dream`
- `reflect`

These five account for ~80% of MCP traffic. Type-safe response shapes immediately surface in Claude Code's tool inspector and Cursor's MCP UI.

### Implementation

For each tool, the response type is already serializable (we have DTOs in `dashboard/wire/`). The work is:

1. Extract a JSON schema from each DTO via `schemars` (already a transitive dep through `axum`).
2. Attach it to the `Tool` definition in `vestige-mcp/src/server/catalog.rs::build_tools_list`.

```rust
// vestige-mcp/src/server/catalog.rs
Tool {
    name: "search".into(),
    description: "...".into(),
    input_schema: search_input_schema(),
    output_schema: Some(schemars::schema_for!(SearchResponseDto)),
}
```

### Verification

- `cargo test -p vestige-mcp --lib server::catalog` — schema generation doesn't panic
- Manual: connect Cursor MCP client, call `search`, verify the result is type-annotated in the chat panel
- No behavioral change — same tests pass

### Effort

**S, 1 day.**

## Ticket 3 — Split `dream.rs` and `search.rs`

### Scope

Two files exceed 700 LOC and mix multiple cognitive phases or pipeline stages. Split each into a directory module:

```
vestige-core/src/cognitive/dream.rs (800 LOC)
   →
vestige-core/src/cognitive/dream/
   ├── mod.rs            (orchestration, public API, ~150 LOC)
   ├── nrem1_triage.rs   (NREM1 phase, ~150 LOC)
   ├── nrem3_consolidation.rs (NREM3 phase, ~200 LOC)
   ├── rem_creative.rs   (REM phase, ~200 LOC)
   └── integration.rs    (final synthesis + validation, ~100 LOC)
```

```
vestige-core/src/cognitive/search.rs (700 LOC)
   →
vestige-core/src/cognitive/search/
   ├── mod.rs            (public API, retrieval modes dispatch, ~120 LOC)
   ├── triple_hybrid.rs  (BM25 + semantic + RRF, ~180 LOC)
   ├── activation.rs     (spreading activation, ~120 LOC)
   ├── competition.rs    (competition suppression, ~80 LOC)
   ├── reranking.rs      (Jina cross-encoder, ~120 LOC)
   └── compound_query.rs (decomposition, ~80 LOC)
```

Each file remains under 250 LOC after split. Re-export via `mod.rs`. No public API change.

### Verification

- `cargo test --workspace` passes (no behavior change)
- `cargo bench -p vestige-core` shows no >2% perf regression on `search` benchmarks
- `wc -l vestige-core/src/cognitive/{dream,search}/*.rs` confirms target file sizes

### Effort

**S, 1 day.**

## Ticket 4 — `.unwrap()` reduction pilot in `dream.rs`

### Scope

`vestige-core/src/cognitive/dream.rs` has 24 `.unwrap()` calls. Each is a potential crash on malformed input. Replace with `?` propagation or explicit fallbacks.

### Idiom (lock in here, reuse in Ticket 5)

```rust
// BEFORE (panics on missing memory)
let memory = self.storage.get_memory(id).unwrap();

// AFTER (propagates via MemoryError)
let memory = self.storage.get_memory(id)
    .ok_or(MemoryError::NotFound(id))?;
```

For cases where the panic is currently masking a logic bug (e.g., `unwrap()` on a value we know was just inserted), keep a panic but make it an `expect` with a descriptive message:

```rust
// Keep panic — invariant violation, not user input
let just_inserted = self.storage.get_memory(id)
    .expect("BUG: memory just inserted but get_memory returned None");
```

### Error enum

```rust
// vestige-core/src/cognitive/errors.rs (new)
#[derive(thiserror::Error, Debug)]
pub enum CognitiveError {
    #[error("memory not found: {0}")]
    MemoryNotFound(Uuid),
    #[error("invalid dream state: {0}")]
    InvalidDreamState(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("embedding error: {0}")]
    Embedding(#[from] EmbeddingError),
}
```

Public API of `dream()` returns `Result<DreamReport, CognitiveError>`. Existing callers either propagate or `.map_err()` to their own error type.

### Verification

- `cargo clippy --workspace -- -D clippy::unwrap_used` passes for `dream.rs` (allow elsewhere temporarily)
- Existing tests pass
- New test: malformed input case (e.g., dream called with non-existent memory id in the cluster cache) returns `Err(MemoryNotFound)`, doesn't panic

### Effort

**M, 2–3 days.**

## Ticket 5 — `.unwrap()` reduction across remaining MCP tools

### Scope

~150 `.unwrap()` calls across `vestige-mcp/src/tools/*`. Apply the idiom from Ticket 4. Estimated 4–5 PRs to keep each under ~40 changed call sites.

### Suggested batching

| PR | Files | Approx unwrap count |
|---|---|---|
| 5a | `tools/memory.rs`, `tools/search.rs` | ~50 |
| 5b | `tools/reflect.rs`, `tools/temporal.rs`, `tools/confidence.rs` | ~30 |
| 5c | `tools/codebase.rs`, `tools/intention.rs`, `tools/explore_connections.rs` | ~30 |
| 5d | `tools/smart_ingest.rs`, `tools/importance_score.rs` | ~25 |
| 5e | Remaining (`gc.rs`, `backup.rs`, `export.rs`, etc.) | ~15 |

### Verification

After all five PRs: `cargo clippy --workspace -- -D clippy::unwrap_used` passes globally.

### Effort

**L, ~2 weeks of focused work across 4–5 PRs.**

## What NOT to bundle

- **Feature proposals (TOPIC-HUBS, INSIGHT-TIER, DECISION-MATRIX, CONTRADICTION-DETECTION).** These are net-new behavior. Don't mix with cleanup, even when files overlap.
- **Cargo dependency bumps.** Separate hygiene task. Especially: don't bump `tokio` or `axum` minor versions in a cleanup PR — those have their own risk profile.
- **Performance work.** If `search.rs` split surfaces an obvious perf bug, file a separate ticket; don't fix it in the refactor PR.

## Estimated total elapsed

- Ticket 2: 1 day
- Ticket 3: 1 day
- Ticket 4: 2–3 days
- Ticket 5: 8–10 days (in 4–5 PRs over a 2-week window)

**Sum: ~3 weeks calendar time, ~2 weeks active dev time.** Compatible with parallel feature work on TOPIC-HUBS (proposal A, M-sized) — different files, different reviewers.

## When this document ends

After all five tickets ship, this file moves to `docs/archive/CLEANUP-ROADMAP-2026-05.md` for the historical record. Subsequent cleanup waves get their own dated file (e.g., `CLEANUP-ROADMAP-2026-11.md`).

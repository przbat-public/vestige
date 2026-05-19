# Decision Matrix — first-class trade-off comparison node

**Status:** Proposed, not implemented
**Author:** May 14, 2026
**Pre-reading:** [TYPED-MEMORY-DESIGN.md](TYPED-MEMORY-DESIGN.md)
**Related:** [CONTRADICTION-DETECTION-DESIGN.md](CONTRADICTION-DETECTION-DESIGN.md) (decisions are the supersession-prone node type)

## Why this exists

`codebase(action="remember_decision")` today stores decisions as free text:

```
Decision: use PostgreSQL over MySQL. Rationale: better JSON support, NOTIFY/LISTEN
for real-time, team familiarity.
```

This works for *one* decision. It fails for **comparison questions** — the queries this node type is supposed to serve:

- "Why did we choose X over Y?" — has to reconstruct the alternatives from prose
- "What did we reject when we chose X?" — alternatives are buried as comma-separated names
- "What were the trade-offs?" — implicit, never structured
- "Are the criteria from this decision still valid?" — no per-criterion versioning

Worse: today's `reflect` returned **0 stale decisions** across a 3 300-memory base. That's not because no decisions went stale. It's because the system has no structural way to detect "this decision's criteria are no longer met" without scoring them numerically.

## Mechanism

Promote `NodeType::Decision` from a tag-styled `Fact` to a structured node with explicit fields.

```rust
pub struct DecisionPayload {
    pub question: String,                   // "Choose state management library"
    pub chosen: Choice,
    pub alternatives: Vec<Choice>,
    pub criteria: Vec<Criterion>,           // ordered by weight
    pub rationale: String,                  // free text explaining final pick
    pub decided_at: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>, // tech moves; decisions expire
    pub superseded_by: Option<Uuid>,        // forward link if revisited
    pub supersedes: Option<Uuid>,           // back-link to prior decision
}

pub struct Choice {
    pub name: String,
    pub scores: HashMap<String, f64>,       // {"perf": 0.9, "DX": 0.7, "cost": 0.6}
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub url_or_reference: Option<String>,   // docs link
}

pub struct Criterion {
    pub name: String,                       // "perf"
    pub weight: f64,                        // 0..1, weights sum ≈ 1.0
    pub description: String,                // "p99 latency under 100ms on 8-core"
    pub measurement: Option<String>,        // how to verify ("loadtest at scale X")
}
```

Backward compatibility: the old `remember_decision` tool stays, marked deprecated in its description. New tool: `remember_decision_v2`. Old decisions remain searchable; they just don't render the comparison table in the dashboard.

### MCP tool signature

```json
{
  "name": "remember_decision_v2",
  "arguments": {
    "question": "Choose state management library",
    "chosen": {
      "name": "Zustand",
      "scores": {"bundle_size": 0.95, "DX": 0.9, "ecosystem": 0.7},
      "pros": ["~1KB gzipped", "no provider hell", "easy migration from Context"],
      "cons": ["less batteries-included than Redux Toolkit"]
    },
    "alternatives": [
      { "name": "Redux Toolkit",
        "scores": {"bundle_size": 0.4, "DX": 0.7, "ecosystem": 0.95},
        "pros": ["large ecosystem", "RTK Query for server state"],
        "cons": ["heavier", "boilerplate"] },
      { "name": "Jotai",
        "scores": {"bundle_size": 0.95, "DX": 0.85, "ecosystem": 0.6},
        "pros": ["atomic primitives", "fine-grained reactivity"],
        "cons": ["atom soup at scale", "smaller community"] }
    ],
    "criteria": [
      { "name": "bundle_size", "weight": 0.4, "description": "< 5KB gzipped" },
      { "name": "DX", "weight": 0.35, "description": "low ceremony, good types" },
      { "name": "ecosystem", "weight": 0.25, "description": "active npm + docs" }
    ],
    "rationale": "Zustand wins weighted score (0.91 vs RTK 0.61 vs Jotai 0.85). Picked Zustand over Jotai on ecosystem maturity — Jotai is technically excellent but smaller community made onboarding slower for the existing team.",
    "valid_until": "2027-05-14T00:00:00Z"
  }
}
```

### Dashboard view

A new page `/decisions` lists structured decisions sortable by:

- `decided_at` (recent first)
- `valid_until` (expiring soonest, with red highlight when past)
- weighted_score(chosen) (how decisive was this pick)
- supersession_depth (revisited often vs stable)

Per-decision: a radar chart with one axis per criterion, overlaid choices, chosen highlighted. Pros/cons lists side-by-side. Forward link to `superseded_by` if exists, back-link to `supersedes`.

## Stale-decision detection

`reflect` gains a new check: **stale decision**. A decision is stale when:

1. `valid_until < now()` — explicit expiry passed
2. The `chosen.name` no longer appears in any memory created in last 90 days (probably no longer in use)
3. The `criteria` no longer hold — e.g., the criterion was "supports Node 18" and current Node version recorded in memory is 22, criterion weight high → flag

Stale decisions appear in `reflect`'s `staleDecisions[]` (currently empty for every reflect call). The agent can then `promote` (still valid, just expired marker), `supersede` (chain a v2 decision), or `demote` (was always wrong).

## Schema delta

Stored in `nodes.extra_json` as `{"decision": DecisionPayload}`. No new table.

Migration v13 (optional, performance-only):

```sql
-- vestige-mcp/src/storage/sqlite/migrations/v13_decisions.sql
CREATE INDEX IF NOT EXISTS idx_decisions_valid_until
  ON nodes(json_extract(extra_json, '$.decision.valid_until'))
  WHERE node_type = 'decision'
    AND json_extract(extra_json, '$.decision.valid_until') IS NOT NULL;
```

Lets `reflect`'s stale-decision query scan only the index, not all decisions.

## Files to touch

| File | Change |
|---|---|
| `vestige-core/src/storage/decision.rs` | New: `DecisionPayload`, `Choice`, `Criterion` |
| `vestige-core/src/cognitive/reflect/stale_decisions.rs` | New: detection logic |
| `vestige-mcp/src/tools/codebase.rs` | Add `remember_decision_v2` variant; deprecate `remember_decision` in tool description |
| `vestige-mcp/src/dashboard/wire/decision.rs` | New: `DecisionDto` + ts-rs export |
| `vestige-mcp/src/dashboard/handlers/decisions.rs` | New: `GET /api/decisions`, filters |
| `apps/dashboard/src/pages/DecisionsPage.tsx` | New: list + filters |
| `apps/dashboard/src/components/DecisionCard.tsx` | New: per-decision view with radar chart |
| `vestige-mcp/src/storage/sqlite/migrations/v13_decisions.sql` | Optional: index for stale detection |

## Effort

**S, 2–3 days.**

- Day 1: schema, DTO, ts-rs export, tool variant.
- Day 2: dashboard list + per-decision view (radar chart via existing chart utilities).
- Day 3: stale-decision check in reflect + verification.

## Verification

1. **Round-trip test.** Ingest 5 sample decisions via `remember_decision_v2`, fetch via search, fetch via `/api/decisions`, render in dashboard. Three different consumers, same data, same shape.
2. **Stale detection.** Plant 3 decisions with `valid_until = 2026-01-01`. Run reflect. Expect `staleDecisions.length >= 3`.
3. **Supersession chain.** Decision A (Zustand) → A.superseded_by → Decision B (Zustand + Jotai for forms). Walking the chain from B should yield A in `supersedes`.

## Migration of existing decisions

Don't auto-migrate. Existing `Decision` memories stay as-is. When the agent encounters a relevant one, it can run `memory(edit)` with the new structured payload — the same dedup rules apply, no special tooling needed.

## Out of scope

- **Multi-decision dependency graphs.** "Decision A requires decision B to be valid" — useful but a follow-up.
- **Quantitative criterion measurement.** The `measurement` field is a string description today, not an executable check. Auto-validation ("loadtest still passes") is a v3 idea.
- **External decision import.** ADRs (Architecture Decision Records) from a repo's `docs/adr/` directory could be ingested as `DecisionPayload`, but the parser for various ADR formats (Nygard, MADR) is its own project.

## Open questions

1. **Required vs optional criteria.** Should the API enforce `weights.sum() ≈ 1.0`? Today: validate weights sum 0.95–1.05 (lenient float). Reject outside that range with a 400.
2. **Score scale.** 0..1 vs 1..5? Going with 0..1 for math (weighted sum is just a dot product). Dashboard displays as percentages.
3. **Should `rationale` be markdown?** Yes — render as such in dashboard, escape in search snippets to avoid injection.

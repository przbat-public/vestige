# Topic Hubs — auto-derived cluster summaries

**Status:** Proposed, not implemented
**Author:** May 14, 2026
**Pre-reading:** [BENCHMARK-IMPROVEMENT-PLAN.md](BENCHMARK-IMPROVEMENT-PLAN.md), [TYPED-MEMORY-DESIGN.md](TYPED-MEMORY-DESIGN.md)
**Related:** [INSIGHT-TIER-DESIGN.md](INSIGHT-TIER-DESIGN.md) (companion proposal — both add new `NodeType` variants to dream output)

## Why this exists

Dream NREM3 currently strengthens memories individually. The graph stores millions of pairwise edges but no "cluster summary" tier — the agent has to re-synthesize "what we know about X" from N atoms every single retrieval.

This bites two ways:

1. **Multi_hop questions reconstruct the same cluster repeatedly.** A question like "how did chunking strategy evolve in Vestige?" pulls 5–8 atoms, the answerer reconstructs the timeline inline, the synthesis is discarded. Next session, same question, same work.
2. **Token budget waste.** A 12 000-char retrieval window holds 8–10 atoms. If 6 of them belong to the same cluster, the agent is reading near-duplicate context six times instead of one consolidated paragraph + four distinct atoms from other clusters.

ENGRAM (arXiv 2511.12960) reports state-of-the-art on LoCoMo with 1% tokens, and their headline mechanism is a separate **semantic memory tier** that holds extracted, deduplicated abstractions. Our [TYPED-MEMORY-DESIGN.md](TYPED-MEMORY-DESIGN.md) PoC validated typed atomic facts (+3.96 pp on LoCoMo). Topic Hubs extend that idea one layer up: not just typed atoms, but typed summaries of clusters of atoms.

### Empirical evidence (this session, 2026-05-14)

The dream cycle run at 14:07 returned:

```json
{ "creativeConnectionsFound": 44,
  "connectionsPersisted": 44,
  "insightsGenerated": 1,
  "phases": [
    { "phase": "REM_Creative",
      "actions": ["Cross-domain pairing: 24 tag groups, 0 connections found",
                  "Pattern extraction: 68 shared patterns found"] } ] }
```

68 shared patterns found, but **zero turned into anything queryable**. The patterns exist in the graph as edges, but there's no node that says "here is the abstraction these 50 memories collectively imply." Topic Hubs add that node.

## Mechanism

After REM_Creative completes, evaluate each pattern cluster against three triggers:

1. **Cluster size ≥ 5** memories (small clusters don't justify a hub)
2. **Internal similarity ≥ 0.7** (avg pairwise embedding cosine)
3. **No existing hub on this cluster within last 7 days** (`hub.cluster_signature` hash check)

If all three pass, emit a `NodeType::Hub` memory linking to the cluster members.

### Content generation: two-phase rollout

**Phase 1 — template-based (default).** No LLM call. Format:

```
[Topic Hub: {top_3_tags_joined}]

This cluster contains {N} memories ({date_range}) about {dominant_entity_or_tag}.

Key threads:
- {top_3_excerpts_first_sentence}

Members: {child_ids}
```

This is good enough to (a) prove the wire works, (b) give search something better than nothing, (c) ship in 3–5 days.

**Phase 2 — LLM synthesis (opt-in via env var).** `VESTIGE_HUB_SYNTHESIS=llm` triggers a `gpt-4o-mini` call per hub (similar to LoCoMo fact extraction — $0.0002/hub, ~5–20 hubs per dream cycle = ~$0.004 per cycle). Output replaces the template body. Extraction prompt is held in `prompts/hub-synthesis.md` for auditing.

The two phases share storage; the only difference is the `hub.generation_method` field ("template_v1" or "llm_gpt-4o-mini_v1") which lets us A/B and roll back content if a model swap regresses quality.

## Schema delta

```rust
// vestige-core/src/storage/node_type.rs
pub enum NodeType {
    Fact, Concept, Event, Person, Place, Note, Pattern, Decision,
    Hub,        // NEW
    Insight,    // NEW (see INSIGHT-TIER-DESIGN.md)
}

// vestige-core/src/cognitive/hub.rs (new file)
pub struct HubMetadata {
    pub child_ids: Vec<Uuid>,
    pub cluster_signature: String,      // hash(sorted child_ids) — detects "same cluster regenerated"
    pub regeneration_count: u32,
    pub last_regenerated_at: DateTime<Utc>,
    pub generation_method: String,      // "template_v1" | "llm_gpt-4o-mini_v1"
    pub dominant_tags: Vec<String>,
    pub date_range: (DateTime<Utc>, DateTime<Utc>),
}
```

Stored in `nodes.extra_json` as `{"hub": HubMetadata}`. No new table.

### Migration v12

```sql
-- vestige-mcp/src/storage/sqlite/migrations/v12_hubs.sql
CREATE INDEX IF NOT EXISTS idx_nodes_hub
  ON nodes(json_extract(extra_json, '$.hub.cluster_signature'))
  WHERE json_extract(extra_json, '$.hub.cluster_signature') IS NOT NULL;

-- Lets us answer "is there already a hub for this cluster?" in O(log N) instead of scanning all hubs.
```

## Search integration

When `search()` returns candidates, post-process: if ≥3 candidates share a `parent_hub_id` (precomputed reverse index), substitute them with the hub. This trades cluster-breadth for atom-depth — appropriate for the kind of queries where hubs help most.

Implementation: `vestige-core/src/cognitive/search/hub_collapse.rs`. Configurable via `min_siblings_to_collapse` (default 3).

Caveat: if the query is specifically about an atom's detail ("what was the exact LoCoMo score on N=300 hybrid?"), collapsing to the hub loses the number. Mitigation: `search(detail_level="full")` skips collapse. Default `summary` collapses.

## Files to touch

| File | Change |
|---|---|
| `vestige-core/src/storage/node_type.rs` | Add `Hub` variant |
| `vestige-core/src/cognitive/hub.rs` | New: `HubMetadata`, `cluster_signature` |
| `vestige-core/src/cognitive/hub_generator.rs` | New: trigger logic + template renderer |
| `vestige-core/src/cognitive/dream.rs` | REM_Creative emits hub candidates to generator |
| `vestige-core/src/cognitive/search/hub_collapse.rs` | New: post-process search results |
| `vestige-mcp/src/storage/sqlite/migrations/v12_hubs.sql` | New: hub index |
| `vestige-mcp/src/dashboard/wire/hub.rs` | New: `HubDto`, ts-rs export |
| `vestige-mcp/src/dashboard/wire/memory.rs` | Add `Hub` to `MemoryDto.nodeType` enum |
| `vestige-mcp/src/dashboard/handlers/memory.rs` | Render hub-with-children panel |
| `apps/dashboard/src/types/generated/HubDto.ts` | Auto-generated |

## Effort

**Phase 1 (template-only):** M, 3–5 days. Hardest part: writing the cluster_signature hash deterministically so regeneration detection works across dream cycles.

**Phase 2 (LLM synthesis):** +3 days. Mostly: prompt design, cost guard (max hubs per cycle), regression test against template baseline.

## Verification

Three measurements:

1. **LoCoMo multi_hop on N=300.** Hypothesis: +3 to +5 pp because reranker can return one hub instead of three near-duplicate atoms, freeing context for cross-topic evidence. **Stop-ship threshold: any regression vs baseline.**
2. **Hub coverage.** After 7 dream cycles, ≥30% of memories accessed in the previous week belong to ≥1 hub. If much lower, the trigger thresholds are wrong.
3. **Hub usefulness.** Track `promote` rate on hub memories vs atomic memories from same cluster. Hubs should rank competitive — if agents systematically demote hubs in favor of atoms, the templating is broken.

## Out of scope (Phase 1)

- **Hub-of-hubs.** Hierarchical clustering of hubs themselves is a Phase 3 problem; don't build it until Phase 1 hubs are showing usage.
- **Cross-codebase hubs.** Hubs are scoped to whatever cluster signature falls out; don't add per-project sharding logic yet.
- **User-curated hubs.** If a hub's content is wrong, the agent should `memory(edit)` it like any other. Manual hub creation API can wait.

## Open questions

1. **Hub retention dynamics.** When a child memory is demoted, does the hub's retention follow? Default: no — the hub is its own evidence, treat as independent FSRS node. May revisit after observing 30 days of use.
2. **Stale-cluster detection.** If 3 of 5 child memories are deleted, the hub should regenerate or be marked stale. Phase 1: regenerate on next dream cycle when `cluster_signature` no longer matches. Phase 2: emit a `staleHubs` automation trigger.

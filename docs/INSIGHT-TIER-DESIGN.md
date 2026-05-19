# Insight Tier — forced synthesis output from REM_Creative

**Status:** Proposed, not implemented
**Author:** May 14, 2026
**Pre-reading:** [BENCHMARK-IMPROVEMENT-PLAN.md](BENCHMARK-IMPROVEMENT-PLAN.md), [TYPED-MEMORY-DESIGN.md](TYPED-MEMORY-DESIGN.md)
**Related:** [TOPIC-HUBS-DESIGN.md](TOPIC-HUBS-DESIGN.md) (companion — both materialize dream output as new memory nodes)

## Why this exists

Today's dream cycle (recorded May 14, 2026 at 14:07) is the motivating evidence:

```json
{
  "phases": [
    { "phase": "REM_Creative",
      "actions": ["Cross-domain pairing: 24 tag groups, 0 connections found",
                  "Pattern extraction: 68 shared patterns found"],
      "memoriesProcessed": 50 },
    { "phase": "Integration",
      "actions": ["Validated 0/44 connections (threshold: 0.4)",
                  "Generated 0 dream insights"] }
  ],
  "stats": { "creativeConnectionsFound": 44, "connectionsPersisted": 44,
             "insightsGenerated": 1 }
}
```

REM_Creative found **44 connections + 68 shared patterns** — and the only "insight" returned to the agent was a generic `TemporalTrend` at confidence 0.41 ("recurring theme across 50 related memories"). That's not actionable. The agent reads it and learns nothing.

This is the synthesis-promotion problem. The mechanism exists (pattern extraction works fine). The validation threshold of 0.4 effectively gates *everything* from leaving the dream subsystem. Result: REM compute is wasted; the agent only sees raw atoms on retrieval.

### Why "raise the threshold" is not the fix

A natural reaction is "your threshold is wrong, lower it to 0.2." But:

- Validation at 0.4 isn't measuring quality — it's measuring graph centrality. Threshold-tuning shuffles which connections persist as edges, not whether the agent ever *reads* a synthesized statement.
- Connections-as-edges don't show up in retrieval. They influence spreading activation, but the agent's `search()` returns atoms.
- The actual gap is: there's no node type that says "here's what these connections *mean*."

## Mechanism

Add `NodeType::Insight` and have REM_Creative **always** emit N candidates per cluster, regardless of confidence. The threshold gates **promotion**, not **creation**.

```rust
// vestige-core/src/cognitive/dream.rs (REM_Creative phase, simplified)
for cluster in pattern_clusters {
    let candidates = insight_generator.synthesize(&cluster, N=3);
    for c in candidates {
        let initial_retention = if c.confidence >= 0.4 { 0.5 } else { 0.3 };
        storage.insert_insight(InsightMemory {
            content: c.text,
            derived_from: cluster.member_ids.clone(),
            generation_method: c.method,
            confidence: c.confidence,
            novelty: c.novelty,
            retention_strength: initial_retention,
            validated_by_agent: false,
        });
    }
}
```

### Generation methods (Phase 1: templates; Phase 2: LLM)

**Templates (Phase 1).** For each cluster, derive insights from structural patterns the dream phase already detects:

| Pattern detected | Template |
|---|---|
| Same fact appearing in 3+ memories within 30 days, no contradictions | "Recurring confirmation: {fact_summary} (observed {N}× since {first_date})" |
| Memory A says X, memory B (newer) says ¬X about same entity | "Possible supersession: {entity} — was '{A.content_summary}' ({A.date}), now '{B.content_summary}' ({B.date})" |
| 5+ memories share key entity but cover different aspects | "Topic envelope: {entity} appears across {N} memories spanning {N_aspects} aspects: {aspect_list}" |
| Decision memory + 2+ outcome memories referencing it | "Outcome chain: decision '{D.summary}' → observed effects: {outcome_list}" |
| Two pattern clusters bridged by ≥3 memories | "Cross-domain link: {topic_A} ↔ {topic_B} via shared mechanism in {bridging_memories}" |

These five templates cover the patterns REM already detects but currently discards.

**LLM synthesis (Phase 2).** A single `gpt-4o-mini` call per cluster gets the top-K cluster members and produces 1 free-form insight. Cost: ~$0.0002/cluster × ~10 clusters/dream cycle = ~$0.002/cycle. Always opt-in (`VESTIGE_INSIGHT_LLM=on`), with a hard daily-budget cap (`VESTIGE_INSIGHT_LLM_DAILY_USD=0.10`).

### Lifecycle

Insights enter the same FSRS-6 retention loop as facts, but with two differences:

1. **Lower starting retention** (0.3 vs 1.0). They're tentative until validated.
2. **Validation signal: agent promote.** When an agent `promote`s an insight, `validated_by_agent` flips to `true` and retention jumps to 0.7. Demote × 3 triggers soft-delete.

This means insights start fragile. The Testing Effect promotes useful ones; unused ones decay naturally without manual cleanup.

## Schema

```rust
pub struct InsightMetadata {
    pub derived_from: Vec<Uuid>,         // parent memories that produced this insight
    pub generation_method: String,       // "template_pattern_v1" | "llm_gpt-4o-mini_v1"
    pub template_id: Option<String>,     // for template-generated, which template
    pub confidence: f64,                 // 0..1, generator's self-estimate
    pub novelty_score: f64,              // 0..1, distance from existing memories
    pub validated_by_agent: bool,        // flipped true on first promote
    pub validation_count: u32,           // total promotes
    pub demotion_count: u32,             // total demotes (soft-delete at 3)
}
```

Stored in `nodes.extra_json`. No new table.

## Search & retrieval integration

Insights appear in `search()` results with an `epistemicTier: "tentative"` flag (DTO field) until validated. The agent UI surfaces this — Cursor sees something like:

> 💡 Tentative insight: "Possible supersession: PostgreSQL — was 'team chose MySQL' (2026-01), now 'migrated to PostgreSQL for JSON support' (2026-03)"

Agent decides: useful → promote (retention → 0.7); wrong → demote with reason.

After validation, insights rank like any other memory.

## Files to touch

| File | Change |
|---|---|
| `vestige-core/src/storage/node_type.rs` | Add `Insight` variant |
| `vestige-core/src/cognitive/insight_generator.rs` | New: 5 templates + dispatcher |
| `vestige-core/src/cognitive/insight_llm.rs` | New (Phase 2): LLM client wrapper |
| `vestige-core/src/cognitive/dream.rs` | REM_Creative calls generator unconditionally |
| `vestige-mcp/src/dashboard/wire/memory.rs` | `InsightMetadata` in `MemoryDto`, `epistemicTier` field |
| `vestige-mcp/src/dashboard/wire/dream.rs` | Dream response includes `insights_created` count + sample |
| `apps/dashboard/src/components/InsightCard.tsx` | New: rendering with tentative badge |
| `vestige-mcp/src/tools/memory.rs` | Promote on insight flips `validated_by_agent` |

## Effort

**Phase 1 (templates only):** M, 3–5 days.

- Day 1: schema, migration, generator skeleton, one template.
- Day 2–3: remaining four templates + dispatcher + dream wiring.
- Day 4: dashboard rendering + DTO + ts-rs.
- Day 5: integration tests + validation that insights show up after a dream cycle.

**Phase 2 (LLM synthesis):** +5–7 days.

- Prompt engineering + golden-set evaluation.
- Cost guards + daily-budget enforcement.
- A/B test: template-only vs template+LLM on a fixed memory set.

## Verification

Three measurements:

1. **Insights generated per cycle.** Currently 0–1. Target after Phase 1: 5–15 per dream cycle. (Bound by cluster count, not template breadth.)
2. **Promote-to-demote ratio.** Per insight, observe over 30 days. Target: >2:1. A ratio close to 1:1 means the templates are noise; <1 means they're net-negative and we should ablate templates one by one.
3. **Agent self-reported value.** Sample 20 promoted insights; the agent (in a fresh session, blinded) judges whether the insight saved retrieval work. Target: ≥60% "saved work."

Stop-ship: if Phase 1 ships and after 30 days promote:demote < 1:2, roll back. The Testing Effect should be self-cleaning, but a runaway noise tier is worse than no tier.

## Out of scope

- **Multi-step reasoning insights.** Insights are one-hop synthesis. "If X then Y then Z" chains are a separate proposal (call it: chain-of-evidence memory).
- **Insight-on-insight.** Don't let insights derive new insights (recursive amplification of noise). The `derived_from` graph stops at atomic memories.
- **Cross-session insights.** All cluster detection is within a single Vestige instance. Federated insights across machines is far future.

## Open questions

1. **Should template-generated insights compete with LLM-generated for retention?** Argument for: same FSRS pipeline, let promotes decide. Argument against: template insights are deterministic — agents won't `promote` something that says "Recurring confirmation: X" because it adds nothing new. Phase 1 ships with both in same bucket; revisit after data.
2. **Daily-budget enforcement granularity.** Per-process or per-Vestige-instance? Default: per-instance (one DB = one budget), so multi-tenant deployments don't fight over a shared quota.

## Relation to TOPIC-HUBS-DESIGN.md

| Aspect | Hubs (proposal A) | Insights (this proposal) |
|---|---|---|
| What | Summary of cluster | Synthesis statement |
| When created | Cluster ≥5, similarity ≥0.7, none for 7d | Every REM_Creative cycle, every cluster |
| Initial retention | 1.0 (cluster is real) | 0.3 (synthesis is tentative) |
| Validation | Implicit (children exist) | Explicit (agent promote) |
| Failure mode | Stale child set | False-positive synthesis |

They are complementary, not redundant. Hubs are "here is what this cluster *is*." Insights are "here is what this cluster *implies*." Same dream cycle can produce both.

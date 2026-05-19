# Contradiction Detection — false-positive filter for reflect

**Status:** Proposed, not implemented
**Author:** May 14, 2026
**Pre-reading:** [BENCHMARK-IMPROVEMENT-PLAN.md](BENCHMARK-IMPROVEMENT-PLAN.md)

## Why this exists

`reflect(depth="standard")` today flags **134 "contradictions" with severity high** on a 3 300-memory base. The agent has no realistic way to triage them — manual audit would take hours, and at >95% false-positive rate (see below) it would be hours of clicking "no, not a contradiction."

### Empirical evidence (this session, 2026-05-14)

Ran reflect at 14:07. Top offender: memory `d1abf5ea-4321-4224-8716-00b7e5b0f7cc` flagged **11 times** as contradicting other memories. The flagged "contradictions" included pairings against:

- `Vestige bug fix issue #25`
- `Partner success metrics`
- `Partner investigation` (a debugging session)
- `Pre-qualification` (market expansion)
- `RESEARCH: ENGRAM paper`

The actual content of `d1abf5ea` (since split — see git history): a compound memory containing 5 separate `[Updated]` sections about 5 different LoCoMo experiments (flat turn-level chunking, hybrid chunk-level, entity-overlap reranking, time-aware reranking, Tier 4 turn_extracted mode). None of those overlap with the business memories. The "contradictions" were pure embedding-similarity false positives — long memory with broad topical surface area hits everything in cosine space.

This means the current detector has two failure modes:

1. **Compound memories** trigger contradictions against many unrelated memories — embedding surface area, not logical conflict.
2. **Long memories** (>2 000 chars) are over-represented in the contradiction list because longer = more vocabulary = more cosine hits.

134 alleged contradictions, manual sample of 20: 1 real (an old Vestige bug-fix memory that was correctly superseded but both versions remained). **19/20 false positive = 95%.**

## Mechanism

A contradiction requires **at least 2 of 3 positive signals**, not just embedding similarity:

### Signal 1 — Shared key entity

Both memories reference the same proper noun, file path, person, or product name. We already extract these via the [preprocessing pipeline](TYPED-MEMORY-DESIGN.md) (`entity:*` tags). If `entities(A) ∩ entities(B) == ∅`, the alleged contradiction is rejected — different topics can't contradict.

### Signal 2 — Negation or supersession language

At least one of the two memories contains one of the following phrases (case-insensitive):

```
"actually", "but", "however", "previously", "wrong", "was incorrect",
"fixed", "updated", "no longer", "instead", "rather than",
"superseded", "obsolete", "deprecated", "now", "currently",
"earlier I said", "earlier we thought", "as of"
```

This signal catches genuine corrections. If neither memory uses any of these markers, it's almost certainly two independent statements that happen to be embedding-similar.

### Signal 3 — Temporal overlap with conflicting values

For memories with structured payloads (decisions, facts with `valid_from/valid_until`), the same entity has different values during overlapping time windows. This is the strongest signal — it's how the contradiction in `d1abf5ea` (the one real case) was identifiable: same fact, two timestamps, two different values.

```rust
fn has_conflicting_payload(a: &Memory, b: &Memory) -> bool {
    let entity_a = extract_subject(&a.content);
    let entity_b = extract_subject(&b.content);
    if entity_a != entity_b { return false; }
    if !time_ranges_overlap(a.valid_from..a.valid_until, b.valid_from..b.valid_until) {
        return false;
    }
    let value_a = extract_value_for(&a.content, &entity_a);
    let value_b = extract_value_for(&b.content, &entity_b);
    value_a != value_b
}
```

### The 2-of-3 rule

A contradiction is reported only if at least 2 signals are positive:

| signal_1 (entity) | signal_2 (language) | signal_3 (payload) | report? |
|---|---|---|---|
| ✓ | ✓ | – | **yes** |
| ✓ | – | ✓ | **yes** |
| – | ✓ | ✓ | **yes** |
| ✓ | – | – | no — same topic, different facets |
| – | ✓ | – | no — corrective tone without target |
| – | – | ✓ | no — name collision (different entities, same value) |
| ✓ | ✓ | ✓ | **yes — high confidence** |

### Special case: compound memories

For memories with `length > 2000` chars OR `node_type ∈ {Hub, Insight}` (new node types from [TOPIC-HUBS-DESIGN.md](TOPIC-HUBS-DESIGN.md) and [INSIGHT-TIER-DESIGN.md](INSIGHT-TIER-DESIGN.md)) — require **3 of 3** signals. These memories have inflated surface area and are over-represented in false positives.

This is also a soft pressure mechanism for memory hygiene: the system flags fewer false alarms when memories are atomic; if you write a 5 000-char monster, contradictions involving it have a higher bar.

## Output format

`reflect`'s `contradictions[]` gains a `confidence` field:

```json
{
  "memory_a": "uuid-A",
  "memory_b": "uuid-B",
  "snippet_a": "...",
  "snippet_b": "...",
  "signals": ["shared_entity", "supersession_language"],
  "confidence": "medium",
  "suggested_action": "review_supersession"
}
```

`confidence` values:
- `"high"` — 3/3 signals → almost certainly a real contradiction
- `"medium"` — 2/3 signals → likely contradiction, agent should review
- (no field) — below threshold, not reported

`suggested_action`:
- `"review_supersession"` — language suggests one supersedes the other; agent should `memory(promote)` newer + `memory(demote)` older, or `memory(edit)` to clarify
- `"review_versioned_fact"` — payload values differ on overlapping time ranges; agent should use `temporal(invalidate)` on the older version
- `"unknown"` — 2/3 signals but no clear shape

## Files to touch

| File | Change |
|---|---|
| `vestige-core/src/cognitive/reflect/contradiction_detector.rs` | Rewrite: 2-of-3 rule + signal extraction |
| `vestige-core/src/preprocessing/negation_cues.rs` | New: list of supersession phrases + matcher |
| `vestige-core/src/preprocessing/entity_extraction.rs` | Existing — reuse for signal 1 |
| `vestige-core/src/preprocessing/value_extraction.rs` | New: extract `entity → value` for signal 3 |
| `vestige-mcp/src/dashboard/wire/reflect.rs` | Add `signals[]`, `confidence`, `suggested_action` to `ContradictionDto` |
| `apps/dashboard/src/components/ContradictionList.tsx` | Filter chips by confidence; show signals as badges |

## Effort

**M, 3–5 days.**

- Day 1: extract signal 1 + signal 2 from existing preprocessing modules + matcher tests.
- Day 2: signal 3 (value extraction for fact-style memories with subject + value).
- Day 3: combine into 2-of-3 rule, update reflect.
- Day 4: DTO + dashboard.
- Day 5: re-run reflect against current 3 300-memory base, manual audit of result, calibrate signal thresholds.

## Verification

1. **False-positive rate.** Re-run reflect on the same base used today. Hypothesis: 134 alleged contradictions → 25–40 surviving, of which manual audit of 20 confirms ≥80% real (8x improvement in precision).
2. **No regression on real contradictions.** Plant 10 synthetic real contradictions (paired facts with shared subject + supersession language + valid_until conflict). Re-run reflect. Expect ≥9/10 detected.
3. **Hub/insight false-positive rate.** Plant 5 compound memories that resemble `d1abf5ea` (multiple topics, broad surface). Verify each generates ≤1 contradiction (today they generate 5–11 each).

Stop-ship: if hypothesis 1 fails — i.e., reduction below 25 contradictions OR audit precision <60% — keep current detector, ablate one signal at a time.

## Out of scope

- **LLM-judged contradiction detection.** A `gpt-4o-mini` "are these contradictory?" call would likely beat 95% precision. Deferred because (a) cost grows with base size, (b) the rule-based filter already gets us to 80–90%, (c) LLM-judged is an obvious follow-up if the rule-based version plateaus.
- **Cross-language contradictions.** Polish memory contradicts English memory on same entity — the negation cue list is English+Polish today, but proper-noun matching may miss when the entity is translated ("Cloud Run" vs "Cloud Run"). Defer.
- **Contradiction *resolution* automation.** The detector flags, the agent acts. Auto-merging or auto-deleting contradictions is a different proposal; this one only fixes detection.

## Open questions

1. **Should `node_type = "hub"` be excluded entirely?** Hubs are by definition summaries; they shouldn't contradict their own children, but they could contradict other hubs. Default: include hubs with 3/3 threshold (same as compound rule).
2. **Negation cue list — Polish/multi-language?** Yes. Initial list: English + Polish (since the user works bilingual). Other languages added as data demands. Cues live in `preprocessing/negation_cues.{en,pl}.txt` for easy review.
3. **Value extraction confidence.** Signal 3 fires when extracted values differ. What if extraction itself is unreliable? Flag with `signals: ["shared_entity", "supersession_language", "payload_uncertain"]` when value extraction confidence <0.5, treat as half-signal toward the 2-of-3 rule.

## Why not just lower the embedding threshold?

The current detector uses embedding similarity as the gating heuristic. The natural fix would be "raise the threshold from 0.X to 0.Y." This doesn't work because:

- Real contradictions can be lexically dissimilar ("We chose PostgreSQL" vs "MySQL was selected for the new service" — same entity space, different wording, low cosine similarity).
- Long memories' embeddings drift toward "average" of all their content, making them similar to almost anything dense.
- Threshold tuning solves precision *or* recall, never both.

The 2-of-3 signal rule trades a single noisy axis (embedding similarity) for three orthogonal axes (entity, language, payload). Even if any single axis is noisy, their conjunction is sharp.

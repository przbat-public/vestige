# Tier 4 — Typed Memory (ENGRAM-style)

**Status:** design proposal, not implemented
**Author:** initial draft May 11, 2026
**Pre-reading:** [ENGRAM paper (arXiv 2511.12960)](https://arxiv.org/abs/2511.12960), [Vestige LoCoMo benchmark plan](BENCHMARK-IMPROVEMENT-PLAN.md)

## Why this exists

Vestige's LoCoMo LLM Judge has plateaued at **62.21% full N / 64.67% on the seed=42 N=300 sample** across six harness-only variants:

| Variant                             | N=300 LLM Judge |
|-------------------------------------|----------------|
| Turn-level (canonical)              | 64.67%         |
| Hybrid (session ∪ turn)             | 65.00%         |
| Turn + entity-overlap rerank        | 63.67%         |
| Turn + time-aware rerank            | 63.67%         |
| Turn OVERFETCH=40 TOPK=10/20        | 64.67%         |
| Turn + hierarchical retrieval K=15  | 64.67%         |
| Turn + gpt-4o answer (vs gpt-4o-mini) | 68.00% (N=200) — only +1 pp at 10× cost |

Hierarchical retrieval lifts Phase 1 Recall@5 from 72.34% to 77.86% (+5.52 pp) — strictly better retrieval — but Phase 2 LLM Judge stays flat. Upgrading the answerer from `gpt-4o-mini` to `gpt-4o` adds +1.00 pp at 10× the cost and leaves `single_hop` (34.21%) and `multi_hop` (44.44%) unchanged. The bottleneck is neither retrieval recall, nor ranking, nor the answerer's reasoning capacity. **It's the memory representation itself.**

LoCoMo turn-level chunks are surface-level utterances:

```
[8 May 2023, 4:25 pm]
Caroline: I went to the LGBTQ support group yesterday.
```

For a question like *"What did Caroline say about her dog?"* the answer often spans multiple turns of casual chit-chat without a single canonical statement. The current pipeline forces the answerer to do **on-the-fly fact extraction** from raw dialogue under a 12 000-char budget and an 8-second timeout. ENGRAM (arXiv 2511.12960) shows that **pre-extracting** atomic facts at ingest time, then retrieving facts by type, yields +5–15 pp on LongMemEval with ~1% of the tokens.

This document proposes how to implement that for Vestige.

## The three memory types

Following ENGRAM, every ingested piece of information gets classified into exactly one of:

| Type        | Storage shape | What gets stored | Retrieved when |
|-------------|---------------|------------------|----------------|
| **Episodic**  | one row per event | "Caroline attended LGBTQ support group on 2023-05-08" | Time-bound question ("when…", "what happened on…") |
| **Semantic**  | one row per (subject, attribute) | "Caroline lives in Berlin" | Attribute question ("where does X live", "what is Y's job") |
| **Procedural**| one row per (actor, action_pattern) | "Caroline goes to therapy every Tuesday" | Habit/pattern question ("how often", "what does X usually do") |

The current single-row `KnowledgeNode` represents none of these well — it's a "raw chunk" that happens to be tagged. The Tier 4 proposal is to keep `KnowledgeNode` as the unit of storage but add a typed view layer that classifies, indexes, and retrieves on top of it.

## Architecture (low-disruption variant)

### Option A — typed metadata, no schema break

Add three optional fields to `KnowledgeNode`:

```rust
pub struct KnowledgeNode {
    // ...existing fields
    pub memory_kind: MemoryKind,                       // NEW
    pub subject: Option<String>,                       // NEW: "Caroline", "Melanie", etc.
    pub predicate: Option<String>,                     // NEW: "lives in", "attended", "likes"
    pub object: Option<String>,                        // NEW: "Berlin", "support group", "running"
    pub episodic_at: Option<DateTime<Utc>>,            // NEW: only for episodic
    pub procedural_frequency: Option<Frequency>,       // NEW: only for procedural
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Raw,        // current default — opaque chunk, no extraction (back-compat)
    Episodic,
    Semantic,
    Procedural,
}
```

The `Raw` variant preserves the current behavior. Existing memories migrate to `Raw` with all extracted fields `None`. Existing code paths unchanged.

### Option B — separate tables per type (cleaner, more disruptive)

Three SQLite tables: `episodic_memories`, `semantic_memories`, `procedural_memories`, each with a foreign key into the existing `knowledge_nodes` table. The `KnowledgeNode` becomes the "source chunk" provenance; the typed tables are the "extracted facts".

Pros: clean separation, type-specific indexes (e.g. R-tree on episodic_at), no nullable columns.
Cons: schema migration, query-time joins, three sources of truth.

**Recommended: Option A first** — get the win measured, then refactor to B if it helps. Smaller surface area, faster to ship and roll back.

## The extraction pipeline

At ingest time, `smart_ingest` already runs a "Content Intelligence Pipeline" (entity extraction, coreference rewriting, temporal anchoring, relation extraction). Reuse it:

```
smart_ingest(content)
├── (existing) entity extraction      → tags
├── (existing) coreference rewrite    → content (self-contained)
├── (existing) temporal anchoring     → valid_from / valid_until
├── (existing) relation extraction    → (subject, predicate, object) edges
├── (NEW) memory_kind classifier      → MemoryKind enum
└── (NEW) per-kind structured fields  → episodic_at / procedural_frequency / etc.
```

The classifier can be a simple LLM call (one prompt, structured output via `response_format: { type: "json_object" }`, ~$0.0002 per memory with gpt-4o-mini). Or a rule-based first pass:

* **Has a date in the content?** → candidate Episodic
* **Matches `<subject> <is/has/lives/works>` ?** → candidate Semantic
* **Has frequency adverb (always / every / weekly / daily) ?** → candidate Procedural
* **Otherwise** → Raw

A hybrid (rules → LLM-disambiguation when rules return multiple candidates) is the lowest-cost path.

## Per-kind retrieval (the win)

When a question arrives, classify the QUESTION type first, then route to the right retriever:

```rust
fn classify_question(q: &str) -> QuestionType {
    if has_temporal_marker(q) { QuestionType::Temporal }
    else if has_frequency_marker(q) { QuestionType::Procedural }
    else if has_attribute_marker(q) { QuestionType::Semantic }
    else { QuestionType::Mixed }
}

match classify_question(question) {
    Temporal     => search_episodic(question, top_k),
    Procedural   => search_procedural(question, top_k),
    Semantic     => search_semantic(question, top_k),
    Mixed        => union(search_all_kinds(question, top_k)),
}
```

Per-kind search is just FTS5 + vector search **filtered by memory_kind**. Same hybrid_search machinery, plus a WHERE clause.

### Why this works for the plateau categories

| LoCoMo category | Question shape | Best kind | What today's flat retrieval gets wrong |
|-----------------|----------------|-----------|----------------------------------------|
| `single_hop`    | "What did X say about Y?" | Semantic (attribute X→Y) | Retrieves 10 turn snippets where X mentioned Y; answerer has to integrate. |
| `temporal`      | "When did X happen?" | Episodic (time-bound event) | Retrieves 10 snippets without explicit dates; answerer hallucinates. |
| `multi_hop`     | "Did X and Y both Z?" | Semantic ∩ Semantic | Retrieves 10 snippets about X and Y separately; needs intersection. |
| `open_domain`   | "Tell me about X." | union of all kinds | Already works OK with flat retrieval. |

The plateau exists because `single_hop` (34%) and `multi_hop` (45%) make up 378 of the 1 540 questions and they are systematically failing under raw-chunk retrieval. Typed memory directly attacks both.

## Estimated impact

ENGRAM reports **+15 pp on LongMemEval** versus flat retrieval. LoCoMo is a different benchmark with shorter sessions and stronger temporal coherence, so a more conservative estimate is **+5–10 pp on Vestige's full N=1 540** — bringing the overall score from 62.21% to the 67–72% range, which clears Mem0 (66.88%) and approaches Zep (75.14%).

The smaller question types where the win should be concentrated:

| Category    | Current | Plausible after Tier 4 | Mechanism |
|-------------|---------|------------------------|-----------|
| single_hop  | 34.40%  | 48–55%                 | Semantic fact direct lookup |
| temporal    | 65.73%  | 72–78%                 | Episodic table with timestamp index |
| multi_hop   | 42.71%  | 55–65%                 | Per-entity semantic search → set operations |
| open_domain | 68.61%  | 68–72%                 | Mostly unchanged (already strong) |

These are **plausibility estimates**, not commitments. Verification happens via the same Phase 1 + Phase 2 harness, with a same-sample N=300 seed=42 A/B before the full N run.

## Migration plan

The work splits naturally into four PRs:

### PR 1 — `MemoryKind` enum + schema migration
* Add `memory_kind`, `subject`, `predicate`, `object`, `episodic_at`, `procedural_frequency` to `KnowledgeNode`.
* SQLite migration: `ALTER TABLE knowledge_nodes ADD COLUMN memory_kind TEXT DEFAULT 'raw';` etc.
* Backfill existing rows with `Raw`.
* All existing tests must still pass; behavior unchanged.

### PR 2 — Extraction pipeline
* New module `crates/vestige-core/src/extraction/typed.rs`.
* Rule-based classifier (`classify_kind(content) -> Vec<MemoryKind>` returning candidates).
* Optional LLM disambiguation (gated behind a feature flag, off by default — we shouldn't make local-only Vestige require an LLM API).
* Hook into `smart_ingest` so new memories get a non-`Raw` kind when extraction succeeds.
* Unit tests on the LoCoMo dataset's known facts.

### PR 3 — Per-kind retrieval
* Extend `Storage::hybrid_search` to accept `Option<MemoryKind>` filter.
* New tool `tools/typed_search` that classifies the query and routes to the right retriever.
* Wire into MCP server.
* Benchmark in `benchmarks/locomo`: add `LOCOMO_TYPED_RETRIEVAL=1` env-var.

### PR 4 — LongMemEval-S hold-out validation
* Build the LongMemEval harness in `benchmarks/longmemeval/`.
* Run Vestige (typed memory enabled) and confirm the gain transfers.
* Document results.

## Risks and mitigations

| Risk | Mitigation |
|------|-----------|
| **Extraction errors degrade retrieval.** If the classifier puts a procedural memory in the episodic bucket, queries miss it. | Keep `Raw` chunks alongside extracted types. Question classifier returns `Mixed` for ambiguous queries, which falls back to union retrieval. |
| **LLM-based extraction costs money at ingest time.** | Rule-based first pass handles ~70% of LoCoMo content deterministically. LLM only for disambiguation. Cap at $0.001 per memory; budget alerts. |
| **Schema migration breaks existing deployments.** | Backfill all existing rows to `Raw` (`memory_kind = 'raw'`) before any new code reads the column. Migration is idempotent and reversible. |
| **Overfitting to LoCoMo.** | PR 4 validates on LongMemEval-S before claiming the gain generalizes. |
| **Privacy / governance.** Typed memories with `subject` field are easier to audit (who's mentioned), but also easier to deanonymize. | Existing `privacy_governance` module already redacts; add type-aware redaction (semantic memories about people get stricter scope than procedural). |

## What this proposal does NOT change

* The cognitive engine (FSRS-6, consolidation, dream cycles, spreading activation) operates on `KnowledgeNode` regardless of kind. No retraining or re-calibration needed.
* The MCP tool surface stays backward compatible — `search` and `smart_ingest` keep their existing parameters; typed routing is opt-in via new tools.
* The dashboard. Typed memory might warrant a new view, but that's PR 5+.

## Decision needed before starting

1. **Option A vs Option B** (typed metadata on existing table vs separate tables per kind). Recommendation: A.
2. **LLM-disambiguation in the extractor** — on by default, off, or feature-gated? Recommendation: feature-gated, off by default for local-only deployments.
3. **Run LongMemEval-S harness before or after PR 3?** Recommendation: after — fastest path to a measurable LoCoMo number, then validate.
4. **Cost ceiling for the extraction LLM at ingest** — $0.001 per memory caps the cost at $5 per 5 000 ingests. Acceptable?

These are the questions to resolve in a follow-up planning conversation before opening PR 1.

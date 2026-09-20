# How Vestige Works

> The cognitive science behind intelligent memory

---

## Overview

Vestige is **inspired by** memory research. Here's what's actually implemented:

| Feature | Research Basis | Implementation |
|---------|----------------|----------------|
| **Spaced repetition** | [FSRS-6](https://github.com/open-spaced-repetition/fsrs4anki) | ✅ Fully implemented (21-parameter power law model) |
| **Context-dependent retrieval** | [Tulving & Thomson, 1973](https://psycnet.apa.org/record/1973-31800-001) | ✅ Fully implemented (temporal, topical, emotional context matching) |
| **Dual-strength model** | [Bjork & Bjork, 1992](https://bjorklab.psych.ucla.edu/wp-content/uploads/sites/13/2016/07/RBjork_EBjork_1992.pdf) | ⚡ Simplified (storage + retrieval strength tracked separately) |
| **Retroactive importance** | [Frey & Morris, 1997](https://www.nature.com/articles/385533a0) | ⚡ Inspired (temporal window capture, not actual synaptic biochemistry) |
| **Memory states** | Multi-store memory models | ⚡ Heuristic (accessibility-based state machine) |

> **Transparency**: The ✅ features closely follow published algorithms. The ⚡ features are engineering heuristics *inspired by* the research—useful approximations, not literal neuroscience.

---

## Prediction Error Gating

Before any comparison, every write passes the self-containedness gate. Content the repository already owns — a code block, a directory tree, copied source, a coverage figure, a version number — is refused outright (`decision: "reject"`, `stored: false`, nothing written), and a memory that would not be readable without the conversation that produced it is stored but flagged (`self_contained.requiresContext` plus `findings[]` naming the offending `kind`, `span` and a `hint`). See [AGENTS.md → Reading the self-containedness gate](../AGENTS.md).

Only then does `smart_ingest` compare new content against existing memories:

| Similarity | Action | Why |
|------------|--------|-----|
| > 0.92 | **REINFORCE** existing | Almost identical—just strengthen |
| > 0.75 | **UPDATE** existing | Related—merge the information |
| < 0.75 | **CREATE** new | Novel—add as new memory |

This prevents duplicate memories and keeps your knowledge base clean.

---

## FSRS-6 Spaced Repetition

Memories decay over time following a **power law forgetting curve** (not exponential):

```
R(t, S) = (1 + factor × t / S)^(-w₂₀)

where factor = 0.9^(-1/w₂₀) - 1
```

- `R` = retrievability (probability of recall)
- `t` = time since last review
- `S` = stability (time for R to drop to 90%)
- `w₂₀` = personalized decay parameter, clamped to FSRS-6's valid decay range **[0.01, 1.0]** (`storage/sqlite/fsrs_personalization.rs::save_personalized_w20`). Upstream default: 0.1542.

FSRS-6 uses 21 parameters optimized on 700M+ Anki reviews. The FSRS team's open [SRS benchmark](https://github.com/open-spaced-repetition/srs-benchmark) compares it against SM-2 and other schedulers; their published number is roughly a 30% reduction in review burden at the same target retention. Vestige uses the upstream default weights as-is and does **not** rerun that benchmark on its own corpus — quote the upstream measurement, not ours.

### Why Power Law?

| Algorithm | Model | Parameters | Source |
|-----------|-------|------------|--------|
| SM-2 (Anki default) | Exponential | 2 | 1987 research |
| SM-17 | Complex | Many | Proprietary |
| **FSRS-6** | Power law | 21 | 700M+ reviews |

Power law forgetting matches empirical data better than the exponential model most apps use.

---

## Memory States

Based on accessibility, memories exist in four states:

| State | Accessibility | Description |
|-------|---------------|-------------|
| **Active** | ≥70% | High retention, immediately retrievable |
| **Dormant** | 40-70% | Medium retention, retrievable with effort |
| **Silent** | 10-40% | Low retention, rarely surfaces |
| **Unavailable** | <10% | Below threshold, effectively forgotten |

Accessibility is calculated as:
```
accessibility = 0.5 × retention + 0.3 × retrieval_strength + 0.2 × storage_strength
```

**Two classifiers, one caveat.** The table above is the accessibility classifier used by `memory(action="state")` and the dashboard (`tools/memory_unified/helpers.rs`, thresholds `0.7 / 0.4 / 0.1`). The search pipeline and consolidation snapshots bucket on raw `retention_strength` instead, with a **0.3** Dormant cut-off (`tools/search_unified/pipeline/scoring.rs`, `storage/sqlite/consolidation.rs`). A memory at `retention_strength = 0.35` is therefore Dormant in `memory(action="state")` but Silent in search scoring and consolidation stats; the thresholds are not yet unified. See [ARCHITECTURE.md → Memory States](../ARCHITECTURE.md#memory-states).

Memories are never deleted automatically. They fade from relevance but can be revived if accessed again.

---

## Dual-Strength Memory

Based on **Bjork & Bjork's New Theory of Disuse (1992)**, every memory has two strengths:

| Strength | What It Means | How It Changes |
|----------|---------------|----------------|
| **Storage Strength** | How well-encoded the memory is | Only increases, never decreases |
| **Retrieval Strength** | How accessible the memory is now | Decays over time, restored by access |

**Why it matters**: A memory can be well-stored but hard to retrieve (like a name on the tip of your tongue).

---

## The Testing Effect

The **Testing Effect** (Roediger & Karpicke, 2006) is the finding that retrieving information strengthens memory more than re-studying it.

In Vestige: **Every search automatically strengthens matching memories.** When Claude recalls something:
- Storage strength increases slightly
- Retrieval strength increases
- The memory becomes easier to find next time

This is why the unified `search` tool is so powerful—using memories makes them stronger.

---

## Spreading Activation

**Spreading Activation** (Collins & Loftus, 1975) is how activating one memory primes related memories.

In Vestige's implementation:
- When you search for "React hooks", memories about "useEffect" surface due to **semantic similarity**
- Semantically related memories are retrieved even without exact keyword matches
- This comes from embedding vectors capturing conceptual relationships

---

## Synaptic Tagging & Capture

**Synaptic Tagging & Capture** (Frey & Morris, 1997) discovered that important events retroactively strengthen recent memories.

In Vestige the mechanism lives in `vestige_core::neuroscience::synaptic_tagging` and runs *inside* the ingest and consolidation paths — there is no `importance` tool to call:

- `smart_ingest` tags a new memory once its importance composite exceeds 0.3, and fires a PRP (plasticity-related protein) event above 0.7 (`tools/smart_ingest/post_ingest.rs`).
- `dream` replays the accumulated tags during consolidation (`tools/dream.rs`).
- The capture window defaults to **9 hours back / 2 hours forward** (`DEFAULT_BACKWARD_HOURS = 9.0`, `DEFAULT_FORWARD_HOURS = 2.0`) and tag lifetime to 12 hours. These are compile-time constants in the core engine, not per-call parameters.

The event taxonomy (`ImportanceEventType`: `UserFlag`, `EmotionalContent`, `NoveltySpike`, `RepeatedAccess`, `CrossReference`, `TemporalProximity`) exists in the core enum, but the MCP layer only ever emits `NoveltySpike` — so the "retroactively flag this as important" flow is not reachable through `tools/list` today. To strengthen a specific memory on demand, call `memory(action="promote")`; to score content before saving it, call `importance_score(content, context_topics, project)`.

---

## Context-Dependent Retrieval

Based on **Tulving's Encoding Specificity (1973)**: we remember better when retrieval context matches encoding context.

In Vestige this is stage 5 of the unified `search` pipeline, driven by the `context_topics` parameter:

```
search(
  query="error handling patterns",
  context_topics=["authentication"],
  token_budget=3000
)
```

Topic overlap between the retrieval topics and a memory's tags scales that memory's score by up to +30% (`tools/search_unified/pipeline/scoring.rs::apply_context_matching`). Only `search` exposes this knob — `session_context(context.topics=[...])` and `predict(context.current_topics=[...])` feed the predictive-retrieval `SessionContext` instead (the first topic becomes `current_focus`), not the search-time boost.

There is no separate `context` tool, and no `mood` / `time_weight` / `topic_weight` knobs — those parameters were never implemented. `context_topics` is the only context channel the search schema accepts.

---

## Hybrid Search with RRF

**Reciprocal Rank Fusion (RRF)** combines multiple ranking lists:

```
RRF_score(d) = Σ 1/(k + rank_i(d))
```

In Vestige:
1. BM25 keyword search produces a ranking.
2. Semantic search (Nomic v1.5 + USearch HNSW) produces a ranking.
3. RRF fuses them into the final ranking.
4. Jina Reranker v2 then cross-encodes the top-K candidates — the model has access to the full query and document text at once, not just their embeddings.
5. Retention strength provides additional weighting via the FSRS-6 trust score.

This gives you exact keyword matching, semantic understanding, **and** cross-encoder rescoring in one search.

## Compound Query Decomposition

A single dense embedding cannot represent two unrelated topics well. Vestige automatically splits queries containing semicolons, question chains, or conjunctions into sub-queries, runs each independently, and merges the results via max-score dedup.

Example: `"auth security; infrastructure costs"` → two searches, merged. `"Who worked on FSRS? And dream consolidation?"` → two searches, merged.

Effect size on production traffic has not been formally benchmarked outside the internal LoCoMo subset; treat the gain as workload-dependent.

## Metacognitive Tools (v3.1)

Three tools that go beyond automatic consolidation to enable deliberate self-examination:

| Tool | Purpose | Scientific Basis |
|------|---------|------------------|
| `reflect` | Active self-examination — contradictions, gaps, stale decisions, overconfident memories, pattern clusters | Flavell 1979 (metacognition), Schön 1983 (reflection-in-action), Nelson & Narens 1990 (metamemory) |
| `temporal` | Bi-temporal fact versioning — current, expired, history, invalidate | Snodgrass 1999 (bi-temporal), Graphiti/Zep 2024 (temporal knowledge graphs) |
| `confidence` | Heuristic multi-dimensional confidence (encoding / retrieval / temporal / evidence); `calibrate` is a retention-based consistency check, **not** Brier-score calibration | Inspired by Kahneman 2011 (dual process), Tetlock 2015 (superforecasting), Mercier & Sperber 2017 |

## Cognitive Reasoning (`deep_reference`)

Beyond search, `deep_reference` runs a full reasoning pipeline across memories: hybrid retrieval → FSRS-6 trust scoring → intent classification (FactCheck / Timeline / RootCause / Comparison / Synthesis) → temporal supersession → contradiction analysis → dream-insight integration → structured synthesis. Use it for "why?", "when did X change?", "what conflicts with this?" — anything that needs evidence, not just relevance.

---

## Embedding Model

**Nomic Embed Text v1.5** (via fastembed):
- 768-dimensional vectors, truncated to 384D via Matryoshka representation learning (~1% MTEB loss vs full 768, ~2× smaller vectors → lower HNSW memory footprint), then L2-renormalized.
- 8,192-token context window.
- ~547 MB ONNX model (`onnx/model.onnx` of `nomic-ai/nomic-embed-text-v1.5`, unquantized — measured in the fastembed cache).
- Runs 100% local (after first download).
- Competitive with OpenAI's `text-embedding-3-small` on MTEB.

## Reranker

**Jina Reranker v2 Base Multilingual** (278 M params, ~1.11 GB ONNX) cross-encodes the top-K candidates after hybrid retrieval. Multilingual coverage (100+ languages) and longer effective context than older bge-reranker variants.

Together the two downloads are **~1.68 GB** on first run. (Some code comments still quote older figures — `search/reranker.rs` says "~150 MB" in one place and "~1.1GB" in another; the measured cache sizes above are authoritative.)

## Cache Location

Models are cached by `fastembed` in platform-specific directories — see [`CONFIGURATION.md`](CONFIGURATION.md#model-cache-location) for paths.

---

## Performance

| Memories | Search Time | Memory Usage |
|----------|-------------|--------------|
| 100 | <10ms | ~50MB |
| 1,000 | <50ms | ~100MB |
| 10,000 | <200ms | ~300MB |
| 100,000 | <1s | ~1GB |

Performance is bounded by:
- SQLite FTS5 for keyword search (very fast)
- HNSW index for semantic search (sublinear scaling)
- Embedding generation (only on ingest, ~100ms each)

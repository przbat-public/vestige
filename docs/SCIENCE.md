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

When you call `smart_ingest`, Vestige compares new content against existing memories:

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
- `w₂₀` = personalized decay parameter (0.1-0.8)

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

In Vestige:
```
importance(
  memory_id="the-important-one",
  event_type="user_flag",
  hours_back=9,
  hours_forward=2
)
```

When you flag something important, it strengthens ALL memories from the surrounding time window (default: 9 hours back, 2 hours forward). This models biological memory consolidation.

---

## Context-Dependent Retrieval

Based on **Tulving's Encoding Specificity (1973)**: we remember better when retrieval context matches encoding context.

The `context` tool exploits this:
```
context(
  query="error handling patterns",
  project="my-api",
  topics=["authentication"],
  time_weight=0.3,
  topic_weight=0.4
)
```

If you learned something while working on auth, you'll recall it better when working on auth again.

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
- 768-dimensional vectors, truncated to 384D via Matryoshka representation learning (sub-linear quality loss, ~2× faster vector search).
- 8,192-token context window.
- ~130 MB model size.
- Runs 100% local (after first download).
- Competitive with OpenAI's `text-embedding-3-small` on MTEB.

## Reranker

**Jina Reranker v2 Base Multilingual** (278 M params, ~600 MB) cross-encodes the top-K candidates after hybrid retrieval. Multilingual coverage (100+ languages) and longer effective context than older bge-reranker variants.

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

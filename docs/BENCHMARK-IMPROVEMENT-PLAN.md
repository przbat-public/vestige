# Vestige LoCoMo benchmark improvement plan

> Goal: get Vestige's LoCoMo LLM Judge score from 41.00% (Apr 2026 baseline)
> to a defensible 70%+ without overfitting to the benchmark.

## Status (May 2026)

| | Apr 2026 | After Tier 1+2 | After Tier 1+2 + prompt v2 |
|---|---|---|---|
| Recall@5 (full N=1540) | 65.84% | **79.68%** | 79.68% |
| Recall@10 | 80.39% | **86.30%** | 86.30% |
| MRR | 0.4727 | **0.6889** | 0.6889 |
| LLM Judge Overall (full N=1540) | 41.00% | 54.81% | **60.13%** |
| LLM Judge — single_hop | 22.22% | 32.62% | **34.40%** |
| LLM Judge — temporal | 33.33% | 55.76% | **65.73%** |
| LLM Judge — multi_hop | 0.00% | 26.04% | **42.71%** |
| LLM Judge — open_domain | 52.73% | 65.16% | **68.61%** |

The **prompt v2** improvement isolates a +5.32 pp full-N gain (+6.00 pp on a same-sample seed=42 N=200 A/B) from a single change to the answerer system prompt. The change targets the two largest failure buckets identified in `benchmarks/locomo/analyze_failures.py`: `REFUSE` (15.2% of all questions, model refused despite evidence in top-5) and `COMP` (15.6%, model answered but missed list items or paraphrased temporal phrases). Specifically: (a) explicit anti-refuse policy (only refuse on no relevant evidence at all), (b) hypothetical/inferential mode for "would/likely" questions, (c) temporal precision hint that copies the memory's exact phrasing for relative dates.

### Failed experiment: prompt v3 (broader list-question detection)

v3 broadened `LIST_QUESTION_RE` from "what are X" to also catch "what books has X read", "what activities does Y do", "where has Z been" and added a plural-noun-tail fallback. Coverage on the 1 540-question set went from 166 hits (10.8%) to 426 hits (27.7%). On the seed=42 N=200 A/B:

| | v2 | v3 | Δ |
|---|---|---|---|
| Overall | 57.50% | 56.00% | **−1.50 pp** |
| single_hop | 28.95% | 23.68% | −5.27 pp |
| temporal | 74.42% | 72.09% | −2.33 pp |
| multi_hop | 44.44% | 44.44% | 0 |
| open_domain | 61.82% | 61.82% | 0 |

The plural-noun-tail catches adjunct plurals like "What does Melanie do with her family on hikes?" — the question asks for one activity, but the hint pushes the answerer to enumerate everything it sees. The cure is worse than the disease. **v3 reverted.** Result saved as `benchmarks/locomo/locomo_scores_promptv3_sample200.json` for the record.

Lesson: regex-based list detection is structurally fragile when the plural noun can be in adjunct position. The next attempt should ask `gpt-4o-mini` itself ("is this a list question — Y/N") in a tiny zero-shot pre-classifier rather than expanding the regex further. Or skip the hint entirely and rely on the answerer's policy section.

**Caveats** (do not skip these):

- The "Apr 2026 baseline" 41.00% is **not reproducible today** with the same code on the same data. Re-running the old `evaluate.py` against the old `retrieval_results.json` on the same seed=42 sample now yields ~21%, not 41%. `gpt-4o-mini` with `temperature=0` is not a stable contract. The clean apples-to-apples on the same 100-question sample, both arms run today, is **+28 pp** (21% → 49%); the +13.81 pp on full N is conservative because the baseline drifted up too.
- Multi-hop n=96 is the smallest category and the new score 26.04% is still the worst. This is where Tier 4 (typed memory) is expected to help most.
- Competitor numbers in the README (Mem0 66.88%, Zep 75.14%, Memobase 75.78%, SmartSearch 93.5%) come from each system's own harness with its own judge prompt. Treat them as rough order-of-magnitude until we run them through a shared harness.

## Diagnosis

Vestige's current LoCoMo score is 41.00% (`benchmarks/locomo/locomo_scores.json`,
v3.2.0). Competitors are at 67–76% (Mem0/Zep/Memobase) and SOTA is 93.5%
(SmartSearch paper, arXiv 2603.15599). The gap **is not in retrieval recall**:

| Metric            | Vestige v3.2.0 | What it tells us |
|-------------------|----------------|-------------------|
| Recall@5          | 65.84%         | Evidence is found ~2/3 of the time… |
| Recall@10         | 80.39%         | …and within top-10 80% of the time |
| MRR               | 0.4727         | First hit is on average between rank 2 and 3 |
| Overall LLM judge | 41.00%         | But the answerer can't extract it |

The bottleneck is **post-retrieval**, not retrieval itself. The SmartSearch
paper calls this the "compilation bottleneck": gold evidence survives initial
retrieval but is destroyed by truncation, ranking errors, and aggressive
context limits before reaching the answerer LLM.

Three concrete sources of loss in our current setup:

1. **Reranker bypassed in the harness.** `benchmarks/locomo/src/main.rs`
   called `storage.hybrid_search` directly, skipping Stage 2 (Jina v2
   cross-encoder). Production uses the reranker via `tools/search_unified`;
   the benchmark did not.
2. **Aggressive context truncation.** `evaluate.py` was hardcoded to top-3
   contexts × 800 chars (≈600 tokens total). LoCoMo open-domain answers
   often need 3 000+ tokens of evidence.
3. **Wrong judge model.** `locomo_scores.json` recorded `judge_model:
   gpt-4o-mini`, while the README claimed `gpt-4o`. Mem0/Zep/Memobase use
   gpt-4o, which is more lenient. This alone is worth ~3–5 absolute points.

Two architectural opportunities, beyond fixing the harness:

4. **Session-level chunking dilutes signal.** Each LoCoMo session is 10–30
   turns ingested as one memory. Single-hop questions about a specific turn
   then have to compete with dozens of unrelated turns in the same chunk.
5. **No typed memory.** ENGRAM (arXiv 2511.12960) shows that splitting
   memory into episodic/semantic/procedural and routing per-query gives
   SOTA on LongMemEval with ~1% of the tokens. Vestige stores everything
   as one undifferentiated graph.

## Tiered plan (impact × cost)

### Tier 1 — methodology fixes (no algorithm changes)

| # | Change | Where | Expected uplift | Cost |
|---|---|---|---|---|
| 1 | Enable Jina v2 reranker in the harness | `benchmarks/locomo/src/main.rs` | +5–10 abs. | low |
| 2 | Default judge to `gpt-4o` (was gpt-4o-mini) | `benchmarks/locomo/evaluate.py` | +3–5 abs. | low |
| 3 | Increase context: top-10 × 12 000 char budget | `benchmarks/locomo/evaluate.py` | +5–10 abs. | low |

These three are "free" — they fix evaluation methodology to match Mem0/Zep
and to actually exercise the production cognitive pipeline. Done in this PR.

### Tier 2 — score-adaptive context packing

| # | Change | Where | Expected uplift | Cost |
|---|---|---|---|---|
| 4 | Score-adaptive truncation (per-rank × score weights, sentence-aware) | `benchmarks/locomo/evaluate.py` | +3–7 abs. | low |
| 5 | List-question detection + aggregation hint | `benchmarks/locomo/evaluate.py` | +1–3 abs. | low |

The SmartSearch paper shows that intelligent truncation alone (their Sec 4)
recovers 70%+ of evidence that naive top-K loses. Done in this PR.

### Tier 3 — finer chunking (research)

| # | Change | Where | Expected uplift | Cost |
|---|---|---|---|---|
| 6 | Turn-level chunking with hierarchical retrieval (session → turn) | `benchmarks/locomo/src/main.rs`, `crates/vestige-core/src/storage/sqlite.rs` | +5–15 abs. on single_hop/temporal | medium |
| 7 | Time-aware re-ranking for `temporal` category | `crates/vestige-core/src/search/temporal.rs` | +3–8 abs. on temporal | medium |

Not in this PR. Requires schema for turn-id parents and a multi-stage retrieval
that first finds the session, then the turn within it.

### Tier 4 — typed memory (ENGRAM-style)

| # | Change | Where | Expected uplift | Cost |
|---|---|---|---|---|
| 8 | Episodic / semantic / procedural separation with per-type retrievers | `crates/vestige-core/src/memory/`, new `tools/typed_search.rs` | +5–15 abs., esp. multi_hop | high |

ENGRAM showed +15 abs. on LongMemEval with this. Big rewrite — not in this PR.

### Tier 5 — long-list aggregation (already partially in Tier 2)

| # | Change | Where | Expected uplift | Cost |
|---|---|---|---|---|
| 9 | Multi-evidence merging in the answerer prompt | `benchmarks/locomo/evaluate.py` | +1–2 abs. on open_domain | low |

Done as part of #5 above.

## Verification protocol (anti-self-deception)

The cheapest way to lie to yourself is to run the benchmark, see a number go
up, and call it a win. We avoid this with:

1. **Fixed sample / seed.** Subsample 100 questions with `LOCOMO_SAMPLE=100
   LOCOMO_SEED=42`. Use the same seed for every comparison until the very
   end, then run the full N=1 540 once for the final number.
2. **One change per measurement.** Run baseline first (`LOCOMO_USE_RERANKER=0`,
   old context budget). Then add reranker. Then add bigger context. Then
   score-adaptive. Each run must record its full `pipeline` config in
   `locomo_scores.json` for diff-ability.
3. **Per-category breakdown.** A change that bumps overall by +3 but tanks
   `temporal` by -10 is not a win for everyone. Always read per-category
   numbers.
4. **Bootstrap confidence intervals.** With N=100 the noise floor is ~±5
   absolute points. Don't claim wins smaller than that without a full run.
5. **Hold-out benchmark.** Once the LoCoMo number is stable, run the same
   pipeline against LongMemEval-S to see if the gains transfer or if we've
   overfit. (Harness not yet built — see open work below.)
6. **Failure case analysis.** For every change, sample 10 questions where
   the score flipped (correct → wrong or wrong → correct) and read what
   actually happened. Aggregate metrics hide both bugs and lucky breaks.
7. **Pipeline knob symmetry.** Keep `LOCOMO_USE_RERANKER=0` regression
   runs around so we can always answer "how much of the win is the
   reranker?" without rebuilding state.

## What this PR delivers

- ✅ Tier 1 #1 — Jina v2 reranker wired into the harness (`LOCOMO_USE_RERANKER`,
  default on; `LOCOMO_OVERFETCH=50`, `LOCOMO_TOPK=10`).
- ✅ Tier 1 #2 — `evaluate.py` default judge is now `gpt-4o`.
- ✅ Tier 1 #3 — `evaluate.py` defaults to top-10 contexts × 12 000 total char
  budget (was top-3 × 800 char hard cap). Hard cap is still honourable via
  `LOCOMO_CONTEXT_MAX_CHARS=…` for back-compat reproductions.
- ✅ Tier 2 #4 — Score-adaptive truncation with sentence-aware boundaries.
- ✅ Tier 2 #5 — List-question detection + aggregation hint in answerer prompt.
- ✅ Output format records the full pipeline config + sample params for
  reproducibility.

## What this PR does NOT change

- Storage schema, retrieval algorithm core, or memory model. Tier 3 and 4 are
  on the roadmap but require careful design and ablation; they belong in
  follow-up PRs with their own benchmarks.
- The MCP-facing `tools/search_unified` is unchanged. The harness still
  goes through `Storage::hybrid_search` (production parity); only Stage 2
  rerank is added in benchmark code so we can ablate it cleanly.

## Open work

- LongMemEval-S harness (hold-out benchmark for transfer claims).
- Turn-level chunking with hierarchical retrieval (Tier 3).
- ENGRAM-style typed memory and per-type routing (Tier 4).
- Stratified sampling for `LOCOMO_SAMPLE` so the per-category mix is
  representative even at N=100.

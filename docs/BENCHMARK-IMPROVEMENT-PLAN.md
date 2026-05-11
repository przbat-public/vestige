# Vestige LoCoMo benchmark improvement plan

> Goal: get Vestige's LoCoMo LLM Judge score from 41.00% (Apr 2026 baseline)
> to a defensible 70%+ without overfitting to the benchmark.

## Status (May 2026)

| | Apr 2026 | Tier 1+2 | + prompt v2 | + turn-level chunking |
|---|---|---|---|---|
| Recall@5 (full N=1540) | 65.84% | **79.68%** | 79.68% | 72.34% |
| Recall@10 | 80.39% | **86.30%** | 86.30% | 76.82% |
| MRR | 0.4727 | **0.6889** | 0.6889 | 0.6042 |
| LLM Judge Overall (full N=1540) | 41.00% | 54.81% | 60.13% | **62.21%** |
| LLM Judge — single_hop | 22.22% | 32.62% | 34.40% | **40.07%** |
| LLM Judge — temporal | 33.33% | 55.76% | 65.73% | **67.60%** |
| LLM Judge — multi_hop | 0.00% | 26.04% | 42.71% | **44.79%** |
| LLM Judge — open_domain | 52.73% | 65.16% | 68.61% | **69.56%** |

The **prompt v2** improvement isolates a +5.32 pp full-N gain (+6.00 pp on a same-sample seed=42 N=200 A/B) from a single change to the answerer system prompt. The change targets the two largest failure buckets identified in `benchmarks/locomo/analyze_failures.py`: `REFUSE` (15.2% of all questions, model refused despite evidence in top-5) and `COMP` (15.6%, model answered but missed list items or paraphrased temporal phrases). Specifically: (a) explicit anti-refuse policy (only refuse on no relevant evidence at all), (b) hypothetical/inferential mode for "would/likely" questions, (c) temporal precision hint that copies the memory's exact phrasing for relative dates.

### Tier 3: turn-level chunking (May 11, 2026)

**Net result: +2.08 pp full-N (60.13% → 62.21%). All four categories improved.** The smoke test on conv-26 alone showed +11.84 pp (53.95% → 65.79%), so the full-dataset gain is conservative — most of the conv-26 lift came from its unusually bad session-level single_hop baseline (12.50%), which average conversations don't share.

Implementation: `LOCOMO_CHUNK_LEVEL=turn` ingests each conversational turn as a separate memory (419–689 turns/conv vs 19–32 sessions). Each chunk carries timestamp + speaker + utterance, tagged with `turn:<dia_id>` for evidence matching. Reranker now sees 20 individual turns and picks the 10 most relevant, instead of 20 dense session blocks where the relevant turn sits next to 19 unrelated ones.

A/B vs session-level prompt v2 (same N=1540, same answerer/judge, same prompts):

| | Session | Turn | Δ |
|---|---|---|---|
| **Overall** | **60.13%** | **62.21%** | **+2.08 pp** |
| single_hop | 34.40% | 40.07% | **+5.67 pp** |
| temporal | 65.73% | 67.60% | +1.87 pp |
| multi_hop | 42.71% | 44.79% | +2.08 pp |
| open_domain | 68.61% | 69.56% | +0.95 pp |
| Recall@5 | 79.68% | 72.34% | −7.34 pp |
| Recall@10 | 86.30% | 76.82% | −9.48 pp |
| MRR | 0.6889 | 0.6042 | −0.0847 |
| Search wall-time | 4 383 s | 534 s | **8× faster** |

The retrieval-vs-judge inversion (Recall down, Judge up) is the compilation bottleneck in action: when each chunk is 1 turn instead of 20, the answerer model gets exactly the evidence it needs, not the evidence buried in a session-sized haystack. The metric "is any evidence dia_id in top-10" punishes turn-level (one match per slot vs many) — but the metric we care about (can the answerer compile the right answer) rewards it.

Multi_hop +2.08 pp was the surprise. The hypothesis was that splitting sessions would hurt multi-hop questions because they need to connect facts across turns. It didn't — apparently the reranker pulls 10 relevant turns from across 3–5 sessions, giving the answerer more breadth, not less.

Cost: +1 100 s of Phase 1 wall-time (the index is ~20× larger) but each query is 5× faster because cross-encoder reranks 20 short turns instead of 20 long sessions. Net Phase 1 time roughly equal. No core changes — pure harness modification.

Result file: `benchmarks/locomo/locomo_scores.json` (canonical). Historical session-level: `benchmarks/locomo/locomo_scores_session_v2.json`. Smoke A/B on conv-26: `/tmp/locomo_{session,turn}_smoke_scores.json`.

### Plateau session (May 11, 2026 — four negative experiments)

After turn-level got us to 62.21%, four more harness-only experiments were attempted on a same-sample N=300 seed=42 A/B. All four failed or didn't move the needle. **Phase 2 has hit a plateau at 64.67% on this sample (62.21% on full N).** Per category numbers on the N=300 sample:

| Variant                       | Overall | single_hop | temporal | multi_hop | open_domain |
|-------------------------------|---------|------------|----------|-----------|-------------|
| Session-level (prompt v2)     | 56.67%  | 24.00%     | 67.24%   | 50.00%    | 63.22%      |
| **Turn-level (canonical)**    | **64.67%** | **40.00%** | **74.14%** | **50.00%** | **70.11%** |
| Hybrid (session ∪ turn)       | 65.00%  | 42.00%     | 68.97%   | 44.44%    | 72.41%      |
| Turn + entity-overlap rerank  | 63.67%  | 40.00%     | 74.14%   | 38.89%    | 69.54%      |
| Turn + time-aware rerank      | 63.67%  | 40.00%     | 68.97%   | 38.89%    | 71.26%      |
| Turn OVERFETCH=40, TOPK=10    | 64.67%  | 34.00%     | 70.69%   | 38.89%    | 74.14%      |
| Turn OVERFETCH=40, TOPK=20    | 64.67%  | 36.00%     | 72.41%   | 44.44%    | 72.41%      |

Sample size n=50 (single_hop), n=58 (temporal), n=18 (multi_hop), n=174 (open_domain) — multi_hop ±11 pp = ±2 questions = within noise.

**Findings:**

1. **Hybrid (session ∪ turn) ties turn-level** at 65.00% vs 64.67% (Δ+0.33 pp, noise floor). Wins single_hop (+2 pp), open_domain (+2.3 pp). Loses temporal (-5.17 pp) and multi_hop (-5.56 pp). Net wash — kept as `LOCOMO_CHUNK_LEVEL=hybrid` env-var, not promoted to default. The mechanism: with mixed session + turn candidates, the reranker sometimes picks the fat session chunk for a temporal question, then `assemble_context_block` truncates it more aggressively, the answerer sees mixed granularity and underperforms.

2. **Entity-overlap rerank is net negative (-1.00 pp).** Implementation: post-Phase-1 re-sort blending 0.7 × normalized Jina score + 0.3 × proper-noun substring match. The Jina cross-encoder already handles entity semantics; the heuristic boost just shuffles ranks without adding signal. Multi_hop hit hardest (-11 pp, n=18 = ±2 questions of noise): entity-rerank pulls only chunks mentioning the question's entity, but multi_hop wants breadth across speakers/sessions. Code kept as `LOCOMO_ENTITY_RERANK=1`, off by default, documented as failed.

3. **Time-aware rerank is also net negative (-1.00 pp), and SURPRISINGLY hurts temporal (-5.17 pp).** Implementation: when the question contains a month name or year, boost contexts whose `[timestamp]` header matches. The trap: in turn-level chunking, ALL turns from one session share the same timestamp (session header). A question mentioning "May 2023" boosts every turn from sessions in May 2023, collapsing the top-10 to a single session worth of turns — multi-hop and temporal questions then lose access to evidence in adjacent sessions. Code kept as `LOCOMO_TIME_RERANK=1`, off by default.

4. **OVERFETCH=40 + TOPK=20 improves retrieval but not LLM Judge.** Phase 1 Recall@5 jumps from 72.34% to 76.88% (+4.54 pp) — Jina has 2× more candidates to rerank. Phase 2 stays at 64.67% on the same N=300 sample regardless of whether we pass 10 or 20 contexts to the answerer. With 20 contexts the per-chunk char budget halves (600 vs 1200 chars), and the answerer's compilation pass dilutes. With 10 contexts the better top-10 doesn't yield better answers either. Diagnosis: the bottleneck is no longer in retrieval recall.

**Plateau hypothesis:** the answerer LLM (`gpt-4o-mini` with temperature=0) is hitting a discrimination ceiling around 64–65% on this sample. Multiple retrieval/rerank configurations cluster at the same score because the answerer cannot extract the right answer even when evidence is in front of it, OR the judge's grading is at saturation. Next round must change the LLM stack (different answer model, different judge calibration) or the memory representation (typed memory, structured fields), not the retrieval ranking.

### What's next (no longer harness-only)

- **Tier 3 #6b — true hierarchical retrieval** (session-then-turn drill-in). The flat turn-level chunker may double-retrieve from one session at the cost of crowding out others. Genuine hierarchy first finds the K most relevant SESSIONS by topic, then drills into the top T turns within each. Expected +1–3 pp on multi_hop and temporal where breadth matters. Requires a multi-stage `Storage::hybrid_search` variant or two passes in the harness with tag-filtered second search.

- **Tier 4 — typed memory (ENGRAM-style)** is the biggest remaining lever. Episodic memories (one event, one timestamp) vs semantic facts (extracted entity attributes) vs procedural patterns. LongMemEval results in the ENGRAM paper (arXiv 2511.12960) suggest +5–15 pp depending on the question type. Requires changes to `crates/vestige-core/src/memory/` to carry a `memory_kind` enum, plus per-kind retrieval routing in `tools/search_unified`. Big rewrite, multi-PR.

- **LongMemEval-S hold-out harness.** Whatever we change next, we should verify on a second dataset before claiming it generalizes. The LoCoMo gains so far are real but the harness is now well-tuned to LoCoMo; a clean external benchmark catches overfitting. Build the harness in `benchmarks/longmemeval/` with the same two-phase split (Rust retrieval + Python LLM judge).

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

| # | Change | Where | Expected uplift | Cost | Status |
|---|---|---|---|---|---|
| 6 | Turn-level chunking — one memory per turn (no hierarchy yet) | `benchmarks/locomo/src/main.rs` | +5–15 abs. on single_hop | medium | ✅ done, +2.08 pp full (+5.67 pp single_hop) |
| 6b | Hierarchical retrieval (session → turn within session) | same files | +1–3 pp on multi_hop/temporal | medium | pending |
| 7 | Time-aware re-ranking for `temporal` category | `crates/vestige-core/src/search/temporal.rs` | +3–8 abs. on temporal | medium | pending |

Tier 3 #6 done in a follow-up commit: `LOCOMO_CHUNK_LEVEL=turn` ingests each turn as a separate memory. No core changes, harness only. Tier 3 #6b (true hierarchy — find session first, then drill into its turns) is the next step; current implementation is "flat turn-level" and may double-retrieve turns from the same session at the cost of crowding out other sessions. Hybrid (`session ∪ turn`) was sketched but not run yet because flat turn-level already net positive.

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

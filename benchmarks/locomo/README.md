# LoCoMo Benchmark for Vestige

Measures Vestige's long-term conversational memory against the [LoCoMo benchmark](https://github.com/snap-research/LoCoMo) (Maharana et al., ACL 2024).

10 conversations, ~300 turns each across ~27 sessions, 1,540 QA pairs in 4 categories: single-hop, temporal, multi-hop, open-domain.

> Dataset shape check (offline, from `data/locomo10.json`): 10 conversations, 27.2
> sessions on average (19–32), 1,986 QA pairs of which 446 are category-5 adversarial
> and skipped, leaving exactly 1,540 scored — matching `retrieval_metrics.count = 1540`
> in every full-N artifact. "~300 turns" is the LoCoMo paper's per-conversation figure;
> the raw file holds ~588 speaker utterances per conversation when both speakers are
> counted.

## LLM Judge — full N=1540 (May 2026 run, this repo)

Every number in the table below is read out of the `locomo_scores_*.json` artifacts
committed next to this file, and `python3 benchmarks/locomo/render_table.py --check`
re-derives all of them and exits non-zero if the README and the artifacts ever drift.
Rows marked **†** have **no artifact in this directory**: they are history, not
measurements reproducible from this tree.

| System                                | Overall  | single_hop | temporal | multi_hop | open_domain |
|---------------------------------------|----------|------------|----------|-----------|-------------|
| **Vestige (current, turn + facts)**   | **66.17%** | 40.78%   | 71.65%   | 46.88%    | 74.79%      |
| Vestige (turn-level only)             | 62.21%   | 40.07%     | 67.60%   | 44.79%    | 69.56%      |
| Vestige (session-level, prompt v2)    | 60.13%   | 34.40%     | 65.73%   | 42.71%    | 68.61%      |
| Vestige (session-level, harness only) † | 54.81% | 32.62%     | 55.76%   | 26.04%    | 65.16%      |
| Vestige (Apr 2026) †                  | 41.00%   | 22.22%     | 33.33%   | 0.00%     | 52.73%      |

Artifact mapping, re-checked by `render_table.py --check`:

| Row | Artifact (in this directory) | n | judge | chunk level |
|-----|------------------------------|---|-------|-------------|
| Vestige (current, turn + facts) | `locomo_scores.json` | 1540 (1529 distinct pairs) | `gpt-4o` | `turn_extracted` |
| Vestige (turn-level only) | `locomo_scores_turn_flat.json` | 1540 (1529) | `gpt-4o` | `turn` |
| Vestige (session-level, prompt v2) | `locomo_scores_session_v2.json` | 1540 (1529) | `gpt-4o` | session |
| Vestige (session-level, harness only) † | **none** | — | — | — |
| Vestige (Apr 2026) † | **none** | — | — | — |

**Correction (2026-09-19).** This table previously printed **34.40%** in the
`single_hop` column of the *turn-level* row. That value belongs to the row below it:
34.40% is `locomo_scores_session_v2.json` `per_category.single_hop.llm_score = 0.343972`.
The turn-level artifact scores **40.07%** (`locomo_scores_turn_flat.json`,
`per_category.single_hop.llm_score = 0.400709`), which is also what
`docs/BENCHMARK-IMPROVEMENT-PLAN.md:14` reports. The row was re-derived from the
artifact rather than hand-edited.

The two † rows are **not reproducible from this tree**: no `locomo_scores*.json` here
yields 54.81% or 41.00%, and the Apr 2026 row's per-category split
(22.22 / 33.33 / 0.00 / 52.73) matches no committed file either. They stay in the table
as history and must not be cited as current measurements.

**Correction (2026-09-19), second paragraph.** The Apr 2026 figure was committed before
the reranker + score-adaptive context were wired into the harness, and it does **not**
reproduce today on the current `gpt-4o-mini` snapshot: re-running the old evaluator on
the same seed=42 100-question sample scores **21.00%**
(`locomo_scores_C_OLD_evaluator.json`, n=100). The best same-questions pair in this tree
is therefore 21.00% → **49.00%** (`locomo_scores_sample100.json`, judge `gpt-4o`; the two
files contain the same 99 distinct `(question, ground_truth)` pairs — asserted by
`render_table.py --check`) = **+28 pp on the small sample**, *not* the "+45 pp" this file
previously claimed. Even that pair changes the judge (`gpt-4o-mini` → `gpt-4o`), and the
41.00% → 66.17% headline (**+25.17 pp**) changes both N (100 → 1540) and judge; it is a
historical summary, not a controlled delta. `docs/BENCHMARK-IMPROVEMENT-PLAN.md`
documents the same caveats at `:198` and `:250`.

The only **judge-controlled and question-controlled** comparisons available in this
directory are full-N pairs. `render_table.py --mcnemar A B` runs an exact two-sided
McNemar test on the shared questions (stdlib only):

| Comparison | Shared questions | Delta (mean score, second − first, full N) | Discordant (first-only / second-only) | Exact two-sided p |
|------------|------------------|----------------------------|----------------------------------------|-------------------|
| `session_v2` → `turn_flat` | 1529 | +2.08 pp | 220 / 251 | 0.1668 |
| `turn_flat` → `turn_extracted` | 1529 | +3.96 pp | 110 / 172 | 0.0003 |
| `C_OLD_evaluator` → `sample100` (different judge) | 99 | +28.00 pp | 0 / 28 | <0.0001 |

Deltas are the published full-set differences (1,540 questions, or 100 for the last
row). Restricting to the shared pairs changes them slightly — +2.03 pp, +4.05 pp and
+28.28 pp respectively — because a handful of duplicate `(question, ground_truth)`
pairs drop out; the p-value is computed on the shared pairs only.

Reading: the turn-level step (+2.08 pp, which
`docs/BENCHMARK-IMPROVEMENT-PLAN.md:25` summarises as "all four categories improved") is
**not statistically significant** on the paired questions, p = 0.1668; the turn+facts
step is, p = 0.0003. The 21% → 49% small-sample pair is extremely one-sided (0 questions
regressed) but changes the judge, so it measures the judge change as much as the harness
change. A standalone calculator for any other pair:
`python3 benchmarks/locomo/render_table.py --mcnemar locomo_scores_turn_flat.json locomo_scores.json`.

**Current best (turn_extracted) ingests BOTH per-turn dialogue chunks AND per-fact atomic memories** extracted at ingest time by `benchmarks/locomo/extract_facts.py`. The reranker picks granularity per question — atomic facts win for `single_hop` and `temporal` direct lookups, raw turns win for `open_domain` narrative questions. Together they break the plateau that seven retrieval-side experiments couldn't crack.

### Competitor reference (different harnesses, treat as rough)

| System     | Score  |
|------------|--------|
| SmartSearch (paper) | 93.5% |
| Memobase   | 75.78% |
| Zep        | 75.14% |
| Mem0-Graph | 68.44% |
| Mem0       | 66.88% |
| LangMem    | 58.10% |
| OpenAI     | 52.90% |

These numbers come from the respective papers/blogs (different judge prompts, different context budgets, sometimes different LoCoMo subsets). They are **external and not covered by any artifact in this directory**, so `render_table.py --check` deliberately ignores them. Direct head-to-head requires running each system through the exact same harness — see `docs/BENCHMARK-IMPROVEMENT-PLAN.md`.

## Quick start

### 1. Download dataset

```bash
mkdir -p benchmarks/locomo/data
curl -L -o benchmarks/locomo/data/locomo10.json \
  https://raw.githubusercontent.com/snap-research/locomo/main/data/locomo10.json
```

### 2. Phase 1: Retrieval evaluation (no LLM needed)

```bash
cargo run --release -p vestige-locomo-bench -- benchmarks/locomo/data/locomo10.json
```

This ingests all sessions using real embeddings (nomic-embed-text-v1.5; first run downloads the ~547 MB ONNX model — corrected 2026-09-19, this line said "~130MB", which contradicts `docs/CONFIGURATION.md:11` and the root `README.md`), searches with hybrid BM25+semantic, and reports retrieval metrics:

- **Recall@5/10**: Does the evidence passage appear in top K results?
- **MRR**: Mean Reciprocal Rank of first evidence match

Output: `benchmarks/locomo/retrieval_results.json`

Expect ~5-15 minutes depending on hardware (embedding 10 conversations × ~27 sessions each). Runtime and cost figures in this section are operational estimates, not artifact-backed numbers.

### 3. Phase 2: LLM judge (needs OpenAI API)

```bash
pip install openai
export OPENAI_API_KEY=sk-...
python benchmarks/locomo/evaluate.py
```

This generates answers from retrieved context (GPT-4o-mini) and judges them against ground truth (GPT-4o). Produces the LLM Judge Score directly comparable to the table above.

Output: `benchmarks/locomo/locomo_scores.json`

Cost estimate: ~$3-5 for 1,540 questions (GPT-4o-mini for answers + GPT-4o for judging).

## Configuration

### Retrieval harness (`vestige-locomo-bench`)

| Variable | Default | Description |
|----------|---------|-------------|
| `LOCOMO_USE_RERANKER` | `1` | Set to `0` to disable Stage 2 cross-encoder rerank (ablation) |
| `LOCOMO_OVERFETCH` | `50` | Initial hybrid_search candidates before rerank |
| `LOCOMO_TOPK` | `10` | Final top-K returned to evaluator |
| `LOCOMO_CHUNK_LEVEL` | `session` | `session` (one memory = one session, 10–30 turns), `turn` (one memory = one utterance; **verified** +2.08 pp over session on full N, paired McNemar p = 0.1668 — *not* significant), `hybrid` (session ∪ turn — **unverified here**: no full-N hybrid artifact; the only one is a 300-question sample at 65.00%, `locomo_scores_hybrid_sample300.json`, with no same-sample session/turn comparison in the tree), `extracted` (atomic facts from `LOCOMO_EXTRACTED_PATH` sidecar, ENGRAM-style; reported as net **negative** alone on full N because facts strip narrative — **no `extracted` artifact in this directory, unverified here**), or `turn_extracted` (turns + facts side-by-side, **recommended**; **verified** +3.96 pp over turn-only on full N, paired McNemar p = 0.0003). |
| `LOCOMO_EXTRACTED_PATH` | `benchmarks/locomo/data/locomo10_extracted.json` | Sidecar produced by `python benchmarks/locomo/extract_facts.py`. Required for `extracted` and `turn_extracted` chunk levels. |
| `LOCOMO_MAX_CONVERSATIONS` | (unset) | Limit to first N conversations for smoke/sample runs |
| `LOCOMO_HIERARCHICAL` | `0` | Two-stage retrieval. Stage 1 over-fetches 3×, groups candidates by `session_key`, keeps only top-K most-relevant sessions, then reranks. Reported as lifting Phase 1 Recall@5 by +5.52 pp (72.34% → 77.86%) while Phase 2 LLM Judge stays flat at the plateau — **not verifiable in this directory**: there is no hierarchical artifact here, and no committed artifact carries recall@5 = 77.86% (the values present are 72.34% turn-flat, 78.18% turn+facts, 79.68% session_v2). Kept for ablation, off by default. |
| `LOCOMO_HIER_SESSIONS` | `5` | How many sessions to keep in hierarchical mode. K=15 worked best on conv-26 smoke; on full N the LLM Judge plateaus regardless. |

### LLM judge (`evaluate.py`) — main knobs

| Variable | Default | Description |
|----------|---------|-------------|
| `LOCOMO_ANSWER_MODEL` | `gpt-4o-mini` | Answer generation model |
| `LOCOMO_JUDGE_MODEL` | `gpt-4o` | Judge model (Mem0/Zep also use this) |
| `LOCOMO_TOP_K_CONTEXTS` | `10` | How many of Phase 1's retrieved contexts to feed the answerer |
| `LOCOMO_TOTAL_CHAR_BUDGET` | `12000` | Total character budget split across contexts |
| `LOCOMO_SAMPLE` / `LOCOMO_SEED` | `0` / `42` | Random subset of questions for A/B (0 = full N=1540) |
| `LOCOMO_CONVERSATIONS` | _(unset)_ | Comma-separated `sample_id`s (`conv-26`, …) to restrict the run to specific conversations. Applied **before** sampling. The list actually used is recorded in the output JSON as `conversations`, which is what a conversation-level tune/hold-out split needs — see the hold-out note in Methodology. |
| `LOCOMO_THROTTLE_SECS` | `1.0` | Sleep between LLM calls (set to `0` if rate limit permits) |
| `LOCOMO_MAX_WORKERS` | `1` | Parallel question workers (set to `4` with throttle=0 for speed) |

### LLM judge (`evaluate.py`) — experimental knobs (off by default, kept for ablation)

The `failed …` verdicts in this table come from the original experiment log and have
**no artifact in this directory** — `render_table.py --check` cannot confirm them and
does not try. Treat them as author notes, not as measured results.

| Variable | Default | Status | Description |
|----------|---------|--------|-------------|
| `LOCOMO_ENTITY_RERANK` | `0` | failed −1.00 pp | Post-retrieval re-rank blending Jina score with proper-noun substring overlap from the question. Jina already handles entities; the heuristic just shuffles ranks. |
| `LOCOMO_ENTITY_BLEND` | `0.3` | — | Blend ratio for entity overlap if enabled |
| `LOCOMO_TIME_RERANK` | `0` | failed −1.00 pp | Post-retrieval re-rank boosting contexts whose `[timestamp]` header matches a month/year in the question. Backfires on turn-level because all turns from one session share the session timestamp — boosting collapses the top-10 to one session and hurts multi-hop / temporal. |
| `LOCOMO_TIME_BLEND` | `0.25` | — | Blend ratio for time overlap if enabled |
| `LOCOMO_KIND_ROUTING` | `0` | failed +0.33 pp (noise) at boost=0.20, regressions at other settings | Per-kind retrieval routing — classifies the question into {semantic, episodic, procedural} and boosts candidates whose `[kind]` prefix matches. Tested with kind-only and kind+subject combined signals; both within ±2 pp of the `turn_extracted` baseline, but the Jina v2 reranker already picks granularity correctly from the candidate pool. Post-retrieval heuristic boosts cannot beat the cross-encoder. |
| `LOCOMO_KIND_BOOST` | `0.20` | — | Boost magnitude when enabled |

### LLM judge (`evaluate.py`)

| Variable | Default | Description |
|----------|---------|-------------|
| `LOCOMO_ANSWER_MODEL` | `gpt-4o-mini` | Model for generating answers from context |
| `LOCOMO_JUDGE_MODEL` | `gpt-4o` | Model for judging correctness (matches Mem0/Zep/Memobase) |
| `LOCOMO_TOP_K_CONTEXTS` | `10` | Number of retrieved contexts to pass to the answerer |
| `LOCOMO_TOTAL_CHAR_BUDGET` | `12000` | Total characters across all contexts (≈3 000 tokens) |
| `LOCOMO_MIN_PER_CONTEXT_CHARS` | `300` | Minimum slice per kept context (drop below this) |
| `LOCOMO_CONTEXT_MAX_CHARS` | _(unset)_ | Hard per-context cap. Set to e.g. `800` to reproduce old runs |
| `LOCOMO_MAX_WORKERS` | `1` | Parallel API calls |
| `LOCOMO_THROTTLE_SECS` | `1.0` | Sleep between requests when MAX_WORKERS=1 |
| `LOCOMO_SAMPLE` | `0` | Subsample N questions (deterministic with `LOCOMO_SEED`) |
| `LOCOMO_SEED` | `42` | RNG seed for sampling |
| `LOCOMO_CONVERSATIONS` | _(unset)_ | Restrict to specific conversations before sampling; recorded in the output as `conversations` |

## How it works

### Phase 1 (Rust)

For each of 10 conversations:
1. Parse sessions from `locomo10.json`.
2. Create a temporary Vestige Storage (SQLite + embeddings + HNSW index).
3. Ingest each session as a memory with content preprocessing.
4. For each QA pair, run **two-stage retrieval**:
   - **Stage 1** — `hybrid_search` (BM25 + semantic via RRF) with `LOCOMO_OVERFETCH` candidates
   - **Stage 2** — Jina Reranker v2 Base Multilingual cross-encoder, returning the top `LOCOMO_TOPK`
5. Compute Recall@5/10 and MRR over the reranked top-K, plus per-category breakdown.

The reranker is loaded once at startup (~1.1 GB, cached after first run).
Set `LOCOMO_USE_RERANKER=0` to measure raw hybrid_search as a baseline.

### Phase 2 (Python)

For each QA pair:
1. Take top-`LOCOMO_TOP_K_CONTEXTS` retrieved contexts from Phase 1 with their reranker scores.
2. **Score-adaptive truncation**: distribute `LOCOMO_TOTAL_CHAR_BUDGET` across contexts using
   per-rank weights blended with the relevance scores. Boundaries are sentence-aware so we don't
   slice mid-thought. This addresses the "compilation bottleneck" identified in the SmartSearch
   paper (arXiv 2603.15599) — high retrieval recall is wasted when truncation eats the gold span.
3. Detect list-style questions ("what are…", "list…", "name all…") and add an aggregation hint
   to the answerer prompt — LoCoMo open-domain has many multi-item answers.
4. Ask `LOCOMO_ANSWER_MODEL` (default gpt-4o-mini) to answer the question given those contexts.
5. Ask `LOCOMO_JUDGE_MODEL` (default gpt-4o) to judge: does the answer match ground truth? (binary).
6. Aggregate into per-category and overall LLM Judge Scores.

## File structure

```
benchmarks/locomo/
├── Cargo.toml              # Rust binary config
├── src/main.rs             # Phase 1: retrieval harness
├── evaluate.py             # Phase 2: LLM judge
├── render_table.py         # Re-derives every published number from the locomo_scores_*.json
│                           #   artifacts; `--check` fails if README.md drifts from them
├── README.md               # This file
└── data/
    └── locomo10.json       # Dataset (not committed, download above)
```

## Methodology notes

- **Chunking is a measured lever, not a fixed default**: this harness defaults to session-level chunking (matching Mem0, Zep and Memobase), but the artifacts here cover session (`locomo_scores_session_v2.json`, 60.13%), turn (`locomo_scores_turn_flat.json`, 62.21%) and turn+facts (`locomo_scores.json`, 66.17%) on the full set — the 2026-09-19 correction above is exactly a case of two of those rows being confused. `hybrid` and `extracted` have no full-N artifact in this directory and are marked unverified wherever they are described.
- **Evidence matching**: LoCoMo QAs include `evidence` fields referencing specific dialog IDs. We check if the session containing those IDs appears in top-K search results.
- **No adversarial questions**: Category 5 (adversarial) is skipped, consistent with published benchmarks.
- **Independent storage per conversation**: Each conversation gets its own database, preventing cross-conversation leakage.
- **Reranker honesty**: The Stage 2 reranker model is only loaded once and shared across all conversations; the model itself is the same Jina v2 used in production via `tools/search_unified`. Disable it via `LOCOMO_USE_RERANKER=0` to measure how much it contributes vs raw hybrid retrieval.
- **Anti-self-deception**: Each `locomo_scores.json` records the full pipeline config (`pipeline.*`, `top_k_contexts`, `total_char_budget`, `judge_model`, `sample_size`, `sample_seed`). Always compare runs with the same config; otherwise the delta is methodology, not improvement.
- **Numbers must come from artifacts**: `render_table.py --check` re-derives the table in this file (overall + per-category, row by row, artifact by artifact) and the derived deltas and McNemar p-values quoted here, failing loudly on any mismatch. Numbers with no artifact behind them are marked as such in this file instead of being quietly dropped.
- **No hold-out set (known limitation)**: prompt v2/v3 and the `hybrid`/`turn_extracted` choices were selected on questions that also appear in the reported set. The prompt-selection sample is 199 distinct `(question, ground_truth)` pairs (`locomo_scores_promptv1/v2/v3_sample200.json` — all three files contain the *same* pairs, and they are a subset of the 297-pair `hybrid` sample, which is a subset of the 1529 distinct reported pairs; `render_table.py --check` asserts all three inclusions). That is ~13% of the reported pairs, so the published 66.17% is optimistically biased by selection on part of its own test set. The fix is a conversation-level tune/hold-out split: `LOCOMO_CONVERSATIONS=conv-26,conv-30 python evaluate.py` on the tune half, then re-run with the conversation list held out, comparing against the recorded `conversations` field. **This split has not been run**, so no number here is a held-out result.
- **Determinism warning**: `gpt-4o-mini` with `temperature=0` is **not** bitwise reproducible. Re-running the old code + old retrieval on seed=42 today yields 21.00% (`locomo_scores_C_OLD_evaluator.json`; the sibling `locomo_scores_C_baseline_sample100.json` on a 3-context budget yields 20.00%) rather than the historical 41.00%, which has no artifact here at all — same code, same data. When you publish a number, either (a) record the exact `gpt-4o-mini-YYYY-MM-DD` snapshot, (b) re-baseline before claiming a delta, or (c) hold the model fixed across all arms of the comparison and report A/B deltas, not absolutes. The comparisons in the significance table above follow (c) for the two full-N pairs.

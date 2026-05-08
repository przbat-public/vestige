# LoCoMo Benchmark for Vestige

Measures Vestige's long-term conversational memory against the [LoCoMo benchmark](https://github.com/snap-research/LoCoMo) (Maharana et al., ACL 2024).

10 conversations, ~300 turns each across ~27 sessions, 1,540 QA pairs in 4 categories: single-hop, temporal, multi-hop, open-domain.

## LLM Judge — full N=1540 (May 2026 run, this repo)

| System                | Overall  | single_hop | temporal | multi_hop | open_domain |
|-----------------------|----------|------------|----------|-----------|-------------|
| **Vestige (current)** | **54.81%** | 32.62%   | 55.76%   | 26.04%    | 65.16%      |
| Vestige (Apr 2026)\*  | 41.00%   | 22.22%     | 33.33%   | 0.00%     | 52.73%      |

\*Apr 2026 figure was committed to this repo in the snapshot before the reranker + score-adaptive context were wired into the harness. It does not reproduce on the current `gpt-4o-mini` snapshot — re-running the same code on the same seed=42 sample today yields ~21%, i.e. apples-to-apples gain on identical questions is **+28 pp**, of which **+13.81 pp** survives the move to full N=1 540.

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

These numbers come from the respective papers/blogs (different judge prompts, different context budgets, sometimes different LoCoMo subsets). Direct head-to-head requires running each system through the exact same harness — see `docs/BENCHMARK-IMPROVEMENT-PLAN.md`.

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

This ingests all sessions using real embeddings (nomic-embed-text-v1.5, first run downloads ~130MB ONNX model), searches with hybrid BM25+semantic, and reports retrieval metrics:

- **Recall@5/10**: Does the evidence passage appear in top K results?
- **MRR**: Mean Reciprocal Rank of first evidence match

Output: `benchmarks/locomo/retrieval_results.json`

Expect ~5-15 minutes depending on hardware (embedding 10 conversations × ~27 sessions each).

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
├── README.md               # This file
└── data/
    └── locomo10.json       # Dataset (not committed, download above)
```

## Methodology notes

- **Session-level chunking**: Each conversation session is ingested as one memory, matching the standard approach used by Mem0, Zep, and Memobase benchmarks. Evaluating turn-level chunking is on the roadmap (see `docs/BENCHMARK-IMPROVEMENT-PLAN.md`).
- **Evidence matching**: LoCoMo QAs include `evidence` fields referencing specific dialog IDs. We check if the session containing those IDs appears in top-K search results.
- **No adversarial questions**: Category 5 (adversarial) is skipped, consistent with published benchmarks.
- **Independent storage per conversation**: Each conversation gets its own database, preventing cross-conversation leakage.
- **Reranker honesty**: The Stage 2 reranker model is only loaded once and shared across all conversations; the model itself is the same Jina v2 used in production via `tools/search_unified`. Disable it via `LOCOMO_USE_RERANKER=0` to measure how much it contributes vs raw hybrid retrieval.
- **Anti-self-deception**: Each `locomo_scores.json` records the full pipeline config (`pipeline.*`, `top_k_contexts`, `total_char_budget`, `judge_model`, `sample_size`, `sample_seed`). Always compare runs with the same config; otherwise the delta is methodology, not improvement.
- **Determinism warning**: `gpt-4o-mini` with `temperature=0` is **not** bitwise reproducible. Re-running the old code + old retrieval on seed=42 today yields ~21% rather than the historical 41% — same code, same data. When you publish a number, either (a) record the exact `gpt-4o-mini-YYYY-MM-DD` snapshot, (b) re-baseline before claiming a delta, or (c) hold the model fixed across all arms of the comparison and report A/B deltas, not absolutes.

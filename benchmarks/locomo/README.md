# LoCoMo Benchmark for Vestige

Measures Vestige's long-term conversational memory against the [LoCoMo benchmark](https://github.com/snap-research/LoCoMo) (Maharana et al., ACL 2024).

10 conversations, ~300 turns each across ~27 sessions, 1,540 QA pairs in 4 categories: single-hop, temporal, multi-hop, open-domain.

## Competitor scores (LLM Judge Overall)

| System     | Score  |
|------------|--------|
| Memobase   | 75.78% |
| Zep        | 75.14% |
| Mem0-Graph | 68.44% |
| Mem0       | 66.88% |
| LangMem    | 58.10% |
| OpenAI     | 52.90% |

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

Environment variables for `evaluate.py`:

| Variable | Default | Description |
|----------|---------|-------------|
| `LOCOMO_ANSWER_MODEL` | `gpt-4o-mini` | Model for generating answers from context |
| `LOCOMO_JUDGE_MODEL` | `gpt-4o` | Model for judging correctness |
| `LOCOMO_MAX_WORKERS` | `8` | Parallel API calls |

## How it works

### Phase 1 (Rust)

For each of 10 conversations:
1. Parse sessions from `locomo10.json`
2. Create a temporary Vestige Storage (SQLite + embeddings + HNSW index)
3. Ingest each session as a memory with content preprocessing
4. For each QA pair: run `hybrid_search`, check if evidence `dia_id`s appear in top-K results
5. Compute Recall@K and MRR

### Phase 2 (Python)

For each QA pair:
1. Take top-5 retrieved contexts from Phase 1
2. Ask GPT-4o-mini to answer the question given those contexts
3. Ask GPT-4o to judge: does the answer match ground truth? (binary: correct/incorrect)
4. Aggregate into per-category and overall LLM Judge Scores

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

- **Session-level chunking**: Each conversation session is ingested as one memory, matching the standard approach used by Mem0, Zep, and Memobase benchmarks.
- **Evidence matching**: LoCoMo QAs include `evidence` fields referencing specific dialog IDs. We check if the session containing those IDs appears in top-K search results.
- **No adversarial questions**: Category 5 (adversarial) is skipped, consistent with published benchmarks.
- **Independent storage per conversation**: Each conversation gets its own database, preventing cross-conversation leakage.

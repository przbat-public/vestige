# Measured results in this tree

Every number here was produced by this tree's harness against the release binary
built from the same commit. Read [`PORTING-NOTES.md`](PORTING-NOTES.md) first — the
deviations from the upstream harness are what make these numbers *internally*
comparable and *not* comparable to the paper's tables.

## Run 1 — retrieval levers A/B (2026-09-19)

Question: does the opt-in MMR diversity stage (`VESTIGE_MMR=on`) change retrieval
quality on conflict questions? And can the ColBERT late-interaction reranker be
measured at all?

```sh
cargo build --release -p vestige-mcp
python3 benchmarks/memconflict/fetch_dataset.py

# A — levers off (the shipped default)
python3 benchmarks/memconflict/run.py --instances 2 --sessions 25 --top-k 5 --warmup 10 \
  --data-dir /tmp/memconflict-ab/datadir-baseline --out results/ab-mmr-off-20260919.json

# B — identical run with VESTIGE_MMR=on
VESTIGE_MMR=on python3 benchmarks/memconflict/run.py --instances 2 --sessions 25 --top-k 5 --warmup 10 \
  --data-dir /tmp/memconflict-ab/datadir-mmr --out results/ab-mmr-on-20260919.json
```

2 simulated users, 25 sessions each, 98 questions, K=5, `recall`→`search` in
`balanced` mode. Raw output: `results/ab-mmr-off-20260919.json`,
`results/ab-mmr-on-20260919.json`.

| arm | n | macro AA | micro AA | dyn AA | UOCS | CRS-lex | static AA | cond AA | retrieved |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `nomem` | 98 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0 |
| `random` | 98 | 0.0322 | 0.0867 | 0.0966 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 687 |
| `bm25` | 98 | 0.4881 | 0.2704 | 0.2500 | 0.1932 | 0.2143 | 0.0000 | 1.0000 | 736 |
| `vestige` (A) | 98 | 0.5766 | 0.3316 | 0.3011 | 0.3295 | 0.4286 | 0.0000 | 1.0000 | 798 |
| `vestige` + MMR (B) | 98 | 0.5766 | 0.3316 | 0.3011 | 0.3295 | 0.4286 | 0.0000 | 1.0000 | 800 |

`vestige` beats BM25 by **+8.85 pp macro AA** / **+6.12 pp micro AA** in both runs.
CRS-struct reads 0.0000 in every arm because the harness has no
`recall(mode="contradictions")` to drive it (see PORTING-NOTES §3).

### What the MMR arm actually shows

A and B are **identical on every metric**, and that is a property of the wiring, not
of the data: `search_unified/pipeline/retrieval.rs` truncates the candidate list to
`config.limit` first (lines ~125/130) and only then runs the MMR stage (line ~180),
which calls `mmr_select(..., keep = scored.len())` — it permutes the survivors and
drops none. The returned *set* is therefore unchanged, and MemConflict's judge scores
the set (it has no position term), so the only visible difference is run-to-run noise
in `n_retrieved` (798 vs 800; the harness documents that the vestige arm is not
bit-identical across runs).

Consequences for the decision:

- **MMR as wired cannot be measured by this harness, and cannot deliver its stated
  benefit** ("keep a diverse set inside the context budget") because it never chooses
  *which* memories survive the cut. To test it, the stage has to run over a larger
  candidate pool *before* the truncation (MMR-select K from top-N, N > K) — then this
  same A/B becomes meaningful.
- **ColBERT is not measurable in this tree at all**: the reranker needs
  `VESTIGE_LATE_INTERACTION=1` plus a local ONNX model in `VESTIGE_COLBERT_MODEL_DIR`,
  and the model cache on this machine holds only `nomic-embed-text-v1.5` and the Jina
  cross-encoder. It is inert in every run above.

Both levers are already off by default (`VESTIGE_MMR` unset, `VESTIGE_LATE_INTERACTION`
unset), so nothing here changes the shipped behaviour — this run is the baseline the
next change has to beat.

### Caveats that apply to every number above

- Absolute values are not comparable to MemConflict's published table: different
  judge, different reader, subset of instances (see PORTING-NOTES).
- `vestige` is not bit-reproducible (FSRS state, timestamps, background
  consolidation); only arm-to-arm differences within one run are meaningful.
- Macro and micro AA disagree because macro weights a small conflict type like a
  large one. Report both or neither.
- Single run, no significance testing.

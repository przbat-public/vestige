# MemConflict benchmark harness

Retrieval benchmark for Vestige against
[MemConflict](https://arxiv.org/abs/2605.20926) (arXiv:2605.20926), with
mandatory no-memory, random, and BM25 controls.

**Read [`PORTING-NOTES.md`](PORTING-NOTES.md) before quoting any number from
this harness.** It lists every deviation from the upstream harness this was
ported from, and why absolute values here are not comparable to the paper's
published tables.

> Upstream's README pointed at `docs/BENCHMARKS.md`. That document is **not
> present in this working tree** (it exists only on `upstream/main`), so it is
> not linked here. Until it is ported, `PORTING-NOTES.md` is the only
> methodology document for this harness.

## Quick start

```sh
cargo build --release -p vestige-mcp          # or set VESTIGE_MCP_BINARY
python3 benchmarks/memconflict/fetch_dataset.py
python3 benchmarks/memconflict/run.py --instances 1 --sessions 7 --top-k 5 --warmup 10
```

No `pip install`. The harness is standard library only, by design: a benchmark
that needs a dependency resolver is a benchmark that stops reproducing.

Full runs use `--sessions 25` (the floor for covering all three conflict types);
see [`PORTING-NOTES.md`](PORTING-NOTES.md) for measured runtimes.

## Files

| File | Purpose |
| --- | --- |
| `DATASET.lock.json` | Pinned upstream revision + SHA-256 of every data file. |
| `fetch_dataset.py` | Downloads the pinned data and verifies hashes. Hard-fails on mismatch. |
| `judge.py` | Faithful port of upstream `Evaluation/eval_scoring.py` rule-based judge. No LLM. |
| `bm25.py` | Okapi BM25 control. Pure stdlib. |
| `mcp_client.py` | JSON-RPC-over-stdio client for `vestige-mcp`. **Ported**: spawns the server with `--data-dir <dir>/vestige.db` on private ports. |
| `run.py` | Orchestrator. Runs every arm, writes results JSON. **Ported** to `search`/`smart_ingest`/`deep_reference`. |
| `PORTING-NOTES.md` | Every deviation from upstream, the smoke command, runtimes, known limitations. |
| `LONGMEMEVAL.lock.json` | Pinned LongMemEval_S (cleaned) revision + hash. |
| `longmemeval.py` | LongMemEval_S **sanity check only** — see below. **Ported**. |
| `results/` | Timestamped run outputs. |

## The four arms

All four run every time. None can be silently dropped.

- **`nomem`** — no retrieval. The floor.
- **`random`** — K random memories, seeded. The blob-inflation control: the
  judge awards partial credit for token overlap, so any K memories score above
  zero by chance. Beat this or you are reporting corpus statistics.
- **`bm25`** — Okapi BM25 on the identical corpus. The earned-complexity bar.
- **`vestige`** — live `search()` over MCP, against one fresh database + server
  process per simulated user.

Same corpus, same reader, same judge, same K for every arm. One variable
changes: retrieval.

## Useful flags

```
--instances N        simulated users to evaluate (default 1)
--sessions N         max sessions ingested per user (default 10)
--top-k N            memories retrieved per question, identical for all arms (default 5)
--retrieval-mode     precise | balanced (default) | exhaustive
--detail-level       brief | summary (default) | full   (brief returns no content — scores ~0)
--min-similarity     search() semantic floor (default 0.0 here; server default is 0.5)
--min-retention      search() retention floor (default 0.0)
--warmup SECONDS     warmup after each server start (default 45)
--arms a,b,c         subset of arms; use for fast offline iteration
--seed N             seed for the random control (default 1234)
--server-binary P    vestige-mcp path (default $VESTIGE_MCP_BINARY or target/release/vestige-mcp)
--data-dir D         base dir for per-user stores (default results/datadir-<stamp>/)
--keep-databases     keep the per-user stores instead of deleting them
--out PATH           results JSON path
```

Static-conflict questions (the ones that exercise CRS) first appear around
session 16-24 depending on the instance. `--sessions 25` is the practical
floor for covering all three conflict types.

Fast offline iteration, no server needed (sub-second):

```sh
python3 benchmarks/memconflict/run.py --arms nomem,random,bm25 --instances 1 --sessions 25
```

## Gotchas

- **`--sessions 5` asks zero questions.** On instance 1 the first questions
  appear in session index 5, so a 5-session run only exercises ingest. Use
  `--sessions 7` for a scoring smoke run (dynamic conflicts only) and
  `--sessions 25` for all three conflict types.
- **One server process per simulated user.** The current tool surface has no
  `scope`/namespace argument, so users are isolated by giving each one an empty
  store (`--data-dir <dir>/vestige.db`). Each user therefore pays a server
  start plus the warmup window.
- **Warmup semantics changed.** Embeddings are initialised synchronously before
  the stdio transport opens in the current server, so the warmup is now a
  reranker/settling margin rather than an embedding requirement. `--warmup 10`
  is fine on a warm model cache; keep it large (300) the first time you run the
  server on a machine with a cold cache, since the cross-encoder downloads
  ~150 MB in a background task.
- **stdin must stay open** for the server's lifetime; closing it ends the run.
- Run size is bounded by ingest throughput (measured on this machine: ~11-12
  memory units/s over MCP, so ~3-5 min per user at 25 sessions).
- **The vestige arm can return fewer than K results.** `search()` runs an
  8-stage pipeline with read-path gating and competition suppression; a
  question BM25 answers may come back shorter or empty. `n_retrieved_mean` and
  the per-question `extra.gated` field record this.
- **The upstream `results/*.json` files are historical.** They were produced by
  the pre-port harness against a Vestige v2.x server (`recall`, `scope`,
  `VESTIGE_DATA_DIR`) and are not reproducible from this tree. They are kept
  only as upstream provenance.

## LongMemEval_S sanity check

```sh
python3 benchmarks/memconflict/longmemeval.py --questions 5
```

**Not a headline benchmark and never quotable as a LongMemEval score.** It runs
the same four arms on an independent dataset and reports one thing:
`evidence_recall@k` — does the retrieved text contain the gold answer string?
It exists to catch a silently broken harness, not to score the product.

It pins `longmemeval-cleaned`. The original `xiaowu0162/longmemeval` dataset is
**deprecated upstream** (it contains noisy history sessions that interfere with
answer correctness), so any number computed against the original is invalid.

Note the download is ~277 MB and the whole file is parsed regardless of
`--questions`, so this check is expensive; it is not part of the smoke test.

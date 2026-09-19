# PORTING-NOTES — MemConflict harness, upstream → current Vestige tool surface

This directory is a port of the `benchmarks/memconflict/` harness that lives on
`upstream/main`. The benchmark itself (dataset, judge, reader, arms) is
unchanged; everything that touches the MCP server was rewritten because the
server's tool surface changed between the version the upstream harness was
written against (Vestige v2.x: `recall`, `scope`, `VESTIGE_DATA_DIR`) and the
current one (Vestige v3.4.0: `search`, `smart_ingest`, `deep_reference`, no
namespaces).

Nothing outside `benchmarks/memconflict/` was modified. No Rust source was
touched. Python 3 standard library only — no new dependencies.

---

## 1. What was ported verbatim

| File | Status |
| --- | --- |
| `judge.py` | byte-identical to upstream (rule-based judge, no LLM) |
| `bm25.py` | byte-identical (Okapi BM25 control) |
| `fetch_dataset.py` | byte-identical (already fetched + SHA-256 verified, see §6) |
| `DATASET.lock.json`, `LONGMEMEVAL.lock.json` | byte-identical |
| `results/*.json` | byte-identical upstream artifacts, **historical only** (see §7) |
| `run.py` | arms, reader, aggregation, console report, judge wiring unchanged; MCP-facing parts rewritten |
| `longmemeval.py` | same treatment as `run.py` |
| `mcp_client.py` | MCP-facing parts rewritten |

## 2. Tool-surface mapping (verified against the working tree)

Verified before coding against
`crates/vestige-mcp/src/server/catalog.rs` (`build_tools_list`),
`crates/vestige-mcp/src/tools/search_unified/{schema,args,execute}.rs`,
`crates/vestige-mcp/src/tools/search_unified/pipeline/{retrieval,finalize}.rs`,
`crates/vestige-mcp/src/tools/smart_ingest/{schema,args}.rs` and
`crates/vestige-mcp/src/tools/cross_reference.rs`.

| Upstream (v2.x) | Ported (v3.4.0) | Notes |
| --- | --- | --- |
| `recall {query, mode: lookup\|reason, limit, scope}` | `search {query, limit, min_retention, min_similarity, detail_level, retrieval_mode}` | `lookup`/`reason` have no equivalent; `--retrieval-mode precise\|balanced\|exhaustive` replaces `--recall-mode` (default `balanced`). |
| `recall` result → `results[].content` | `search` result → `results[].content` | Same key; `_texts_from_search` reads `results` first. |
| `smart_ingest {items[], scope, batchMergePolicy: force_create}` | `smart_ingest {items[{content, node_type, forceCreate: true}]}` | Batch cap is still 20. `forceCreate` is the analogue of `batchMergePolicy: force_create`. `scope` is gone (unknown fields are ignored by serde, so passing it would silently do nothing — it is not passed). |
| `recall {mode: contradictions, topic, limit}` → `contradictionsFound`, `memoriesAnalyzed` | `deep_reference {query, depth}` → `len(contradictions)`, `len(evidence)` | See §3. |
| `VESTIGE_DATA_DIR=<dir>` env var | `--data-dir <DB FILE PATH>` CLI flag | See §4. |
| `VESTIGE_DASHBOARD_ENABLED=false`, `VESTIGE_HTTP_ENABLED=0` | not supported | See §5. |
| server binary `target/debug/vestige-mcp` | `target/release/vestige-mcp` | `--server-binary` (alias `--binary`) or `$VESTIGE_MCP_BINARY`. |

## 3. `crs_struct`: contradictions API → `deep_reference`

Upstream's CRS-struct probe called `recall(mode="contradictions")`, which does
not exist on the current surface. The probe now calls:

```json
{"name": "deep_reference", "arguments": {"query": "<question>", "depth": 50}}
```

and scores `1.0` when the response's `contradictions` array is non-empty.
`deep_reference` retrieves up to `depth` memories for the query and runs
pairwise relation assessment (supports / supersedes / contradicts), so this is
still a **structural** signal — the harness never keyword-matches the response.

Deviations from upstream, stated plainly:

* The search space is the tool's top-`depth` (≤50) retrieval for that question,
  not a system-wide scan of up to 200 memories. Recall is therefore structurally
  lower than upstream's probe; a `0.0` here is weaker evidence than upstream's.
* Because users are isolated by database (§4), the probe now *is* isolated —
  upstream's was not scope-isolated and could see other simulated users. This
  removes one upstream caveat and changes the metric's meaning slightly.
* It remains vestige-only: `bm25`/`random`/`nomem` have no contradiction channel,
  so `crs_struct` is a capability report, never a head-to-head win.

`--no-contradiction-probe` still disables it; `--contradiction-limit` was
replaced by `--contradiction-depth` (default 50, the tool's maximum).

## 4. Isolation: `scope` → one fresh database + server process per user

**What the server actually does** (this is the part that had to be discovered
from source, because the CLI flag is misleadingly named):

* `crates/vestige-mcp/src/main.rs` parses `--data-dir <PATH>` into
  `Config::data_dir` (line ~103) and calls `Storage::new(config.data_dir)`
  (line ~199).
* `crates/vestige-core/src/storage/sqlite/init.rs::Storage::new` treats
  `Some(p)` as the **database file path** — `Connection::open(&path)` — and only
  joins `vestige.db` onto a directory when the argument is `None`
  (the default `~/.local/share/…`/`~/Library/Application Support/com.vestige.core`
  ProjectDirs location).
* There is **no** `VESTIGE_DATA_DIR` environment variable anywhere in the
  current server (`grep -rn 'VESTIGE_DATA_DIR' crates/` → no hits), and no
  runtime "clear the store" affordance.
* The HNSW sidecars are derived from that path with `Path::with_extension`
  (`path_with_suffix`), so they land next to the database file, not in a shared
  cache.

**What the harness therefore does**, per simulated user:

1. creates `<data_root>/instance-<id>/` (`--data-dir` base, default
   `results/datadir-<stamp>/`);
2. spawns `vestige-mcp --data-dir <data_root>/instance-<id>/vestige.db`
   (the client appends `vestige.db`, because the flag takes a *file* path);
3. runs `initialize` → `notifications/initialized` → the warmup window →
   ingest → all questions for that user;
4. terminates the server, and (unless `--keep-databases`) deletes the store, so
   every user starts from an empty store and users cannot bleed into each other.

Cost: one process start + one warmup window per simulated user. That is the
price of losing namespaces, and it is recorded per instance in the results file
under `diagnostics.vestige_instance_warmups`.

The contradiction probe is *more* isolated than upstream's as a result (§3).

## 5. Other deviations

* **Private ports.** The current server unconditionally starts the dashboard
  (default 3927) and the HTTP transport (default 3928) alongside stdio. The
  client hands each child its own free loopback ports via
  `VESTIGE_HTTP_PORT` / `VESTIGE_DASHBOARD_PORT` so a benchmark run cannot
  collide with a developer's running instance.
* **Throwaway auth token.** `VESTIGE_AUTH_TOKEN` is set to a dummy value so the
  HTTP transport never reads or creates the shared `auth_token` file in the
  global Vestige data directory. The harness therefore writes **nothing** to the
  global data directory.
* **Warmup semantics.** Upstream's 45 s warmup existed because its server
  initialised embeddings *asynchronously after* `initialize`. In the current
  server `Storage::init_embeddings()` runs synchronously in `main()` before the
  stdio transport is created, so embeddings are provably ready when
  `initialize` answers. What is still asynchronous is the cross-encoder
  reranker (background task, ~150 MB download on a cold cache). The warmup is
  kept — now as a reranker/settling margin — and both signals
  (`embedding_ready_signal`, `reranker_signal`) are recorded per instance.
* **Search defaults chosen by the harness.** `min_similarity=0.0` (the server
  default is `0.5`, which silently drops keyword-only hits and has no
  counterpart in the BM25 control), `min_retention=0.0`, `detail_level=summary`
  (returns `content`; `brief` returns no content at all, hence the warning in
  `--help`), `retrieval_mode=balanced`. All four are flags.
* **Per-question `extra`.** Records now carry `extra.total` (result count before
  truncation) and `extra.gated` when the read-path gate skipped retrieval for a
  trivial query. Retrieval is gated only for ≤3-word trivial phrases
  (`is_trivial_query`), so benchmark questions should never trip it — if any do,
  they are visible instead of silently scoring zero.
* **`docs/BENCHMARKS.md` is not in this tree** (it exists only on
  `upstream/main`). The README's link to it was replaced with a pointer to this
  file; porting that document was out of scope (files outside
  `benchmarks/memconflict/` must not be touched).
* **`--binary` renamed** to `--server-binary` (the old name still works as an
  alias because scripts and the docs mention it).

## 6. Dataset

Fetched and verified with the ported script:

```sh
python3 benchmarks/memconflict/fetch_dataset.py
# downloading https://raw.githubusercontent.com/TaoZhen1110/MemConflict/<rev>/Data/Step4_4.jsonl
# OK  Step4_4.jsonl  sha256=8ef9ec8589eccb86...  bytes=39671712
```

30 instances / 3750 questions, exactly as pinned in `DATASET.lock.json`.
`data/` is gitignored.

## 7. Known limitations

1. **Upstream `results/*.json` are not reproducible from this tree.** Their
   `server_stderr_tail` records `VESTIGE_DASHBOARD_ENABLED=false`,
   `VESTIGE_HTTP_ENABLED=0` and an autopilot — a Vestige v2.x server that no
   longer exists. They are kept as provenance only. Do not compare numbers
   across them and fresh runs.
2. **`crs_struct` is a weaker signal than upstream's** (§3).
3. **`balanced` mode can return fewer than `top_k` results**, and the reader then
   hands the judge a shorter blob than the controls get. `n_retrieved_mean` and
   `reader_chars_mean` are printed for exactly this reason; the console prints a
   confound warning at ≥1.25× blob-size ratio.
4. **Preprocessing may rewrite ingested text** (coreference rewriting, temporal
   anchoring), so vestige's retrieved content is not always byte-identical to
   the unit the BM25/random arms index. Ingest uses `forceCreate: true`, so
   nothing is merged away.
5. **Per-user server restarts make long runs slower** than upstream (one start
   + warmup per simulated user, ~10-15 s each with a warm model cache).
6. **The judge is rule-based**, so absolute answer-accuracy numbers are not
   comparable to the paper's LLM-judged Table 3. Only cross-arm deltas within a
   single run mean anything.
7. **LongMemEval_S check not exercised here**: it downloads ~277 MB and parses
   the whole file regardless of `--questions`. It was ported the same way as
   `run.py` (per-question fresh store, `search` instead of `recall`) but has not
   been smoke-tested.

## 8. Smoke command and expected runtime

Dataset fetch (one-off, ~40 MB, SHA-256 verified): seconds to a minute.

Offline arms only (no server, no API key, no model cache):

```sh
python3 benchmarks/memconflict/run.py --arms nomem,random,bm25 --instances 1 --sessions 25 --top-k 5
# measured: 0.4 s wall, 52 questions, 1109 memory units
```

Full smoke (all four arms, needs `target/release/vestige-mcp`):

```sh
python3 benchmarks/memconflict/run.py --instances 1 --sessions 7 --top-k 5 --warmup 10
# measured: 40 s wall (server start ~4 s + 10 s warmup + 303 units ingested +
#           9 questions scored across 4 arms); 0 ingest errors, 0 retrieve errors
```

Run that also exercises the static-conflict path (and therefore the
`deep_reference` CRS-struct probe, which replaces `recall(mode="contradictions")`):

```sh
python3 benchmarks/memconflict/run.py --instances 1 --sessions 17 --top-k 5 --warmup 10
# measured: 67 s wall, 755 units, 29 questions (28 dynamic + 1 static);
#           0 ingest errors, 0 retrieve errors, every question returned 5/5 results,
#           probe returned {"found": 0, "analyzed": 50} for the static question
```

> `--sessions 5` — the example in the task brief — scores **zero questions** on
> instance 1: its first questions appear in session index 5. It is a pure
> ingest/plumbing check. Use `--sessions 7` for a scoring smoke run, and
> `--sessions 25` to reach static (index ≥16) and conditional (index ≥22)
> conflicts.

Extrapolating to a full run: ingest throughput measured here is ~11-12 memory
units/s over MCP (755 units in ~65 s of ingest), i.e. roughly 3-5 min per
simulated user at 25 sessions (~1100 units) plus search time per question
(mean ~0.8 s), plus one warmup per user. A 30-instance / 25-session run is
therefore a few hours, not minutes. Re-measure before scheduling it.

## 9. What was actually smoke-tested (and what was not)

Verified in this tree:

| Check | Result |
| --- | --- |
| `ast.parse` on `run.py`, `mcp_client.py`, `bm25.py`, `judge.py`, `fetch_dataset.py`, `longmemeval.py` | all OK |
| `run.py --help`, `longmemeval.py --help` | both render, all flags documented |
| `nomem` / `random` / `bm25` arms, 1 instance × 25 sessions | 0.4 s, 52 questions, no server needed |
| Full 4-arm run, 1 instance × 7 sessions, `--warmup 10` | 40 s, 9 questions, 0 errors |
| Full 4-arm run, 1 instance × 17 sessions, `--warmup 10` | 67 s, 29 questions incl. 1 static, `deep_reference` probe executed, 0 errors |
| Per-user store cleanup | base dir removed after the run; `results/` left with only the upstream artifacts |

Not verified (deliberately, to stay under the smoke-run budget): `longmemeval.py`
end-to-end (277 MB download + full-file parse), multi-instance runs, and runs
with `--keep-databases`.

Everything the smoke runs produced went to `/tmp`; no run artifacts were left in
the repository.

## 10. Decisions for the human

1. **`--warmup` default is still 45 s** (upstream's value), applied per simulated
   user now. For a 30-instance run that is ~20 min of pure warmup. Lower it
   (10 s is enough on a warm model cache) or make it apply only to the first
   instance — say which you prefer and it can be changed.
2. **`crs_struct`**: keep the `deep_reference`-based probe, or drop the metric
   entirely since it has no analogue of upstream's system-wide contradictions
   API?
3. **Blob-size parity**: `search()` may return fewer/larger units than BM25.
   Options are to leave it (honest, with the confound warning) or to force
   `--top-k` parity by padding — the latter would be dishonest and was not done.
4. **`docs/BENCHMARKS.md`**: it exists upstream and the README used to require
   reading it. Porting it needs a write outside `benchmarks/memconflict/`, which
   this task forbade.

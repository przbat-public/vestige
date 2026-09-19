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

<!-- wygenerowane przez: python3 benchmarks/memconflict/run.py --render-table benchmarks/memconflict/results/ab-mmr-off-20260919.json,benchmarks/memconflict/results/ab-mmr-on-20260919.json --markdown-table /tmp/results-run1.md -- nie edytuj tabeli ręcznie -->

| run | arm | n | macroAA | microAA | dynAA | UOCS | statAA | CRSlex | condAA | chars |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ab-mmr-off-20260919 | `nomem` | 98 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0.0000* | 0.0000* | 0.0000* | 0 |
| ab-mmr-off-20260919 | `random` | 98 | 0.0322 | 0.0867 | 0.0966 | 0.0000 | 0.0000* | 0.0000* | 0.0000* | 687 |
| ab-mmr-off-20260919 | `bm25` | 98 | 0.4881 | 0.2704 | 0.2500 | 0.1932 | 0.2143* | 0.0000* | 1.0000* | 736 |
| ab-mmr-off-20260919 | `vestige` | 98 | 0.5766 | 0.3316 | 0.3011 | 0.3295 | 0.4286* | 0.0000* | 1.0000* | 799 |
| ab-mmr-on-20260919 | `nomem` | 98 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0.0000* | 0.0000* | 0.0000* | 0 |
| ab-mmr-on-20260919 | `random` | 98 | 0.0322 | 0.0867 | 0.0966 | 0.0000 | 0.0000* | 0.0000* | 0.0000* | 687 |
| ab-mmr-on-20260919 | `bm25` | 98 | 0.4881 | 0.2704 | 0.2500 | 0.1932 | 0.2143* | 0.0000* | 1.0000* | 736 |
| ab-mmr-on-20260919 | `vestige` | 98 | 0.5766 | 0.3316 | 0.3011 | 0.3295 | 0.4286* | 0.0000* | 1.0000* | 800 |

\* reported on fewer than 10 questions (`--min-reportable-n`): treat as noise, not signal (nomem:statAA n=7, nomem:CRSlex n=7, nomem:condAA n=3, random:statAA n=7, random:CRSlex n=7, random:condAA n=3, bm25:statAA n=7, bm25:CRSlex n=7, bm25:condAA n=3, vestige:statAA n=7, vestige:CRSlex n=7, vestige:condAA n=3).

The table above is a byte-for-byte paste of what that command writes to
`/tmp/results-run1.md`; it is **generated, never transcribed by hand**. Column order
and names come from `TABLE_COLUMNS` in `run.py`, where each label is pinned to exactly
one JSON key path and `validate_column_contract()` / `validate_summary()` refuse to
render a table whose labels and keys disagree — the 2026-09-19 hand-written version of
this table had `statAA` and `CRSlex` swapped, which is now a hard error instead of a
publication bug. The `run` column names the artifact each row was read from; `vestige`
in row 4 is run A (MMR unset) and in row 8 run B (`VESTIGE_MMR=on`). The last column is
`reader_chars_mean`, the blob-inflation confound the harness warns about, not a
retrieval count. `vestige` beats BM25 by **+8.85 pp macro AA** / **+6.12 pp micro AA**
in both runs, while handing the judge 799 (run A) / 800 (run B) chars/question against
BM25's 736 (1.09×, under the harness's 1.25× warning threshold — the comparison is not
a blob artefact).
Whether that gap is *signal* is a separate question, answered in "Statistical
significance" below — and the answer is not favourable.

`CRSlex` is 0.0000 in every arm, and `crs_struct` is 0.0000 for `vestige`: the
harness *did* run the contradiction probe — 7 questions, 50 evidence entries per probe
(the probe's `analyzed` field is `len(evidence)` in the `deep_reference` response, not
a count of distinct memories), **0 pairs found** (`results/ab-mmr-off-20260919.json`;
`contradiction_probe` is present in all 7 static-conflict records of the `vestige` arm,
since the probe exists only there). So
this is not a missing channel in the harness, it is a measured product result: on
static-conflict questions `deep_reference` surfaces no contradiction pairs at all, even
though the same arm answers 42.86% of those questions correctly. Closing that gap is
tracked in `docs/review/`.

### Statistical significance (added 2026-09-19, after the review)

The harness now computes an optional exact two-sided McNemar test over the questions
both arms answered, on binarised `answer_accuracy` (≥ 0.5), and stores the p-value with
a per-instance (cluster) and per-conflict-type breakdown in the results JSON under
`significance`. For the committed run A:

```sh
python3 benchmarks/memconflict/run.py --render-table \
  benchmarks/memconflict/results/ab-mmr-off-20260919.json --mcnemar vestige:bm25
# vestige_vs_bm25: n=98 a_only=19 b_only=7 p=0.0290 (exact two-sided McNemar)
```

- **`vestige` vs `bm25`: n=98 pairs, 19 questions only vestige gets right, 7 only BM25,
  p = 0.0290.** Nominally significant at 0.05 — but read the two caveats below before
  repeating that sentence anywhere.
- **The effect sits in one of the two simulated users.** Split by cluster:
  instance `3c2e5fe5…` n=52 → 11 vs 7, p = 0.4807; instance `9841f645…` n=46 → 8 vs 0,
  p = 0.0078. McNemar assumes independent pairs; questions are nested in users, so the
  p-value is optimistic and the effective sample is ~2 clusters, not 98 questions.
- **No single conflict type is significant on its own.** Split by stratum:
  `dynamic_conflict` 16 vs 7, p = 0.0931; `static_conflict` 3 vs 0, p = 0.2500;
  `conditional_conflict` 0 vs 0, p = 1.0. The pooled p = 0.0290 comes from pooling the
  three strata, not from any one of them.
- **The paired delta is +12.24 pp, not the +6.12 pp micro-AA delta.** McNemar needs a
  pass/fail outcome, so the judge's 0.5 partial credit is binarised at ≥ 0.5: 62/98
  questions correct for `vestige`, 50/98 for `bm25`. Micro AA keeps the partial credit.
  Both numbers are true, they measure different things, and swapping one for the other
  would overstate the gap by a factor of two.
- **80.7% of the published `+8.85 pp` macro delta comes from the n=7 stratum.** Per-type
  deltas (vestige − bm25, run A): `dynamic_conflict` +0.0511 on n=88 → +0.0170 of the
  macro average; `static_conflict` +0.2143 on n=7 → +0.0714 (0.0714 / 0.0885 = 80.7%);
  `conditional_conflict` 0.0000 on n=3 → 0. The micro delta (+6.12 pp) is over all 98
  questions and does not concentrate this way, but it is an average of partial credit
  from a 0/0.5/1 judge.
- **The MMR A/B is a true null, by paired test:** run A and run B agree on all 98
  questions (0 discordant pairs, so p = 1 by construction). That is stronger evidence
  than "identical on every metric": the two runs are question-for-question identical.
- **No multiple-comparison correction.** Comparing several arms and metrics in one run
  inflates the chance of a p < 0.05 by chance.

Verdict for the decision this run was meant to support: *"vestige beats BM25 by
+8.85 pp macro AA"* should be reported as **not established** — one nominally
significant paired test over 98 clustered questions, four fifths of the macro delta
coming from a 7-question stratum the harness itself flags as small-n. A next run needs
more simulated users (clusters) and more static-conflict questions before this number
can carry a product claim.

### What the MMR arm actually shows

A and B are **identical on every metric**, and that is a property of the wiring, not
of the data: `search_unified/pipeline/retrieval.rs` truncates the candidate list to
`config.limit` first (lines ~125/130) and only then runs the MMR stage (line ~180),
which calls `mmr_select(..., keep = scored.len())` — it permutes the survivors and
drops none. The returned *set* is therefore unchanged, and MemConflict's judge scores
the set (it has no position term), so the only visible difference is run-to-run noise
in `n_retrieved` (798 vs 800; the harness documents that the vestige arm is not
bit-identical across runs).

The paired test agrees: the two runs' `vestige` arms have **0 discordant pairs out of
98** (p = 1 by construction), i.e. they are question-for-question identical, not merely
equal in the aggregate. That is the strongest form of "neutral" this harness can
express, and it is a property of the wiring above rather than of MMR as an algorithm.
Separately: `VESTIGE_MMR` is read once into a `OnceLock` in
`search_unified/pipeline/retrieval.rs`, so no in-process test can toggle it — an env
var changed after the first search is silently inert.

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

## Run 2 — the retrieval levers after they were rewired (2026-09-19)

Run 1 measured MMR as a no-op, and the reason was structural: the pipeline truncated to
`limit` *before* the MMR stage, which then kept `scored.len()` items, so it could only
permute a set that had already been decided. Two changes followed — the cross-encoder score
now travels into `combined_score` (it used to be dropped, leaving the pre-rerank hybrid
order as the ranking), and MMR now selects `final_limit` from a reranked pool four times
that size.

```sh
cargo build --release -p vestige-mcp
env -u VESTIGE_MMR python3 benchmarks/memconflict/run.py --instances 1 --sessions 25 --top-k 5 --warmup 10 --data-dir /tmp/mmr-ab2/d1 --out /tmp/mmr-ab2/off.json
VESTIGE_MMR=on     python3 benchmarks/memconflict/run.py --instances 1 --sessions 25 --top-k 5 --warmup 10 --data-dir /tmp/mmr-ab2/d2 --out /tmp/mmr-ab2/on.json
```

| arm | n | macroAA | microAA | dynAA | statAA | CRSlex | retrieved/q | reader chars/q |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `vestige`, MMR off | 52 | 0.6014 | 0.3462 | 0.3043 | 0.5000 | 0.0000 | 2.94 | 546 |
| `vestige`, MMR on | 52 | 0.6087 | 0.3654 | 0.3261 | 0.5000 | 0.0000 | 4.94 | 857 |

The lever is no longer inert: the arm now returns 4.94 memories per question instead of
2.94, so the pool change reaches the answer. The metric movement (+0.73 pp macro,
+1.92 pp micro) **cannot be attributed to diversity**, because the same change hands the
judge 57% more text and the harness's own confound check (`reader_chars_mean`, warned above
1.25×) flags exactly that. Separating the two needs a third arm that keeps the larger pool
but skips the MMR selection, so both arms return the same number of memories — until that
exists, treat the delta as unproven and keep the lever off by default.

### Caveats that apply to every number above

- Absolute values are not comparable to MemConflict's published table: different
  judge, different reader, subset of instances (see PORTING-NOTES).
- `vestige` is not bit-reproducible (FSRS state, timestamps, background
  consolidation); only arm-to-arm differences within one run are meaningful.
- Macro and micro AA disagree because macro weights a small conflict type like a
  large one. Report both or neither.
- Single run. Paired McNemar tests are reported only where requested (`--mcnemar`,
  default `vestige:bm25`): they assume independent question pairs, while questions are
  clustered inside simulated users, so every p is optimistic — always read it with the
  `significance.*.per_instance` breakdown. There are no confidence intervals, no
  cluster bootstrap and no multiple-comparison correction.
- `statAA`/`CRSlex` (n=7) and `condAA` (n=3) carry a `*` from `--min-reportable-n 10`:
  they are printed for completeness, but any difference on them is noise, not a
  result. `statAA` is nevertheless where 80.7% of the published macro delta lives —
  see "Statistical significance".

## Run 3 — FactConsolidation: does the current fact win? (2026-09-19)

A different benchmark and a different question from Runs 1 and 2. MemoryAgentBench
FactConsolidation (`arXiv:2507.05257`) gives one haystack in which the same
`(subject, relation)` is stated more than once with different values, and asks about it.
Only the **last** statement is current, so the correct answer is the last statement's
value. This is the harness half of the review's "deterministic freshness resolution"
item; the protocol, its deviations and its limits are in
[`PORTING-NOTES.md`](PORTING-NOTES.md) §11. Nothing here is comparable to Runs 1–2, to
MemConflict, or to the paper's published table.

```sh
python3 benchmarks/memconflict/fetch_factconsolidation.py
cargo build --release -p vestige-mcp
python3 benchmarks/memconflict/factconsolidation.py \
  --source factconsolidation_sh_6k --scenarios-per-store 10 --ingest all --warmup 10 \
  --out benchmarks/memconflict/results/factconsolidation-sh6k-20260919.json
```

59 scenarios, each the question plus every haystack statement for its key (all are
2-statement conflicts), 381 statements ingested per store in document order, `--top-k 5`,
`balanced`, `summary`. Binary `target/release/vestige-mcp`, sha256
`37391d12da15dca9…`, built from a dirty tree at commit `6ca19748` (HEAD advanced during
the run; no tracked source file changed between the build and the run). Dataset digest
`24d5c3f09ce0ce15…`, `jsonl` digest `1368e3cdcb08487b…`.

| metric | n | value |
| --- | --- | --- |
| answer accuracy (official `substring_exact_match`) | 59 | 0.9831 |
| top-1 is the current value | 59 | 0.2542 |
| top-1 is the superseded value | 59 | 0.7458 |
| current value retrieved anywhere in top-5 | 59 | 0.9831 |
| superseded value retrieved anywhere in top-5 | 59 | 1.0000 |
| any statement of the conflict group retrieved | 59 | 1.0000 |
| empty retrieval | 59 | 0.0000 |

Reproduced three times at 0.9831 (58/59) with identical latency order of magnitude
(~178–226 s); the single failure was the same scenario every time.

**What this says about deterministic freshness resolution.** Retrieval gets the conflict
right and the ranking does not. In 58 of 59 scenarios both conflicting statements reach
the top-5, so the current value is available and the official metric scores the answer
correct — but the current value is the *first* result only 25.4% of the time, and in the
other 74.6% the superseded statement outranks it. Any consumer that reads one memory, or
that takes the first convincing statement, gets the stale fact in three quarters of these
conflicts. The measured bottleneck is therefore ordering, not recall.

A second, smaller finding: the one failure (`What type of music does John McVie play?`,
serial 90 vs 415) returned only the superseded statement — one result, `total: 1` — so the
current value was never in the context. That is a retrieval miss, not a freshness failure,
and the per-scenario JSON marks it as such (`retrieved_any_statement: true`,
`current_value_retrieved: false`).

### The same protocol against the pre-change binary

The identical command was run against the pre-2026-09-19 build (the `target/release`
binary as it stood at 12:02 local, saved before the rebuild):

```sh
python3 benchmarks/memconflict/factconsolidation.py \
  --source factconsolidation_sh_6k --scenarios-per-store 10 --ingest all --warmup 10 \
  --server-binary /tmp/vestige-mcp-stale-20260919 \
  --out benchmarks/memconflict/results/factconsolidation-sh6k-20260919-prechange-binary.json
```

It reproduces the headline numbers exactly: answer accuracy 0.9831 (58/59), top-1 is
current 0.2542, top-1 is superseded 0.7458, both statements retrieved 0.9831. Only the
count of memories returned per question differs, on 4 of 59 questions, without crossing a
scoring boundary.

**So today's freshness work did not move this benchmark.** The 0.9831/0.2542 split is a
property of the retrieval path as it already behaved, not a result of the new resolver.
Either the resolver is not on the `search` path yet, or it is on it and changes nothing
here; this harness cannot separate those two readings, and does not claim to. What it does
establish is the baseline any resolver has to beat and where the headroom is (top-1
staleness 0.7458 while 0.9831 of conflicts are fully retrieved). It also means that a claim
that "the new resolver improves conflict resolution" cannot be supported by this
measurement, because this measurement cannot see the difference.

**What this does not say.** The official metric is a substring check, so "correct" here
means "the current value appears somewhere in the retrieved text", not "the system chose
it". The 0.9831 must therefore never be quoted as a conflict-resolution score: read it
with the 0.2542 next to it. There is no confidence interval, no significance test, no
second system to compare against, and only the 6k single-hop haystack was run.

### Caveats that apply to Run 3

- **`--ingest all` only.** The `--ingest conflicts` mode in the same script is a plumbing
  check that removes every distractor; its numbers are not results and are not reported
  here.
- **Retrieval stands in for a context window.** The paper feeds an LLM the whole haystack;
  here `search` returns 5 memories (`n_retrieved_mean` 2.44). The numbers are a floor and
  are not comparable to `arXiv:2507.05257`'s table.
- **19.8% of the haystack's fact lines do not parse** into a `(subject, relation)` pair
  (381/455; 74 lines) and 41 of 100 questions are refused (15 unmapped templates, 26 with
  no conflicting statement). Coverage is reported in the results JSON under
  `scenario_coverage`; nothing is silently dropped.
- **`factconsolidation_mh_*` builds zero scenarios** (multi-hop questions need a hop chain
  the harness refuses to invent) and is not measured.
- **Single run family.** `vestige` is not bit-reproducible; only the per-scenario records
  are, and those were stable across three runs here.

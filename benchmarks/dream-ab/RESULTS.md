# dream-ab — measured results in this tree

Every number below was produced by [`run.py`](run.py) against the release binary built
from this tree. Read [`README.md`](README.md) first: it defines the arms, the verdict
rule, and what this harness refuses to conclude.

Both runs snapshotted the live store **read-only** and ran every arm on its own copy
under a temporary `HOME`. Binary: `target/release/vestige`, sha256
`6ab64aef27e9c89b4d435dfcbed06a9af984b177c45e3c18fb0803d0aca06e42`. That binary
contains two character-boundary fixes described under [Finding](#finding-dream-could-not-complete-at-all-before-this-run)
below; without them the `dream` arm cannot run at all.

**One thing moved under this file after it was written.** The quality payload has since
gained a `rates.*` object (`selfContainment`, `anchorResolvability`, `retrieval`),
computed in one place so no consumer picks its own denominator. The runs below were
taken against the payload without it, which is why the harness derives every rate from
the counts. That is not a defect in the record: the harness re-derives the same numbers
from the same counts, and cross-checks any rate a surface supplies. Re-verified against
the current binary on the unchanged store — `rates` reports `selfContainment: 1.0`,
`anchorResolvability: null`, `retrieval: 0.6153846153846154`, identical to the harness's
own `7/7`, `unmeasured` and `8/13`, with no cross-check finding. The counts in the
tables below are unaffected by that payload change; only the raw JSON artifacts are
older than the field.

```sh
cargo build --release -p vestige-mcp -p vestige-restore

python3 benchmarks/dream-ab/run.py --pass dream   --runs 3     # Run 1
python3 benchmarks/dream-ab/run.py --pass reflect --runs 3     # Run 2
```

Raw artifacts: `results/20260920T174559Z-dream.json`,
`results/20260920T174647Z-reflect.json` (plus `-payloads.json` with every raw pass
response and `-table.md`, the generated tables pasted below).

## The corpus both runs measured

One snapshot of the live store, taken with the SQLite backup API from a `mode=ro`
connection, then copied once per arm.

| fact | value |
| --- | --- |
| `knowledge_nodes` | 13 |
| `memory_access_log` rows | 13 |
| `code_refs` (anchors) | 0 |
| containment | clean 7, flagged 0, **unchecked 6** |
| anchors | fresh 0, stale 0, orphaned 0, unchecked 0 |
| use | `retrievedAtLeastOnce` 8/13, `everAccessed` 10/13 |
| live store rows before / after each run | 13 / 13 (unchanged; the live store was only ever read) |

Isolation probes passed in both runs (`contamination` is impossible to hide here —
the probe would have caught it):

| probe | expected | dream run | reflect run |
| --- | --- | --- | --- |
| report total == snapshot `knowledge_nodes` | 13 | 13 ✓ | 13 ✓ |
| perturbed copy (one row deleted) reports | 12 | 12 ✓ | 12 ✓ |
| canary ingest into a throw-away copy reports | 14 | 14 ✓ | 14 ✓ |

## Run 1 — `dream` (2026-09-20, 3 pairs)

```sh
python3 benchmarks/dream-ab/run.py --pass dream --runs 3
```

13 memories, 7 clean / 0 flagged / 6 unchecked, 0 anchors. Warmup 15 s before each
pass; the `dream` call itself took 0.2–0.6 s and reported `memoriesReplayed: 13`,
`connectionsPersisted: 12`, `insightsGenerated: 3`.

<!-- wygenerowane przez: python3 benchmarks/dream-ab/run.py --render-table benchmarks/dream-ab/results/20260920T174559Z-dream.json -- nie edytuj tabeli ręcznie -->

### Per-run values — pass: `dream`

| measure | run | control before | control after | treatment before | treatment after | Δ(treatment) − Δ(control) |
| --- | --- | --- | --- | --- | --- | --- |
| `containment.selfContainmentRate` | 1 | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | treatment 0, control 0 |
| `containment.clean` | 1 | 7/13 | 7/13 | 7/13 | 7/17 | treatment -0.1267, control 0 |
| `containment.flagged` | 1 | 0/13 | 0/13 | 0/13 | 0/17 | treatment 0, control 0 |
| `containment.unchecked` | 1 | 6/13 | 6/13 | 6/13 | 10/17 | treatment +0.1267, control 0 |
| `anchors.resolvabilityRate` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.fresh` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.stale` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.orphaned` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.unchecked` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `use.retrievalRate` | 1 | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | 0.765 (13/17) | treatment +0.1493, control 0 |
| `use.everAccessedRate` | 1 | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | 0.765 (13/17) | treatment -0.0045, control 0 |
| `containment.selfContainmentRate` | 2 | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | treatment 0, control 0 |
| `containment.clean` | 2 | 7/13 | 7/13 | 7/13 | 7/17 | treatment -0.1267, control 0 |
| `containment.flagged` | 2 | 0/13 | 0/13 | 0/13 | 0/17 | treatment 0, control 0 |
| `containment.unchecked` | 2 | 6/13 | 6/13 | 6/13 | 10/17 | treatment +0.1267, control 0 |
| `anchors.resolvabilityRate` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.fresh` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.stale` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.orphaned` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.unchecked` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `use.retrievalRate` | 2 | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | 0.765 (13/17) | treatment +0.1493, control 0 |
| `use.everAccessedRate` | 2 | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | 0.765 (13/17) | treatment -0.0045, control 0 |
| `containment.selfContainmentRate` | 3 | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | treatment 0, control 0 |
| `containment.clean` | 3 | 7/13 | 7/13 | 7/13 | 7/17 | treatment -0.1267, control 0 |
| `containment.flagged` | 3 | 0/13 | 0/13 | 0/13 | 0/17 | treatment 0, control 0 |
| `containment.unchecked` | 3 | 6/13 | 6/13 | 6/13 | 10/17 | treatment +0.1267, control 0 |
| `anchors.resolvabilityRate` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.fresh` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.stale` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.orphaned` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.unchecked` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `use.retrievalRate` | 3 | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | 0.765 (13/17) | treatment +0.1493, control 0 |
| `use.everAccessedRate` | 3 | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | 0.765 (13/17) | treatment -0.0045, control 0 |

### Verdict per measure (§12.2 rule 4: three runs, same direction)

| measure | counts as improvement | Δ run 1 | Δ run 2 | Δ run 3 | verdict |
| --- | --- | --- | --- | --- | --- |
| `containment.selfContainmentRate` | up | +0.0000 | +0.0000 | +0.0000 | **no effect** |
| `containment.clean` | up | -0.1267 | -0.1267 | -0.1267 | **worse** |
| `containment.flagged` | down | +0.0000 | +0.0000 | +0.0000 | **no effect** |
| `containment.unchecked` | down | +0.1267 | +0.1267 | +0.1267 | **worse** |
| `anchors.resolvabilityRate` | up | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.fresh` | up | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.stale` | down | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.orphaned` | down | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.unchecked` | down | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `use.retrievalRate` | up | +0.1493 | +0.1493 | +0.1493 | **improved** ⚠ confounded (see notes) |
| `use.everAccessedRate` | up | -0.0045 | -0.0045 | -0.0045 | **worse** ⚠ confounded (see notes) |

### Determinism control

- two control runs on two identical copies produced identical store measures
- control run 1 vs control run 2: identical
- control run 1 vs control run 3: identical
- control run 2 vs control run 3: identical

### Run facts (denominators and the pass's own activity)

| run | window total: control | window total: treatment | pass call | call seconds | access-log rows written during the pass (supplementary read) |
| --- | --- | --- | --- | --- | --- |
| 1 | no new rows | +4 rows | `dream` ok | 0.2 | search_hit: 13 rows on 13 nodes |
| 2 | no new rows | +4 rows | `dream` ok | 0.6 | search_hit: 13 rows on 13 nodes |
| 3 | no new rows | +4 rows | `dream` ok | 0.2 | search_hit: 13 rows on 13 nodes |

<!-- end of generated tables -->

### Reading the `dream` result

- **Self-containment rate: no effect.** 7/7 checked memories were clean before and
  after; the pass flagged nothing. `containment.flagged` stayed 0/13 → 0/17. The rate
  only counts `clean + flagged`, so it could not move here.
- **`containment.clean` (share of the window): worse, and `containment.unchecked`:
  worse — the same fact seen from two sides.** `dream` wrote 4 new rows
  (13 → 17). A fourth run kept for inspection
  (`python3 benchmarks/dream-ab/run.py --runs 1 --label inspect --keep-work --warmup 5`,
  query and full output in
  [`results/20260920T174559Z-dream-new-rows.txt`](results/20260920T174559Z-dream-new-rows.txt))
  shows those rows are **3 `insight` + 1 `hub`**, all with `self_contained IS NULL` —
  the ingest gate never runs on the pass's own writes. So `unchecked` rose
  6/13 = 0.462 → 10/17 = 0.588 and the clean share fell 7/13 = 0.538 → 7/17 = 0.412,
  entirely through the denominator and the new never-checked rows. Nothing that was
  clean became flagged.
- **`use.retrievalRate`: "improved" — and that verdict is the confound, not a
  result.** The numerator went 8 → 13, i.e. *every* memory in the store was retrieved
  after the pass. The supplementary read of `memory_access_log` inside the pass window
  accounts for exactly that: **13 `search_hit` rows on 13 distinct nodes**, written by
  `dream` replaying the store it is being measured on. The "improvement" is the pass
  reading its own corpus; no external consumer retrieved anything in either arm.
- **`use.everAccessedRate`: "worse" is a denominator artefact.** Numerator 10 → 13,
  denominator 13 → 17; the rate falls only because four new rows entered the
  denominator and nothing has accessed them yet. No memory lost an access.
- **Anchors: `unmeasured`, honestly.** The store has 0 `code_refs`, so
  resolvability has no denominator and the harness prints `unmeasured` rather than
  `0` or `100%`. `anchors.unchecked` is also 0 — this store never recorded an anchor
  at all, which is an environment fact (no `code_refs` were ever written), not a
  clean bill of health.
- **`process.*` (not comparable across arms).** Both the CLI reads and the
  post-pass `memory_health` read report `rejected: 0, flagged: 0`: the pass's writes
  do not go through the ingest gate at all, which is the same fact as their
  `self_contained IS NULL`. These counters describe the answering process, so the
  table above gives them no verdict.

## Run 2 — `reflect` (2026-09-20, 3 pairs)

```sh
python3 benchmarks/dream-ab/run.py --pass reflect --runs 3
```

Same 13-memory snapshot, same warmup and isolation probes.

<!-- wygenerowane przez: python3 benchmarks/dream-ab/run.py --render-table benchmarks/dream-ab/results/20260920T174647Z-reflect.json -- nie edytuj tabeli ręcznie -->

### Per-run values — pass: `reflect`

| measure | run | control before | control after | treatment before | treatment after | Δ(treatment) − Δ(control) |
| --- | --- | --- | --- | --- | --- | --- |
| `containment.selfContainmentRate` | 1 | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | treatment 0, control 0 |
| `containment.clean` | 1 | 7/13 | 7/13 | 7/13 | 7/13 | treatment 0, control 0 |
| `containment.flagged` | 1 | 0/13 | 0/13 | 0/13 | 0/13 | treatment 0, control 0 |
| `containment.unchecked` | 1 | 6/13 | 6/13 | 6/13 | 6/13 | treatment 0, control 0 |
| `anchors.resolvabilityRate` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.fresh` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.stale` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.orphaned` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.unchecked` | 1 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `use.retrievalRate` | 1 | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | treatment 0, control 0 |
| `use.everAccessedRate` | 1 | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | treatment 0, control 0 |
| `containment.selfContainmentRate` | 2 | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | treatment 0, control 0 |
| `containment.clean` | 2 | 7/13 | 7/13 | 7/13 | 7/13 | treatment 0, control 0 |
| `containment.flagged` | 2 | 0/13 | 0/13 | 0/13 | 0/13 | treatment 0, control 0 |
| `containment.unchecked` | 2 | 6/13 | 6/13 | 6/13 | 6/13 | treatment 0, control 0 |
| `anchors.resolvabilityRate` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.fresh` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.stale` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.orphaned` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.unchecked` | 2 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `use.retrievalRate` | 2 | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | treatment 0, control 0 |
| `use.everAccessedRate` | 2 | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | treatment 0, control 0 |
| `containment.selfContainmentRate` | 3 | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | 1.000 (7/7) | treatment 0, control 0 |
| `containment.clean` | 3 | 7/13 | 7/13 | 7/13 | 7/13 | treatment 0, control 0 |
| `containment.flagged` | 3 | 0/13 | 0/13 | 0/13 | 0/13 | treatment 0, control 0 |
| `containment.unchecked` | 3 | 6/13 | 6/13 | 6/13 | 6/13 | treatment 0, control 0 |
| `anchors.resolvabilityRate` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.fresh` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.stale` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.orphaned` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `anchors.unchecked` | 3 | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | unmeasured (0 in the denominator) | treatment unmeasured, control unmeasured |
| `use.retrievalRate` | 3 | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | 0.615 (8/13) | treatment 0, control 0 |
| `use.everAccessedRate` | 3 | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | 0.769 (10/13) | treatment 0, control 0 |

### Verdict per measure (§12.2 rule 4: three runs, same direction)

| measure | counts as improvement | Δ run 1 | Δ run 2 | Δ run 3 | verdict |
| --- | --- | --- | --- | --- | --- |
| `containment.selfContainmentRate` | up | +0.0000 | +0.0000 | +0.0000 | **no effect** |
| `containment.clean` | up | +0.0000 | +0.0000 | +0.0000 | **no effect** |
| `containment.flagged` | down | +0.0000 | +0.0000 | +0.0000 | **no effect** |
| `containment.unchecked` | down | +0.0000 | +0.0000 | +0.0000 | **no effect** |
| `anchors.resolvabilityRate` | up | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.fresh` | up | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.stale` | down | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.orphaned` | down | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `anchors.unchecked` | down | unmeasured | unmeasured | unmeasured | **unmeasured** |
| `use.retrievalRate` | up | +0.0000 | +0.0000 | +0.0000 | **no effect** ⚠ confounded (see notes) |
| `use.everAccessedRate` | up | +0.0000 | +0.0000 | +0.0000 | **no effect** ⚠ confounded (see notes) |

### Determinism control

- two control runs on two identical copies produced identical store measures
- control run 1 vs control run 2: identical
- control run 1 vs control run 3: identical
- control run 2 vs control run 3: identical

### Run facts (denominators and the pass's own activity)

| run | window total: control | window total: treatment | pass call | call seconds | access-log rows written during the pass (supplementary read) |
| --- | --- | --- | --- | --- | --- |
| 1 | no new rows | no new rows | `reflect` ok | 0.0 | 0 rows |
| 2 | no new rows | no new rows | `reflect` ok | 0.0 | 0 rows |
| 3 | no new rows | no new rows | `reflect` ok | 0.0 | 0 rows |

<!-- end of generated tables (the full per-run value table is in `results/20260920T174647Z-reflect-table.md`; every cell is the unchanged 13-memory baseline: clean 7/13, unchecked 6/13, use 8/13 and 10/13) -->

### Reading the `reflect` result

**`reflect` changed nothing in the store — a true null, three times over.** All four
measures and every count are identical before and after, in all three runs:

| measure | control | reflect | denominator |
| --- | --- | --- | --- |
| self-containment rate | 1.000 (7/7) | 1.000 (7/7) | 7 checked of 13 |
| clean share | 7/13 = 0.538 | 7/13 = 0.538 | 13 |
| unchecked | 6/13 = 0.462 | 6/13 = 0.462 | 13 |
| anchored: resolvability | unmeasured | unmeasured | 0 anchors |
| use: retrieved at least once | 8/13 = 0.615 | 8/13 = 0.615 | 13 |
| use: ever accessed | 10/13 = 0.769 | 10/13 = 0.769 | 13 |

This is not a harness failure to observe a change: the pass reported what it did. The
raw payload (`results/20260920T174647Z-reflect-payloads.json`) says
`memoriesAnalyzed: 13`, `stats.insights_generated: 0`, `contradictions: []`,
`patternClusters: []`, `staleDecisions: []`, and `knowledgeGaps: 11` — it read all 13
memories and wrote none. `reflect` on this store is a read-only report.

An aside on the same payload, not a measured result: `summary` reads *"Knowledge base
looks consistent — no contradictions, gaps, or stale decisions detected"* while the
same response lists 11 entries under `knowledgeGaps`. The summary line is wrong about
its own payload.

## Finding: `dream` could not complete at all before this run

The first end-to-end attempt of this harness killed the pass. Verbatim from the
treatment server's log of that smoke run (line numbers are from the pre-fix file; the
kept `/tmp` tree has since been removed, and the message is reproduced here because it
is the evidence):

```
thread 'tokio-rt-worker' (757773) panicked at crates/vestige-core/src/consolidation/phases/rem.rs:181:23:
byte index 60 is not a char boundary; it is inside 'ł' (bytes 59..61) of `Objaw: emulowana gra nie wchodziła do poziomu, tylko w kółko odtwarzała intro, na komputerze i na płytce tak samo. Przyczyna leżała w testerze, a nie w emulatorze: skrypt sterujący padem dzielił tekst zagnieżdżonym wywołaniem, które resetował`[...]
```

The MCP call died with the connection (`Remote end closed connection without
response`); the store stayed at whatever the phases before REM had written. The cause
was `&a.content[..60]` in `generate_connection_insight` — a raw byte slice — and a
second instance of the same class in `dreamer_hubs::first_sentence`
(`trimmed[..cut]`, 160-byte cap). Both are on the dream path and both were fixed with
`str::floor_char_boundary`, the idiom `codebase_unified.rs` and
`search_unified/format.rs::capped` already use:

- `crates/vestige-core/src/consolidation/phases/rem.rs` (committed as `0b923220`,
  "fix(dream): a summary cuts on a character, not on a byte"): the truncation is now
  the named helper `summary_of(content, 60)`, with two tests.
- `crates/vestige-core/src/advanced/dreams/dreamer_hubs.rs` (uncommitted at the time of
  the run): `let cut = trimmed.floor_char_boundary(cut);` before `trimmed[..cut]`,
  with `a_hub_excerpt_never_splits_a_character`.

Red evidence for both, captured before the fix by running the new tests against the
pre-fix semantics:

```
thread '...a_summary_never_splits_a_character' panicked at .../rem.rs:273:17:
byte index 60 is not a char boundary; it is inside 'ł' (bytes 59..61) of `Objaw: ...`
thread '...a_hub_excerpt_never_splits_a_character' panicked at .../dreamer_hubs.rs:193:12:
byte index 160 is not a char boundary; it is inside 'ł' (bytes 159..161) of `aaaa…ał oraz dalszy ciąg bez kropki`
```

Both tests pass after the fix. `cargo test -p vestige-core --lib` (864 passed),
`cargo test -p vestige-mcp --lib` (672 passed) and
`cargo clippy -p vestige-core --all-targets` (no warnings) were run on this tree.

**What this means for the numbers above.** The A/B ran on a binary that includes both
fixes. There is no "before" arm to compare the panic against: before the fix, `dream`
could not complete *at all* on a store containing non-ASCII content — the whole pass
died in REM, so no quality report after a completed `dream` was even producible. That
is a separate finding from the effect verdicts, and it is not folded into them: the
`dream` table above says what a *completed* dream does, not what the crashing one did.

## What these runs cannot say

- **A denominator of 13.** Every rate here is over 13 memories (or 7 checked ones).
  `7/13`, `8/13`, `10/13` are single-digit numerators; a change of one memory moves a
  rate by 0.077. No confidence interval, no significance test, no cluster bootstrap.
- **Anchors are unmeasured on this store**, because it holds 0 `code_refs`. Nothing
  here says anything about anchor resolvability, stale anchors or orphaned anchors.
- **`useCounts` cannot support a value claim in this design** — the treatment pass
  reads the store it is measured on. The `dream` "improved" on `use.retrievalRate` is
  that read (13 `search_hit` rows on 13 nodes), and the `everAccessed` "worse" is the
  pass's own 4 new rows entering the denominator.
- **`process.*` is not comparable across arms**: each read is a different process, and
  a refused write stores nothing, so the store cannot witness it either.
- **One store, one machine, one day.** Three runs of one 13-memory store with the same
  binary. The determinism control says the *measurement* is repeatable (identical
  control reports across runs), not that the result generalises.
- **`reflect`'s null is about this store.** It analyzed 13 memories and generated no
  insights; whether it writes anything on a larger or more contradictory corpus is not
  measured here.
- **The treatment arm is "the pass run through the server"** (§12.4), so anything the
  server itself did while serving the call is inside the treatment delta. Here that is
  at most inline consolidation, and the deltas are small and fully accounted for by the
  pass's own writes and reads.

## Verdicts, plainly

| pass | self-containment rate | clean share | unchecked | anchor resolvability | use: retrieved | use: ever accessed |
| --- | --- | --- | --- | --- | --- | --- |
| `dream` | no effect | worse (−0.127, 7/13 → 7/17) | worse (+0.127, 6/13 → 10/17) | unmeasured (0 anchors) | improved (+0.149, 8/13 → 13/17) — **confounded: the pass's own 13 reads** | worse (−0.005, 10/13 → 13/17) — **denominator artefact** |
| `reflect` | no effect | no effect | no effect | unmeasured (0 anchors) | no effect | no effect |

**Nothing got better in a way that is attributable to the pass.** `dream` adds four
never-checked rows and reads its own corpus; `reflect` changes nothing at all. Per
§12.2 rule 3, that is a result, and it is recorded as one: on this store, neither pass
improved the wave-5 measures.

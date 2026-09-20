# dream-ab — an honest A/B of `dream` / `reflect` against the wave-5 measures

`dream` (consolidation) and `reflect` (metacognition) mutate the store. This
harness asks the only question §12.2 allows: **did the wave-5 measures move, on
the same corpus, with and without the pass?** It is not a benchmark of answer
quality — §12.3 rules that out on purpose — and it is built so that the honest
answer "no effect" is as easy to write down as a positive one.

The binding specification is
[`docs/SELF-CONTAINED-MEMORY-DESIGN.md`](../../docs/SELF-CONTAINED-MEMORY-DESIGN.md) §12:
§12.1 defines the four measures and their traps, §12.2 the A/B rules, §12.4 the
implementation contract (including this harness). **Read §12.2 and §12.4 before
changing anything here.** The harness exists to enforce those rules, not to
produce a nice-looking number.

Measured numbers live in [`RESULTS.md`](RESULTS.md). They are generated from the
run JSON, never transcribed by hand.

## What is measured

One `MemoryQualityReport` per read — from `vestige quality --json` when that
surface exists (§12.4 names it for harnesses), otherwise `memory_health.quality`
over the server's MCP endpoint. Both are the same
`Storage::memory_quality()` payload; the harness records which surface answered.

| measure | definition | what the harness does with it |
| --- | --- | --- |
| containment | `clean`, `flagged`, `unchecked`, `total` | rate = `clean / (clean + flagged)`; **`unchecked` never enters the numerator or the denominator**; `unchecked` is its own row and a rise in it counts as *worse* |
| flagged by kind | `flaggedByKind[]` | reported next to the containment rows |
| anchors | `fresh`, `stale`, `orphaned`, `unchecked`, `total` | rate = `fresh / (fresh + stale + orphaned)`; `unchecked` (no repository, no revision) sits **beside** the rate; an all-`unchecked` window reports `unmeasured`, not `0` |
| use | `retrievedAtLeastOnce`, `everAccessed`, `total` | two rates, each printed with its denominator — and both marked confounded in this design (below) |
| process | `rejected`, `flagged`, `*ByKind` | printed, **not comparable across arms** and given no verdict: it describes the process that answered, not the store |

Every rate is computed by the harness from the counts it was handed, with exact
rational arithmetic, so `a/b` is always visible next to the percentage; if a
surface also ships a rate, a disagreement is recorded as a finding instead of
being silently preferred.

## The arms

One verified snapshot of the source store; every arm runs on its own copy of it.

- **`control`** — the copy, no pass. Read once at the start and once at the end
  of the pair's wall-clock slot.
- **`treatment`** — a second copy of the same snapshot, read, then the pass
  (`dream`, or `reflect`, or both in that order) run through the server's MCP
  endpoint, then read again after the server has stopped.
- **Determinism control** — because `dream` and `reflect` have no RNG, two
  control runs on two identical copies must produce identical store measures.
  The harness compares every `control.before` report across runs, and every
  control `before` against its own `after`, on everything except `until` (a
  clock, which cannot be equal) and `process` (per-process counters). Any
  difference is printed as a finding and is **not** averaged away.
- **Three runs** (`--runs 3`). Per measure, the effect of run *i* is
  `(treatment_after − treatment_before) − (control_after − control_before)`.
  A verdict needs the same non-zero sign in **all three** runs:
  - all three exactly zero → `no effect`
  - all three the same sign → `improved` / `worse`, read against the direction
    that counts as improvement for that measure
  - anything else, including a zero in one run and a rise in another →
    `unresolved`
  - no denominator anywhere in the comparison → `unmeasured`

  A negative result is a result: "nothing got worse" is not "it helped".

If the pass fails (a crash, a timeout), the run is **not** discarded: the
after-state is still read and the raw delta recorded, but every verdict is
forced to `unresolved` and the failure is printed with its error.

## How to run

```sh
cargo build --release -p vestige-mcp -p vestige-restore

# the default: `dream`, three control/treatment pairs, live store as the source
python3 benchmarks/dream-ab/run.py

# the other pass, and both in one treatment
python3 benchmarks/dream-ab/run.py --pass reflect
python3 benchmarks/dream-ab/run.py --pass both

# fast plumbing check (one pair, keep the temp tree for inspection)
python3 benchmarks/dream-ab/run.py --runs 1 --warmup 5 --keep-work --label smoke

# re-render a stored run's markdown (nothing is executed)
python3 benchmarks/dream-ab/run.py --render-table benchmarks/dream-ab/results/<run>.json
```

| flag | default | meaning |
| --- | --- | --- |
| `--source-store` | the live store | snapshotted **read-only**; the arms never touch it |
| `--pass` | `dream` | `dream` \| `reflect` \| `both` |
| `--runs` | `3` | control/treatment pairs |
| `--json-out` | `benchmarks/dream-ab/results/` | run JSON, raw pass payloads, generated table |
| `--quality-source` | `auto` | `cli` (preferred) \| `mcp` \| `auto` |
| `--since` | none | window start (`YYYY-MM-DD`, UTC) passed to the surface |
| `--warmup` | `15` | seconds between server readiness and the pass |
| `--keep-work` | off | keep the `/tmp/dream-ab-*` tree; without it, removed on exit |
| `--render-table` | — | re-render markdown from stored JSON, run nothing |

Standard library only, deliberately, like `benchmarks/memconflict/run.py`: a
benchmark that needs a dependency resolver is a benchmark that stops
reproducing. The only external programs it runs are the built `vestige` binary
and — for one supplementary count, clearly labelled as such — a read-only SQLite
query.

## Isolation is not optional

The store path comes from `directories::ProjectDirs`, so `HOME` decides which
store a process opens. Each arm gets its own temporary `HOME` containing a copy
of the snapshot; the server runs on a random free port with a harness-owned
`HOME` in its environment.

Before any arm is measured, the harness proves that relocating `HOME` really
selects the copy — with probes that cannot pollute the live store:

1. **Snapshot verification.** The source is opened `mode=ro` and copied with the
   SQLite backup API (a torn file copy of a hot WAL database is exactly the
   "compared a store against itself" failure this must prevent). The copy's
   `PRAGMA integrity_check` must be `ok`, its `knowledge_nodes` count must be
   non-zero, and its report total must equal that count.
2. **Delete probe.** One row is removed from a throw-away copy; the report read
   with `HOME` pointing at it must show exactly one row fewer. If it still shows
   the snapshot's count, the harness exits (`4`) **before writing anything**.
3. **Canary probe.** A canary memory is ingested through the CLI into a second
   throw-away copy; the report must count exactly one more row. This is the
   write-path half of the same claim.

The live store's row counts are recorded before and after the run as evidence
that it was only ever read. The temp tree is removed on exit unless
`--keep-work`.

## What this harness refuses to conclude

- **Nothing about answer quality.** No LoCoMo/LongMemEval score, no
  "was the memory true" metric (§12.3): there is no ground truth here.
- **No significance, no confidence interval, no trend.** Three runs of one
  store, reported as three runs. A direction that three runs do not share is
  `unresolved`.
- **Nothing about the store's *value* from `useCounts` alone** — see the
  confound below.
- **Nothing about the store from `process.*`**, which is process-scoped.
- **No generalisation beyond the source store.** The recorded run's store has
  the row counts printed in `RESULTS.md`; a rate there rests on a denominator
  you can read in the same table.

## Sources of a dishonest result, and what the harness does about each

| the trap | what this harness does |
| --- | --- |
| counting the run's own internal reads as "use" | `useCounts` is confounded **by construction**: the treatment pass retrieves from the store it is measured on, so its own `search_hit` rows and `access_count` bumps land in the treatment delta, while the control arm performs no reads at all. Both use rows are marked `[CONFOUNDED HERE]`, the verdict is still computed from the numbers, and a supplementary read-only query counts the access-log rows written inside the pass window so the confound has a number, not just a warning |
| comparing a store against itself when the copy failed | snapshot is integrity-checked and row-counted; the delete probe proves `HOME` selects the copy; the canary probe proves writes land there; any failure aborts with exit code `4` before a single arm is measured |
| reporting one run as a trend | three runs, and a verdict only when all three agree on a non-zero sign; `unresolved` otherwise; the determinism control is reported next to it |
| letting a `null` (never-checked) memory count as clean | `unchecked` is a separate row, is excluded from `selfContainmentRate`'s denominator, is never added to `clean`, and a rise in it is treated as worse |
| folding `unchecked` anchors into resolvability | the rate is `fresh / (fresh + stale + orphaned)`; `unchecked` is a separate row; when nothing is checkable the rate is `unmeasured`, not `0` |
| letting the pass's own new rows inflate the denominators | every rate is printed as `numerator/denominator` in both arms and both reads; the window total before/after is printed per run ("denominator inflation") and a pass that adds rows is visible in that column |
| comparing `process.*` across arms | `process.*` describes the process that answered each read (for the CLI surface, a process that writes nothing). It is printed and given no verdict |
| attributing everything to the pass | the treatment arm is defined as "the pass run through the server" (§12.4), so any inline consolidation the server performs while serving the call is inside the delta. The README and the JSON say so rather than pretending the arm is a pure function call |
| hiding a crash | a failed pass is recorded with its error, its run's after-state is still measured, and every verdict is forced to `unresolved` |

## Files

| file | purpose |
| --- | --- |
| `run.py` | the harness: snapshot, isolation probes, arms, measures, verdicts, renderer |
| `README.md` | this file: what is measured, how to run it, what it refuses to conclude |
| `RESULTS.md` | measured numbers, each with the command that produced it and a control column |
| `results/` | timestamped run JSON (with the binary's sha256), raw pass payloads, generated tables, supplementary evidence |

Temporary working directories live under `/tmp/dream-ab-*` and are removed on exit
unless `--keep-work` is passed.

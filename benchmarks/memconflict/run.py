#!/usr/bin/env python3
"""MemConflict retrieval benchmark for Vestige, with mandatory controls.

One command, pinned data, deterministic judge, JSON results.

    python3 benchmarks/memconflict/run.py --instances 2 --sessions 12

PORTED HARNESS -- read PORTING-NOTES.md before running. The upstream harness
this was ported from drove a Vestige v2.x server (`recall`, `scope`,
`VESTIGE_DATA_DIR`). This port drives the current v3.x tool surface (`search`,
`smart_ingest`, `deep_reference`) and isolates simulated users by giving each
one a fresh database + server process instead of a scope namespace.

WHAT THIS MEASURES
------------------
Whether a retrieval arm surfaces the evidence needed to answer a
conflict-sensitive question about a simulated user's multi-session history,
and -- for static conflicts -- whether the system RECOGNISES that the store
holds contradictory claims.

THE ARMS (all four run every time; none is optional)
---------------------------------------------------
  nomem    No retrieval at all. The reader receives an empty string.
           This is the true floor. Any metric that scores above ~0 here is
           measuring the judge, not the memory system.
  random   K memory units chosen uniformly at random from the same corpus,
           with a fixed seed. This is the blob-inflation control: the judge
           awards partial credit for token overlap, and a concatenation of
           any K memories shares tokens with the gold answer by chance.
           If an arm cannot beat `random`, its score is corpus statistics.
  bm25     Okapi BM25 over the identical corpus. The "earned complexity" bar.
  vestige  search() against the live MCP server (one fresh database + server
           process per simulated user -- there is no scope/namespace argument
           on the current tool surface; see PORTING-NOTES.md).

Every arm is fed to the SAME deterministic reader and the SAME judge, so the
only variable between arms is retrieval. That is the MemDelta discipline:
change one component at a time.

THE READER
----------
Deliberately not an LLM. The reader concatenates the top-K retrieved memory
texts and hands that to the judge. This removes the model confound entirely
and keeps the harness free and deterministic. The cost is that absolute
numbers are NOT comparable to any published table; only cross-arm differences
within a single run are meaningful. K is held identical across arms so no arm
gets a longer blob than another.

WHAT THIS DOES NOT MEASURE (read before quoting a delta)
-------------------------------------------------------
  * Significance by default is limited to one paired test. `--mcnemar`
    (default `vestige:bm25`) runs an exact two-sided McNemar test over the
    questions the two arms both answered, and the p-value is written into the
    results JSON. It is OPTIONAL to read and IMPOSSIBLE to get for a metric
    that is not defined per question.
  * The McNemar test assumes independent question pairs. Questions are nested
    inside simulated users (2 in the reference A/B run), so the effective
    sample is much smaller than n and the p-value is OPTIMISTIC. Every p is
    therefore stored next to a per-instance breakdown; read them together.
  * No confidence intervals and no cluster bootstrap: with one run, one seed
    and 2 users, a CI would be wider than any delta reported here.
  * No multiple-comparison correction: the more arms/metrics you compare in a
    single run, the more likely one p crosses 0.05 by chance.
  * Small strata are reported but flagged, never silently averaged away:
    `--min-reportable-n` (default 10) marks any column computed on fewer
    questions, in the console report, in the JSON (`power_analysis`) and in
    the markdown table (trailing `*`).

THE RESULTS TABLE IS GENERATED
------------------------------
The console table, the `--markdown-table` output and `--render-table` (which
re-renders a stored results JSON without re-running anything) all come from
`TABLE_COLUMNS`. Each label is pinned to the exact JSON key path it may
display, and `validate_summary()` refuses to print a table whose column key is
missing from a results file that has questions of that conflict type. The
2026-09-19 hand-transcribed table swapped the adjacent `statAA` and `CRSlex`
columns; that class of error now fails loudly instead of shipping.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import pathlib
import platform
import random as _random
import shutil
import subprocess
import statistics
import sys
import time
from typing import Any, Dict, Iterable, List, NamedTuple, Optional, Sequence, Tuple

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE))

import bm25 as bm25_mod  # noqa: E402
import judge as judge_mod  # noqa: E402
from mcp_client import VestigeMCP, VestigeMCPError  # noqa: E402

ARMS = ("nomem", "random", "bm25", "vestige")
CONFLICT_TYPES = ("dynamic_conflict", "static_conflict", "conditional_conflict")

#: Keys `aggregate()` may write into `results.<arm>.per_conflict_type.<type>`.
PER_TYPE_METRICS = frozenset({"n", "answer_accuracy", "uocs", "crs_lex", "crs_struct"})
#: Keys `aggregate()` may write into `results.<arm>`.
WHOLE_RUN_METRICS = frozenset({
    "n_questions", "reader_chars_mean", "n_retrieved_mean",
    "per_conflict_type", "macro_answer_accuracy", "micro_answer_accuracy",
})
#: Metrics a conflict type MUST carry once it is present with n > 0. A column
#: whose key is missing here is report/harness schema drift, not a dash.
REQUIRED_PER_TYPE: Dict[str, Tuple[str, ...]] = {
    "dynamic_conflict": ("answer_accuracy", "uocs"),
    "static_conflict": ("answer_accuracy", "crs_lex"),
    "conditional_conflict": ("answer_accuracy",),
}


class TableColumn(NamedTuple):
    """One results-table column, declared once and rendered everywhere."""
    label: str
    ctype: Optional[str]   # None = whole-run metric
    metric: str


#: ORDER IS PART OF THE OUTPUT. `statAA` (static_conflict.answer_accuracy) and
#: `CRSlex` (static_conflict.crs_lex) were swapped in the hand-written
#: 2026-09-19 RESULTS.md table; here each label is bound to its key path in the
#: same tuple that renders it, and LABEL_KEY_PATHS below re-checks the binding.
TABLE_COLUMNS: Tuple[TableColumn, ...] = (
    TableColumn("arm", None, "arm"),
    TableColumn("n", None, "n_questions"),
    TableColumn("macroAA", None, "macro_answer_accuracy"),
    TableColumn("microAA", None, "micro_answer_accuracy"),
    TableColumn("dynAA", "dynamic_conflict", "answer_accuracy"),
    TableColumn("UOCS", "dynamic_conflict", "uocs"),
    TableColumn("statAA", "static_conflict", "answer_accuracy"),
    TableColumn("CRSlex", "static_conflict", "crs_lex"),
    TableColumn("condAA", "conditional_conflict", "answer_accuracy"),
    TableColumn("chars", None, "reader_chars_mean"),
)

#: Anti-transposition gate: every label is pinned to the ONE JSON key path it
#: is allowed to display. Adding a column without pinning it here (or pinning
#: it to a different path than TABLE_COLUMNS declares) is a hard error, so a
#: rename/reorder cannot silently re-label a number.
LABEL_KEY_PATHS: Dict[str, str] = {
    "arm": "arm",
    "n": "n_questions",
    "macroaa": "macro_answer_accuracy",
    "microaa": "micro_answer_accuracy",
    "dynaa": "dynamic_conflict.answer_accuracy",
    "uocs": "dynamic_conflict.uocs",
    "stataa": "static_conflict.answer_accuracy",
    "crslex": "static_conflict.crs_lex",
    "condaa": "conditional_conflict.answer_accuracy",
    "chars": "reader_chars_mean",
}

#: Default paired significance test. Only pairs whose arms both ran are used.
DEFAULT_MCNEMAR_PAIRS = "vestige:bm25"


def column_key_path(col: TableColumn) -> str:
    """The JSON key path a column is allowed to display."""
    return col.metric if col.ctype is None else f"{col.ctype}.{col.metric}"


def validate_column_contract() -> None:
    """Hard-fail if the declared table columns and their pinned key paths drift.

    This is what makes a column swap impossible-by-accident: every label must
    exist in LABEL_KEY_PATHS and point at exactly the key path TABLE_COLUMNS
    declares, and every metric `aggregate()` must emit needs a column.
    """
    labels: Dict[str, str] = {}
    paths: Dict[str, str] = {}
    for col in TABLE_COLUMNS:
        low = col.label.lower()
        if low in labels:
            raise SystemExit(f"table contract: duplicate column label {col.label!r}")
        labels[low] = col.label
        path = column_key_path(col)
        if path != "arm":
            if path in paths:
                raise SystemExit(
                    f"table contract: {path!r} is displayed by two columns "
                    f"({paths[path]!r} and {col.label!r}); one metric, one column"
                )
            paths[path] = col.label
        pinned = LABEL_KEY_PATHS.get(low)
        if pinned is None:
            raise SystemExit(
                f"table contract: column {col.label!r} is not pinned in "
                f"LABEL_KEY_PATHS; add the label->key-path binding before rendering"
            )
        if pinned != path:
            raise SystemExit(
                f"table contract: column {col.label!r} renders {path!r} but "
                f"LABEL_KEY_PATHS pins it to {pinned!r} -- refusing to print a "
                f"table that can mis-label its own numbers"
            )
    required: Dict[str, str] = {
        metric: metric for metric in
        ("n_questions", "macro_answer_accuracy", "micro_answer_accuracy", "reader_chars_mean")
    }
    for ctype, metrics in REQUIRED_PER_TYPE.items():
        for metric in metrics:
            required[f"{ctype}.{metric}"] = f"{ctype}.{metric}"
    missing = {k: v for k, v in required.items() if v not in paths}
    if missing:
        raise SystemExit(
            f"table contract: no column renders required metric(s): "
            f"{', '.join(sorted(missing))}"
        )


def validate_summary(summary: Dict[str, Any], min_reportable_n: int) -> List[str]:
    """Validate every arm summary against the table contract.

    Returns the small-sample warnings (never silent). Raises SystemExit when a
    results file violates the schema -- most importantly when a conflict type
    with n > 0 is missing a metric that a column is supposed to show, which is
    exactly the failure mode a hand-edited table hides as a dash.
    """
    warnings: List[str] = []
    for arm, s in summary.items():
        if not isinstance(s, dict):
            raise SystemExit(f"results schema: results[{arm!r}] is not an object")
        missing_whole = sorted(
            key for key in ("n_questions", "reader_chars_mean", "per_conflict_type",
                            "macro_answer_accuracy", "micro_answer_accuracy")
            if key not in s
        )
        if missing_whole:
            raise SystemExit(
                f"results schema: results[{arm!r}] misses {', '.join(missing_whole)}; "
                f"refusing to render a table from an incomplete results file"
            )
        unknown_whole = sorted(set(s) - WHOLE_RUN_METRICS - {"significance", "power_analysis"})
        if unknown_whole:
            raise SystemExit(
                f"results schema: results[{arm!r}] carries unknown top-level key(s) "
                f"{', '.join(unknown_whole)}; update WHOLE_RUN_METRICS or the results writer"
            )
        pt = s["per_conflict_type"]
        if not isinstance(pt, dict):
            raise SystemExit(f"results schema: results[{arm!r}].per_conflict_type is not an object")
        for ctype, block in pt.items():
            if ctype not in CONFLICT_TYPES:
                raise SystemExit(
                    f"results schema: results[{arm!r}] has unknown conflict type {ctype!r}; "
                    f"known: {', '.join(CONFLICT_TYPES)}"
                )
            if not isinstance(block, dict):
                raise SystemExit(f"results schema: results[{arm!r}].{ctype} is not an object")
            unknown = sorted(set(block) - PER_TYPE_METRICS)
            if unknown:
                raise SystemExit(
                    f"results schema: results[{arm!r}].{ctype} carries unknown key(s) "
                    f"{', '.join(unknown)}; add them to PER_TYPE_METRICS and to a column"
                )
            n = block.get("n")
            if not isinstance(n, int) or isinstance(n, bool) or n <= 0:
                raise SystemExit(
                    f"results schema: results[{arm!r}].{ctype}.n = {n!r}; expected a positive int"
                )
            for metric in REQUIRED_PER_TYPE.get(ctype, ()):
                if metric not in block:
                    raise SystemExit(
                        f"results schema: results[{arm!r}] has {ctype} with n={n} but no "
                        f"{metric!r} key. The table contract expects it; a missing key here "
                        f"is how a swapped/absent column ships as a dash."
                    )
            if n < min_reportable_n:
                warnings.append(
                    f"{arm}: {ctype} is reported on n={n} question(s) "
                    f"(< --min-reportable-n {min_reportable_n})"
                )
        n_q = s["n_questions"]
        if isinstance(n_q, int) and not isinstance(n_q, bool) and n_q < min_reportable_n:
            warnings.append(
                f"{arm}: whole arm is reported on n={n_q} question(s) "
                f"(< --min-reportable-n {min_reportable_n})"
            )
    return warnings

#: Default server binary location. Our release binary, not upstream's debug one.
DEFAULT_SERVER_BINARY = str(REPO / "target" / "release" / "vestige-mcp")
SERVER_BINARY_ENV = "VESTIGE_MCP_BINARY"


# --------------------------------------------------------------------------
# dataset
# --------------------------------------------------------------------------

def load_instances(path: pathlib.Path, limit: Optional[int]) -> List[Dict[str, Any]]:
    rows = []
    with path.open() as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))
            if limit and len(rows) >= limit:
                break
    return rows


def session_units(session: Dict[str, Any]) -> List[str]:
    """The memory units contributed by one session.

    One unit per user utterance, date-stamped. Assistant turns are excluded:
    they are the agent's own words, not facts about the user, and including
    them would pad every arm's corpus with paraphrase.
    """
    date = session.get("Date", "")
    dialogue = session.get("Session_Dialogue") or {}
    units: List[str] = []
    # dialogue_turn_N keys must be walked in numeric order, not string order.
    def turn_no(k: str) -> int:
        try:
            return int(k.rsplit("_", 1)[1])
        except (IndexError, ValueError):
            return 0

    for key in sorted(dialogue.keys(), key=turn_no):
        for msg in dialogue[key] or []:
            if msg.get("role") != "user":
                continue
            text = (msg.get("content") or "").strip()
            if text:
                units.append(f"[{date}] {text}")
    return units


def iter_sessions(instance: Dict[str, Any], max_sessions: Optional[int]) -> Iterable[Dict[str, Any]]:
    chain = instance.get("Full_Session_Chain") or []
    chain = sorted(chain, key=lambda s: s.get("Session_ID", 0))
    if max_sessions:
        chain = chain[:max_sessions]
    return chain


# --------------------------------------------------------------------------
# readers / arms
# --------------------------------------------------------------------------

def reader(texts: List[str]) -> str:
    """Deterministic reader: concatenate retrieved memory texts."""
    return "\n".join(t for t in texts if t)


class NoMemArm:
    name = "nomem"

    def reset(self, instance_id: str) -> None:
        pass

    def add(self, units: List[str]) -> None:
        pass

    def retrieve(self, question: str, k: int) -> Tuple[List[str], Dict[str, Any]]:
        return [], {}


class RandomArm:
    name = "random"

    def __init__(self, seed: int) -> None:
        self.seed = seed
        self.corpus: List[str] = []
        self.rng = _random.Random(seed)

    def reset(self, instance_id: str) -> None:
        self.corpus = []
        # Re-seed per instance so results do not depend on instance order.
        self.rng = _random.Random(f"{self.seed}:{instance_id}")

    def add(self, units: List[str]) -> None:
        self.corpus.extend(units)

    def retrieve(self, question: str, k: int) -> Tuple[List[str], Dict[str, Any]]:
        if not self.corpus:
            return [], {}
        n = min(k, len(self.corpus))
        return self.rng.sample(self.corpus, n), {}


class BM25Arm:
    name = "bm25"

    def __init__(self) -> None:
        self.corpus: List[str] = []
        self._index: Optional[bm25_mod.BM25] = None

    def reset(self, instance_id: str) -> None:
        self.corpus = []
        self._index = None

    def add(self, units: List[str]) -> None:
        self.corpus.extend(units)
        self._index = None  # invalidate; rebuilt lazily on next retrieve

    def retrieve(self, question: str, k: int) -> Tuple[List[str], Dict[str, Any]]:
        if not self.corpus:
            return [], {}
        if self._index is None:
            self._index = bm25_mod.BM25(self.corpus)
        hits = self._index.top_k(question, k)
        return [self.corpus[i] for i, _ in hits], {}


class VestigeArm:
    """Live `search` over MCP.

    Isolation model (ported): the current server has no `scope`/namespace
    argument, so each simulated user gets its own database file inside its own
    temp directory and therefore its own server process. `reset()` is where
    that happens -- it closes the previous server, starts a new one against an
    empty store, and blocks through the warmup window.
    """

    name = "vestige"

    def __init__(self, binary: str, data_root: pathlib.Path, mode: str,
                 detail_level: str, min_similarity: float, min_retention: float,
                 warmup_seconds: float, contradiction_probe: bool,
                 contradiction_depth: int = 50, keep_databases: bool = False,
                 extra_env: Optional[Dict[str, str]] = None) -> None:
        self.binary = binary
        self.data_root = pathlib.Path(data_root)
        self.mode = mode
        self.detail_level = detail_level
        self.min_similarity = min_similarity
        self.min_retention = min_retention
        self.warmup_seconds = warmup_seconds
        self.contradiction_probe = contradiction_probe
        self.contradiction_depth = contradiction_depth
        self.keep_databases = keep_databases
        self.extra_env = dict(extra_env or {})
        self.client: Optional[VestigeMCP] = None
        self.instance_id = "default"
        self.db_dir: Optional[pathlib.Path] = None
        self.ingest_errors = 0
        self.retrieve_errors = 0
        self.latencies: List[float] = []
        self.instance_dirs: List[str] = []
        self.warmups: List[Dict[str, Any]] = []

    # -- lifecycle ---------------------------------------------------------

    def reset(self, instance_id: str) -> None:
        self.close()
        self.instance_id = instance_id
        safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in instance_id)[:64]
        self.db_dir = self.data_root / f"instance-{safe}"
        self.db_dir.mkdir(parents=True, exist_ok=True)
        self.instance_dirs.append(str(self.db_dir))
        # Fresh store per simulated user. `db_dir` (not the --data-dir value):
        # the client appends vestige.db itself.
        self.client = VestigeMCP(
            self.binary,
            str(self.db_dir),
            warmup_seconds=self.warmup_seconds,
            extra_env=self.extra_env,
        )
        self.client.start()
        self.client.initialize()
        self.warmups.append({"instance": instance_id, **self.client.observed_warmup})

    def close(self) -> None:
        if self.client is not None:
            self.client.close()
            self.client = None
        if self.db_dir is not None and not self.keep_databases:
            shutil.rmtree(self.db_dir, ignore_errors=True)
        self.db_dir = None

    def cleanup_root(self) -> None:
        """Remove the (now empty) base store directory when nothing was kept."""
        if self.keep_databases:
            return
        try:
            self.data_root.rmdir()
        except OSError:
            pass  # non-empty or already gone -- leave it alone

    # -- ingest / retrieve -------------------------------------------------

    def add(self, units: List[str]) -> None:
        if self.client is None:
            raise VestigeMCPError("vestige arm used before reset()")
        # smart_ingest batch mode caps at 20 items per call.
        for start in range(0, len(units), 20):
            chunk = units[start:start + 20]
            # `forceCreate` is our analogue of upstream's
            # `batchMergePolicy: force_create`: every unit is stored verbatim
            # instead of being merged into an existing near-duplicate.
            items = [
                {"content": text, "node_type": "event", "forceCreate": True}
                for text in chunk
            ]
            try:
                self.client.call_tool(
                    "smart_ingest",
                    {"items": items},
                    timeout=600.0,
                )
            except VestigeMCPError:
                self.ingest_errors += 1

    @staticmethod
    def _texts_from_search(payload: Any) -> List[str]:
        """Pull memory CONTENT out of a `search` payload.

        Deliberately extracts only stored memory text, never the tool's own
        framing, headers, labels or field names. Scoring the envelope would
        let the harness award conflict-recognition credit for a canned string
        the server always emits.

        Current surface: `{"query", "total", "results": [{"id","content",...}]}`.
        The legacy key list is kept as a defensive fallback only -- `results`
        is the only key this server emits.
        """
        out: List[str] = []
        if isinstance(payload, str):
            return [payload]
        if not isinstance(payload, dict):
            return out
        for key in ("results", "memories", "nodes", "matches"):
            for item in payload.get(key) or []:
                if isinstance(item, dict):
                    text = item.get("content") or item.get("preview") or item.get("text")
                    if text:
                        out.append(str(text))
                elif isinstance(item, str):
                    out.append(item)
        return out

    def retrieve(self, question: str, k: int) -> Tuple[List[str], Dict[str, Any]]:
        extra: Dict[str, Any] = {}
        t0 = time.monotonic()
        try:
            payload = self.client.call_tool(
                "search",
                {
                    "query": question,
                    "limit": k,
                    "detail_level": self.detail_level,
                    "min_similarity": self.min_similarity,
                    "min_retention": self.min_retention,
                    "retrieval_mode": self.mode,
                },
                timeout=300.0,
            )
            texts = self._texts_from_search(payload)[:k]
            if isinstance(payload, dict):
                if payload.get("gated"):
                    extra["gated"] = payload.get("reason")
                if payload.get("total") is not None:
                    extra["total"] = payload["total"]
        except VestigeMCPError as exc:
            self.retrieve_errors += 1
            extra["error"] = str(exc)[:200]
            texts = []
        self.latencies.append(time.monotonic() - t0)
        return texts, extra

    def probe_contradictions(self, topic: str, depth: int = 50) -> Dict[str, Any]:
        """Structural contradiction signal (Vestige-only capability).

        Upstream called `recall(mode="contradictions")`, which does not exist on
        the current surface. We call `deep_reference` instead: it retrieves up
        to `depth` memories for the topic and runs pairwise relation assessment
        (supports / supersedes / CONTRADICTS) over them, returning a
        `contradictions` array. We score the length of that array -- never
        keyword-match the response -- so the metric cannot be satisfied by the
        tool merely saying the word "contradiction".

        This is a capability report, not a head-to-head win: the control arms
        have no contradiction channel at all.
        """
        try:
            payload = self.client.call_tool(
                "deep_reference",
                {"query": topic, "depth": depth},
                timeout=300.0,
            )
        except VestigeMCPError as exc:
            return {"error": str(exc)[:200], "found": 0}
        if not isinstance(payload, dict):
            return {"found": 0}
        contradictions = payload.get("contradictions") or []
        return {
            "found": len(contradictions) if isinstance(contradictions, list) else 0,
            "analyzed": len(payload.get("evidence") or []),
        }


# --------------------------------------------------------------------------
# aggregation
# --------------------------------------------------------------------------

def aggregate(records: List[Dict[str, Any]]) -> Dict[str, Any]:
    """Aggregate per-question scores.

    Metrics are averaged only over the questions where they apply. A metric
    that does not apply to a conflict type is absent, never zero, so the
    denominator is always correct.
    """
    def mean_of(key: str, ctype: Optional[str] = None) -> Optional[float]:
        vals = [
            r["metrics"][key]
            for r in records
            if key in r["metrics"] and (ctype is None or r["metrics"]["conflict_type"] == ctype)
        ]
        return round(statistics.fmean(vals), 4) if vals else None

    def count(ctype: str) -> int:
        return sum(1 for r in records if r["metrics"]["conflict_type"] == ctype)

    per_type = {}
    for ctype in CONFLICT_TYPES:
        n = count(ctype)
        if not n:
            continue
        per_type[ctype] = {"n": n, "answer_accuracy": mean_of("answer_accuracy", ctype)}
    if "dynamic_conflict" in per_type:
        per_type["dynamic_conflict"]["uocs"] = mean_of("uocs", "dynamic_conflict")
    if "static_conflict" in per_type:
        per_type["static_conflict"]["crs_lex"] = mean_of("crs_lex", "static_conflict")
        crs_struct = mean_of("crs_struct", "static_conflict")
        if crs_struct is not None:
            per_type["static_conflict"]["crs_struct"] = crs_struct

    # Macro-average over conflict types present, matching the paper's
    # "Average AA" column (a mean of the three type-level means, NOT a
    # micro-average over questions, which the type imbalance would dominate).
    type_means = [v["answer_accuracy"] for v in per_type.values() if v["answer_accuracy"] is not None]
    chars = [r.get("answer_chars", 0) for r in records]
    retrieved = [r.get("n_retrieved", 0) for r in records]
    return {
        "n_questions": len(records),
        # Blob-size fairness check. The judge awards partial credit for token
        # overlap, so an arm that hands it more text has a structural advantage
        # that has nothing to do with retrieval quality. If these differ
        # materially between arms, the AA comparison is confounded -- say so
        # rather than reporting the winner.
        "reader_chars_mean": round(statistics.fmean(chars), 1) if chars else 0.0,
        "n_retrieved_mean": round(statistics.fmean(retrieved), 2) if retrieved else 0.0,
        "per_conflict_type": per_type,
        "macro_answer_accuracy": round(statistics.fmean(type_means), 4) if type_means else None,
        "micro_answer_accuracy": mean_of("answer_accuracy"),
    }


# --------------------------------------------------------------------------
# significance (paired questions, exact McNemar)
# --------------------------------------------------------------------------

def exact_mcnemar(b: int, c: int) -> float:
    """Exact two-sided McNemar p-value (binomial, stdlib only).

    `b` and `c` are the discordant pair counts: b = arm A correct / arm B
    wrong, c = the reverse. Under H0 each discordant pair is a fair coin, so
    the two-sided p is 2 * P(X <= min(b, c)) with X ~ Binomial(b + c, 0.5),
    capped at 1.0. Exact rather than chi-square because the discordant counts
    in this harness are small (single digits for the static stratum).
    """
    n = b + c
    if n <= 0:
        return 1.0
    k = min(b, c)
    tail = sum(math.comb(n, i) for i in range(k + 1)) / (2 ** n)
    return min(1.0, 2.0 * tail)


def _binarised_correct(records: Sequence[Dict[str, Any]]) -> Dict[Tuple[Any, Any, Any], bool]:
    """Map each question to 'was it answered correctly' (answer_accuracy >= 0.5).

    The judge emits 0.0 / 0.5 / 1.0 for dynamic and static conflicts and 0/1
    for conditional ones, so 0.5 is the pass mark the judge itself uses for
    conditional conflicts. Partial credit is deliberately collapsed here: a
    paired test needs a per-question outcome, and `micro_answer_accuracy`
    (which keeps partial credit) is reported next to it.
    """
    out: Dict[Tuple[Any, Any, Any], bool] = {}
    for rec in records:
        key = (rec.get("instance"), rec.get("session"), rec.get("question_id"))
        out[key] = float(rec.get("metrics", {}).get("answer_accuracy") or 0.0) >= 0.5
    return out


def compute_significance(
    records_by_arm: Dict[str, List[Dict[str, Any]]],
    pairs: Sequence[Tuple[str, str]],
) -> Dict[str, Any]:
    """Paired McNemar tests between arms, over the questions both arms answered.

    Every entry records the discordant counts, the exact two-sided p-value,
    the binarised delta, a per-conflict-type breakdown and a per-instance
    (cluster) breakdown. The p-value treats questions as independent; the
    per-instance block is what exposes the clustering, so the two must be read
    together.
    """
    out: Dict[str, Any] = {}
    for arm_a, arm_b in pairs:
        recs_a = records_by_arm.get(arm_a) or []
        recs_b = records_by_arm.get(arm_b) or []
        key = f"{arm_a}_vs_{arm_b}"
        if not recs_a or not recs_b:
            out[key] = {
                "arm_a": arm_a,
                "arm_b": arm_b,
                "status": "skipped",
                "reason": "one or both arms did not run (no per-question records)",
            }
            continue
        a = _binarised_correct(recs_a)
        b = _binarised_correct(recs_b)
        shared = sorted(set(a) & set(b))
        if not shared:
            out[key] = {
                "arm_a": arm_a, "arm_b": arm_b, "status": "skipped",
                "reason": "no shared (instance, session, question_id) pairs",
            }
            continue

        a_only = sum(1 for k in shared if a[k] and not b[k])
        b_only = sum(1 for k in shared if b[k] and not a[k])
        n = len(shared)
        correct_a = sum(1 for k in shared if a[k])
        correct_b = sum(1 for k in shared if b[k])
        p = exact_mcnemar(a_only, b_only)

        conflict_type: Dict[Tuple[Any, Any, Any], Any] = {}
        for rec in recs_a:
            rec_key = (rec.get("instance"), rec.get("session"), rec.get("question_id"))
            conflict_type[rec_key] = rec.get("metrics", {}).get("conflict_type")
        by_type: Dict[str, Any] = {}
        for ctype in CONFLICT_TYPES:
            keys = [k for k in shared if conflict_type.get(k) == ctype]
            if not keys:
                continue
            ao = sum(1 for k in keys if a[k] and not b[k])
            bo = sum(1 for k in keys if b[k] and not a[k])
            by_type[ctype] = {
                "n_pairs": len(keys),
                f"{arm_a}_only_correct": ao,
                f"{arm_b}_only_correct": bo,
                "mcnemar_exact_two_sided_p": round(exact_mcnemar(ao, bo), 6),
            }
        by_instance: Dict[str, Any] = {}
        for inst in sorted({k[0] for k in shared}, key=str):
            keys = [k for k in shared if k[0] == inst]
            ao = sum(1 for k in keys if a[k] and not b[k])
            bo = sum(1 for k in keys if b[k] and not a[k])
            by_instance[str(inst)] = {
                "n_pairs": len(keys),
                f"{arm_a}_only_correct": ao,
                f"{arm_b}_only_correct": bo,
                "mcnemar_exact_two_sided_p": round(exact_mcnemar(ao, bo), 6),
            }

        out[key] = {
            "arm_a": arm_a,
            "arm_b": arm_b,
            "status": "ok",
            "method": ("exact two-sided McNemar (binomial) on per-question "
                       "answer_accuracy >= 0.5"),
            "n_pairs": n,
            "arm_a_correct": correct_a,
            "arm_b_correct": correct_b,
            "arm_a_only_correct": a_only,
            "arm_b_only_correct": b_only,
            "discordant_pairs": a_only + b_only,
            "delta_binarised_accuracy": round((correct_a - correct_b) / n, 4),
            "mcnemar_exact_two_sided_p": round(p, 6),
            "significant_at_0_05": bool(p < 0.05),
            "per_conflict_type": by_type,
            "per_instance": by_instance,
            "cluster_caveat": (
                "Questions are nested inside simulated users; McNemar assumes "
                "independent pairs, so this p is optimistic. Read it next to "
                "per_instance (cluster = one simulated user)."
            ),
            "multiplicity_caveat": (
                "No multiple-comparison correction: many arm/metric "
                "comparisons in one run inflate the chance of a p < 0.05."
            ),
        }
    return out


def _fmt_p(p: float) -> str:
    return f"{p:.4f}" if p >= 1e-4 else "<0.0001"


# --------------------------------------------------------------------------
# results table rendering (one source of truth: TABLE_COLUMNS)
# --------------------------------------------------------------------------

def column_value(summary: Dict[str, Any], col: TableColumn) -> Optional[float]:
    if col.metric == "arm":
        return None
    if col.ctype is None:
        value = summary.get(col.metric)
    else:
        value = (summary.get("per_conflict_type") or {}).get(col.ctype, {}).get(col.metric)
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def column_n(summary: Dict[str, Any], col: TableColumn) -> Optional[int]:
    """How many questions a column is actually reported on."""
    if col.ctype is None:
        n = summary.get("n_questions")
    else:
        n = (summary.get("per_conflict_type") or {}).get(col.ctype, {}).get("n")
    return n if isinstance(n, int) and not isinstance(n, bool) else None


def _fmt_number(value: Optional[float]) -> str:
    return f"{value:.4f}" if value is not None else "  -  "


def _fmt_cell(col: TableColumn, value: Optional[float]) -> str:
    if value is None:
        return "  -  "
    if col.metric == "n_questions":
        return str(int(value))
    if col.metric == "reader_chars_mean":
        # Half-up, so the generated table reproduces the published RESULTS.md
        # rounding (735.5 -> 736, 798.5 -> 799) instead of Python's round-half-even.
        return str(int(value + 0.5))
    return _fmt_number(value)


def table_entries(table_rows_data: Sequence[Tuple[str, Dict[str, Any], Optional[str]]],
                  min_reportable_n: int) -> Tuple[List[str], List[List[str]], List[str]]:
    """Build header + rows from the column contract.

    `table_rows_data` is a sequence of (arm_label, summary, run_label) where
    `run_label` is only used when several results files are rendered together.
    """
    with_run = any(run_label for _, _, run_label in table_rows_data)
    header = (["run"] if with_run else []) + [col.label for col in TABLE_COLUMNS]
    rows: List[List[str]] = []
    small: List[str] = []
    for arm_label, summary, run_label in table_rows_data:
        cells: List[str] = [run_label or ""] if with_run else []
        for col in TABLE_COLUMNS:
            if col.metric == "arm":
                cells.append(f"`{arm_label}`")
                continue
            value = column_value(summary, col)
            text = _fmt_cell(col, value)
            n = column_n(summary, col)
            if n is not None and n < min_reportable_n:
                text += "*"
                small.append(f"{arm_label}:{col.label} n={n}")
            cells.append(text)
        rows.append(cells)
    return header, rows, small


def render_markdown_table(table_rows_data: Sequence[Tuple[str, Dict[str, Any], Optional[str]]],
                          min_reportable_n: int) -> str:
    """Render the results table as markdown, straight from the column contract."""
    header, rows, small = table_entries(table_rows_data, min_reportable_n)
    lines = [
        "| " + " | ".join(header) + " |",
        "| " + " | ".join("---" for _ in header) + " |",
    ]
    lines += ["| " + " | ".join(cells) + " |" for cells in rows]
    if small:
        lines.append("")
        lines.append(
            f"\\* reported on fewer than {min_reportable_n} questions "
            f"(`--min-reportable-n`): treat as noise, not signal "
            f"({', '.join(dict.fromkeys(small))})."
        )
    return "\n".join(lines)


def render_console_table(table_rows_data: Sequence[Tuple[str, Dict[str, Any], Optional[str]]]) -> None:
    """Console report. Same contract, ASCII padding, no `*` markers."""
    header, rows, _ = table_entries(table_rows_data, min_reportable_n=0)
    rows = [[cell.strip("`") for cell in cells] for cells in rows]
    left = [label in ("run", "arm") for label in header]
    widths = [
        max(len(h), 9 if h == "arm" else (len(h) if h == "run" else 8))
        for h in header
    ]
    for cells in rows:
        for i, cell in enumerate(cells):
            widths[i] = max(widths[i], len(cell))
    print("".join(
        (header[i].ljust(widths[i] + 1) if left[i] else header[i].rjust(widths[i] + 1))
        if i < len(header) - 1 else
        (header[i].ljust(widths[i]) if left[i] else header[i].rjust(widths[i]))
        for i in range(len(header))
    ))
    print("-" * (sum(widths) + len(header) - 1))
    for cells in rows:
        print("".join(
            (cells[i].ljust(widths[i] + 1) if left[i] else cells[i].rjust(widths[i] + 1))
            if i < len(cells) - 1 else
            (cells[i].ljust(widths[i]) if left[i] else cells[i].rjust(widths[i]))
            for i in range(len(cells))
        ))


def load_results_file(path: pathlib.Path) -> Dict[str, Any]:
    """Load a run.py results JSON for `--render-table` / `--check-summary`."""
    try:
        data = json.loads(path.read_text())
    except json.JSONDecodeError as exc:
        raise SystemExit(f"{path}: not valid JSON ({exc})")
    if not isinstance(data, dict) or "results" not in data or "per_question" not in data:
        raise SystemExit(
            f"{path}: not a run.py results file (needs 'results' and 'per_question')"
        )
    return data


def parse_mcnemar_pairs(spec: Optional[str]) -> List[Tuple[str, str]]:
    if spec is None or not spec.strip() or spec.strip().lower() == "none":
        return []
    pairs: List[Tuple[str, str]] = []
    for chunk in spec.split(","):
        chunk = chunk.strip()
        if not chunk:
            continue
        if ":" not in chunk:
            raise SystemExit(f"--mcnemar expects arm:arm pairs, got {chunk!r}")
        arm_a, arm_b = (part.strip() for part in chunk.split(":", 1))
        for arm in (arm_a, arm_b):
            if arm not in ARMS:
                raise SystemExit(f"--mcnemar: unknown arm {arm!r}; choose from {ARMS}")
        if arm_a == arm_b:
            raise SystemExit(f"--mcnemar: cannot pair {arm_a!r} with itself")
        pairs.append((arm_a, arm_b))
    return pairs


# --------------------------------------------------------------------------
# environment capture
# --------------------------------------------------------------------------

def git_rev(repo: pathlib.Path) -> Dict[str, Any]:
    def sh(*args: str) -> Optional[str]:
        try:
            return subprocess.run(
                args, cwd=repo, capture_output=True, text=True, timeout=20
            ).stdout.strip() or None
        except Exception:
            return None
    return {
        "commit": sh("git", "rev-parse", "HEAD"),
        "branch": sh("git", "rev-parse", "--abbrev-ref", "HEAD"),
        "dirty": bool(sh("git", "status", "--porcelain")),
    }


def machine_info() -> Dict[str, Any]:
    info = {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "python": sys.version.split()[0],
        "cpu_count": None,
        "memory_bytes": None,
    }
    try:
        import os
        info["cpu_count"] = os.cpu_count()
    except Exception:
        pass
    try:
        out = subprocess.run(["sysctl", "-n", "hw.memsize"], capture_output=True, text=True, timeout=10)
        if out.returncode == 0:
            info["memory_bytes"] = int(out.stdout.strip())
    except Exception:
        pass
    try:
        out = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True, timeout=10)
        if out.returncode == 0 and out.stdout.strip():
            info["cpu_brand"] = out.stdout.strip()
    except Exception:
        pass
    return info


def render_table_only(
    paths: Sequence[pathlib.Path],
    markdown_path: Optional[pathlib.Path],
    min_reportable_n: int,
    mcnemar_pairs: Sequence[Tuple[str, str]],
) -> int:
    """`--render-table`: re-render stored results JSON without running anything.

    This is how RESULTS.md is kept generated: point it at the committed
    `results/*.json` artifacts and paste the output. It validates each file
    against the table contract first, so a stored artifact that predates a
    column cannot be rendered as if it had one.
    """
    if not paths:
        raise SystemExit("--render-table needs at least one results JSON path")
    entries: List[Tuple[str, Dict[str, Any], Optional[str]]] = []
    with_run_label = len(paths) > 1
    for path in paths:
        if not path.exists():
            raise SystemExit(f"--render-table: no such file: {path}")
        data = load_results_file(path)
        summary = data["results"]
        if not isinstance(summary, dict) or not summary:
            raise SystemExit(f"{path}: 'results' is empty; nothing to render")
        validate_summary(summary, min_reportable_n)  # hard-fails on schema drift
        for arm_label, arm_summary in summary.items():
            entries.append((arm_label, arm_summary, path.stem if with_run_label else None))
        print(f"# {path}")
        for key, payload in (compute_significance(
                data.get("per_question") or {}, mcnemar_pairs) if mcnemar_pairs else {}).items():
            if payload.get("status") == "ok":
                print(f"  {key}: n={payload['n_pairs']} "
                      f"a_only={payload['arm_a_only_correct']} "
                      f"b_only={payload['arm_b_only_correct']} "
                      f"p={_fmt_p(payload['mcnemar_exact_two_sided_p'])} (exact two-sided McNemar)")
            else:
                print(f"  {key}: {payload.get('reason')}")

    print()
    render_console_table(entries)
    table = render_markdown_table(entries, min_reportable_n)
    print()
    print(table)
    if markdown_path is not None:
        markdown_path.parent.mkdir(parents=True, exist_ok=True)
        command = "python3 benchmarks/memconflict/run.py " + " ".join(sys.argv[1:])
        markdown_path.write_text(
            "<!-- wygenerowane przez: " + command + " -- nie edytuj tabeli ręcznie -->\n\n"
            + table + "\n"
        )
        print(f"\nmarkdown table written: {markdown_path}")
    return 0


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dataset", default=str(HERE / "data" / "Step4_4.jsonl"))
    ap.add_argument("--server-binary", "--binary", dest="server_binary",
                    default=os.environ.get(SERVER_BINARY_ENV) or DEFAULT_SERVER_BINARY,
                    help=f"path to vestige-mcp (default: ${SERVER_BINARY_ENV} or target/release/vestige-mcp)")
    ap.add_argument("--instances", type=int, default=1, help="number of simulated users to evaluate")
    ap.add_argument("--sessions", type=int, default=10, help="max sessions ingested per user")
    ap.add_argument("--top-k", type=int, default=5, help="retrieved memories per question (same for every arm)")
    ap.add_argument("--retrieval-mode", default="balanced", choices=["precise", "balanced", "exhaustive"],
                    help="search(retrieval_mode=...) for the vestige arm")
    ap.add_argument("--detail-level", default="summary", choices=["brief", "summary", "full"],
                    help="search(detail_level=...); 'brief' returns NO content and will score ~0")
    ap.add_argument("--min-similarity", type=float, default=0.0,
                    help="search(min_similarity=...); 0.0 disables the semantic floor so keyword-only "
                         "hits survive (server default is 0.5)")
    ap.add_argument("--min-retention", type=float, default=0.0,
                    help="search(min_retention=...)")
    ap.add_argument("--warmup", type=float, default=45.0,
                    help="warmup seconds after each server start (per simulated user); "
                         "embeddings are ready at initialize in v3.x, this covers the async reranker")
    ap.add_argument("--seed", type=int, default=1234)
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--data-dir", default=None,
                    help="base directory for per-instance stores (default: results/datadir-<stamp>/)")
    ap.add_argument("--keep-databases", action="store_true",
                    help="keep the per-instance stores after each simulated user finishes")
    ap.add_argument("--out", default=None, help="results JSON path")
    ap.add_argument("--no-contradiction-probe", action="store_true")
    ap.add_argument("--contradiction-depth", type=int, default=50,
                    help="memories deep_reference analyses for the CRS-struct probe (tool max is 50)")
    ap.add_argument("--mcnemar", default=DEFAULT_MCNEMAR_PAIRS, metavar="ARM:ARM[,ARM:ARM]",
                    help="paired exact McNemar tests over questions both arms answered "
                         f"(default: {DEFAULT_MCNEMAR_PAIRS}; 'none' disables). p-values are "
                         "written into the results JSON under 'significance'")
    ap.add_argument("--min-reportable-n", type=int, default=10, metavar="N",
                    help="warn (console, JSON 'power_analysis', markdown '*') whenever a column "
                         "is reported on fewer than N questions (default: 10)")
    ap.add_argument("--markdown-table", default=None, metavar="PATH",
                    help="also write the results table as markdown to PATH (generated from "
                         "TABLE_COLUMNS; RESULTS.md must not be transcribed by hand)")
    ap.add_argument("--render-table", default=None, metavar="JSON[,JSON]",
                    help="do not run anything: re-render the results table (and any requested "
                         "McNemar tests) from stored results JSON file(s)")
    args = ap.parse_args()

    validate_column_contract()

    selected = [a.strip() for a in args.arms.split(",") if a.strip()]
    for a in selected:
        if a not in ARMS:
            sys.exit(f"unknown arm {a!r}; choose from {ARMS}")
    mcnemar_pairs = parse_mcnemar_pairs(args.mcnemar)

    if args.render_table:
        return render_table_only(
            [pathlib.Path(p.strip()) for p in args.render_table.split(",") if p.strip()],
            pathlib.Path(args.markdown_table) if args.markdown_table else None,
            args.min_reportable_n,
            mcnemar_pairs,
        )

    dataset = pathlib.Path(args.dataset)
    if not dataset.exists():
        sys.exit(f"dataset not found at {dataset}\nRun: python3 {HERE / 'fetch_dataset.py'}")

    lock = json.loads((HERE / "DATASET.lock.json").read_text())
    started = time.time()
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime(started))
    out_path = pathlib.Path(args.out) if args.out else HERE / "results" / f"memconflict-{stamp}.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)

    exact_command = f"python3 {pathlib.Path(__file__).relative_to(REPO)} " + " ".join(sys.argv[1:])
    print("=" * 78)
    print("MemConflict retrieval benchmark (Vestige)")
    print("=" * 78)
    print(f"command : {exact_command}")
    print(f"dataset : {dataset.name} @ {lock['revision'][:12]}")
    print(f"arms    : {', '.join(selected)}")
    print(f"top_k   : {args.top_k}   sessions/user: {args.sessions}   users: {args.instances}")
    print()

    instances = load_instances(dataset, args.instances)
    print(f"loaded {len(instances)} instance(s)")

    client: Optional[VestigeMCP] = None
    arms: Dict[str, Any] = {}
    if "nomem" in selected:
        arms["nomem"] = NoMemArm()
    if "random" in selected:
        arms["random"] = RandomArm(args.seed)
    if "bm25" in selected:
        arms["bm25"] = BM25Arm()

    warmup_record: Dict[str, Any] = {}
    data_root = pathlib.Path(args.data_dir) if args.data_dir else HERE / "results" / f"datadir-{stamp}"
    try:
        if "vestige" in selected:
            binary = pathlib.Path(args.server_binary)
            if not binary.exists():
                sys.exit(
                    f"vestige-mcp binary not found at {binary}\n"
                    "Build it: cargo build --release -p vestige-mcp\n"
                    f"Or point at one with --server-binary / ${SERVER_BINARY_ENV}"
                )
            print(f"server  : {binary}")
            print(f"stores  : {data_root}/instance-<id>/vestige.db (one fresh store per user)")
            print(f"mode    : retrieval_mode={args.retrieval_mode} detail_level={args.detail_level} "
                  f"min_similarity={args.min_similarity} min_retention={args.min_retention}")
            arms["vestige"] = VestigeArm(
                str(binary.resolve()),
                data_root,
                args.retrieval_mode,
                args.detail_level,
                args.min_similarity,
                args.min_retention,
                args.warmup,
                not args.no_contradiction_probe,
                args.contradiction_depth,
                args.keep_databases,
            )

        records: Dict[str, List[Dict[str, Any]]] = {a: [] for a in selected}

        for inst_no, inst in enumerate(instances, 1):
            inst_id = inst.get("ID", f"inst{inst_no}")
            print(f"\n[{inst_no}/{len(instances)}] instance {inst_id}")
            for arm in arms.values():
                if isinstance(arm, VestigeArm):
                    print(f"  starting fresh vestige-mcp + initialize + {args.warmup:.0f}s warmup ...",
                          flush=True)
                arm.reset(inst_id)
            if isinstance(arms.get("vestige"), VestigeArm):
                warmup_record = arms["vestige"].warmups[-1]
                client = arms["vestige"].client
                print(f"  server: {client.server_info}")
                print(f"  warmup: {warmup_record}")

            sessions = list(iter_sessions(inst, args.sessions))
            total_units = 0
            asked = 0
            for s in sessions:
                units = session_units(s)
                total_units += len(units)
                for arm in arms.values():
                    arm.add(units)

                questions = s.get("Session_Questions") or []
                for q in questions:
                    qtext = q.get("question", "")
                    for name, arm in arms.items():
                        texts, extra = arm.retrieve(qtext, args.top_k)
                        answer = reader(texts)
                        metrics = judge_mod.score_question(q, answer)

                        if (name == "vestige" and metrics["conflict_type"] == "static_conflict"
                                and not args.no_contradiction_probe):
                            probe = arm.probe_contradictions(qtext, arm.contradiction_depth)
                            metrics["crs_struct"] = 1.0 if probe.get("found", 0) > 0 else 0.0
                            extra = {**extra, "contradiction_probe": probe}

                        records[name].append({
                            "instance": inst_id,
                            "session": s.get("Session_ID"),
                            "question_id": q.get("question_id"),
                            "n_retrieved": len(texts),
                            "answer_chars": len(answer),
                            "metrics": metrics,
                            **({"extra": extra} if extra else {}),
                        })
                    asked += 1
                print(f"  session {s.get('Session_ID'):>3}  units={len(units):>3}  "
                      f"cum_units={total_units:>4}  questions={len(questions)}", flush=True)
            print(f"  -> {asked} questions asked over {total_units} memory units")

        summary = {arm: aggregate(recs) for arm, recs in records.items() if recs}
        # Contract gate: a column whose JSON key is absent, or a results schema
        # that drifted, must fail loudly here instead of printing as a dash in a
        # published table (the 2026-09-19 swap went out that way).
        power_warnings = validate_summary(summary, args.min_reportable_n)
        significance = compute_significance(records, mcnemar_pairs) if mcnemar_pairs else {}

    finally:
        vestige_arm_for_close = arms.get("vestige")
        if isinstance(vestige_arm_for_close, VestigeArm):
            vestige_arm_for_close.close()
            vestige_arm_for_close.cleanup_root()
        elif client is not None:
            client.close()

    vestige_arm = arms.get("vestige")
    results = {
        "benchmark": "MemConflict",
        "harness_version": "1.1-vestige-port",
        "port_notes": "benchmarks/memconflict/PORTING-NOTES.md",
        "generated_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(started)),
        "wall_seconds": round(time.time() - started, 1),
        "exact_command": exact_command,
        "reproduce": [
            "cargo build --release -p vestige-mcp",
            "python3 benchmarks/memconflict/fetch_dataset.py",
            exact_command,
        ],
        "dataset": {
            "source": lock["repo"],
            "paper": lock["paper"],
            "revision": lock["revision"],
            "files": lock["files"],
            "instances_evaluated": len(instances),
            "sessions_per_instance_cap": args.sessions,
        },
        "vestige": {
            **git_rev(REPO),
            "server_info": client.server_info if client else None,
            "binary": str(pathlib.Path(args.server_binary).resolve()),
            "mcp_tools_used": ["smart_ingest", "search", "deep_reference"],
            "isolation": "one fresh database (--data-dir <db_dir>/vestige.db) + one server process per simulated user",
            "data_root": str(data_root),
        },
        "config": {
            "arms": selected,
            "top_k": args.top_k,
            "retrieval_mode": args.retrieval_mode,
            "detail_level": args.detail_level,
            "min_similarity": args.min_similarity,
            "min_retention": args.min_retention,
            "seed": args.seed,
            "contradiction_depth": args.contradiction_depth,
            "keep_databases": args.keep_databases,
            "reader": "deterministic concatenation of top-k retrieved memory texts",
            "judge": "rule-based port of upstream Evaluation/eval_scoring.py (no LLM)",
            "warmup": warmup_record,
        },
        "machine": machine_info(),
        "results": summary,
        "significance": significance,
        "power_analysis": {
            "min_reportable_n": args.min_reportable_n,
            "warnings": power_warnings,
            "method": ("every column is checked against the n of the stratum it is "
                       "computed on; the markdown table appends '*' to sub-threshold "
                       "columns and does not suppress them"),
        },
        "diagnostics": {
            "vestige_ingest_errors": vestige_arm.ingest_errors if vestige_arm else None,
            "vestige_retrieve_errors": vestige_arm.retrieve_errors if vestige_arm else None,
            "vestige_retrieval_latency_s": (
                {
                    "n": len(vestige_arm.latencies),
                    "mean": round(statistics.fmean(vestige_arm.latencies), 4),
                    "median": round(statistics.median(vestige_arm.latencies), 4),
                    "max": round(max(vestige_arm.latencies), 4),
                }
                if vestige_arm and vestige_arm.latencies else None
            ),
            "vestige_instance_warmups": vestige_arm.warmups if vestige_arm else None,
            "vestige_instance_stores": (
                (vestige_arm.instance_dirs if args.keep_databases else
                 f"{len(vestige_arm.instance_dirs)} per-instance stores under {data_root} (deleted after each user)")
                if vestige_arm else None
            ),
            "server_stderr_tail": client.stderr_tail(15) if client else None,
        },
        "caveats": [
            "Absolute scores are NOT comparable to arXiv:2605.20926 Table 3: that table used an LLM judge (gpt-5.0-mini) and a different reader; this harness uses a deterministic rule-based judge and a non-LLM reader.",
            "Only cross-arm differences within a single run are meaningful.",
            "crs_struct is measured with deep_reference (contradiction pairs among the top-N memories for the question), NOT with upstream's dedicated contradictions API -- that API does not exist on this server. It has no counterpart in the bm25/random/nomem arms, so it is a capability report for vestige only, never a head-to-head win.",
            "Simulated users are isolated by a fresh database + server process each, not by a scope namespace (the current tool surface has no scope argument). A retrieval can therefore never cross users, but every user also pays a server start + warmup.",
            "search() runs an 8-stage cognitive pipeline with read-path gating and competition suppression, so it may return fewer than top_k results (or none) for a question that BM25 answers; see n_retrieved_mean and the per-question 'extra.gated' field.",
            "Vestige stores each ingested unit as its own node (forceCreate=true), but preprocessing (coreference, temporal anchoring) may rewrite content, so retrieved text is not always byte-identical to the corpus the bm25/random arms index. Check reader_chars_mean before comparing answer accuracy across arms.",
            "Small subsets have wide error bars. Check per_conflict_type n before reading any difference as signal; macro_answer_accuracy weights a 3-question conflict type equally with an 88-question one. Every stratum below --min-reportable-n (default 10) is listed in 'power_analysis.warnings' and marked '*' in the markdown table.",
            "Single run, and only the paired tests requested with --mcnemar are reported: 'significance.<A>_vs_<B>' holds exact two-sided McNemar p-values over the questions both arms answered, on binarised answer_accuracy (>= 0.5). McNemar assumes independent pairs; questions are clustered inside simulated users, so the p-value is OPTIMISTIC -- read 'per_instance' next to it. No multiple-comparison correction.",
            "CauseBench is retracted and must never be cited.",
        ],
        "per_question": {arm: recs for arm, recs in records.items()},
    }
    out_path.write_text(json.dumps(results, indent=2))

    # ---- console report --------------------------------------------------
    print("\n" + "=" * 78)
    print("RESULTS  (higher is better; all arms share one reader and one judge)")
    print("=" * 78)
    # Per-type question counts are printed in the header so a small-n subset
    # can never be mistaken for a solid result.
    counts = {}
    for arm in selected:
        s_ = summary.get(arm)
        if s_:
            counts = {ct: v["n"] for ct, v in s_["per_conflict_type"].items()}
            break
    print(f"questions by conflict type: "
          f"dynamic={counts.get('dynamic_conflict', 0)}  "
          f"static={counts.get('static_conflict', 0)}  "
          f"conditional={counts.get('conditional_conflict', 0)}")
    small = [ct for ct, n in counts.items() if n < args.min_reportable_n]
    if small:
        print(f"WARNING small sample (n<{args.min_reportable_n}): {', '.join(small)} "
              f"-- macroAA weights these equally with large types; differences here are noise")
    print()
    # Rendered from TABLE_COLUMNS: the header and the values come from the same
    # declarative spec, so a column cannot be swapped on the way to the report.
    table_entries_data = [(arm, summary[arm], None) for arm in selected if arm in summary]
    render_console_table(table_entries_data)

    if power_warnings:
        print(f"\nPOWER WARNING (--min-reportable-n {args.min_reportable_n}):")
        for warning in power_warnings:
            print(f"  - {warning}")
        print("  These columns are printed (and marked '*' in the markdown table), not "
              "suppressed:\na reader must see them to know they are noise.")

    live = [(a, summary[a]["reader_chars_mean"]) for a in selected
            if a in summary and a != "nomem"]
    if len(live) >= 2:
        biggest = max(live, key=lambda x: x[1])
        smallest = min(live, key=lambda x: x[1])
        if smallest[1] > 0 and biggest[1] / smallest[1] >= 1.25:
            print(f"\nCONFOUND WARNING: '{biggest[0]}' hands the judge {biggest[1]:.0f} chars/question "
                  f"vs '{smallest[0]}' at {smallest[1]:.0f} "
                  f"({biggest[1]/smallest[1]:.2f}x).")
            print("  The judge awards partial credit for token overlap, so the larger blob has a")
            print("  structural advantage unrelated to retrieval quality. Treat the AA gap between")
            print("  these two arms as CONFOUNDED, not as a result.")

    vs = summary.get("vestige", {}).get("per_conflict_type", {}).get("static_conflict", {})
    if "crs_struct" in vs:
        print(f"\nvestige CRS-struct (deep_reference contradiction pairs, vestige-only): "
              f"{_fmt_number(vs['crs_struct'])}")
        print("  controls have no contradiction channel; this is a capability report, not a head-to-head win.")

    bm = summary.get("bm25", {})
    vm = summary.get("vestige", {})
    for label, key in (("macro AA", "macro_answer_accuracy"), ("micro AA", "micro_answer_accuracy")):
        b, v = bm.get(key), vm.get(key)
        if b is None or v is None:
            continue
        delta = v - b
        verdict = "BEATS" if delta > 0 else ("TIES" if abs(delta) < 1e-9 else "LOSES TO")
        print(f"\nvestige {verdict} bm25 on {label} by {delta:+.4f}  (vestige {v:.4f} vs bm25 {b:.4f})")
        if delta <= 0:
            print("  A memory system that cannot beat naive BM25 has not earned its complexity.")

    for key, payload in significance.items():
        if payload.get("status") != "ok":
            print(f"\npaired test {key}: skipped ({payload.get('reason')})")
            continue
        arm_a, arm_b = payload["arm_a"], payload["arm_b"]
        print(f"\npaired McNemar {arm_a} vs {arm_b} (exact, two-sided, on binarised AA >= 0.5):")
        print(f"  pairs={payload['n_pairs']}  {arm_a}-only-correct={payload['arm_a_only_correct']}  "
              f"{arm_b}-only-correct={payload['arm_b_only_correct']}  "
              f"delta(binarised)={payload['delta_binarised_accuracy']:+.4f}  "
              f"p={_fmt_p(payload['mcnemar_exact_two_sided_p'])}")
        if payload["discordant_pairs"] == 0:
            print("  No discordant pairs: the arms agree on every shared question (p=1 by construction).")
        else:
            print(f"  {'SIGNIFICANT' if payload['significant_at_0_05'] else 'NOT significant'} at 0.05.")
        print("  Caveat: questions are nested inside simulated users, so this p is optimistic; "
              "per-cluster:")
        for inst, block in payload["per_instance"].items():
            print(f"    instance {str(inst)[:8]}: n={block['n_pairs']} "
                  f"{arm_a}-only={block[f'{arm_a}_only_correct']} "
                  f"{arm_b}-only={block[f'{arm_b}_only_correct']} "
                  f"p={_fmt_p(block['mcnemar_exact_two_sided_p'])}")
        for ctype, block in payload["per_conflict_type"].items():
            print(f"    {ctype}: n={block['n_pairs']} "
                  f"{arm_a}-only={block[f'{arm_a}_only_correct']} "
                  f"{arm_b}-only={block[f'{arm_b}_only_correct']} "
                  f"p={_fmt_p(block['mcnemar_exact_two_sided_p'])}")

    if small:
        print("\n  NOTE: macro and micro disagree only because macroAA gives a small "
              "conflict type\n        the same weight as a large one. Report both, or neither.")

    if args.markdown_table:
        markdown_path = pathlib.Path(args.markdown_table)
        markdown_path.parent.mkdir(parents=True, exist_ok=True)
        markdown_path.write_text(
            "<!-- wygenerowane przez: " + exact_command + " -- nie edytuj tabeli ręcznie -->\n\n"
            + render_markdown_table(table_entries_data, args.min_reportable_n) + "\n"
        )
        print(f"\nmarkdown table written: {markdown_path}")

    print(f"\nresults written: {out_path}")
    print(f"reproduce with : {exact_command}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

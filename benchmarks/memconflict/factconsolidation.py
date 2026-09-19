#!/usr/bin/env python3
"""MemoryAgentBench FactConsolidation protocol for Vestige.

WHAT THIS MEASURES
------------------
FactConsolidation (MemoryAgentBench, arXiv:2507.05257) hands a system a long
list of facts in which the same (subject, relation) pair is stated more than
once with *different* values, then asks a question about that pair. Only the
statement that appears LAST is current, so the correct answer is the last
statement's value. Everything else in the haystack is noise the system has to
see past.

That makes this the cheapest available test of one specific competence:
deterministic freshness resolution. The harness never inspects how Vestige
resolves the conflict -- it ingests through the real MCP server, asks the real
question, and scores the retrieved memory text. If Vestige returns the older
statement's value, the reader cannot produce the current one, whatever the
internal mechanism.

WHAT IT IS NOT
--------------
Not a ranking and not comparable to the paper's published table:

  * the paper runs an LLM with the whole haystack in context; this harness runs
    retrieval and then a substring reader, so the numbers here are a floor, not
    a score for "Vestige";
  * published FactConsolidation numbers use an LLM judge path; this uses the
    official rule-based `substring_exact_match` only;
  * multi-hop (`factconsolidation_mh_*`) questions compose two facts: with an
    empty hop chain the ground truth is computed per fact group, and each
    scenario records whether the benchmark's answer actually IS the last
    statement's value (`ground_truth_is_current`). Read the run JSON: if the
    scenario set has no such scenarios, nothing was measured.

Determinism: fixed seed, no sampling unless a scenario cap is requested, fresh
store per server, ingest strictly in document order (document order IS the
protocol -- a hash-ordered ingest would destroy the thing being measured).

Usage:
    python3 factconsolidation.py --help
    python3 factconsolidation.py --source factconsolidation_sh_6k \\
        --scenarios 20 --scenarios-per-store 10 --warmup 10
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import pathlib
import platform
import random
import shutil
import subprocess
import sys
import time
from typing import Any, Dict, List, Optional, Sequence, Tuple

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import factconsolidation_data as fcd  # noqa: E402
from mcp_client import VestigeMCP, VestigeMCPError, mtime_utc  # noqa: E402

DEFAULT_JSONL = HERE / "data" / "factconsolidation.jsonl"
LOCK = HERE / "FACTCONSOLIDATION.lock.json"
DEFAULT_BINARY = "target/release/vestige-mcp"
INGEST_BATCH = 20  # smart_ingest batch cap, same as run.py


# --------------------------------------------------------------------------
# provenance
# --------------------------------------------------------------------------

def git_rev(repo: pathlib.Path) -> Dict[str, Any]:
    """Same shape as run.py's helper, so results files read alike."""
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


def dataset_provenance(jsonl_path: pathlib.Path) -> Dict[str, Any]:
    lock = json.loads(LOCK.read_text())
    rel, meta = next(iter(lock["files"].items()))
    return {
        "revision": lock["revision"],
        "url": meta["url"],
        "sha256": meta["sha256"],
        "bytes": meta["bytes"],
        "rows": meta["rows"],
        "jsonl": str(jsonl_path),
        "jsonl_sha256": fcd_sha256(jsonl_path) if jsonl_path.exists() else None,
        "metric": "substring_exact_match (official calculate_metrics)",
    }


def fcd_sha256(path: pathlib.Path) -> Optional[str]:
    import hashlib
    try:
        h = hashlib.sha256()
        with path.open("rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()
    except OSError:
        return None


# --------------------------------------------------------------------------
# ingest planning
# --------------------------------------------------------------------------

def plan_ingest(scenarios: Sequence[Dict[str, Any]], mode: str
                ) -> List[Dict[str, Any]]:
    """Return the statements to ingest, in document (serial) order.

    `all` replays the entire haystack, which is what the benchmark does and the
    only mode whose numbers mean anything next to the paper: retrieval has to
    find the right statements among thousands of distractors. `conflicts`
    ingests only the statements of the scenario groups themselves -- a
    plumbing check with essentially no retrieval difficulty, useful for
    separating "the server cannot resolve freshness" from "the server never
    found the two statements".

    Either way the order is the dataset's own numbering, never sorted by
    anything else: the last statement is current *because* it is last.
    """
    by_serial: Dict[int, Dict[str, Any]] = {}
    for sc in scenarios:
        for st in (sc["statements"] if mode == "conflicts" else sc.get("corpus") or []):
            by_serial.setdefault(st["serial"], st)
    return [by_serial[k] for k in sorted(by_serial)]


def corpus_facts(facts: Sequence[fcd.Fact]) -> List[Dict[str, Any]]:
    return [f.to_json() for f in facts]


# --------------------------------------------------------------------------
# one store = one fresh server + one ingest order + N questions
# --------------------------------------------------------------------------

class StoreRun:
    """Drives one fresh Vestige store through ingest and questions."""

    def __init__(self, binary: str, data_root: pathlib.Path, warmup: float,
                 top_k: int, detail_level: str, min_similarity: float,
                 min_retention: float, retrieval_mode: str,
                 keep_databases: bool) -> None:
        self.binary = binary
        self.data_root = data_root
        self.warmup = warmup
        self.top_k = top_k
        self.detail_level = detail_level
        self.min_similarity = min_similarity
        self.min_retention = min_retention
        self.retrieval_mode = retrieval_mode
        self.keep_databases = keep_databases
        self.client: Optional[VestigeMCP] = None
        self.db_dir: Optional[pathlib.Path] = None
        self.ingest_errors: List[str] = []
        self.retrieve_errors: List[str] = []
        self.provenance: Dict[str, Any] = {}

    def __enter__(self) -> "StoreRun":
        self.db_dir = self.data_root / f"store-{int(time.time() * 1000)}"
        self.db_dir.mkdir(parents=True, exist_ok=True)
        self.client = VestigeMCP(self.binary, str(self.db_dir),
                                 warmup_seconds=self.warmup)
        self.client.start()
        self.client.initialize()
        self.provenance = self.client.launch_provenance()
        self.provenance["observed_warmup"] = dict(self.client.observed_warmup)
        return self

    def __exit__(self, *exc) -> None:
        if self.client is not None:
            self.client.close()
            self.client = None
        if self.db_dir is not None and not self.keep_databases:
            shutil.rmtree(self.db_dir, ignore_errors=True)

    def ingest(self, statements: Sequence[Dict[str, Any]]) -> int:
        """Ingest in the given order. Returns the number of units sent.

        Batches preserve order; `smart_ingest` stores each item with its own
        creation time, and the server's ordering by creation time is what
        makes the last statement current. `forceCreate` is used for the same
        reason `run.py` uses it: a near-duplicate must not be merged away,
        because the pair of statements is the experiment.
        """
        assert self.client is not None
        sent = 0
        for start in range(0, len(statements), INGEST_BATCH):
            chunk = statements[start:start + INGEST_BATCH]
            items = [{"content": st["text"], "node_type": "fact",
                      "forceCreate": True} for st in chunk]
            try:
                self.client.call_tool("smart_ingest", {"items": items},
                                      timeout=600.0)
                sent += len(items)
            except VestigeMCPError as exc:
                self.ingest_errors.append(f"batch@{start}: {str(exc)[:200]}")
        return sent

    def ask(self, question: str) -> Tuple[List[str], Dict[str, Any]]:
        assert self.client is not None
        extra: Dict[str, Any] = {}
        t0 = time.monotonic()
        try:
            payload = self.client.call_tool("search", {
                "query": question,
                "limit": self.top_k,
                "detail_level": self.detail_level,
                "min_similarity": self.min_similarity,
                "min_retention": self.min_retention,
                "retrieval_mode": self.retrieval_mode,
            }, timeout=300.0)
        except VestigeMCPError as exc:
            self.retrieve_errors.append(str(exc)[:200])
            return [], {"error": str(exc)[:200]}
        extra["latency_s"] = round(time.monotonic() - t0, 3)
        texts = texts_from_search(payload)[: self.top_k]
        if isinstance(payload, dict):
            if payload.get("gated"):
                extra["gated"] = payload.get("reason")
            if payload.get("total") is not None:
                extra["total"] = payload["total"]
        return texts, extra


def texts_from_search(payload: Any) -> List[str]:
    """Pull stored memory CONTENT out of a `search` payload.

    Same discipline as `run.py::VestigeArm._texts_from_search`: only stored
    memory text is read, never the tool's own framing or field names, so the
    reader cannot be fed a canned string the server always emits.
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


# --------------------------------------------------------------------------
# scoring
# --------------------------------------------------------------------------

def score_scenario(scenario: Dict[str, Any], texts: Sequence[str],
                   extra: Dict[str, Any]) -> Dict[str, Any]:
    """Score one scenario with the official rule-based metric.

    `correct` is `substring_exact_match(answer, ground_truth)` over the
    concatenation of retrieved memories -- exactly what the official harness
    computes over a model reply, with retrieval standing in for generation.

    Two extra signals are recorded because "wrong" and "did not retrieve the
    conflict at all" are different failures and the review has already had to
    correct one harness that conflated them:

      * `retrieved_any_statement`: did any returned text match a statement in
        this scenario's group at all? False means the reader never saw the
        conflict, so a wrong answer says nothing about freshness resolution.
      * `superseded_value_seen`: was an older value retrieved? Combined with
        `correct`, this is what separates "returned the stale value" (the
        failure mode the recommendation targets) from "returned neither".
    """
    blob = "\n".join(texts)
    correct = fcd.substring_exact_match(blob, scenario["gold"])
    current_seen = fcd.substring_exact_match(blob, scenario["current_value"])
    seen_serials = []
    for st in scenario["statements"]:
        if fcd.substring_exact_match(blob, st["text"]):
            seen_serials.append(st["serial"])
    superseded_serials = set(scenario["superseded_serials"])
    # Top-1 diagnostics. `substring_exact_match` over the whole blob is the
    # official metric, but it cannot see ORDER: it credits "the current value is
    # somewhere in the text" the same as "the current value is the first thing
    # returned". The two come apart badly here (answer accuracy is high while
    # top-1 accuracy is not), and only the first result matters to a caller that
    # reads one memory, so both are recorded.
    top1 = texts[0] if texts else ""
    top1_hits = [st["serial"] for st in scenario["statements"]
                 if fcd.substring_exact_match(top1, st["text"])]
    top1_is_current = scenario["current_serial"] in top1_hits
    top1_is_superseded = any(s in superseded_serials for s in top1_hits)
    return {
        "question": scenario["question"],
        "key": scenario["key"],
        "gold": scenario["gold"],
        "current_serial": scenario["current_serial"],
        "current_value": scenario["current_value"],
        "superseded_serials": scenario["superseded_serials"],
        "ground_truth_is_current": scenario["ground_truth_is_current"],
        "n_retrieved": len(texts),
        "reader_chars": len(blob),
        "correct": correct,
        "current_value_retrieved": current_seen,
        "superseded_value_retrieved": any(s in superseded_serials for s in seen_serials),
        "retrieved_any_statement": bool(seen_serials),
        "retrieved_serials": seen_serials,
        "top1_correct": bool(texts) and fcd.substring_exact_match(top1, scenario["gold"]),
        "top1_is_current": top1_is_current,
        "top1_is_superseded": top1_is_superseded and not top1_is_current,
        "both_statements_retrieved": current_seen and superseded_serials.issubset(set(seen_serials)),
        **extra,
    }


def aggregate(records: Sequence[Dict[str, Any]]) -> Dict[str, Any]:
    n = len(records)
    if n == 0:
        return {"n": 0}

    def frac(pred) -> Optional[float]:
        return round(sum(1 for r in records if pred(r)) / n, 4)

    measurable = [r for r in records if r["ground_truth_is_current"]]
    accs = [r["reader_chars"] for r in records]
    lats = [r["latency_s"] for r in records if "latency_s" in r]
    return {
        "n_scenarios": n,
        "n_with_current_ground_truth": len(measurable),
        "answer_accuracy": frac(lambda r: r["correct"]),
        "answer_accuracy_measurable": (
            round(sum(1 for r in measurable if r["correct"]) / len(measurable), 4)
            if measurable else None
        ),
        "top1_accuracy": frac(lambda r: r["top1_correct"]),
        "top1_is_current": frac(lambda r: r["top1_is_current"]),
        "top1_is_superseded": frac(lambda r: r["top1_is_superseded"]),
        "both_statements_retrieved": frac(lambda r: r["both_statements_retrieved"]),
        "retrieved_current_value": frac(lambda r: r["current_value_retrieved"]),
        "retrieved_superseded_value": frac(lambda r: r["superseded_value_retrieved"]),
        "retrieved_any_statement": frac(lambda r: r["retrieved_any_statement"]),
        "empty_retrieval": frac(lambda r: r["n_retrieved"] == 0),
        "n_retrieved_mean": round(sum(r["n_retrieved"] for r in records) / n, 2),
        "reader_chars_mean": round(sum(accs) / n, 1),
        "latency_s_mean": round(sum(lats) / len(lats), 3) if lats else None,
    }


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

def scenario_counts(scenarios: Sequence[Dict[str, Any]]) -> Dict[str, int]:
    subjects = [s["key"]["subject"] for s in scenarios]
    return {
        "n_scenarios": len(scenarios),
        "n_current_ground_truth": sum(1 for s in scenarios if s["ground_truth_is_current"]),
        "n_superseded": sum(1 for s in scenarios if s["superseded_serials"]),
        # Sharing one store between scenarios is only sound when their subjects
        # are disjoint; otherwise one scenario's statements are distractors for
        # another's question by construction. Measured and recorded, not assumed.
        "n_distinct_subjects": len(set(subjects)),
        "subjects_disjoint": len(set(subjects)) == len(subjects),
    }


def describe(source: str, scenarios: Sequence[Dict[str, Any]],
             refused: Sequence[str], facts: Sequence[fcd.Fact]) -> None:
    """Print what would be measured, without starting a server.

    `--describe` exists so the coverage question ("did this actually build
    conflict scenarios, or did it refuse everything?") is answerable before
    burning an hour of ingest and not only after.
    """
    import collections
    counts = collections.Counter(refused)
    print(f"source: {source}")
    print(f"  facts parsed:        {len(facts)}")
    print(f"  scenarios built:     {len(scenarios)}")
    print(f"  ground truth == last statement: "
          f"{sum(1 for s in scenarios if s['ground_truth_is_current'])}")
    print(f"  questions refused:   {len(refused)}")
    for reason, n in counts.most_common():
        print(f"      {n:4d}  {reason}")
    sizes = collections.Counter(len(s["statements"]) for s in scenarios)
    print(f"  statements per scenario: {dict(sorted(sizes.items()))}")
    if scenarios:
        total = sum(len(s["statements"]) for s in scenarios)
        print(f"  statements in union: {total}")


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dataset", default=str(DEFAULT_JSONL),
                    help="JSONL produced by fetch_factconsolidation.py")
    ap.add_argument("--source", default="factconsolidation_sh_6k",
                    help="sub-dataset name (default: the smallest single-hop set)")
    ap.add_argument("--server-binary", "--binary", dest="server_binary",
                    default=None, help="vestige-mcp path "
                    "(default $VESTIGE_MCP_BINARY or target/release/vestige-mcp)")
    ap.add_argument("--scenarios", type=int, default=None,
                    help="cap the number of scenarios (default: all that qualify)")
    ap.add_argument("--scenarios-per-store", type=int, default=10,
                    help="scenarios answered per server process (default 10)")
    ap.add_argument("--ingest", choices=("all", "conflicts"), default="all",
                    help="'all' replays the whole haystack (the benchmark); "
                         "'conflicts' ingests only the conflict statements "
                         "(plumbing check, no retrieval difficulty)")
    ap.add_argument("--top-k", type=int, default=5)
    ap.add_argument("--retrieval-mode", default="balanced",
                    choices=["precise", "balanced", "exhaustive"])
    ap.add_argument("--detail-level", default="summary",
                    choices=["brief", "summary", "full"])
    ap.add_argument("--min-similarity", type=float, default=0.0)
    ap.add_argument("--min-retention", type=float, default=0.0)
    ap.add_argument("--warmup", type=float, default=45.0)
    ap.add_argument("--seed", type=int, default=1234,
                    help="recorded in results; used only if a cap forces sampling")
    ap.add_argument("--data-dir", default=None,
                    help="base dir for per-store databases (default results/datadir-<stamp>/)")
    ap.add_argument("--keep-databases", action="store_true")
    ap.add_argument("--out", default=None, help="results JSON path")
    ap.add_argument("--describe", action="store_true",
                    help="print scenario coverage and exit without running")
    args = ap.parse_args()

    binary = args.server_binary or os.environ.get("VESTIGE_MCP_BINARY") or DEFAULT_BINARY
    binary_path = pathlib.Path(binary)
    if not args.describe and not binary_path.exists():
        raise SystemExit(
            f"server binary not found: {binary_path}\n"
            "build it with:  cargo build --release -p vestige-mcp\n"
            "or pass --server-binary / set $VESTIGE_MCP_BINARY"
        )

    jsonl = pathlib.Path(args.dataset)
    if not jsonl.exists():
        raise SystemExit(
            f"dataset not found: {jsonl}\n"
            "fetch it with:  python3 benchmarks/memconflict/fetch_factconsolidation.py"
        )
    rows = fcd.read_jsonl_rows(str(jsonl))
    by_source = fcd.group_source_rows(rows)
    if args.source not in by_source:
        raise SystemExit(
            f"source {args.source!r} not in {jsonl}; available: "
            + ", ".join(sorted(by_source))
        )
    row = by_source[args.source]

    facts, unparsed = fcd.parse_facts(row["context"])
    scenarios, refused = fcd.build_scenarios(facts, row["questions"], row["answers"])
    corpus = corpus_facts(facts)
    for sc in scenarios:
        sc["corpus"] = corpus

    if args.scenarios is not None and args.scenarios < len(scenarios):
        rng = random.Random(args.seed)
        scenarios = rng.sample(scenarios, args.scenarios)
        scenarios.sort(key=lambda s: s["current_serial"])

    if args.describe:
        describe(args.source, scenarios, refused, facts)
        return 0

    if not scenarios:
        raise SystemExit(
            f"no conflict scenarios could be built for {args.source}. "
            "Nothing was measured -- this is a refusal to report, not a zero score."
        )

    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    data_root = pathlib.Path(args.data_dir) if args.data_dir else (
        HERE / "results" / f"datadir-fc-{stamp}")
    out_path = pathlib.Path(args.out) if args.out else (
        HERE / "results" / f"factconsolidation-{stamp}.json")

    print(f"source            {args.source}")
    print(f"scenarios         {len(scenarios)}")
    print(f"ingest mode       {args.ingest}")
    print(f"server binary     {binary_path}")
    print(f"data root         {data_root}")
    print(f"results           {out_path}")
    print()

    started = time.monotonic()
    records: List[Dict[str, Any]] = []
    store_provenance: List[Dict[str, Any]] = []
    total_ingested = 0
    total_ingest_errors = 0
    total_retrieve_errors = 0

    groups = [scenarios[i:i + max(1, args.scenarios_per_store)]
              for i in range(0, len(scenarios), max(1, args.scenarios_per_store))]
    for gi, group in enumerate(groups):
        statements = plan_ingest(group, args.ingest)
        t0 = time.monotonic()
        with StoreRun(binary, data_root, args.warmup, args.top_k,
                      args.detail_level, args.min_similarity, args.min_retention,
                      args.retrieval_mode, args.keep_databases) as store:
            ingested = store.ingest(statements)
            total_ingested += ingested
            total_ingest_errors += len(store.ingest_errors)
            ingest_s = time.monotonic() - t0
            print(f"store {gi + 1}/{len(groups)}: ingested {ingested} statements "
                  f"in {ingest_s:.1f}s", flush=True)
            for sc in group:
                texts, extra = store.ask(sc["question"])
                records.append(score_scenario(sc, texts, extra))
            total_retrieve_errors += len(store.retrieve_errors)
            store_provenance.append({
                "store_index": gi,
                "statements_ingested": ingested,
                "ingest_seconds": round(ingest_s, 2),
                "ingest_errors": store.ingest_errors,
                "retrieve_errors": store.retrieve_errors,
                **store.provenance,
            })

    summary = aggregate(records)
    elapsed = time.monotonic() - started
    payload = {
        "harness": "factconsolidation",
        "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(
            timespec="seconds"),
        "protocol": {
            "benchmark": "MemoryAgentBench FactConsolidation",
            "paper": "arXiv:2507.05257",
            "source": args.source,
            "metric": "substring_exact_match over concatenated retrieved memories",
            "ground_truth_rule": "value of the LAST statement for the question's "
                                 "(subject, relation) key",
            "ingest_mode": args.ingest,
            "ingest_order": "dataset document order (the serial numbering)",
            "top_k": args.top_k,
            "retrieval_mode": args.retrieval_mode,
            "detail_level": args.detail_level,
            "min_similarity": args.min_similarity,
            "min_retention": args.min_retention,
            "warmup_seconds": args.warmup,
            "scenarios_per_store": args.scenarios_per_store,
            "seed": args.seed,
            "statement_count": len(corpus),
            "statements_ingested": total_ingested,
        },
        "scenario_coverage": {
            **scenario_counts(scenarios),
            "refused": len(refused),
            "refusal_reasons": _reason_counts(refused),
            "unparsed_fact_sentences": len(unparsed),
        },
        "provenance": {
            "git": git_rev(HERE.parent.parent),
            "binary": str(binary_path),
            "binary_mtime_utc": mtime_utc(str(binary_path)),
            "python": sys.version.split()[0],
            "platform": platform.platform(),
            "machine": platform.machine(),
            "harness_argv": list(sys.argv),
            "env": {
                k: v for k, v in os.environ.items() if k.startswith("VESTIGE_")
            },
            "dataset": dataset_provenance(jsonl),
        },
        "stores": store_provenance,
        "summary": summary,
        "per_scenario": records,
        "errors": {
            "ingest": total_ingest_errors,
            "retrieve": total_retrieve_errors,
        },
        "runtime_seconds": round(elapsed, 1),
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")
    if data_root.exists() and not args.keep_databases:
        shutil.rmtree(data_root, ignore_errors=True)

    print()
    print(f"scenarios           {summary['n_scenarios']}")
    print(f"with current GT     {summary['n_with_current_ground_truth']}")
    print(f"answer accuracy     {summary['answer_accuracy']}")
    print(f"current value seen  {summary['retrieved_current_value']}")
    print(f"superseded seen     {summary['retrieved_superseded_value']}")
    print(f"any statement seen  {summary['retrieved_any_statement']}")
    print(f"top1 is current     {summary['top1_is_current']}")
    print(f"top1 is superseded  {summary['top1_is_superseded']}")
    print(f"both retrieved      {summary['both_statements_retrieved']}")
    print(f"empty retrieval     {summary['empty_retrieval']}")
    print(f"ingest / retrieve errors  {total_ingest_errors} / {total_retrieve_errors}")
    print(f"runtime             {elapsed:.1f}s")
    print(f"\nresults written to {out_path}")
    return 0


def _reason_counts(reasons: Sequence[str]) -> Dict[str, int]:
    import collections
    return dict(collections.Counter(reasons))


if __name__ == "__main__":
    sys.exit(main())

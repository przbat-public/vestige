#!/usr/bin/env python3
"""LongMemEval_S sanity check. NOT a headline benchmark.

Purpose: confirm the retrieval plumbing behaves sensibly on a second, entirely
independent dataset. It exists to catch a harness that is silently broken -- a
retriever that returns nothing, an ingest path that drops content, an embedding
service that never warmed up. It is NOT evidence of end-to-end task quality and
its numbers must never be quoted as a LongMemEval score.

Metric: `evidence_recall@k` -- does the concatenation of the top-k retrieved
memories contain the gold answer string (normalised)? This is deliberately
reader-independent and unambiguous. It asks one question: did retrieval put the
answer in front of the reader? A real LongMemEval score additionally requires a
reader/judge and would be a different, much larger claim.

    python3 benchmarks/memconflict/longmemeval.py --questions 3

PORTED HARNESS: the vestige arm calls `search` and gets a fresh database +
server process per question (the current tool surface has no scope argument).
See PORTING-NOTES.md.

IMPORTANT: the original `xiaowu0162/longmemeval` dataset is DEPRECATED upstream
and replaced by `longmemeval-cleaned`, which removes noisy history sessions that
interfere with answer correctness. This harness pins the CLEANED release. A
LongMemEval number computed against the deprecated original is invalid.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import random as _random
import shutil
import statistics
import sys
import time
import urllib.request
from typing import Any, Dict, List

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE))

import bm25 as bm25_mod  # noqa: E402
import judge as judge_mod  # noqa: E402
from mcp_client import VestigeMCP, VestigeMCPError  # noqa: E402

LOCK = HERE / "LONGMEMEVAL.lock.json"
DATA_DIR = HERE / "data"

#: Default server binary location (same convention as run.py).
DEFAULT_SERVER_BINARY = str(REPO / "target" / "release" / "vestige-mcp")
SERVER_BINARY_ENV = "VESTIGE_MCP_BINARY"


def fetch() -> pathlib.Path:
    lock = json.loads(LOCK.read_text())
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    name, meta = next(iter(lock["files"].items()))
    dest = DATA_DIR / name
    if not dest.exists():
        print(f"downloading {meta['url']}\n         -> {dest}", flush=True)
        urllib.request.urlretrieve(meta["url"], dest)
    h = hashlib.sha256()
    with dest.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    if h.hexdigest() != meta["sha256"]:
        sys.exit(
            f"FAIL: hash mismatch for {dest}\n  expected {meta['sha256']}\n"
            f"  actual   {h.hexdigest()}\nRefusing to run on unverified data."
        )
    print(f"OK  {dest.name}  sha256={h.hexdigest()[:16]}...  bytes={dest.stat().st_size}")
    return dest


def session_units(sessions: List[Any], dates: List[str]) -> List[str]:
    """User turns only, date-stamped -- same convention as the MemConflict harness."""
    units: List[str] = []
    for idx, session in enumerate(sessions):
        date = dates[idx] if idx < len(dates) else ""
        for turn in session or []:
            if not isinstance(turn, dict) or turn.get("role") != "user":
                continue
            text = (turn.get("content") or "").strip()
            if text:
                units.append(f"[{date}] {text}")
    return units


def evidence_recall(gold: str, retrieved: List[str]) -> float:
    g = judge_mod.normalize_text(gold)
    if not g:
        return 0.0
    blob = judge_mod.normalize_text("\n".join(retrieved))
    return 1.0 if g and g in blob else 0.0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--questions", type=int, default=3, help="questions to evaluate (500 available)")
    ap.add_argument("--top-k", type=int, default=5)
    ap.add_argument("--warmup", type=float, default=45.0,
                    help="warmup seconds after each server start (one fresh server + store per question)")
    ap.add_argument("--seed", type=int, default=1234)
    ap.add_argument("--server-binary", "--binary", dest="server_binary",
                    default=os.environ.get(SERVER_BINARY_ENV) or DEFAULT_SERVER_BINARY,
                    help=f"path to vestige-mcp (default: ${SERVER_BINARY_ENV} or target/release/vestige-mcp)")
    ap.add_argument("--retrieval-mode", default="balanced", choices=["precise", "balanced", "exhaustive"])
    ap.add_argument("--keep-databases", action="store_true",
                    help="keep the per-question stores after each question finishes")
    ap.add_argument("--arms", default="nomem,random,bm25,vestige")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    selected = [a.strip() for a in args.arms.split(",") if a.strip()]
    path = fetch()
    print("loading dataset (277MB, this takes a moment) ...", flush=True)
    data = json.loads(path.read_text())
    items = data[: args.questions]
    started = time.time()
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime(started))
    exact_command = f"python3 {pathlib.Path(__file__).relative_to(REPO)} " + " ".join(sys.argv[1:])

    print("=" * 78)
    print("LongMemEval_S SANITY CHECK  (not a headline benchmark)")
    print("=" * 78)
    print(f"command: {exact_command}")
    print(f"metric : evidence_recall@{args.top_k} -- is the gold answer inside the retrieved text?")
    print(f"items  : {len(items)} of {len(data)}\n")

    client = None
    data_root = HERE / "results" / f"lme-datadir-{stamp}"
    scores: Dict[str, List[float]] = {a: [] for a in selected}
    try:
        if "vestige" in selected:
            binary = pathlib.Path(args.server_binary)
            if not binary.exists():
                sys.exit(
                    f"vestige-mcp binary not found at {binary}\n"
                    "Build it: cargo build --release -p vestige-mcp\n"
                    f"Or point at one with --server-binary / ${SERVER_BINARY_ENV}"
                )

        for qi, item in enumerate(items, 1):
            units = session_units(item.get("haystack_sessions") or [], item.get("haystack_dates") or [])
            question, gold = item.get("question", ""), item.get("answer", "")
            print(f"[{qi}/{len(items)}] {item.get('question_type')} :: {question[:60]}")
            print(f"   units={len(units)} gold={gold[:50]!r}", flush=True)

            rng = _random.Random(f"{args.seed}:{item.get('question_id')}")
            qid = "".join(c if c.isalnum() or c in "-_" else "_" for c in str(item.get("question_id", "q")))[:48]
            db_dir = data_root / f"q-{qid}"

            # One fresh store + server per question: there is no scope argument
            # on the current tool surface, so this is how questions are kept
            # from bleeding into each other.
            if "vestige" in selected:
                client = VestigeMCP(str(pathlib.Path(args.server_binary).resolve()), str(db_dir),
                                    warmup_seconds=args.warmup)
                client.start()
                print(f"   server + {args.warmup:.0f}s warmup ...", flush=True)
                client.initialize()
                for start in range(0, len(units), 20):
                    batch = [{"content": u, "node_type": "event", "forceCreate": True}
                             for u in units[start:start + 20]]
                    try:
                        client.call_tool("smart_ingest", {"items": batch}, timeout=600)
                    except VestigeMCPError as exc:
                        print(f"   ingest error: {exc}"[:160])

            for arm in selected:
                if arm == "nomem":
                    got: List[str] = []
                elif arm == "random":
                    got = rng.sample(units, min(args.top_k, len(units))) if units else []
                elif arm == "bm25":
                    idx = bm25_mod.BM25(units)
                    got = [units[i] for i, _ in idx.top_k(question, args.top_k)]
                else:
                    try:
                        payload = client.call_tool(
                            "search",
                            {"query": question, "limit": args.top_k,
                             "detail_level": "summary", "min_similarity": 0.0,
                             "min_retention": 0.0, "retrieval_mode": args.retrieval_mode},
                            timeout=300)
                        got = [r.get("content", "") for r in (payload.get("results") or [])][: args.top_k]
                    except VestigeMCPError as exc:
                        print(f"   search error: {exc}"[:160])
                        got = []
                s = evidence_recall(gold, got)
                scores[arm].append(s)
                print(f"     {arm:<8} evidence_recall={s:.0f}")

                if arm == "vestige" and client is not None:
                    client.close()
                    client = None
                    if not args.keep_databases:
                        shutil.rmtree(db_dir, ignore_errors=True)
    finally:
        if client:
            client.close()

    print("\n" + "=" * 78)
    print(f"evidence_recall@{args.top_k}  (n={len(items)} -- far too few to be a benchmark result)")
    print("=" * 78)
    summary = {}
    for arm in selected:
        v = statistics.fmean(scores[arm]) if scores[arm] else 0.0
        summary[arm] = round(v, 4)
        print(f"  {arm:<9}{v:.4f}")

    out = pathlib.Path(args.out) if args.out else HERE / "results" / f"longmemeval-{stamp}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({
        "benchmark": "LongMemEval_S (cleaned) -- SANITY CHECK ONLY",
        "harness_version": "1.1-vestige-port",
        "port_notes": "benchmarks/memconflict/PORTING-NOTES.md",
        "exact_command": exact_command,
        "dataset": json.loads(LOCK.read_text()),
        "questions_evaluated": len(items),
        "questions_available": len(data),
        "metric": f"evidence_recall@{args.top_k}",
        "vestige": {
            "binary": str(pathlib.Path(args.server_binary).resolve()),
            "retrieval_mode": args.retrieval_mode,
            "isolation": "one fresh database + server process per question (no scope argument on this tool surface)",
            "data_root": str(data_root),
        },
        "results": summary,
        "per_question": {a: scores[a] for a in selected},
        "caveats": [
            "SANITY CHECK ONLY. Not a LongMemEval score and must never be quoted as one.",
            "evidence_recall only asks whether the gold answer string appears in the retrieved text. It involves no reader and no judge.",
            "A previous Vestige LongMemEval run was invalidated and must not be cited.",
            "The original longmemeval dataset is deprecated upstream; this pins longmemeval-cleaned.",
            "Ported to the current tool surface: search() + fresh store per question instead of recall(scope=...). See PORTING-NOTES.md.",
        ],
    }, indent=2))
    print(f"\nwritten: {out}")
    print(f"reproduce: {exact_command}")
    print("\nSANITY CHECK ONLY -- do not quote these as LongMemEval scores.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

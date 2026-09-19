#!/usr/bin/env python3
"""Fetch the pinned MemoryAgentBench FactConsolidation dataset and verify it.

The dataset is NOT vendored into this repository (same policy as
`fetch_dataset.py`): it is downloaded from the HuggingFace revision pinned in
`FACTCONSOLIDATION.lock.json`, checked against a SHA-256, and only then
converted into the JSONL the benchmark reads.

WHY A CONVERSION STEP
---------------------
The canonical distribution is a parquet file. `factconsolidation.py` is
standard library only, and a hand-rolled parquet decoder is exactly the kind of
component that fails silently, so this script -- which runs once, interactively
-- uses pyarrow and writes a plain JSONL. Every benchmark run after that reads
JSONL with no third-party dependency at all.

pyarrow is therefore an *optional* dependency of the harness: needed to fetch,
never needed to run. There is no requirements.txt; install it with
`pip install pyarrow` if the import below fails. That deviation from the
harness's usual "standard library only" rule is recorded in PORTING-NOTES.md.

Usage:
    python3 fetch_factconsolidation.py              # fetch + verify + convert
    python3 fetch_factconsolidation.py --verify-only  # re-verify an existing copy
    python3 fetch_factconsolidation.py --repin      # print the current live digest
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
LOCK = HERE / "FACTCONSOLIDATION.lock.json"
DATA_DIR = HERE / "data"
JSONL_NAME = "factconsolidation.jsonl"


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_lock() -> dict:
    return json.loads(LOCK.read_text())


def verify(path: pathlib.Path, meta: dict) -> str:
    actual = sha256_file(path)
    if actual != meta["sha256"]:
        sys.exit(
            f"FAIL: hash mismatch for {path}\n"
            f"  expected {meta['sha256']}\n"
            f"  actual   {actual}\n"
            "Refusing to benchmark against unverified data."
        )
    size = path.stat().st_size
    if size != meta["bytes"]:
        sys.exit(f"FAIL: size mismatch for {path}: {size} != {meta['bytes']}")
    return actual


def convert(parquet_path: pathlib.Path, jsonl_path: pathlib.Path,
            expected_rows: int, expected_questions: int) -> dict:
    """Convert the pinned parquet to JSONL and sanity-check its shape.

    The checks are the point of this function, not the conversion: the run path
    validates the JSONL against the same numbers on every start, so a
    half-written or silently truncated extraction cannot be benchmarked. A
    harness that reports 0% because its dataset was empty is the exact failure
    this repository already had to correct once.
    """
    try:
        import pyarrow.parquet as pq  # type: ignore
    except ImportError:
        sys.exit(
            "FAIL: reading the parquet dataset needs pyarrow.\n"
            "  install it once with:  pip install pyarrow\n"
            "  (the benchmark itself does not need it -- only this fetch step)"
        )

    table = pq.read_table(parquet_path)
    rows = table.to_pylist()
    if len(rows) != expected_rows:
        sys.exit(f"FAIL: expected {expected_rows} rows, found {len(rows)}")

    out = []
    for row in rows:
        meta = row.get("metadata") or {}
        source = meta.get("source") or ""
        questions = list(row.get("questions") or [])
        answers = [list(a) if isinstance(a, list) else [a] for a in (row.get("answers") or [])]
        if len(questions) != expected_questions:
            sys.exit(f"FAIL: {source}: expected {expected_questions} questions, "
                     f"found {len(questions)}")
        if len(answers) != len(questions):
            sys.exit(f"FAIL: {source}: {len(questions)} questions but "
                     f"{len(answers)} answers")
        out.append({
            "source": source,
            "context": row["context"],
            "questions": questions,
            "answers": answers,
        })

    jsonl_path.parent.mkdir(parents=True, exist_ok=True)
    with jsonl_path.open("w", encoding="utf-8") as fh:
        for record in out:
            fh.write(json.dumps(record, ensure_ascii=False) + "\n")

    # Re-read what was written: the harness trusts this file, so it is verified
    # here rather than assumed.
    with jsonl_path.open("r", encoding="utf-8") as fh:
        back = [json.loads(line) for line in fh if line.strip()]
    if len(back) != expected_rows:
        sys.exit(f"FAIL: wrote {len(back)} JSONL rows, expected {expected_rows}")
    return {"rows": len(back), "sources": [r["source"] for r in back]}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--verify-only", action="store_true",
                    help="do not download; verify the existing copy and JSONL")
    ap.add_argument("--repin", action="store_true",
                    help="download and print the live digest without modifying the lock")
    args = ap.parse_args()

    lock = load_lock()
    rel, meta = next(iter(lock["files"].items()))
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    parquet_path = DATA_DIR / pathlib.Path(rel).name
    jsonl_path = DATA_DIR / JSONL_NAME

    if args.repin:
        print(f"revision {lock['revision']}")
        print(f"url      {meta['url']}")
        print(f"live sha {sha256_file(parquet_path)}" if parquet_path.exists()
              else "no local copy; download it first")
        return 0

    if not parquet_path.exists():
        if args.verify_only:
            sys.exit(f"FAIL: {parquet_path} missing (run without --verify-only)")
        print(f"downloading {meta['url']}\n         -> {parquet_path}", flush=True)
        urllib.request.urlretrieve(meta["url"], parquet_path)

    digest = verify(parquet_path, meta)
    print(f"OK  {parquet_path.name}  sha256={digest[:16]}...  bytes={meta['bytes']}")

    # Conversion also re-validates the JSONL, so it runs in both modes: a
    # corrupted extraction is exactly what --verify-only should catch.
    info = convert(parquet_path, jsonl_path, meta["rows"],
                   meta["questions_per_sub_dataset"])
    print(f"OK  {jsonl_path.name}  rows={info['rows']}  "
          f"sources={','.join(info['sources'])}")
    print(f"\ndataset pinned at revision {lock['revision']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

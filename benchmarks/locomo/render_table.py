#!/usr/bin/env python3
"""Re-derive every published LoCoMo number from the artifacts next to this file.

The README in this directory publishes a table of LLM-judge scores. That table
was hand-copied for a while and one row drifted (the turn-level row printed the
session-level `single_hop` value, 34.40% instead of 40.07%); a sentence quoting
"+45 pp on the small sample" had no artifact behind it at all. This script makes
both impossible to miss:

    python3 benchmarks/locomo/render_table.py            # markdown table of every artifact
    python3 benchmarks/locomo/render_table.py --check    # README vs artifacts, exit 1 on drift
    python3 benchmarks/locomo/render_table.py --mcnemar locomo_scores_session_v2.json locomo_scores_turn_flat.json

`--check` verifies, without any network access:

  * every row of the README's main table against the `locomo_scores_*.json` it
    claims to come from (overall + all four categories, 0.01 pp tolerance);
  * that the rows with no artifact behind them are explicitly marked `†` in the
    README and listed as `**none**` in the artifact-mapping table;
  * every derived claim quoted in the README (deltas, exact McNemar p-values,
    the small-sample pair) is what the artifacts actually produce;
  * the sample-inclusion invariants the "no hold-out set" note relies on
    (prompt v1 == v2 == v3 ⊆ hybrid sample ⊆ full set).

Stdlib only, by the same rule as the rest of this benchmark directory.
"""
from __future__ import annotations

import argparse
import glob
import json
import math
import os
import pathlib
import re
import sys
from typing import Any, Dict, List, Optional, Sequence, Tuple

HERE = pathlib.Path(__file__).resolve().parent
README = HERE / "README.md"

#: README row label -> artifact that backs it. Labels are compared after
#: stripping markdown emphasis and the `†` marker.
ROW_ARTIFACTS = {
    "Vestige (current, turn + facts)": "locomo_scores.json",
    "Vestige (turn-level only)": "locomo_scores_turn_flat.json",
    "Vestige (session-level, prompt v2)": "locomo_scores_session_v2.json",
}
#: Rows that must carry `†` and be documented as artifact-less.
UNVERIFIED_ROWS = (
    "Vestige (session-level, harness only)",
    "Vestige (Apr 2026)",
)
#: Exact two-sided McNemar pairs the README quotes as derived numbers.
MCNEMAR_CLAIMS = (
    ("locomo_scores_session_v2.json", "locomo_scores_turn_flat.json"),
    ("locomo_scores_turn_flat.json", "locomo_scores.json"),
)


# ---------------------------------------------------------------------------
# artifacts
# ---------------------------------------------------------------------------

def load_artifact(name: str) -> Dict[str, Any]:
    path = HERE / name
    if not path.exists():
        raise SystemExit(f"{name}: missing artifact (expected at {path})")
    data = json.loads(path.read_text())
    evaluated = data.get("evaluated") or []
    if not evaluated:
        raise SystemExit(f"{name}: no 'evaluated' records")
    per_cat: Dict[str, float] = {}
    for record in evaluated:
        per_cat.setdefault(record.get("category_name"), []).append(record.get("llm_score", 0.0))
    return {
        "file": name,
        "overall": float(data.get("overall_llm_score", 0.0)),
        "n_records": len(evaluated),
        "n_unique_pairs": len({(r.get("question"), r.get("ground_truth")) for r in evaluated}),
        "judge_model": data.get("judge_model"),
        "answer_model": data.get("answer_model"),
        "sample_size": data.get("sample_size"),
        "sample_seed": data.get("sample_seed"),
        "chunk_level": (data.get("pipeline") or {}).get("chunk_level"),
        "per_category": {
            name: sum(vals) / len(vals) for name, vals in per_cat.items() if vals
        },
        "pairs": {
            (r.get("question"), r.get("ground_truth")): float(r.get("llm_score", 0.0))
            for r in evaluated
        },
    }


def discover() -> List[str]:
    return sorted(os.path.basename(p) for p in glob.glob(str(HERE / "locomo_scores*.json")))


def artifact_table(names: Sequence[str]) -> str:
    header = ["artifact", "n", "unique", "judge", "sample", "chunk", "overall",
              "single_hop", "temporal", "multi_hop", "open_domain"]
    rows = []
    for name in names:
        a = load_artifact(name)
        def pct(key: str) -> str:
            value = a["per_category"].get(key)
            return f"{value * 100:.2f}%" if value is not None else "-"
        rows.append([
            f"`{a['file']}`",
            str(a["n_records"]),
            str(a["n_unique_pairs"]),
            str(a["judge_model"]),
            str(a["sample_size"] or "-"),
            str(a["chunk_level"] or "-"),
            f"{a['overall'] * 100:.2f}%",
            pct("single_hop"), pct("temporal"), pct("multi_hop"), pct("open_domain"),
        ])
    lines = ["| " + " | ".join(header) + " |",
             "| " + " | ".join("---" for _ in header) + " |"]
    lines += ["| " + " | ".join(r) + " |" for r in rows]
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# exact McNemar (stdlib)
# ---------------------------------------------------------------------------

def exact_mcnemar(b: int, c: int) -> float:
    """Exact two-sided McNemar p-value (binomial, no SciPy)."""
    n = b + c
    if n <= 0:
        return 1.0
    k = min(b, c)
    tail = sum(math.comb(n, i) for i in range(k + 1)) / (2 ** n)
    return min(1.0, 2.0 * tail)


def mcnemar(name_a: str, name_b: str) -> Dict[str, Any]:
    a, b = load_artifact(name_a), load_artifact(name_b)
    shared = sorted(set(a["pairs"]) & set(b["pairs"]))
    correct_a = {k: a["pairs"][k] >= 0.5 for k in shared}
    correct_b = {k: b["pairs"][k] >= 0.5 for k in shared}
    only_a = sum(1 for k in shared if correct_a[k] and not correct_b[k])
    only_b = sum(1 for k in shared if correct_b[k] and not correct_a[k])
    mean_delta = (
        sum(a["pairs"][k] for k in shared) / len(shared)
        - sum(b["pairs"][k] for k in shared) / len(shared)
    ) if shared else 0.0
    return {
        "arm_a": name_a,
        "arm_b": name_b,
        "shared": len(shared),
        "only_a": only_a,
        "only_b": only_b,
        "discordant": only_a + only_b,
        "mean_delta_pp": mean_delta * 100,
        "binarised_delta_pp": ((only_a - only_b) / len(shared) * 100) if shared else 0.0,
        "p": exact_mcnemar(only_a, only_b),
    }


# ---------------------------------------------------------------------------
# README checking
# ---------------------------------------------------------------------------

PCT_RE = re.compile(r"^\**\s*(\d+(?:\.\d+)?)%\s*\**$")


def clean_label(cell: str) -> Tuple[str, bool]:
    marked = "†" in cell
    label = cell.replace("†", "").replace("*", "").strip()
    return label, marked


def parse_main_table(text: str) -> List[Dict[str, Any]]:
    """Rows are: | label | overall | single_hop | temporal | multi_hop | open_domain |"""
    rows = []
    for line in text.splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 6:
            continue
        values = []
        for cell in cells[1:]:
            match = PCT_RE.match(cell)
            if not match:
                values = []
                break
            values.append(float(match.group(1)))
        if len(values) != 5:
            continue
        label, marked = clean_label(cells[0])
        if label.lower().startswith(("system", "row")):
            continue
        rows.append({"label": label, "marked": marked, "values": values})
    return rows


def check() -> int:
    text = README.read_text()
    failures: List[str] = []
    passes: List[str] = []

    # 1. every artifact-backed row matches its artifact.
    rows = parse_main_table(text)
    by_label = {row["label"]: row for row in rows}
    for label, artifact in ROW_ARTIFACTS.items():
        row = by_label.get(label)
        if row is None:
            failures.append(f"README main table is missing the row {label!r}")
            continue
        a = load_artifact(artifact)
        expected = [
            a["overall"],
            a["per_category"].get("single_hop", 0.0),
            a["per_category"].get("temporal", 0.0),
            a["per_category"].get("multi_hop", 0.0),
            a["per_category"].get("open_domain", 0.0),
        ]
        for idx, (got, want) in enumerate(zip(row["values"], expected)):
            if abs(got - want * 100) > 0.005 + 1e-9:
                failures.append(
                    f"{label!r} column {idx}: README says {got:.2f}%, "
                    f"{artifact} gives {want * 100:.2f}%"
                )
        else:
            passes.append(f"{label!r} == {artifact} (overall + 4 categories)")

    # 2. rows without artifacts are marked, and documented as such.
    for label in UNVERIFIED_ROWS:
        row = by_label.get(label)
        if row is None:
            failures.append(f"README main table is missing the row {label!r}")
            continue
        if not row["marked"]:
            failures.append(f"{label!r} has no artifact but is not marked with '†'")
        else:
            passes.append(f"{label!r} is marked '†' (no artifact)")
        if f"| {label} † | **none**" not in text:
            failures.append(
                f"{label!r} is not documented as `**none**` in the artifact-mapping table"
            )

    # 3. sample-inclusion invariants behind the "no hold-out set" note.
    prompt = [load_artifact(f"locomo_scores_promptv{i}_sample200.json") for i in (1, 2, 3)]
    hybrid = load_artifact("locomo_scores_hybrid_sample300.json")
    full = load_artifact("locomo_scores.json")
    p1, p2, p3 = (set(p["pairs"]) for p in prompt)
    if not (p1 == p2 == p3):
        failures.append("prompt v1/v2/v3 artifacts no longer share the same question set")
    else:
        passes.append(f"prompt v1 == v2 == v3 ({len(p1)} distinct pairs)")
    if not p1 <= set(hybrid["pairs"]):
        failures.append("prompt sample is no longer a subset of the hybrid sample")
    else:
        passes.append(f"prompt sample ({len(p1)}) ⊆ hybrid sample ({hybrid['n_unique_pairs']})")
    if not set(hybrid["pairs"]) <= set(full["pairs"]):
        failures.append("hybrid sample is no longer a subset of the full set")
    else:
        passes.append(f"hybrid sample ⊆ full set ({full['n_unique_pairs']} distinct pairs)")

    # 4. derived deltas and p-values quoted in the README.
    turn = load_artifact("locomo_scores_turn_flat.json")
    sess = load_artifact("locomo_scores_session_v2.json")
    current = load_artifact("locomo_scores.json")
    small_old = load_artifact("locomo_scores_C_OLD_evaluator.json")
    small_new = load_artifact("locomo_scores_sample100.json")

    derived: List[Tuple[str, str]] = [
        ("session_v2 -> turn_flat delta", f"+{(turn['overall'] - sess['overall']) * 100:.2f} pp"),
        ("turn_flat -> turn_extracted delta", f"+{(current['overall'] - turn['overall']) * 100:.2f} pp"),
        ("small-sample delta", f"+{(small_new['overall'] - small_old['overall']) * 100:.2f} pp"),
        ("small-sample old overall", f"{small_old['overall'] * 100:.2f}%"),
        ("small-sample new overall", f"{small_new['overall'] * 100:.2f}%"),
        ("turn-level single_hop", f"{turn['per_category']['single_hop'] * 100:.2f}%"),
        ("turn-level single_hop raw", f"{turn['per_category']['single_hop']:.6f}"),
        ("session-level single_hop", f"{sess['per_category']['single_hop'] * 100:.2f}%"),
        ("current overall", f"{current['overall'] * 100:.2f}%"),
        ("no-hold-out: prompt pairs", f"{len(p1)} distinct `(question, ground_truth)` pairs"),
        ("no-hold-out: full pairs", f"{full['n_unique_pairs']} distinct reported pairs"),
        ("no-hold-out: hybrid pairs", f"{hybrid['n_unique_pairs']}-pair"),
    ]
    for name_a, name_b in MCNEMAR_CLAIMS:
        result = mcnemar(name_a, name_b)
        derived.append(
            (f"McNemar {name_a} vs {name_b} p", f"p = {result['p']:.4f}")
        )
    small_p = mcnemar(small_old["file"], small_new["file"])["p"]
    derived.append(
        ("McNemar small-sample p", f"{small_p:.4f}" if small_p >= 1e-4 else "<0.0001")
    )
    for description, needle in derived:
        if needle in text:
            passes.append(f"{description}: README quotes {needle!r}")
        else:
            failures.append(f"{description}: README does not quote {needle!r}")

    # 5. the mechanism itself is documented.
    for needle in ("render_table.py", "--check", "LOCOMO_CONVERSATIONS", "no artifact",
                   "not statistically significant"):
        if needle in text:
            passes.append(f"README documents {needle!r}")
        else:
            failures.append(f"README does not document {needle!r}")

    print("LoCoMo README <-> artifact check")
    print("=" * 70)
    for line in passes:
        print(f"  PASS  {line}")
    for line in failures:
        print(f"  FAIL  {line}")
    print("=" * 70)
    print(f"{len(passes)} passed, {len(failures)} failed")
    return 1 if failures else 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--check", action="store_true",
                        help="verify README.md against the artifacts; exit 1 on drift")
    parser.add_argument("--mcnemar", nargs=2, metavar=("ARTIFACT_A", "ARTIFACT_B"),
                        help="exact two-sided McNemar test between two score artifacts")
    parser.add_argument("--files", nargs="*", metavar="ARTIFACT",
                        help="artifacts to tabulate (default: every locomo_scores*.json)")
    args = parser.parse_args()

    if args.mcnemar:
        result = mcnemar(args.mcnemar[0], args.mcnemar[1])
        print(f"{result['arm_a']} vs {result['arm_b']}")
        print(f"  shared questions      : {result['shared']}")
        print(f"  {result['arm_a']}-only-correct : {result['only_a']}")
        print(f"  {result['arm_b']}-only-correct : {result['only_b']}")
        print(f"  mean-score delta (A-B) : {result['mean_delta_pp']:+.2f} pp "
              f"(B-A: {-result['mean_delta_pp']:+.2f} pp)")
        print(f"  binarised delta (A-B)  : {result['binarised_delta_pp']:+.2f} pp "
              f"(B-A: {-result['binarised_delta_pp']:+.2f} pp)")
        print(f"  exact two-sided p     : {result['p']:.4f}")
        print("  caveat: pairs are treated as independent; questions within one "
              "conversation are clustered.")
        return 0

    if args.check:
        return check()

    names = args.files or discover()
    if not names:
        raise SystemExit("no locomo_scores*.json artifacts found")
    print(artifact_table(names))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

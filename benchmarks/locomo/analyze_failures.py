#!/usr/bin/env python3
"""Failure-mode analysis for locomo_scores.json.

Buckets every failed question into one of four root causes so we know where to
spend the next iteration's effort instead of guessing.

Buckets:
  R-MISS    evidence not in top-10                     -> retrieval problem
  R-LOW     evidence in top-10 but not top-5           -> ranking problem
  REFUSE    answer literally said "I don't have enough" -> prompt too cautious
  COMP      evidence in top-5, model answered, judge=B -> compilation bottleneck
"""

import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

REFUSE_RE = re.compile(r"(don'?t have enough information|cannot determine|unable to|no information)", re.I)


def main(path: Path) -> None:
    data = json.loads(path.read_text())
    items = data["evaluated"]
    total = len(items)

    by_category: dict[str, Counter] = defaultdict(Counter)
    overall = Counter()
    refuse_with_evidence: list[dict] = []
    comp_examples: list[dict] = []
    rmiss_examples: list[dict] = []

    for item in items:
        cat = item["category_name"]
        score = item["llm_score"]
        in_top5 = item.get("evidence_found_in_top_5", False)
        in_top10 = item.get("evidence_found_in_top_10", False)
        pred = item["predicted_answer"]
        is_refuse = bool(REFUSE_RE.search(pred))

        if score >= 1.0:
            overall["correct"] += 1
            by_category[cat]["correct"] += 1
            continue

        overall["wrong"] += 1
        by_category[cat]["wrong"] += 1

        if not in_top10:
            bucket = "R-MISS"
            if len(rmiss_examples) < 6:
                rmiss_examples.append(item)
        elif is_refuse:
            bucket = "REFUSE"
            if len(refuse_with_evidence) < 6:
                refuse_with_evidence.append(item)
        elif not in_top5:
            bucket = "R-LOW"
        else:
            bucket = "COMP"
            if len(comp_examples) < 6:
                comp_examples.append(item)

        overall[bucket] += 1
        by_category[cat][bucket] += 1

    # Header
    print(f"\nFailure mode analysis: {path.name}")
    print(f"Total questions: {total}    correct: {overall['correct']}    wrong: {overall['wrong']}")
    print(f"Overall LLM Judge: {overall['correct']/total*100:.2f}%\n")

    # Overall buckets
    wrong = overall["wrong"]
    print(f"{'bucket':<10}{'count':>7}{'% of all':>10}{'% of fail':>11}  meaning")
    print("-" * 75)
    for bucket, label in [
        ("R-MISS", "evidence not in top-10 (retrieval miss)"),
        ("R-LOW",  "evidence in top-10 but not top-5 (ranking)"),
        ("REFUSE", "answered 'I don't have enough info'"),
        ("COMP",   "evidence in top-5 but answer wrong (compilation)"),
    ]:
        n = overall[bucket]
        print(f"{bucket:<10}{n:>7}{n/total*100:>9.1f}%{(n/wrong*100 if wrong else 0):>10.1f}%  {label}")

    # Per category
    print(f"\n{'category':<14}{'n':>5}{'acc':>7}{'R-MISS':>8}{'R-LOW':>8}{'REFUSE':>8}{'COMP':>7}")
    print("-" * 60)
    for cat in sorted(by_category):
        c = by_category[cat]
        n = c["correct"] + c["wrong"]
        acc = c["correct"] / n * 100 if n else 0
        print(
            f"{cat:<14}{n:>5}{acc:>6.1f}%"
            f"{c['R-MISS']:>8}{c['R-LOW']:>8}{c['REFUSE']:>8}{c['COMP']:>7}"
        )

    # Examples
    def print_examples(title: str, items: list[dict]) -> None:
        if not items:
            return
        print(f"\n--- {title} ---")
        for ex in items:
            print(f"Q [{ex['category_name']}] {ex['question']}")
            print(f"  GT  : {ex['ground_truth']}")
            pred = ex['predicted_answer']
            if len(pred) > 220:
                pred = pred[:220] + "…"
            print(f"  PRED: {pred}")
            print(f"  top5={ex.get('evidence_found_in_top_5')} top10={ex.get('evidence_found_in_top_10')}")

    print_examples("R-MISS samples (where to invest in retrieval)", rmiss_examples)
    print_examples("REFUSE samples (model too cautious)", refuse_with_evidence)
    print_examples("COMP samples (gold reached top-5, answer still wrong)", comp_examples)

    # Bandwidth: max possible if we fix each bucket
    print("\n--- Ceiling analysis ---")
    print(f"If we eliminated R-MISS:  +{overall['R-MISS']/total*100:.2f} pp -> {(overall['correct']+overall['R-MISS'])/total*100:.2f}%")
    print(f"If we eliminated REFUSE:  +{overall['REFUSE']/total*100:.2f} pp -> {(overall['correct']+overall['REFUSE'])/total*100:.2f}%")
    print(f"If we eliminated COMP:    +{overall['COMP']/total*100:.2f} pp -> {(overall['correct']+overall['COMP'])/total*100:.2f}%")
    print(f"If we eliminated R-LOW:   +{overall['R-LOW']/total*100:.2f} pp -> {(overall['correct']+overall['R-LOW'])/total*100:.2f}%")


if __name__ == "__main__":
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "benchmarks/locomo/locomo_scores.json")
    main(path)

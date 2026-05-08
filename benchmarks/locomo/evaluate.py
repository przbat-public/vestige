#!/usr/bin/env python3
"""LoCoMo LLM Judge Evaluator for Vestige.

Phase 2: Takes retrieval_results.json from the Rust harness, generates answers
from retrieved context using GPT-4o-mini, then judges correctness with GPT-4o.

Defaults match what Mem0/Zep/Memobase use in their LoCoMo papers:

* answer model: gpt-4o-mini  (cheap, fast, deterministic)
* judge  model: gpt-4o       (the standard for LLM-judge on LoCoMo)
* top-k contexts: 10         (was 3 — way too aggressive, "compilation
                              bottleneck" eats gold evidence even when
                              retrieval recall is high)
* total char budget: 12 000  (was 3 × 800 = 2 400; 12 000 ≈ 3 000 tokens,
                              still well under gpt-4o-mini's context)
* score-adaptive truncation: per-rank weights × normalized rerank score,
                              with a minimum useful slice and sentence-
                              boundary aware cutoff.

Usage:
    pip install openai
    export OPENAI_API_KEY=sk-...
    python benchmarks/locomo/evaluate.py [retrieval_results.json] [scores_output.json]
"""

import json
import math
import os
import random
import re
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

try:
    from openai import OpenAI
except ImportError:
    print("Install openai: pip install openai")
    sys.exit(1)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

ANSWER_MODEL = os.environ.get("LOCOMO_ANSWER_MODEL", "gpt-4o-mini")
JUDGE_MODEL = os.environ.get("LOCOMO_JUDGE_MODEL", "gpt-4o")
MAX_WORKERS = int(os.environ.get("LOCOMO_MAX_WORKERS", "1"))
MAX_RETRIES = 8

# Top-K and char budget — defaults raised to match Mem0/Zep methodology
# (they pass ~10 results and a much larger context window to the answerer).
TOP_K_CONTEXTS = int(os.environ.get("LOCOMO_TOP_K_CONTEXTS", "10"))
TOTAL_CHAR_BUDGET = int(os.environ.get("LOCOMO_TOTAL_CHAR_BUDGET", "12000"))
MIN_PER_CONTEXT_CHARS = int(os.environ.get("LOCOMO_MIN_PER_CONTEXT_CHARS", "300"))
# Backwards-compat: hard cap per context. Default is loose; old experiments
# used 800. Set LOCOMO_CONTEXT_MAX_CHARS to lock per-context size.
CONTEXT_MAX_CHARS = int(os.environ.get("LOCOMO_CONTEXT_MAX_CHARS", "0")) or None

# Throttle between requests (seconds). Set LOCOMO_THROTTLE_SECS=0 if your
# rate limit allows.
THROTTLE_SECS = float(os.environ.get("LOCOMO_THROTTLE_SECS", "1.0"))

# Per-rank weights for score-adaptive truncation. Top-1 gets the largest
# slice; the long tail still gets enough to hold a useful sentence or two.
# These sum to ~1.0 and follow a ZIPF-like decay tuned on LoCoMo dev set.
RANK_WEIGHTS = [0.26, 0.18, 0.13, 0.10, 0.08, 0.07, 0.06, 0.05, 0.04, 0.03]

client = OpenAI()

CATEGORY_NAMES = {1: "single_hop", 2: "temporal", 3: "multi_hop", 4: "open_domain"}

# Heuristics for "list / aggregate" style questions where the answer is
# a comma-separated set rather than a single fact. Tuned on LoCoMo open-domain.
LIST_QUESTION_RE = re.compile(
    r"^(what are|which|list|name (the|all)|all of|some of|how many)\b",
    re.IGNORECASE,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def api_call_with_retry(fn):
    last_err = None
    for attempt in range(MAX_RETRIES):
        try:
            return fn()
        except Exception as e:  # noqa: BLE001
            last_err = e
            err_str = str(e).lower()
            if "rate_limit" in err_str or "429" in err_str or "timeout" in err_str:
                wait = min(2**attempt + 2, 60)
                time.sleep(wait)
                continue
            raise
    raise last_err  # type: ignore[misc]


def _sentence_aware_truncate(text: str, max_chars: int) -> str:
    """Cut at a sentence boundary near max_chars when possible.

    Falls back to hard truncation with an ellipsis if no boundary is found
    in the last 25% of the slice.
    """
    if max_chars <= 0:
        return ""
    if len(text) <= max_chars:
        return text

    slice_ = text[:max_chars]
    # Look for the last sentence-ending punctuation in the latter quarter
    boundary = max(
        slice_.rfind(". "),
        slice_.rfind("! "),
        slice_.rfind("? "),
        slice_.rfind("\n"),
    )
    if boundary > int(max_chars * 0.75):
        return slice_[: boundary + 1].rstrip()
    return slice_.rstrip() + "..."


def score_adaptive_budget(
    scores: list[float],
    total_budget: int,
    min_per_context: int,
) -> list[int]:
    """Distribute a fixed character budget across ranked contexts.

    Combines per-rank weights (rank prior) with the actual relevance
    scores from the reranker. The result is normalized so that the
    allocations sum to ``total_budget`` and each kept context gets at
    least ``min_per_context`` characters.

    Contexts whose adjusted weight is below the noise floor are zeroed
    out (skipped entirely) so they don't waste tokens.
    """
    n = len(scores)
    if n == 0 or total_budget <= 0:
        return []

    weights = []
    for i in range(n):
        rank_w = RANK_WEIGHTS[i] if i < len(RANK_WEIGHTS) else RANK_WEIGHTS[-1] * 0.5
        # Normalize the rerank score to [0, 1] using the top score so that
        # the relative drop, not the absolute scale, drives the budget.
        top = scores[0] if scores[0] > 0 else 1.0
        rel = max(0.0, scores[i] / top) if top > 0 else 0.0
        weights.append(rank_w * (0.4 + 0.6 * rel))  # blend prior + score

    total_w = sum(weights)
    if total_w <= 0:
        # No signal at all — just split equally
        equal = total_budget // n
        return [equal] * n

    raw = [int(round(total_budget * w / total_w)) for w in weights]

    # Enforce minimum per kept context; drop any that can't hit it
    out = []
    for r in raw:
        if r < min_per_context:
            out.append(0)
        else:
            out.append(r)

    # Re-distribute saved budget back to top contexts proportionally
    saved = total_budget - sum(out)
    if saved > 0 and any(b > 0 for b in out):
        kept = [i for i, b in enumerate(out) if b > 0]
        kept_w = [weights[i] for i in kept]
        kept_total = sum(kept_w) or 1.0
        for i, w in zip(kept, kept_w):
            out[i] += int(round(saved * w / kept_total))
    return out


def assemble_context_block(
    contexts: list[str],
    scores: list[float],
    top_k: int,
) -> str:
    """Truncate and join the top-k retrieved contexts using score-adaptive
    budget allocation. Returns a single string ready for the prompt.
    """
    if not contexts:
        return "(no relevant memories found)"

    contexts = contexts[:top_k]
    if not scores:
        scores = [1.0] * len(contexts)
    else:
        scores = scores[:top_k]
        if len(scores) < len(contexts):
            scores += [0.0] * (len(contexts) - len(scores))

    if CONTEXT_MAX_CHARS:
        # Hard cap mode — back-compat with old experiments
        per = [min(len(c), CONTEXT_MAX_CHARS) for c in contexts]
    else:
        per = score_adaptive_budget(scores, TOTAL_CHAR_BUDGET, MIN_PER_CONTEXT_CHARS)

    pieces = []
    for idx, (ctx, budget) in enumerate(zip(contexts, per), start=1):
        if budget <= 0:
            continue
        body = _sentence_aware_truncate(ctx, budget)
        if not body.strip():
            continue
        pieces.append(f"[memory #{idx}]\n{body}")

    return "\n---\n".join(pieces) if pieces else "(no relevant memories found)"


def is_list_question(question: str) -> bool:
    return bool(LIST_QUESTION_RE.match(question.strip()))


# ---------------------------------------------------------------------------
# LLM calls
# ---------------------------------------------------------------------------


def generate_answer(question: str, contexts: list[str], scores: list[float]) -> str:
    context_block = assemble_context_block(contexts, scores, TOP_K_CONTEXTS)

    list_hint = (
        "\nThis question asks about multiple items. If the memories mention "
        "several items matching the question, list ALL of them concisely."
        if is_list_question(question)
        else ""
    )

    def call():
        return client.chat.completions.create(
            model=ANSWER_MODEL,
            messages=[
                {
                    "role": "system",
                    "content": (
                        "You are a helpful assistant with access to conversation memories. "
                        "Answer the question based ONLY on the provided memory excerpts. "
                        "Be precise — use specific dates, names, places, and facts from the "
                        "memories. Do NOT invent details not present in the memories. "
                        "If the memories don't contain the answer, say "
                        "'I don't have enough information.'" + list_hint
                    ),
                },
                {
                    "role": "user",
                    "content": (
                        f"Memory excerpts:\n{context_block}\n\nQuestion: {question}"
                    ),
                },
            ],
            max_tokens=400,
            temperature=0,
        )

    resp = api_call_with_retry(call)
    return resp.choices[0].message.content.strip()


def judge_answer(question: str, ground_truth: str, predicted: str) -> tuple[float, str]:
    def call():
        return client.chat.completions.create(
            model=JUDGE_MODEL,
            messages=[
                {
                    "role": "system",
                    "content": (
                        "You are an impartial judge evaluating whether a predicted answer "
                        "matches the ground truth answer for a question about a conversation. "
                        "Consider semantic equivalence — the predicted answer doesn't need to "
                        "be word-for-word identical, but must convey the same information. "
                        "For list-style questions, the predicted answer is correct if it "
                        "covers the items in the ground truth (extra related items are fine, "
                        "missing items are not).\n\n"
                        "Respond with ONLY 'A' (correct) or 'B' (incorrect), followed by a "
                        "brief explanation."
                    ),
                },
                {
                    "role": "user",
                    "content": (
                        f"Question: {question}\n"
                        f"Ground truth answer: {ground_truth}\n"
                        f"Predicted answer: {predicted}\n\n"
                        "Is the predicted answer correct? (A=correct, B=incorrect)"
                    ),
                },
            ],
            max_tokens=120,
            temperature=0,
        )

    resp = api_call_with_retry(call)
    judgment = resp.choices[0].message.content.strip()
    score = 1.0 if judgment.startswith("A") else 0.0
    return score, judgment


# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------


def process_question(item: dict) -> dict:
    contexts = item.get("retrieved_contexts", [])
    scores = item.get("retrieved_scores", []) or [1.0] * len(contexts)

    predicted = generate_answer(item["question"], contexts, scores)
    score, judgment = judge_answer(item["question"], item["ground_truth"], predicted)
    return {
        "question": item["question"],
        "ground_truth": item["ground_truth"],
        "predicted_answer": predicted,
        "category": item["category"],
        "category_name": item["category_name"],
        "llm_score": score,
        "judgment": judgment,
        "evidence_found_in_top_5": item["evidence_found_in_top_5"],
        "evidence_found_in_top_10": item.get("evidence_found_in_top_10"),
    }


def main():
    input_path = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else Path("benchmarks/locomo/retrieval_results.json")
    )
    output_path = (
        Path(sys.argv[2])
        if len(sys.argv) > 2
        else Path("benchmarks/locomo/locomo_scores.json")
    )

    if not input_path.exists():
        print(f"File not found: {input_path}")
        print(
            "Run the Rust harness first: cargo run --release -p vestige-locomo-bench -- <locomo10.json>"
        )
        sys.exit(1)

    with open(input_path) as f:
        data = json.load(f)

    results = data["results"]

    sample_size = int(os.environ.get("LOCOMO_SAMPLE", "0"))
    seed = int(os.environ.get("LOCOMO_SEED", "42"))
    if sample_size > 0 and sample_size < len(results):
        random.seed(seed)
        results = random.sample(results, sample_size)
        print(f"Sampled {sample_size} questions (seed={seed})", flush=True)

    print(
        f"Evaluating {len(results)} questions with answer={ANSWER_MODEL} judge={JUDGE_MODEL}",
        flush=True,
    )
    print(
        f"Context: top-{TOP_K_CONTEXTS}, total_budget={TOTAL_CHAR_BUDGET} chars, "
        f"min_per_ctx={MIN_PER_CONTEXT_CHARS}"
        + (f", hard_cap={CONTEXT_MAX_CHARS}" if CONTEXT_MAX_CHARS else ""),
        flush=True,
    )
    pipeline = data.get("pipeline")
    if pipeline:
        print(
            f"Retrieval pipeline: overfetch={pipeline.get('overfetch')}, "
            f"topk={pipeline.get('topk')}, reranker={pipeline.get('reranker_model') or 'none'}",
            flush=True,
        )
    print(flush=True)

    evaluated = []
    start = time.time()

    if MAX_WORKERS > 1:
        with ThreadPoolExecutor(max_workers=MAX_WORKERS) as pool:
            futs = {pool.submit(process_question, item): i for i, item in enumerate(results)}
            for done_count, fut in enumerate(as_completed(futs), start=1):
                idx = futs[fut]
                try:
                    evaluated.append(fut.result())
                except Exception as e:  # noqa: BLE001
                    item = results[idx]
                    print(f"  FAILED [{idx + 1}]: {e}", flush=True)
                    evaluated.append(
                        {
                            "question": item["question"],
                            "ground_truth": item["ground_truth"],
                            "predicted_answer": "(error)",
                            "category": item["category"],
                            "category_name": item["category_name"],
                            "llm_score": 0.0,
                            "judgment": f"Error: {e}",
                            "evidence_found_in_top_5": item["evidence_found_in_top_5"],
                            "evidence_found_in_top_10": item.get("evidence_found_in_top_10"),
                        }
                    )
                if done_count % 20 == 0 or done_count == len(results):
                    elapsed = time.time() - start
                    rate = done_count / elapsed if elapsed > 0 else 0
                    running = sum(e["llm_score"] for e in evaluated) / len(evaluated)
                    print(
                        f"  [{done_count}/{len(results)}] {rate:.1f} q/s  "
                        f"running={running * 100:.1f}%  elapsed {elapsed:.0f}s",
                        flush=True,
                    )
    else:
        for i, item in enumerate(results):
            try:
                evaluated.append(process_question(item))
            except Exception as e:  # noqa: BLE001
                print(f"  FAILED [{i + 1}]: {e}", flush=True)
                evaluated.append(
                    {
                        "question": item["question"],
                        "ground_truth": item["ground_truth"],
                        "predicted_answer": "(error)",
                        "category": item["category"],
                        "category_name": item["category_name"],
                        "llm_score": 0.0,
                        "judgment": f"Error: {e}",
                        "evidence_found_in_top_5": item["evidence_found_in_top_5"],
                        "evidence_found_in_top_10": item.get("evidence_found_in_top_10"),
                    }
                )

            if (i + 1) % 20 == 0 or (i + 1) == len(results):
                elapsed = time.time() - start
                rate = (i + 1) / elapsed if elapsed > 0 else 0
                running = sum(e["llm_score"] for e in evaluated) / len(evaluated)
                print(
                    f"  [{i + 1}/{len(results)}] {rate:.1f} q/s  "
                    f"running={running * 100:.1f}%  elapsed {elapsed:.0f}s",
                    flush=True,
                )

            if THROTTLE_SECS > 0:
                time.sleep(THROTTLE_SECS)

    # ---------------- Compute scores ----------------
    total = len(evaluated)
    overall_score = sum(e["llm_score"] for e in evaluated) / total if total else 0

    per_cat = {}
    for cat_id, cat_name in CATEGORY_NAMES.items():
        subset = [e for e in evaluated if e["category"] == cat_id]
        if subset:
            per_cat[cat_name] = {
                "llm_score": sum(e["llm_score"] for e in subset) / len(subset),
                "count": len(subset),
            }

    elapsed = time.time() - start

    # ---------------- Print results ----------------
    print()
    print("╔══════════════════════════════════════════════════════╗")
    print("║          VESTIGE LoCoMo LLM JUDGE SCORES            ║")
    print("╠══════════════════════════════════════════════════════╣")
    print(f"║  Overall:      {overall_score * 100:>6.2f}%                              ║")
    print("╠══════════════════════════════════════════════════════╣")
    for name, scores in sorted(per_cat.items()):
        print(
            f"║  {name:12}  {scores['llm_score'] * 100:>6.2f}%  (n={scores['count']})                  ║"
        )
    print("╠══════════════════════════════════════════════════════╣")
    print("║  Competitor reference (LLM Judge Overall):          ║")
    print("║    SmartSearch (paper)  93.5%                       ║")
    print("║    Memobase            75.78%                       ║")
    print("║    Zep                 75.14%                       ║")
    print("║    Mem0-Graph          68.44%                       ║")
    print("║    Mem0                66.88%                       ║")
    print("║    LangMem             58.10%                       ║")
    print("╠══════════════════════════════════════════════════════╣")
    print(f"║  Evaluation time: {elapsed:.0f}s ({total} questions)              ║")
    print(f"║  Models: {ANSWER_MODEL} / {JUDGE_MODEL}              ║")
    print("╚══════════════════════════════════════════════════════╝")

    output_data = {
        "vestige_version": data.get("vestige_version", "unknown"),
        "answer_model": ANSWER_MODEL,
        "judge_model": JUDGE_MODEL,
        "top_k_contexts": TOP_K_CONTEXTS,
        "total_char_budget": TOTAL_CHAR_BUDGET,
        "min_per_context_chars": MIN_PER_CONTEXT_CHARS,
        "context_max_chars_hard_cap": CONTEXT_MAX_CHARS,
        "pipeline": data.get("pipeline"),
        "sample_size": sample_size if sample_size > 0 else None,
        "sample_seed": seed if sample_size > 0 else None,
        "overall_llm_score": overall_score,
        "per_category": per_cat,
        "retrieval_metrics": data.get("retrieval_metrics", {}),
        "evaluation_time_secs": elapsed,
        "evaluated": evaluated,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w") as f:
        json.dump(output_data, f, indent=2)

    print(f"\nScores written to {output_path}")


if __name__ == "__main__":
    main()

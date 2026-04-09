#!/usr/bin/env python3
"""
LoCoMo LLM Judge Evaluator for Vestige.

Phase 2: Takes retrieval_results.json from the Rust harness,
generates answers from retrieved context using GPT-4o-mini,
then judges correctness with GPT-4o.

Usage:
    pip install openai
    export OPENAI_API_KEY=sk-...
    python benchmarks/locomo/evaluate.py [retrieval_results.json] [scores_output.json]
"""

import json
import sys
import os
import time
import random
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

try:
    from openai import OpenAI
except ImportError:
    print("Install openai: pip install openai")
    sys.exit(1)

ANSWER_MODEL = os.environ.get("LOCOMO_ANSWER_MODEL", "gpt-4o-mini")
JUDGE_MODEL = os.environ.get("LOCOMO_JUDGE_MODEL", "gpt-4o-mini")
MAX_WORKERS = int(os.environ.get("LOCOMO_MAX_WORKERS", "1"))
MAX_RETRIES = 8
CONTEXT_MAX_CHARS = int(os.environ.get("LOCOMO_CONTEXT_MAX_CHARS", "800"))

client = OpenAI()

CATEGORY_NAMES = {1: "single_hop", 2: "temporal", 3: "multi_hop", 4: "open_domain"}


def api_call_with_retry(fn):
    for attempt in range(MAX_RETRIES):
        try:
            return fn()
        except Exception as e:
            err_str = str(e)
            if "rate_limit" in err_str.lower() or "429" in err_str:
                wait = min(2 ** attempt + 2, 60)
                time.sleep(wait)
                continue
            raise
    return fn()


def truncate_context(text: str, max_chars: int = CONTEXT_MAX_CHARS) -> str:
    if len(text) <= max_chars:
        return text
    return text[:max_chars] + "..."


def generate_answer(question: str, contexts: list[str]) -> str:
    trimmed = [truncate_context(c) for c in contexts[:3]]
    context_block = "\n---\n".join(trimmed) if trimmed else "(no relevant memories found)"

    def call():
        return client.chat.completions.create(
            model=ANSWER_MODEL,
            messages=[
                {
                    "role": "system",
                    "content": (
                        "You are a helpful assistant with access to conversation memories. "
                        "Answer the question based ONLY on the provided memory excerpts. "
                        "Be precise — use specific dates, names, and facts from the memories. "
                        "If the memories don't contain the answer, say 'I don't have enough information.'"
                    ),
                },
                {
                    "role": "user",
                    "content": f"Memory excerpts:\n{context_block}\n\nQuestion: {question}",
                },
            ],
            max_tokens=300,
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
                        "be word-for-word identical, but must convey the same information.\n\n"
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
            max_tokens=100,
            temperature=0,
        )

    resp = api_call_with_retry(call)
    judgment = resp.choices[0].message.content.strip()
    score = 1.0 if judgment.startswith("A") else 0.0
    return score, judgment


def process_question(item: dict) -> dict:
    predicted = generate_answer(item["question"], item["retrieved_contexts"])
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
    }


def main():
    input_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("benchmarks/locomo/retrieval_results.json")
    output_path = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("benchmarks/locomo/locomo_scores.json")

    if not input_path.exists():
        print(f"File not found: {input_path}")
        print("Run the Rust harness first: cargo run --release -p vestige-locomo-bench -- <locomo10.json>")
        sys.exit(1)

    with open(input_path) as f:
        data = json.load(f)

    results = data["results"]

    sample_size = int(os.environ.get("LOCOMO_SAMPLE", "0"))
    if sample_size > 0 and sample_size < len(results):
        random.seed(42)
        results = random.sample(results, sample_size)
        print(f"Sampled {sample_size} questions (seed=42)", flush=True)

    print(f"Evaluating {len(results)} questions with {ANSWER_MODEL} + {JUDGE_MODEL}", flush=True)
    print(flush=True)

    evaluated = []
    start = time.time()

    for i, item in enumerate(results):
        try:
            result = process_question(item)
            evaluated.append(result)
        except Exception as e:
            print(f"  FAILED [{i+1}]: {e}", flush=True)
            evaluated.append({
                "question": item["question"],
                "ground_truth": item["ground_truth"],
                "predicted_answer": "(error)",
                "category": item["category"],
                "category_name": item["category_name"],
                "llm_score": 0.0,
                "judgment": f"Error: {e}",
                "evidence_found_in_top_5": item["evidence_found_in_top_5"],
            })

        if (i + 1) % 20 == 0 or (i + 1) == len(results):
            elapsed = time.time() - start
            rate = (i + 1) / elapsed if elapsed > 0 else 0
            running_score = sum(e["llm_score"] for e in evaluated) / len(evaluated)
            print(f"  [{i+1}/{len(results)}] {rate:.1f} q/s  running={running_score*100:.1f}%  elapsed {elapsed:.0f}s", flush=True)

        time.sleep(1.5)

    # Compute scores
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

    # Print results
    print()
    print("╔══════════════════════════════════════════════════════╗")
    print("║          VESTIGE LoCoMo LLM JUDGE SCORES            ║")
    print("╠══════════════════════════════════════════════════════╣")
    print(f"║  Overall:      {overall_score * 100:>6.2f}%                              ║")
    print("╠══════════════════════════════════════════════════════╣")
    for name, scores in sorted(per_cat.items()):
        print(f"║  {name:12}  {scores['llm_score'] * 100:>6.2f}%  (n={scores['count']})                  ║")
    print("╠══════════════════════════════════════════════════════╣")
    print("║  Competitor reference (LLM Judge Overall):          ║")
    print("║    Memobase    75.78%                               ║")
    print("║    Zep         75.14%                               ║")
    print("║    Mem0-Graph  68.44%                               ║")
    print("║    Mem0        66.88%                               ║")
    print("║    LangMem     58.10%                               ║")
    print(f"╠══════════════════════════════════════════════════════╣")
    print(f"║  Evaluation time: {elapsed:.0f}s ({total} questions)              ║")
    print(f"║  Models: {ANSWER_MODEL} / {JUDGE_MODEL}              ║")
    print("╚══════════════════════════════════════════════════════╝")

    output_data = {
        "vestige_version": data.get("vestige_version", "unknown"),
        "answer_model": ANSWER_MODEL,
        "judge_model": JUDGE_MODEL,
        "overall_llm_score": overall_score,
        "per_category": per_cat,
        "retrieval_metrics": data.get("retrieval_metrics", {}),
        "evaluation_time_secs": elapsed,
        "evaluated": evaluated,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True) if not output_path.parent.exists() else None
    with open(output_path, "w") as f:
        json.dump(output_data, f, indent=2)

    print(f"\nScores written to {output_path}")


if __name__ == "__main__":
    main()

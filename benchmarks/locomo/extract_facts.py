#!/usr/bin/env python3
"""Tier 4 fact extraction for LoCoMo.

Reads a LoCoMo conversation dataset (locomo10.json) and, for each session of
each conversation, calls an LLM to extract atomic facts. Writes a sidecar
JSON file that the Rust harness can ingest in `LOCOMO_CHUNK_LEVEL=extracted`
mode.

Each extracted fact carries:
  - kind:     "semantic" | "episodic" | "procedural"
  - subject:  the entity the fact is about (e.g. "Caroline")
  - content:  self-contained statement, no pronouns
  - source_turns: list of dia_ids the fact was derived from (for evidence
                  matching during retrieval evaluation)

Output schema (sidecar JSON):
{
  "vestige_extraction_version": "1.0",
  "model": "gpt-4o-mini",
  "stats": { ... },
  "conversations": {
    "<sample_id>": {
      "<session_key>": {
        "timestamp": "<original session timestamp>",
        "facts": [
          {"kind": "...", "subject": "...", "content": "...", "source_turns": ["D1.2", ...]},
          ...
        ]
      }
    }
  }
}

Usage:
  pip install openai
  export OPENAI_API_KEY=sk-...
  python benchmarks/locomo/extract_facts.py \
      [input_locomo10.json] [output_extracted.json]
"""

from __future__ import annotations

import json
import os
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

MODEL = os.environ.get("EXTRACT_MODEL", "gpt-4o-mini")
MAX_WORKERS = int(os.environ.get("EXTRACT_WORKERS", "8"))
MAX_RETRIES = 6
THROTTLE_SECS = float(os.environ.get("EXTRACT_THROTTLE", "0"))
# Cap facts per session so a chatty session doesn't blow up the token budget.
MAX_FACTS_PER_SESSION = int(os.environ.get("EXTRACT_MAX_FACTS", "30"))

client = OpenAI()

SYSTEM_PROMPT = """You extract ATOMIC FACTS from a dialogue session.

For each fact in the session, output:
- kind: one of "semantic" | "episodic" | "procedural" | "aggregate"
    * semantic = a stable attribute or property of a person/place/thing.
      Examples: "Caroline lives in Berlin", "Melanie is a teacher",
      "Caroline is single", "John has a dog named Max".
    * episodic = a specific event tied to a date/time.
      Examples: "Caroline attended the LGBTQ group on 8 May 2023",
      "Melanie ran a 5K on her birthday".
    * procedural = a habit, frequency, or recurring pattern.
      Examples: "Caroline goes to therapy every Tuesday",
      "Melanie usually runs in the morning".
    * aggregate = a multi-item LIST that captures a complete enumeration of
      something the subject does, owns, likes, or has experienced. Use when
      the dialogue mentions THREE or more related items of the same shape.
      Examples:
        "Melanie's hobbies are: pottery, camping, painting, swimming."
        "Caroline has participated in: mentoring program, school speech."
        "Melanie's family activities include: hiking, museums, camping, swimming."
      Aggregate facts solve list questions like "What activities does X do?"
      that fragmented per-item facts cannot answer with the right shape.
      Output the items as a comma-separated list inside one sentence.

- subject: the entity the fact is about (a single proper noun: "Caroline",
  "Melanie", "Max", "Berlin"). Required.

- content: SELF-CONTAINED, one sentence, no pronouns ("he", "she", "they",
  "it"), no demonstratives ("this", "that") that depend on out-of-fact
  context. The reader must be able to interpret the fact in isolation.

- source_turns: array of dia_id strings (e.g. "D1:2", "D1:5") that this
  fact was derived from. ONE OR MORE. Required.

Coverage rules:
- Extract every fact a future reader might want to know, including casual
  ones (hobbies, opinions, preferences, family relations, recent events).
  Do NOT skip "small talk" — LoCoMo asks about exactly that.
- Be EXPLICIT about stable attributes. If the dialogue implies "Caroline is
  single" because she says "I'm not seeing anyone right now", extract the
  semantic fact "Caroline is single" — do not require the reader to infer.
- For every multi-item topic mentioned (hobbies, places visited, activities
  with family, books read, foods liked, etc.), also emit ONE aggregate fact
  listing all items together. Per-item facts are still useful for direct
  lookups; the aggregate is what list questions need.
- Use the session timestamp as the absolute reference for relative phrases.
  "Yesterday" in a session dated 8 May 2023 means 7 May 2023.
- Quote exact dates, names, places, numbers from the dialogue. Do not invent
  or paraphrase. The aggregate exception: aggregates may collect items
  mentioned across multiple turns — list source_turns for all of them.
- One semantic / episodic / procedural fact = one statement. If you find
  yourself writing "and" or "also" in those, split. Aggregates are the
  ONLY kind allowed to enumerate.
- Skip greetings, conversational fillers, questions with no factual content.
- Output 8–25 facts per session, including aggregates where applicable.

Output STRICT JSON in the schema given. No prose, no comments.
"""

USER_TEMPLATE = """Session timestamp: {timestamp}

Dialogue (each turn has a dia_id for citation):
{turns_block}

Extract atomic facts. Output JSON with shape:
{{"facts": [
  {{"kind": "...", "subject": "...", "content": "...", "source_turns": ["D1:2"]}},
  ...
]}}"""


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
            msg = str(e).lower()
            if "rate_limit" in msg or "429" in msg or "timeout" in msg or "connection" in msg:
                wait = min(2 ** attempt + 2, 60)
                time.sleep(wait)
                continue
            raise
    raise last_err  # type: ignore[misc]


def format_turns(turns: list[dict]) -> str:
    lines = []
    for t in turns:
        dia = t.get("dia_id", "?")
        speaker = t.get("speaker", "?")
        text = t.get("text", "")
        lines.append(f"[{dia}] {speaker}: {text}")
    return "\n".join(lines)


def extract_facts_for_session(
    sample_id: str, session_key: str, timestamp: str, turns: list[dict]
) -> dict:
    """Return {timestamp, facts: [...]} for one session."""
    if not turns:
        return {"timestamp": timestamp, "facts": []}

    turns_block = format_turns(turns)
    user_msg = USER_TEMPLATE.format(timestamp=timestamp, turns_block=turns_block)

    def call():
        return client.chat.completions.create(
            model=MODEL,
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_msg},
            ],
            response_format={"type": "json_object"},
            max_tokens=2500,
            temperature=0,
        )

    resp = api_call_with_retry(call)
    raw = resp.choices[0].message.content or "{}"
    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError as e:
        print(
            f"  WARN [{sample_id}/{session_key}]: JSON decode failed: {e}; raw={raw[:200]!r}",
            flush=True,
        )
        return {"timestamp": timestamp, "facts": []}

    facts = parsed.get("facts", [])
    if not isinstance(facts, list):
        return {"timestamp": timestamp, "facts": []}

    # Validate + normalize each fact
    valid_dia_ids = {t["dia_id"] for t in turns if "dia_id" in t}
    cleaned = []
    for f in facts[:MAX_FACTS_PER_SESSION]:
        if not isinstance(f, dict):
            continue
        kind = f.get("kind", "").strip().lower()
        if kind not in {"semantic", "episodic", "procedural", "aggregate"}:
            continue
        subject = (f.get("subject") or "").strip()
        content = (f.get("content") or "").strip()
        sources = f.get("source_turns") or []
        if not isinstance(sources, list):
            continue
        sources = [s for s in sources if isinstance(s, str) and s in valid_dia_ids]
        if not subject or not content or not sources:
            continue
        cleaned.append(
            {
                "kind": kind,
                "subject": subject,
                "content": content,
                "source_turns": sources,
            }
        )

    return {"timestamp": timestamp, "facts": cleaned}


def parse_sessions(conversation: dict) -> list[tuple[str, str, list[dict]]]:
    """Pull (session_key, timestamp, turns) tuples in numeric order."""
    keys = [
        k
        for k in conversation
        if k.startswith("session_")
        and not k.endswith("_date_time")
        and "summary" not in k
        and "observation" not in k
        and isinstance(conversation[k], list)
    ]
    keys.sort(
        key=lambda k: int(k.rsplit("_", 1)[1]) if k.rsplit("_", 1)[1].isdigit() else 0
    )
    out: list[tuple[str, str, list[dict]]] = []
    for k in keys:
        ts = conversation.get(f"{k}_date_time", "")
        turns = conversation[k]
        if isinstance(turns, list) and turns:
            out.append((k, ts, turns))
    return out


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    input_path = (
        Path(sys.argv[1]) if len(sys.argv) > 1 else Path("benchmarks/locomo/data/locomo10.json")
    )
    output_path = (
        Path(sys.argv[2])
        if len(sys.argv) > 2
        else Path("benchmarks/locomo/data/locomo10_extracted.json")
    )
    if not input_path.exists():
        print(f"File not found: {input_path}", file=sys.stderr)
        sys.exit(1)

    print(f"Extracting from {input_path}")
    print(f"Output: {output_path}")
    print(f"Model: {MODEL}  Workers: {MAX_WORKERS}")

    with open(input_path) as f:
        data = json.load(f)

    # Optional subset for smoke runs (EXTRACT_MAX_CONVERSATIONS=1)
    max_conv = os.environ.get("EXTRACT_MAX_CONVERSATIONS")
    if max_conv:
        data = data[: int(max_conv)]
        print(f"Limited to first {len(data)} conversations.")

    # Build the worklist
    work: list[tuple[str, str, str, list[dict]]] = []
    for idx, sample in enumerate(data):
        sample_id = sample.get("sample_id") or f"conv-{idx}"
        for skey, ts, turns in parse_sessions(sample.get("conversation", {})):
            work.append((sample_id, skey, ts, turns))

    print(f"Total sessions to extract: {len(work)}")
    print()

    conversations: dict[str, dict[str, dict]] = {}

    start = time.time()
    completed = 0
    total_facts = 0
    by_kind = {"semantic": 0, "episodic": 0, "procedural": 0}

    def process(item):
        sample_id, skey, ts, turns = item
        result = extract_facts_for_session(sample_id, skey, ts, turns)
        return sample_id, skey, result

    if MAX_WORKERS > 1:
        with ThreadPoolExecutor(max_workers=MAX_WORKERS) as pool:
            futs = {pool.submit(process, w): w for w in work}
            for fut in as_completed(futs):
                sample_id, skey, result = fut.result()
                conversations.setdefault(sample_id, {})[skey] = result
                total_facts += len(result["facts"])
                for f in result["facts"]:
                    by_kind[f["kind"]] = by_kind.get(f["kind"], 0) + 1
                completed += 1
                if completed % 5 == 0 or completed == len(work):
                    elapsed = time.time() - start
                    rate = completed / elapsed if elapsed > 0 else 0
                    print(
                        f"  [{completed}/{len(work)}] {rate:.1f} sess/s  "
                        f"facts so far: {total_facts}  elapsed {elapsed:.0f}s",
                        flush=True,
                    )
                if THROTTLE_SECS > 0:
                    time.sleep(THROTTLE_SECS)
    else:
        for w in work:
            sample_id, skey, result = process(w)
            conversations.setdefault(sample_id, {})[skey] = result
            total_facts += len(result["facts"])
            for f in result["facts"]:
                by_kind[f["kind"]] = by_kind.get(f["kind"], 0) + 1
            completed += 1
            if completed % 5 == 0 or completed == len(work):
                elapsed = time.time() - start
                print(
                    f"  [{completed}/{len(work)}] facts so far: {total_facts}  "
                    f"elapsed {elapsed:.0f}s",
                    flush=True,
                )

    elapsed = time.time() - start
    output = {
        "vestige_extraction_version": "1.0",
        "model": MODEL,
        "stats": {
            "conversations": len(conversations),
            "sessions": completed,
            "facts_total": total_facts,
            "facts_by_kind": by_kind,
            "extraction_time_secs": elapsed,
        },
        "conversations": conversations,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w") as f:
        json.dump(output, f, indent=2)

    print()
    print(f"Wrote {total_facts} facts across {completed} sessions to {output_path}")
    print(f"Breakdown: {by_kind}")
    print(f"Total time: {elapsed:.0f}s")


if __name__ == "__main__":
    main()

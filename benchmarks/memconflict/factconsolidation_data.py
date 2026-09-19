"""Dataset access + conflict-scenario construction for FactConsolidation.

WHY THIS FILE EXISTS
--------------------
`run.py` benchmarks MemConflict, whose unit of work is a *simulated user*: many
sessions, many questions, four arms. FactConsolidation (MemoryAgentBench,
arXiv:2507.05257) has a different shape and a different question: given a
haystack of facts in which the same (subject, relation) pair is stated more
than once with different values, does the system answer with the value of the
*last* statement? That is the competence the review calls "deterministic
freshness resolution", and it cannot be measured through `run.py` without
distorting either benchmark, so it gets its own entry point
(`factconsolidation.py`) and this shared helper module.

Everything here is Python 3 standard library, like the rest of the harness.
The parquet reading is ours for the same reason `bm25.py` is ours: the
repository does not take a dependency on pyarrow just to read one file. See
PORTING-NOTES.md for the pinned revision and the fetch/verify story.

WHAT THE DATASET LOOKS LIKE (verified against the pinned revision)
------------------------------------------------------------------
The pinned parquet holds 8 rows, one per sub-dataset:

    factconsolidation_{sh,mh}_{6k,32k,64k,262k}

`sh` = single-hop questions, `mh` = multi-hop. The four sizes are *independent
haystacks*, not nested subsets: 455 / 2310 / 4580 / 18332 numbered fact lines,
one hundred questions each, different answers in each size. Each row carries:

    context     "Here is a list of facts:\n0. <fact>\n1. <fact>\n..."
    questions   list[str]
    answers     list[list[str]]      -- one answer list per question
    metadata.source  the sub-dataset name

The official metric for this split is `substring_exact_match`
(`utils/eval_other_utils.py::calculate_metrics`): normalize the ground truth
(lowercase, drop punctuation, drop a/an/the, collapse whitespace) and ask
whether it is a substring of the model's reply. The official harness also
applies `parse_output` to pull the text after "Answer:" -- that assumes an
instructed LLM, and this harness has none, so it is not applied. That
deviation is recorded in PORTING-NOTES.md.
"""
from __future__ import annotations

import json
import re
from typing import Any, Dict, List, Optional, Sequence, Tuple

# --------------------------------------------------------------------------
# Normalization + metric (faithful port of the official scoring functions)
# --------------------------------------------------------------------------

_PUNCT_RE = re.compile(r"[^a-z0-9 ]+")
_ARTICLES_RE = re.compile(r"\b(a|an|the)\b")


def normalize_answer(text: str) -> str:
    """Port of `normalize_answer` from the official `eval_other_utils.py`.

    The upstream implementation lowercases, deletes every character in
    `string.punctuation` (it deletes, it does not replace with a space), drops
    the articles a/an/the, then collapses whitespace. Deleting punctuation
    without a space is what turns "Washington, D.C." into "washington dc", so
    it is kept bug-compatible on purpose: a harness that scores differently
    from the benchmark it claims to port is not a port.
    """
    import string as _string

    text = text.lower()
    text = "".join(ch for ch in text if ch not in _string.punctuation)
    text = _ARTICLES_RE.sub(" ", text)
    return " ".join(text.split())


def substring_exact_match(prediction: str, ground_truth: str) -> bool:
    """Port of `substring_exact_match_score`: ground truth inside prediction."""
    return normalize_answer(ground_truth) in normalize_answer(prediction)


# --------------------------------------------------------------------------
# Dataset row access
# --------------------------------------------------------------------------
#
# The canonical distribution is a HuggingFace *parquet* file. Reading parquet
# needs a real implementation: Thrift compact protocol for the footer and page
# headers, page encodings, and a Snappy decompressor. A hand-rolled stdlib
# reader was attempted here and abandoned -- it silently mis-decoded the pinned
# file's footer list header (the compact-protocol list header byte packs the
# size in the high nibble and the element type in the low nibble, and a wrong
# reading yields type ids like 252 instead of 12). Rather than ship a decoder
# that "works" only by accident, `fetch_factconsolidation.py` uses pyarrow when
# it is importable and writes a plain JSONL the rest of the harness reads with
# the standard library. See PORTING-NOTES.md section 11 for the tradeoff.

PARQUET_SOURCES = ("content", "questions", "answers", "source")


def read_dataset_rows(path: str) -> List[Dict[str, Any]]:
    """Read the pinned parquet into one dict per sub-dataset.

    Returns rows of the shape:
        {"source": str, "context": str, "questions": [str], "answers": [[str]]}

    pyarrow is required *here only*. The run path (`factconsolidation.py`)
    consumes the JSONL that `fetch_factconsolidation.py` writes, so a
    benchmark run has no third-party dependency.
    """
    try:
        import pyarrow.parquet as pq  # type: ignore
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise FactConsolidationDataError(
            "reading the parquet dataset needs pyarrow (`pip install pyarrow`). "
            "Run `python3 fetch_factconsolidation.py` once on a machine that has "
            "it, then benchmark against the JSONL it writes: the run path itself "
            "is standard library only."
        ) from exc

    table = pq.read_table(path)
    columns = set(table.column_names)
    missing = {"context", "questions", "answers"} - columns
    if missing:
        raise FactConsolidationDataError(
            f"parquet file is missing expected columns: {sorted(missing)}"
        )
    sources = table.column("metadata").to_pylist() if "metadata" in columns else None
    rows: List[Dict[str, Any]] = []
    for i, row in enumerate(table.to_pylist()):
        meta = (sources[i] or {}) if sources is not None else (row.get("metadata") or {})
        rows.append({
            "source": meta.get("source") or "",
            "context": row["context"],
            "questions": list(row["questions"] or []),
            "answers": [list(a) if isinstance(a, list) else [a]
                        for a in (row["answers"] or [])],
        })
    return rows


class FactConsolidationDataError(RuntimeError):
    """Raised when the dataset is absent, unverified, or misshapen."""


def read_jsonl_rows(path: str) -> List[Dict[str, Any]]:
    """Read the JSONL that `fetch_factconsolidation.py` writes."""
    rows: List[Dict[str, Any]] = []
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, 1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise FactConsolidationDataError(
                    f"{path}:{lineno} is not JSON: {exc}"
                ) from exc
    if not rows:
        raise FactConsolidationDataError(f"{path} contains no rows")
    return rows


# --------------------------------------------------------------------------
# Fact parsing + conflict scenarios
# --------------------------------------------------------------------------

# Fact sentence templates. Only relations whose exact wording is shared between
# the haystack and the question set are listed: scenario construction picks a
# *question* first and needs the fact side to agree with it lexically, so a
# template that is too loose would manufacture conflicts the benchmark never
# asked about. Coverage against the pinned 6k haystack is printed by
# `factconsolidation.py --describe` instead of being claimed here.
FACT_TEMPLATES: Sequence[Tuple[re.Pattern, str]] = (
    (re.compile(r"^(.+?) is a citizen of (.+?)$", re.I), "citizen_of"),
    (re.compile(r"^The capital of (.+?) is (.+?)$", re.I), "capital_of"),
    (re.compile(r"^(.+?) is located in the continent of (.+?)$", re.I), "continent"),
    (re.compile(r"^The author of (.+?) is (.+?)$", re.I), "author_of"),
    (re.compile(r"^(.+?) is associated with the sport of (.+?)$", re.I), "sport"),
    (re.compile(r"^(.+?) is married to (.+?)$", re.I), "married_to"),
    (re.compile(r"^The name of the current head of the (.+?) government is (.+?)$", re.I),
     "head_of_govt"),
    (re.compile(r"^(.+?) was performed by (.+?)$", re.I), "performed_by"),
    (re.compile(r"^The type of music that (.+?) plays is (.+?)$", re.I), "music_genre"),
    (re.compile(r"^(.+?) is famous for (.+?)$", re.I), "famous_for"),
    (re.compile(r"^(.+?) was developed by (.+?)$", re.I), "developed_by"),
    (re.compile(r"^The chief executive officer of (.+?) is (.+?)$", re.I), "ceo_of"),
    (re.compile(r"^The director of (.+?) is (.+?)$", re.I), "director_of"),
    (re.compile(r"^(.+?) died in the city of (.+?)$", re.I), "died_in"),
    (re.compile(r"^(.+?) was founded by (.+?)$", re.I), "founded_by"),
    (re.compile(r"^(.+?) is affiliated with the religion of (.+?)$", re.I), "religion"),
    (re.compile(r"^The univeristy where (.+?) was educated is (.+?)$", re.I), "educated_at"),
    (re.compile(r"^The official language of (.+?) is (.+?)$", re.I), "official_language"),
    (re.compile(r"^(.+?) was created in the country of (.+?)$", re.I), "created_in"),
    (re.compile(r"^(.+?) plays the position of (.+?)$", re.I), "position"),
    (re.compile(r"^(.+?) worked in the city of (.+?)$", re.I), "worked_in"),
    (re.compile(r"^(.+?) was born in the city of (.+?)$", re.I), "born_in"),
)

# Question templates for the single-hop sub-datasets, each mapped to the same
# relation name as the fact side above. `subj_group`/`val_group` say which
# regex group holds the subject and which holds the value, because the two
# languages are not always aligned.
QUESTION_TEMPLATES: Sequence[Tuple[re.Pattern, str, int, int]] = (
    (re.compile(r"^(.+?) is the country of citizenship of (.+?)$", re.I), "citizen_of", 2, 1),
    (re.compile(r"^(.+?) is the capital of (.+?)$", re.I), "capital_of", 2, 1),
    (re.compile(r"^(.+?) continent is (.+?) located in$", re.I), "continent", 2, 1),
    (re.compile(r"^(.+?) is the author of (.+?)$", re.I), "author_of", 2, 1),
    (re.compile(r"^(.+?) sport is (.+?) associated with$", re.I), "sport", 2, 1),
    (re.compile(r"^(.+?) is (.+?) married to$", re.I), "married_to", 2, 1),
    (re.compile(r"^(.+?) is the name of the current head of the (.+?) government$", re.I),
     "head_of_govt", 2, 1),
    (re.compile(r"^(.+?) performed (.+?)$", re.I), "performed_by", 2, 1),
    (re.compile(r"^(.+?) type of music does (.+?) play$", re.I), "music_genre", 2, 1),
    (re.compile(r"^(.+?) is (.+?) famous for$", re.I), "famous_for", 2, 1),
    (re.compile(r"^(.+?) is the developer of (.+?)$", re.I), "developed_by", 2, 1),
    (re.compile(r"^(.+?) is the chief executive officer of (.+?)$", re.I), "ceo_of", 2, 1),
    (re.compile(r"^(.+?) is the director of (.+?)$", re.I), "director_of", 2, 1),
    (re.compile(r"^(.+?) city did (.+?) die in$", re.I), "died_in", 2, 1),
    (re.compile(r"^(.+?) was (.+?) founded$", re.I), "founded_by", 2, 1),
    (re.compile(r"^(.+?) religion is (.+?) affiliated with$", re.I), "religion", 2, 1),
    (re.compile(r"^(.+?) university was (.+?) educated at$", re.I), "educated_at", 2, 1),
    (re.compile(r"^(.+?) is the official language of (.+?)$", re.I), "official_language", 2, 1),
    (re.compile(r"^(.+?) country was (.+?) created in$", re.I), "created_in", 2, 1),
    (re.compile(r"^(.+?) position does (.+?) play$", re.I), "position", 2, 1),
    (re.compile(r"^(.+?) city did (.+?) work in$", re.I), "worked_in", 2, 1),
)

SUB_DATASETS = ("factconsolidation_sh_6k", "factconsolidation_sh_32k",
                "factconsolidation_sh_64k", "factconsolidation_sh_262k",
                "factconsolidation_mh_6k", "factconsolidation_mh_32k",
                "factconsolidation_mh_64k", "factconsolidation_mh_262k")


class Fact:
    """One numbered line of the haystack, parsed into a (subject, relation, value)."""

    __slots__ = ("serial", "text", "subject", "relation", "value")

    def __init__(self, serial: int, text: str, subject: str, relation: str, value: str) -> None:
        self.serial = serial
        self.text = text
        self.subject = subject
        self.relation = relation
        self.value = value

    @property
    def key(self) -> Tuple[str, str]:
        return (self.subject, self.relation)

    def to_json(self) -> Dict[str, Any]:
        return {"serial": self.serial, "text": self.text, "subject": self.subject,
                "relation": self.relation, "value": self.value}


def parse_facts(context: str) -> Tuple[List[Fact], List[str]]:
    """Parse the haystack into Facts; also return the lines that did not parse.

    The serial is the numbering in the text, which is the document order the
    benchmark uses to decide which statement is current. It is never
    re-derived from anything else.
    """
    facts: List[Fact] = []
    unparsed: List[str] = []
    for line in context.splitlines():
        line = line.strip()
        if not line:
            continue
        m = re.match(r"^(\d+)\.\s(.*)$", line)
        if not m:
            continue  # the "Here is a list of facts:" header
        serial = int(m.group(1))
        sentence = m.group(2).strip()
        parsed = _parse_fact_sentence(sentence)
        if parsed is None:
            unparsed.append(sentence)
            continue
        subject, relation, value, _raw_subject, _raw_value = parsed
        facts.append(Fact(serial, sentence, subject, relation, value))
    return facts, unparsed


def _parse_fact_sentence(sentence: str) -> Optional[Tuple[str, str, str, str, str]]:
    s = sentence.strip().rstrip(".")
    for rx, relation in FACT_TEMPLATES:
        m = rx.match(s)
        if m:
            return (normalize_answer(m.group(1)), relation, normalize_answer(m.group(2)),
                    m.group(1), m.group(2))
    return None


def match_question(question: str) -> Optional[Tuple[str, str]]:
    """Map a question to the (subject, relation) key it asks about, or None.

    Only the single-hop question templates are known here. Multi-hop questions
    (`factconsolidation_mh_*`) compose two facts and are deliberately not
    mapped: building a "current value" ground truth for them needs the hop
    chain, and guessing it would produce numbers this harness cannot defend.
    """
    q = question.strip().rstrip("?")
    for rx, relation, subj_group, _val_group in QUESTION_TEMPLATES:
        m = rx.match(q)
        if m:
            return (normalize_answer(m.group(subj_group)), relation)
    return None


def build_scenarios(facts: Sequence[Fact], questions: Sequence[str],
                    answers: Sequence[Sequence[str]]) -> Tuple[List[Dict[str, Any]], List[str]]:
    """Build one conflict scenario per question whose key has >= 2 statements.

    A scenario is "the question plus every haystack statement about its
    (subject, relation) key, in document order". Its ground truth is the value
    of the *last* statement -- that is what "fact consolidation" means in this
    benchmark, and it is why the ingest order is part of the protocol rather
    than an implementation detail.

    Refused scenarios are returned with a reason so a reader can see the
    coverage instead of trusting it.
    """
    by_key: Dict[Tuple[str, str], List[Fact]] = {}
    for f in facts:
        by_key.setdefault(f.key, []).append(f)
    # The haystack is not guaranteed free of duplicate sentences; document
    # order is the tie-break, so a stable sort by serial is all that is needed.
    for v in by_key.values():
        v.sort(key=lambda f: f.serial)

    scenarios: List[Dict[str, Any]] = []
    refused: List[str] = []
    for q, ans in zip(questions, answers):
        key = match_question(q)
        if key is None:
            refused.append("question template not mapped")
            continue
        group = by_key.get(key)
        if not group or len(group) < 2:
            refused.append("no conflicting statement for the question's key")
            continue
        values = [f.value for f in group]
        if len(set(values)) < 2:
            refused.append("statement repeated without a conflicting value")
            continue
        gold = answers[0] if isinstance(ans, str) else (ans[0] if ans else "")
        current = group[-1]
        scenarios.append({
            "question": q,
            "key": {"subject": key[0], "relation": key[1]},
            "gold": gold,
            "current_serial": current.serial,
            "current_value": current.value,
            "superseded_serials": [f.serial for f in group[:-1]],
            "statements": [f.to_json() for f in group],
            "ground_truth_is_current": normalize_answer(gold) == current.value,
        })
    return scenarios, refused


def group_source_rows(rows: Sequence[Dict[str, Any]]) -> Dict[str, Dict[str, Any]]:
    """Index dataset rows by sub-dataset name.

    Accepts both shapes the harness sees: the reader output (a top-level
    `source` key) and a raw HuggingFace row (`metadata.source`). Keeping both
    means a caller cannot silently get an empty index by passing the other
    shape -- that mistake yields zero scenarios, which reads like a product
    failure rather than a plumbing bug.
    """
    out: Dict[str, Dict[str, Any]] = {}
    for row in rows:
        src = row.get("source") or (row.get("metadata") or {}).get("source")
        if src:
            out[src] = row
    return out

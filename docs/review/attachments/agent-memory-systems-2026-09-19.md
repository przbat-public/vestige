# Four Agent-Memory Systems: State of the Art as of 2026-09-19

Every claim below carries a source URL and its publication/release date. Where a
claim could not be verified from a primary source, it is marked **not found**.

---

## 1. Mem0 / Mem0^g

### (a) Latest version + date

- **Python SDK / PyPI `mem0ai` v2.1.0, released 2026-09-18**
  ([PyPI JSON](https://pypi.org/pypi/mem0ai/json) — upload time `2026-09-18T11:12:07Z`;
  [GitHub release v2.1.0](https://github.com/mem0ai/mem0/releases/tag/v2.1.0)).
  Requires Python `>=3.10,<4.0`.
- Same-day coordinated releases on 2026-09-18: **Node SDK v3.2.0**, Vercel AI SDK
  provider v3.0.3, Python CLI v0.2.13, and plugins for Pi, OpenCode, OpenClaw and
  DeepSeek
  ([releases feed](https://github.com/mem0ai/mem0/releases.atom)).
- License: **Apache 2.0** ([LICENSE](https://raw.githubusercontent.com/mem0ai/mem0/main/LICENSE)).
- Release cadence in 2026 is roughly weekly: 2.0.13 (2026-07-22) → 2.0.20
  (2026-09-02) → 2.1.0 (2026-09-18) ([PyPI JSON](https://pypi.org/pypi/mem0ai/json)).

### (b) URLs

Paper: <https://arxiv.org/abs/2504.19413> · Repo: <https://github.com/mem0ai/mem0> ·
Docs: <https://docs.mem0.ai> · Release notes:
<https://docs.mem0.ai/changelog/sdk> and <https://docs.mem0.ai/changelog/platform>

### (c) Architecture

- **Original two-phase pipeline (paper, 2025-04-28).** Mem0 processes an incoming
  message pair `(m_{t-1}, m_t)` in two phases: *extraction* (LLM proposes candidate
  facts using a conversation summary plus recent messages) and *update*
  ([arXiv:2504.19413v1](https://arxiv.org/html/2504.19413v1)).
- **ADD / UPDATE / DELETE / NOOP.** For each extracted fact the system retrieves the
  top-`s` semantically similar memories and presents them with the candidate fact to
  the LLM through a function-calling "tool call". The LLM itself picks one of four
  operations — ADD (no semantically equivalent memory exists), UPDATE (augment with
  complementary information), DELETE (remove contradicted memory), NOOP (no change)
  ([arXiv:2504.19413v1](https://arxiv.org/html/2504.19413v1), Algorithm 1). Because
  the LLM has no context of its own, this is the only state-tracking mechanism.
- **`Mem0^g` (graph variant).** A graph-based pipeline over **Neo4j** using
  LLM extractors and an update module via GPT-4o-mini function calling, combining
  graph representation with embeddings
  ([arXiv:2504.19413v1 §2.2](https://arxiv.org/html/2504.19413v1)).
- **Configuration in the paper:** `m` = 10 previous messages, `s` = 10 similar
  memories; all LLM operations used **GPT-4o-mini** as the inference engine; dense
  embeddings for similarity search
  ([arXiv:2504.19413v1 §2.1](https://arxiv.org/html/2504.19413v1)).
- **2026 rewrite — the two-phase pipeline is gone.** On **2026-04-16** Mem0 shipped a
  "token-efficient memory algorithm" that replaces the two passes with **single-pass,
  ADD-only extraction**: the old system "extracted memories in two LLM passes… the
  second reconciled those facts against existing memories using ADD, UPDATE, and
  DELETE operations. That reconciliation step was slow, and it was where context got
  destroyed." The new algorithm is one LLM call that only adds; changed facts live
  alongside old ones. It "cuts extraction latency roughly in half" and ships on both
  the platform and the OSS SDK
  ([Mem0 blog, 2026-04-16](https://mem0.ai/blog/mem0-the-token-efficient-memory-algorithm)).
- **Graph memory is now platform-native, not Neo4j.** The docs call the built-in
  graph "the native successor to Mem0's earlier graph store integration… Earlier
  versions connected an external graph database (Neo4j and others) and exposed a
  `relations` field. Mem0 now builds the graph itself from your memories", requiring
  no external graph store ([docs.mem0.ai graph memory](https://docs.mem0.ai/platform/features/graph-memory)).
  The platform release notes for **2026-04-16** record removal of "the legacy
  external-graph-store visualization tab, page, and its references from dashboard,
  sidebar, project settings, playground, and billing"
  ([platform release notes](https://docs.mem0.ai/changelog/platform)).

### (d) Benchmark numbers

**Paper (arXiv:2504.19413v1, 2025-04-28) — verified verbatim from the abstract:**

- "Mem0 achieves **26% relative improvements in the LLM-as-a-Judge metric over
  OpenAI**" ✔
- "**Mem0 with graph memory achieves around 2% higher overall score** than the base
  configuration" ✔
- "Mem0 attains a **91% lower p95 latency** and saves **more than 90% token cost**" ✔
  (all three from the [abstract](https://arxiv.org/abs/2504.19413)).

**The 66.9 / 68.5 figures — confirmed, with more precision.** Table 2 reports overall
LLM-as-a-Judge scores of **Mem0 = 66.88 ± 0.15%** and **Mem0^g = 68.44 ± 0.17%**
(Full-context = 72.90 ± 0.19%, Zep = 65.99 ± 0.16%, OpenAI = 52.90 ± 0.14%,
A-Mem = 48.38 ± 0.15%, LangMem = 58.10 ± 0.21%)
([arXiv:2504.19413v1 Table 2](https://arxiv.org/html/2504.19413v1)).

**Judge model — not named in the paper's main text.** The paper states only that it
uses "LLM-as-a-Judge… a separate, **more capable LLM**" and that "due to the
stochastic nature of J evaluations, we conducted **10 independent runs** for each
method on the entire dataset and report the mean scores along with ±1 s.d."
([arXiv:2504.19413v1 §3.2](https://arxiv.org/html/2504.19413v1)). GPT-4o-mini is
documented as the *inference engine* for all LLM operations and as the model used by
the re-run baselines, but the specific judge model identifier is **not found** in the
paper text; the LoCoMo audit independently notes "Only Mem0 documents a multi-run
methodology"
([locomo-audit README](https://raw.githubusercontent.com/dial481/locomo-audit/main/README.md)).

**2026 numbers (Mem0's own, platform-only features):**

| Benchmark | April 2026 algorithm | May 2026 update | Avg tokens/query |
|---|---|---|---|
| LoCoMo | 91.6% | **92.5%** | 6,956 |
| LongMemEval | 93.4% | **94.4%** | 6,787 |
| BEAM (1M) | — | 64.1% | 6,719 |
| BEAM (10M) | — | 48.6% | 6,914 |

Source: [Mem0 blog, 2026-05-14 (updated 2026-07-31)](https://mem0.ai/blog/the-token-efficient-memory-algorithm-now-has-temporal-reasoning)
and [Mem0 blog, 2026-04-16 (updated 2026-07-10)](https://mem0.ai/blog/mem0-the-token-efficient-memory-algorithm).
LongMemEval category deltas: temporal-reasoning 93.2%→97.0%, multi-session
86.5%→88.0%, knowledge-update 96.2%→**93.6% (regression)**, single-session-assistant
100.0%→98.2% (regression). LoCoMo category deltas: single-hop 92.3%→94.6%,
multi-hop 93.3%→95.4%, open-domain 76.0%→82.3%, temporal 92.8%→**92.5% (regression)**
([2026-05-14 blog](https://mem0.ai/blog/the-token-efficient-memory-algorithm-now-has-temporal-reasoning)).
*Caveat:* the April post's table (retroactively updated 2026-07-10) lists the "new
algorithm" at 92.5/94.4, while the May post describes the April algorithm as
91.6/93.4 — a versioning inconsistency in Mem0's own published numbers.

**MemConflict (2026).** The benchmark is now a paper: **arXiv:2605.20926,
"MemConflict: Evaluating Long-Term Memory Systems Under Memory Conflicts", submitted
2026-05-20** ([arXiv](http://arxiv.org/abs/2605.20926v1),
[repo](https://github.com/TaoZhen1110/MemConflict)). Third-party results on the
expanded `Step4_4.jsonl` set (30 personas, 3,750 questions) with contract v5
(qwen3.5-4b answer model, gemma-4-12b shared judge): **mem0 scores 0.392 macro answer
accuracy — second of eight providers**, behind Honcho (0.477) and ahead of
Supermemory (0.288), Hindsight (0.281/0.218), RetainDB (0.270), OpenViking (0.132)
and Mnemosyne (0.116); "no provider reaches 0.5"
([hermes-memconflict README](https://raw.githubusercontent.com/EngTurtle/hermes-memconflict/main/README.md)).
A distinct but related 2026 benchmark is **TANGLE** (arXiv:2608.13921, 2026-08-14),
which targets *irreducible* memory conflicts across 541 instances
([arXiv](https://arxiv.org/abs/2608.13921)).

**LongMemEval (2026).** 94.4% overall across 500 questions, up from 93.4%
([Mem0 blog, 2026-05-14](https://mem0.ai/blog/the-token-efficient-memory-algorithm-now-has-temporal-reasoning)).

### (e) What is genuinely new in 2026

- Single-pass **ADD-only** extraction replacing ADD/UPDATE/DELETE/NOOP
  (2026-04-16).
- **Temporal Reasoning** (extracts when an event happened, ongoing/completed state,
  timing precision; `state key` + `event_end` linking) and **Memory Decay** (1.5×
  boost for recent, dampening toward 0.3×) — both **platform-only**, added
  2026-05-04/2026-05-13 ([platform release notes](https://docs.mem0.ai/changelog/platform),
  [blog](https://mem0.ai/blog/the-token-efficient-memory-algorithm-now-has-temporal-reasoning)).
- Platform-native graph memory replacing the external Neo4j integration
  ([docs](https://docs.mem0.ai/platform/features/graph-memory)).
- `agent_custom_instructions` for agent-scoped extraction (SDK 2.0.17, 2026-08-05)
  ([SDK changelog](https://docs.mem0.ai/changelog/sdk)).

### (f) Known limitations / critiques

- **Issue #2800, "Unable to reproduce locomo eval scores locally"** — opened
  2025-05-26, now **closed**. The reporter swapped `MemoryClient` for the local
  `Memory` class and got "significantly lower" scores. Maintainer
  `prateekchhikara` replied on 2025-06-09 that "on the platform we have made some
  improvements in terms of addition and search, which is why the scores are lower
  when you use the open-source mem0" — citing Contextual ADD and Custom Instructions.
  A commenter on 2025-06-23 flagged that the paper specifies `m`=10 / `s`=10 while the
  eval scripts use batch_size=2 / topk=30
  ([issue #2800](https://github.com/mem0ai/mem0/issues/2800)).
- **Issue #3944, "Failed to reproduce the accuracy on LOCOMO via Mem0 platform"** —
  opened 2026-01-28, closed 2026-03-27. Reporter measured LLM score ≈ **0.20** via the
  *platform* with GPT-4o-mini and found a LoCoMo question whose gold answer is
  "7 May 2023" had been stored as a memory about "early January 2026" — i.e. the
  platform used wall-clock time instead of dataset timestamps. Mem0 acknowledged on
  2026-03-24 ("points to the platform storing fabricated details rather than
  extracting facts") and marked it fixed 2026-03-27
  ([issue #3944](https://github.com/mem0ai/mem0/issues/3944)).
- **Issue #4003** reports that the A-Mem baseline numbers in Mem0's paper Table 1 do
  not match the original A-Mem paper
  ([locomo-audit reproducibility.md](https://raw.githubusercontent.com/dial481/locomo-audit/main/methodology/reproducibility.md)).
- **Independent LoCoMo audit** (Feb 2026 provenance): 99 of 1,540 questions (6.4%)
  have wrong gold answers giving a **93.57% theoretical ceiling**; category sample
  sizes range 96–841 so "56% of adjacent-pair per-category comparisons [are]
  statistically indistinguishable"; and an adversarial stress test found **62.81% of
  intentionally wrong "vague-but-topical" answers were accepted by the LLM judge**
  ([locomo-audit README](https://raw.githubusercontent.com/dial481/locomo-audit/main/README.md)).
- **The Zep–Mem0 dispute** (context, not a Mem0-specific flaw): Mem0's CTO filed
  getzep/zep-papers#5 alleging Zep's 84% LoCoMo figure was inflated by a Category-5
  numerator/denominator error; Zep acknowledged the error and published a corrected
  **75.14% ± 0.17%** over 10 runs
  ([archived thread](https://raw.githubusercontent.com/lhl/agentic-memory/main/benchmarks/sources/zep-papers-issue-5.md),
  [locomo-audit](https://raw.githubusercontent.com/dial481/locomo-audit/main/methodology/reproducibility.md)).
  Wider discussion: HN item 44883133, "AI Startup Caught Cheating on Benchmark Papers"
  (cited in the same audit file).
- **Mem0^g as a distinct 2026 line of work: not found.** Searching found no new
  "Mem0^g" paper or release in 2026; `Mem0^g` remains the 2025 paper's Neo4j variant,
  superseded operationally by the platform-native graph.

---

## 2. A-MEM (Agentic Memory)

### (a)/(b) Identity, version, date

- **arXiv:2502.12110**, "A-MEM: Agentic Memory for LLM Agents"
  ([arXiv](https://arxiv.org/abs/2502.12110)). **v1 submitted 2025-02-17; latest
  v11 revised 2025-10-08** — eleven versions, no 2026 revision.
- Venue: **NeurIPS 2025** (paper comments field).
- Authors: Wujiang Xu, Zujie Liang, Kai Mei, Hang Gao, Juntao Tan, Yongfeng Zhang.
- **No GitHub releases and no official PyPI package.** `WujiangXu/A-mem` and
  `WujiangXu/A-mem-sys` both return **0 entries** on their release feeds.

### (c) Architecture

- **Zettelkasten-style atomic notes.** Each memory is a note with structured
  attributes: timestamp, content, **contextual description, keywords, tags**,
  embedding ([arXiv abstract](https://arxiv.org/abs/2502.12110)).
- **Dynamic link generation.** On insert, the system retrieves candidate historical
  notes and uses an LLM to decide whether meaningful links exist, building an
  interconnected knowledge network.
- **Memory evolution.** New memories can "trigger updates to the contextual
  representations and attributes of existing historical memories, allowing the memory
  network to continuously refine its understanding"
  ([arXiv abstract](https://arxiv.org/abs/2502.12110)).
- **Storage/retrieval** uses ChromaDB with embeddings computed over content *plus*
  generated metadata, with graph-like link traversal at read time
  ([A-mem-sys README](https://raw.githubusercontent.com/WujiangXu/A-mem-sys/main/README.md)).
- The paper's design claim is **adaptability**: fixed operations/structures in prior
  systems "limit their adaptability across diverse tasks", so A-MEM makes organization
  agent-driven ([arXiv abstract](https://arxiv.org/abs/2502.12110)).

### (d) Benchmark numbers

- The abstract claims only "**Empirical experiments on six foundation models show
  superior improvement against existing SOTA baselines**" — no headline number
  ([arXiv abstract](https://arxiv.org/abs/2502.12110)).
- A third-party deep-dive states evaluation was on **LoCoMo and DialSim**
  (multi-party long-dialogue QA), with token-length reductions vs full-context via
  top-k retrieval ([agentic-memory A-MEM reference](https://raw.githubusercontent.com/lhl/agentic-memory/main/references/xu-a-mem.md), dated 2026-02-22).
- Mem0's paper had to **re-run A-Mem with temperature 0** to obtain LLM-as-a-Judge
  scores (`A-Mem*`), and reports A-Mem at **48.38% overall J** on LoCoMo
  ([arXiv:2504.19413v1 Table 1/2](https://arxiv.org/html/2504.19413v1)) — but those
  re-run numbers are themselves disputed (issue #4003).

### (e) What is genuinely new in 2026

- **Effectively nothing.** Latest paper revision is v11 (2025-10-08); the eval repo's
  last commit is **2026-03-05** ("Add robust evaluation pipeline with vLLM/Ollama
  backend support") and the system repo's last commit is **2025-11-06** ("add gpt5
  model token limit support")
  ([A-mem commits](https://github.com/WujiangXu/A-mem/commits.atom),
  [A-mem-sys commits](https://github.com/WujiangXu/A-mem-sys/commits.atom)).
- **No "A-MEM v2" or official successor: not found.**

### (f) Known limitations / critiques

- **Memory evolution rewrites older notes without version history.** "Memory
  evolution implies rewriting older notes; without versioned history this is hard to
  debug and can be unsafe… Link generation + evolution are LLM-driven and can
  hallucinate structure unless constrained/verified"
  ([agentic-memory reference](https://raw.githubusercontent.com/lhl/agentic-memory/main/references/xu-a-mem.md)).
  The same source frames the safe-use rule as "don't silently rewrite high-influence
  memories."
- **Benchmark hygiene.** Evaluation is on LoCoMo (plus DialSim); the LoCoMo audit
  shows the benchmark's gold answers are 6.4% wrong and its per-category comparisons
  are largely underpowered
  ([locomo-audit](https://raw.githubusercontent.com/dial481/locomo-audit/main/README.md)).
- **Naming hazard.** The PyPI package **`a-mem` (v0.2.6, 2026-01-17) is a
  third-party project** — `DiaaAj/a-mem-mcp`, a "Self-evolving memory system… MCP
  server for Claude" that merely *cites* the arXiv paper as its "Research Paper". It
  is **not** the official A-MEM implementation
  ([PyPI a-mem JSON](https://pypi.org/pypi/a-mem/json)).
- **A dedicated "A-MEM is flawed" critique paper: not found.** The criticisms above
  are documented in secondary analyses and benchmark-audit tooling, not a rebuttal
  paper.

### Code + license

- Two repos: `WujiangXu/AgenticMemory` (evaluation/benchmark; also reachable as
  `WujiangXu/A-mem`) and `WujiangXu/A-mem-sys` (the system)
  ([arXiv abstract](https://arxiv.org/abs/2502.12110)).
- **Both MIT-licensed**: the eval repo "Copyright (c) 2025 Wujiang Xu, Zujie Liang,
  Kai Mei, Hang Gao, Juntao Tan, Yongfeng Zhang", the system repo "Copyright (c) 2025
  AGI Research" ([A-mem LICENSE](https://raw.githubusercontent.com/WujiangXu/A-mem/main/LICENSE),
  [A-mem-sys LICENSE](https://raw.githubusercontent.com/WujiangXu/A-mem-sys/main/LICENSE)).

---

## 3. MemOS (MemTensor)

### (a)/(b) Identity, version, date

- **Short/vision paper: arXiv:2505.22101**, "MemOS: An Operating System for
  Memory-Augmented Generation (MAG) in Large Language Models", **submitted 2025-05-28,
  v1 only**, CC BY 4.0 ([arXiv](https://arxiv.org/abs/2505.22101)).
- **Full paper: arXiv:2507.03724**, "MemOS: A Memory OS for AI System", **v1
  2025-07-04, v4 2025-12-03** ([arXiv](https://arxiv.org/abs/2507.03724)).
- Latest release: **GitHub v2.0.33, 2026-09-03**; PyPI **`MemoryOS` 2.0.33,
  2026-09-03T11:30:01Z** ([release](https://github.com/MemTensor/MemOS/releases/tag/v2.0.33),
  [PyPI MemoryOS](https://pypi.org/pypi/MemoryOS/json)). Repo HEAD is already on
  v2.0.34 dev (commit 2026-09-16)
  ([commits](https://github.com/MemTensor/MemOS/commits.atom)).
- License: **Apache 2.0**
  ([LICENSE](https://raw.githubusercontent.com/MemTensor/MemOS/main/LICENSE)).

### (c) Architecture

- **MemCube** — "a standardized memory abstraction that enables tracking, fusion, and
  migration of heterogeneous memory, while offering structured, traceable access
  across tasks and contexts"; it "encapsulates both memory content and metadata such
  as provenance and versioning"
  ([2505.22101 abstract](https://arxiv.org/abs/2505.22101),
  [2507.03724 abstract](https://arxiv.org/abs/2507.03724)).
- **Three memory types** unified under one scheduler: **plaintext, activation
  (KV-cache), and parameter (weights/LoRA)** memory
  ([2505.22101 abstract](https://arxiv.org/abs/2505.22101)).
- **MemScheduler** — "Asynchronous Ingestion via MemScheduler: Run memory operations
  asynchronously with millisecond-level latency for production stability under high
  concurrency" ([MemOS README](https://raw.githubusercontent.com/MemTensor/MemOS/main/README.md)).
- **Unified Memory API + multi-cube KBs** — add/retrieve/edit/delete over a
  graph-structured store (not a black-box embedding store), with composable memory
  cubes for isolation and sharing across users, projects and agents
  ([MemOS README](https://raw.githubusercontent.com/MemTensor/MemOS/main/README.md)).
- **Implementation reality vs. paper** (third-party code audit, updated 2026-03-25):
  Neo4j is the primary store with vector search via Neo4j's native index; the
  plaintext path is mature while "**activation memory (KV-cache) has basic support**"
  and "**parametric memory (LoRA) has a stub implementation (`lora.py`) that is
  essentially a placeholder**". Critically, the `Reorganizer`'s relation/reasoning
  detection "is largely disabled in the current code (commented out)", and there is
  "no sleep-like offline consolidation, no scheduled background processing, and no
  decay mechanism" ([somnigraph MemOS analysis](https://raw.githubusercontent.com/AlexisOlson/somnigraph/main/research/sources/memos.md)).

### MemOS 2.0

**Yes, MemOS 2.0 exists** — branded **"MemOS 2.0 Stardust（星尘）"** in the README
([README](https://raw.githubusercontent.com/MemTensor/MemOS/main/README.md)). The
lineage: memos-local-plugin 2.0 on **2026-05-09**, OpenClaw plugins on **2026-03-08**,
Hermes plugin on **2026-04-10**, and the current v2.0.x core series through
**2026-09-03**; the README's news feed also announces **DeepSeek Harness support on
2026-08-17** and a local plugin release v2.0.19 on **2026-09-08**
([README](https://raw.githubusercontent.com/MemTensor/MemOS/main/README.md),
[releases feed](https://github.com/MemTensor/MemOS/releases.atom)).

### (d) Benchmark numbers and verification

**MemOS's own README table** (evaluated via their OmniMemEval harness against
commercial memory products across 5 user-memory + 5 agent-memory tasks)
([README](https://raw.githubusercontent.com/MemTensor/MemOS/main/README.md)):

| Benchmark | Score |
|---|---|
| LoCoMo | 88.83 |
| LongMemEval | 89.20 |
| PersonaMem v2 | 40.58 |
| HaluMem | 80.91 |
| BEAM-10M | 56.75 |
| GDPVal | 62.07 |
| LiveCodeBench | 64.96 |
| OmniMath | 61.00 |
| SWE-Bench | 38.46 |
| BrowseComp-Plus | 23.85 |

Also claimed: OpenClaw average task completion **36.63% → 50.87%** across five agent
tasks, and leadership in OmniMemEval over 14 commercial products
([README, news dated 2026-07-02](https://raw.githubusercontent.com/MemTensor/MemOS/main/README.md)).

**Paper numbers differ from README numbers.** The 2507.03724v4 paper reports SOTA on
**LoCoMo 75.80** overall LLM judge, **LongMemEval 77.8**, PreFEval 77.2, PersonaMem
61.2 precision, beating MIRIX (64.33), **Mem0 (64.57)**, Zep (59.22), Memobase
(72.01), MemU (56.55), Supermemory (55.34) — all on a GPT-4o-mini backbone. It also
claims up to **94.2% TTFT reduction** from KV-cache memory on Qwen models and **100%
API success rate at 100 QPS** (vs. degradation in all other systems)
([somnigraph analysis of 2507.03724v4](https://raw.githubusercontent.com/AlexisOlson/somnigraph/main/research/sources/memos.md)).

**Third-party verification is mixed.** The same audit rates the benchmark comparisons
"Strong" (consistent GPT-4o-mini backbone, five task types) but rates three
architectural claims as **"Overreached"** or **"Partially implemented"**: the
three-memory-type hierarchy (only plaintext is mature), MemLifecycle ("No freeze, no
rollback, no TTL enforcement"), and hierarchical topic-concept-fact graph
organization ("graph nodes are not themselves organized into topic-concept-fact
layers"). Independent **PERMA** results (arXiv:2603.23231) show MemOS best-in-class
on MCQ accuracy and robustness to noise, but with the same ~44% multi-domain collapse
as every other system
([somnigraph analysis](https://raw.githubusercontent.com/AlexisOlson/somnigraph/main/research/sources/memos.md)).
A peer-reviewed independent reproduction of the OmniMemEval/README numbers was
**not found**.

### (e) What is genuinely new in 2026

- MemOS 2.0 "Stardust" core series (v2.0.16 → v2.0.33 between 2026-08-24 and
  2026-09-03).
- Agent-ecosystem plugins: OpenClaw (cloud + local, **2026-03-08**), Hermes Agent
  local plugin (**2026-04-10**), memos-local-plugin 2.0 (**2026-05-09**), DeepSeek
  Harness adapter (**2026-08-17/2026-08-28**)
  ([README](https://raw.githubusercontent.com/MemTensor/MemOS/main/README.md),
  [v2.0.32 notes](https://github.com/MemTensor/MemOS/releases/tag/v2.0.32)).
- SkillMemory / preference-memory fixes and per-model Redis GCRA rate limiting
  ([v2.0.33](https://github.com/MemTensor/MemOS/releases/tag/v2.0.33)).

### (f) The MemoryOS confusion — clarified

There are **three distinct things**, and the arXiv ID in the research brief is wrong:

1. **MemOS** (MemTensor) — arXiv:2505.22101 / arXiv:2507.03724. The subject of this
   section. **Its PyPI package is named `MemoryOS`**, which is the source of much of
   the confusion ([PyPI MemoryOS](https://pypi.org/pypi/MemoryOS/json)).
2. **MemoryOS** (a different 2025 system) — **arXiv:2506.06326, "Memory OS of AI
   Agent", submitted 2025-05-30**. Inspired by OS memory-management principles:
   hierarchical storage with short-term / mid-term / long-term personal memory, a
   dialogue-chain FIFO rule for short→mid updates, and "segmented page organization"
   for mid→long updates; reports "an average improvement of **49.11% on F1 and 46.18%
   on BLEU**" on LoCoMo ([arXiv:2506.06326](https://arxiv.org/abs/2506.06326)).
3. **arXiv:2505.13516 is neither.** That ID resolves to **"HALO: Hierarchical
   Autonomous Logic-Oriented Orchestration for Multi-Agent LLM Systems"** (submitted
   2025-05-17) — a multi-agent orchestration framework with MCTS workflow search,
   unrelated to memory systems ([arXiv:2505.13516](https://arxiv.org/abs/2505.13516)).

---

## 4. cognee

### (a)/(b) Latest version + date

- **cognee v1.6.0, released 2026-09-18** — GitHub tag `v1.6.0` published
  `2026-09-18T22:50:51Z`, PyPI upload `2026-09-18T22:50:42Z`
  ([release](https://github.com/topoteretes/cognee/releases/tag/v1.6.0),
  [PyPI cognee](https://pypi.org/pypi/cognee/json)).
- **cognee v1.0.0 shipped 2026-04-11** (PyPI `1.0.0`, `2026-04-11T18:17:18Z`); the
  1.x line then ran 1.1.0 (2026-05-16) → 1.2.0 (2026-06-21) → 1.3.0 (2026-07-12) →
  1.4.0 (2026-07-17) → 1.5.x (Aug 2026) → 1.6.0
  ([PyPI JSON](https://pypi.org/pypi/cognee/json)).
- License: **Apache 2.0**
  ([LICENSE](https://raw.githubusercontent.com/topoteretes/cognee/main/LICENSE)).

### (c) Architecture

- **ECL pipeline — Extract, Cognify, Load.** "Rather than treating knowledge as a bag
  of embedded chunks, it builds a structured, persistent knowledge graph using a
  pipeline it calls ECL: Extract, Cognify, Load"
  ([cognee blog, 2026-03-17](https://www.cognee.ai/grounding-ai-memory)).
- **Ontology-based entity validation.** An ontology is "an optional RDF/OWL file you
  provide as a reference vocabulary"; the validation layer sits inside `cognify()`,
  matching entity types to OWL classes (80% fuzzy cutoff), canonicalizing node names
  to URI-derived forms, BFS-attaching ontology subgraph relationships, and tagging
  every node `ontology_valid = True/False`. Ontologies are parsed via **RDFLib**, so
  RDF/XML, Turtle, N-Triples and JSON-LD all work
  ([cognee blog, 2026-03-17](https://www.cognee.ai/grounding-ai-memory)).
- **Graph + vector in one store.** Cognee positions this as "A single Postgres for
  graph, vectors, sessions, and metadata" with "Hybrid search with no separate index
  to build or keep in sync"
  ([cognee vs mem0](https://www.cognee.ai/cognee-vs-mem0)).
- **cognee 1.0 memory-native API (2026):** four verbs — `remember`, `recall`,
  `forget`, `improve` — replacing `add`/`cognify`/`search`, with **Ladybug as the
  default embedded graph store** and a self-improvement loop that re-weights memory
  from feedback, importance and frequency (on by default via
  `self_improvement=True`) ([Inside cognee 1.0](https://www.cognee.ai/inside-cognee-1-0)).
- **Temporal awareness.** v1.5.4rc1 (2026-09-15) added a
  `HybridDecompositionRetriever` for context-primed query decomposition, structured
  machine-readable context evidence from `HYBRID_COMPLETION`, external metadata
  stamped on chunks, and an **LLM-free GLiNER graph-extraction backend** plus typed
  edges ([release notes](https://github.com/topoteretes/cognee/releases/tag/v1.5.4rc1)).
  v1.6.0 added keyless-first local model flows, crash-recoverable pipeline runs,
  per-dataset embedding-model recording, a `cognee-mcp` client/server, and telemetry
  privacy (dataset names fingerprinted before leaving the host)
  ([release notes](https://github.com/topoteretes/cognee/releases/tag/v1.6.0)).

### (d) Benchmark numbers (their own claims)

**Head-to-head on HotPotQA** (24-question subset, 45 repeated runs per system on
Modal, DeepEval scoring, **competitors at published defaults, cognee tuned**)
([cognee blog, 2026-01-07](https://www.cognee.ai/knowledge-graph-memory-benchmarks)):

| System | Config | Human-like correctness | DeepEval correctness | F1 |
|---|---|---|---|---|
| Cognee | GRAPH_COMPLETION_COT, tuned | 0.93 | **0.85** | **0.84** |
| Graphiti | LangChain + Neo4j, default | 0.88 | 0.74 | 0.70 |
| LightRAG | default | 0.96 | 0.67 | 0.09 |
| **Mem0** | OpenAI memory QA, default | 0.72 | 0.54 | 0.12 |

**Paper (Markovic et al., 2025)** — baseline vs. tuned vs. hold-out
([cognee blog, 2026-01-07](https://www.cognee.ai/knowledge-graph-memory-benchmarks)):
HotPotQA correctness 0.476 → 0.815 (hold-out 0.715); TwoWikiMultiHop 0.348 → 0.582
(hold-out F1 0.704); MuSiQue 0.414 → 0.674 (hold-out 0.596).

**BEAM (cognee blog, 2026-06-26, updated 2026-09-03):** **0.79 at 100K** vs. reported
SOTA 0.735, and **0.67 at 10M** vs. reported SOTA 0.641; the company states the first
is "+6.5%" over SOTA and the 10M result is "on par"
([cognee on BEAM](https://www.cognee.ai/benchmarking-cognee-on-beam)).

**cognee vs mem0 on BEAM 10M:** cognee 67% vs. **mem0 48.6%**
([cognee vs mem0](https://www.cognee.ai/cognee-vs-mem0)). Note this is consistent
with Mem0's own published BEAM-10M figure of 48.6%
([Mem0 blog, 2026-04-16](https://mem0.ai/blog/mem0-the-token-efficient-memory-algorithm)).

### (e) What is genuinely new in 2026

- **v1.0 and the memory-native API** (2026-04-11 / announced 2026-06-26): four verbs,
  typed return shapes, Python + TypeScript SDKs, Ladybug default embedded graph store,
  and explicit **migration paths from Mem0, Zep and Letta**
  ([Inside cognee 1.0](https://www.cognee.ai/inside-cognee-1-0)).
- Self-improvement loop on by default; feedback attached to the nodes/edges/evidence
  that produced an answer.
- BEAM evaluation at 100K and 10M scale, positioned as "without a benchmark-specific
  memory system" ([cognee on BEAM](https://www.cognee.ai/benchmarking-cognee-on-beam)).
- v1.6.0's keyless-first operation (2026-09-18).

### (f) Known limitations

- **Self-declared bias.** "This is cognee's benchmark, not an independent study. We
  built the harness and we published the numbers. That is useful but not neutral."
  The same post discloses "**cognee was running tuned and Graphiti wasn't. We didn't
  run a tuning sweep on the competitors**", and that the head-to-head is a
  **24-question subset**, "not directly comparable to the full-dataset paper numbers"
  ([cognee blog, 2026-01-07](https://www.cognee.ai/knowledge-graph-memory-benchmarks)).
- **BEAM 10M caveat, in cognee's own words.** "The margin is small, and the number
  should be taken with a grain of salt. This result did involve **more parameter
  exploration on the target conversation than we'd like**… we're not trying to
  overstate the meaning of a single 10M run." Multi-turn agentic retrieval pushed the
  score higher but was rejected as "outside the spirit of the benchmark"
  ([cognee on BEAM](https://www.cognee.ai/benchmarking-cognee-on-beam)).
- **Breaking changes in v1.6.0:** "GLiNER removed from default Docker image" and
  "Database & adapter refactors may require review… Self-hosted deployments or custom
  adapter integrations should review their configuration and custom adapter code for
  compatibility" ([release notes](https://github.com/topoteretes/cognee/releases/tag/v1.6.0)).
- **Benchmark noise acknowledged.** "Academic benchmarks… can contain noisy corpus
  text, overstate their purpose, and feature golden question-answer pairs of varying
  quality"; BEAM's turn references "contain some errors and inconsistencies"
  ([cognee on BEAM](https://www.cognee.ai/benchmarking-cognee-on-beam)).
- A fully independent third-party reproduction of cognee's BEAM or HotPotQA numbers
  was **not found**.

---

## Cross-cutting takeaways

1. **Mem0's headline architecture changed in 2026** — the ADD/UPDATE/DELETE/NOOP
   reconciliation described in the widely-cited paper was replaced by ADD-only
   single-pass extraction on 2026-04-16, and the external Neo4j graph by a
   platform-native graph. Papers and blog posts citing the four-operation pipeline as
   current are describing the 2025 system.
2. **Every LoCoMo-based number in this space inherits a 6.4%-wrong gold set and
   underpowered per-category comparisons**, per the independent
   [locomo-audit](https://raw.githubusercontent.com/dial481/locomo-audit/main/README.md).
   Only Mem0 documents multi-run methodology; most systems report single-run point
   estimates.
3. **Benchmarks are shifting from LoCoMo to LongMemEval, BEAM, MemConflict and
   PERMA** in 2026 — long-horizon and conflict-resolution settings where all systems
   score far lower (mem0 leads no MemConflict provider; no provider exceeds 0.5 macro
   accuracy).
4. **Vendor benchmark numbers should be treated as marketing until independently
   reproduced.** Cognee says so about its own work; MemOS's README (LoCoMo 88.83)
   and its paper (LoCoMo 75.80) disagree with each other; Mem0's April 2026 post was
   retroactively edited to show the May 2026 numbers.
5. **The "MemoryOS" name is a three-way collision** — MemTensor's MemOS publishes to
   PyPI *as* `MemoryOS`, while arXiv:2506.06326 is a separate MemoryOS system, and
   arXiv:2505.13516 (cited in the brief) is an unrelated multi-agent paper.

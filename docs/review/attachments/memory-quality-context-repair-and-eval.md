# Retrieval-Time Context Repair, and Whether Anyone Measures Whether a Memory Stands Alone

**Angle:** the read side. What happens *after* storage, when a retrieved memory turns out
to be an unresolvable scrap — and whether any published evaluation would ever have caught it.

**Date:** 2026-09-20. **Scope:** primary sources only (papers, official docs, shipped prompt
files). Vendor blogs and product marketing are labelled as claims at the point of use.
Negative findings are stated as findings.

---

## 0. The problem, restated precisely

Two distinct defects are being conflated in the bug report, and they have different fixes:

| Defect | What is broken | Where it can be fixed |
|---|---|---|
| **D1 — deictic / anaphoric hole** | The text contains a reference whose referent lived in the source conversation: unresolved pronouns, "this", "the above", "yesterday", "next Friday", "the second option". | Write time (rewrite), or read time (rehydrate from provenance) |
| **D2 — external pointer** | The text points outside memory at an artefact that rots: `src/foo.rs:412`, a branch URL, a commit that was force-pushed away. | Write time (pin), or read time (resolve or fail loudly) |

Both share one property: **the memory's meaning is not recoverable from the memory**. That
property — call it *self-containedness*, *context independence*, *decontextuality* — is the
thing this report went looking for in the evaluation literature.

---

## 1. Q1 — Does any benchmark measure self-containedness of a stored memory?

**Verdict: no. This is a genuine negative finding, and it is a big one.**

Every agent-memory benchmark in current use grades the **downstream answer**, or at best the
**retrieval rank of a gold item**. None grades the stored artefact. The construct *is*
formalised — but in natural-language-generation, for sentence rewrites and extracted claims,
not for memory-store records.

### 1.1 The benchmark metric inventories

**LoCoMo** — [arXiv:2402.17753](https://arxiv.org/abs/2402.17753) ·
[HTML](https://arxiv.org/html/2402.17753v1). Three tasks: question answering, event
summarization, multi-modal dialogue generation. QA is scored by F1 over normalised
answers; event summarization by an adapted FactScore precision/recall → F1; MM-dialogue by
MM-Relevance. The memory representations (sessions, "observations", session summaries) are
retrieval units scored by Recall@k — they are never themselves graded. Coreference appears in
the paper only as a *difficulty of the data* ("temporal and causal coreferences present in the
dialogues"), never as a metric.

**LongMemEval** — [arXiv:2410.10813](https://arxiv.org/abs/2410.10813) ·
[v2 HTML](https://arxiv.org/html/2410.10813v2). Five abilities: information extraction,
multi-session reasoning, temporal reasoning, knowledge updates, abstention. Two metrics:
question answering (GPT-4o LLM judge; the authors report >97% agreement with human experts)
and **Memory Recall** (Recall@k, NDCG@k). No record-quality measure.

> **The one place a self-containedness requirement *is* stated in a benchmark — as a prompt
> instruction, not a metric.** LongMemEval's fact-extraction prompt (Figure 11 of the v2 HTML)
> reads:
>
> > "Make sure you include all details such as life events, personal experience, preferences,
> > specific numbers, locations, or dates. State each piece of information in a simple
> > sentence. Put these sentences in a json list, each element being a **standalone personal
> > fact** about the user. **Minimize the coreference across the facts, e.g., replace pronouns
> > with actual entities.**"

This is the closest the memory-benchmark literature comes to our construct — and it is
enforced only by prompt compliance and measured only indirectly, through downstream QA.

**LongMemEval-V2** — [arXiv:2605.12493](https://arxiv.org/abs/2605.12493) ·
[repo](https://github.com/xiaowu0162/LongMemEval-V2). 451 curated questions, five abilities
(static state recall, dynamic state tracking, workflow knowledge, environment gotchas, premise
awareness). Metric, §3.3: *"We report answer accuracy and query latency. Accuracy is computed
by normalized string matching for structured answers and an LLM judge for free-form answers."*
The memory backend returns context items that are truncated (200k) and handed to a fixed
reader; retrieval is scored only through the answer. The leaderboard's **LAFS** score is
accuracy × latency against a reference frontier (repo `leaderboard/README.md`); no closed-form
formula is published, and the term does not appear in the paper itself.

**MemoryAgentBench** — [arXiv:2507.05257](https://arxiv.org/abs/2507.05257) (ICLR 2026) ·
[repo](https://github.com/HUST-AI-HYZ/MemoryAgentBench) ·
[data](https://huggingface.co/datasets/ai-hyz/MemoryAgentBench). Four competencies: Accurate
Retrieval (AR), Test-Time Learning (TTL), Long-Range Understanding (LRU), Selective Forgetting
(SF). The repo's own metric-clarification table maps every dataset to `substring_exact_match`,
`exact_match`, `Recall@5`, LLM-judge, or F1 (HELMET-style) — all end-task. Note the repo now
labels the fourth competency **"Conflict Resolution (CR)"** while paper v4 says **"Selective
Forgetting (SF)"**; the datasets (`fact_sh`, `fact_mh`) are the same.

The SF definition (Appendix B.4.1) is explicitly a *capability of the agent*, not a property
of a memory item:

> "We define Selective Forgetting (SF) as the agent's ability to detect and resolve
> contradictions between out of date knowledge and newly acquired information, ensuring the
> agent remains aligned with current realities and user states."

**FactConsolidation** is built from MQUAKE counterfactual edit pairs — a true fact and a
rewritten contradictory version, ordered so the rewrite appears later, concatenated into 6K /
32K / 64K / 262K-token contexts. Single-hop and multi-hop variants. Scored with SubEM
(substring exact match) on QA (Appendix B.4.2).

**There is no "contextual" or "abstraction" axis in MemoryAgentBench.** The word "abstractive"
appears once, as a typo for "Accurate Retrieval" in B.4.1; the LRU competency mentions
"abstract, high-level comprehension" in prose but is scored by F1/Accuracy. I checked this
directly against the v4 HTML.

MemoryAgentBench's memory-construction prompts (Appendix D.1, Figure 4) are, in full, of the
form *"The following context is the facts that I have learned: ⟨chunk⟩. Please memorize it and
I will need you to answer the questions based on the order of facts."* — **no instruction
anywhere about disambiguation, standalone-ness, or reference resolution.**

**MemConflict** — [arXiv:2605.20926](https://arxiv.org/abs/2605.20926). This one is the
strongest case of grading the *memory* rather than only the answer. It "treats memory validity
as a query-conditioned fitness-for-use problem" and runs a two-level protocol: black-box
**Answer Accuracy** `AA = (1/N)Σ1(ŷᵢ = y*ᵢ)` plus white-box memory metrics —
**Support Evidence Hit@K** `SEH@K = (1/N)Σ1(m*ᵢ ∈ Rᵢᴷ)` and
**Support Rank Score** `SRS = (1/N)Σ 1/log₂(rank(m*ᵢ)+1)`, plus UOCS/CRS. But SEH@K and SRS
grade *whether the right memory was retrieved and ranked*, not whether the retrieved memory is
readable on its own. The paper's own diagnostic categories are "failures from missing
supporting memories and ineffective use of retrieved memories", and its headline finding is
that "answer correctness often diverging from memory retrieval and ranking".

**Other 2026 benchmarks checked, all end-task:**

- **AgentMemBench** — [arXiv:2608.00009](https://arxiv.org/abs/2608.00009): Recall@k, MRR,
  nDCG@k, Answer F1, LLM-judge Faithfulness, Memory Footprint, Latency.
- **PERMA** — [arXiv:2603.23231](https://arxiv.org/abs/2603.23231): the closest to grading
  memory text. Its **Memory Fidelity** dimension is "the BERT-f1 between the retrieved context
  and ground-truth dialogues, complemented by a Memory Score … derived via 'LLM-as-a-judge' …
  [which] penalizes the omission of critical persona facets while rewarding concise,
  task-relevant retrievals". Gold-relative, not standalone-ness.
- **DeMem** — [arXiv:2605.10870](https://arxiv.org/abs/2605.10870): a **Decision-Compatibility
  Audit** over an annotated LoCoMo subset with four support-level metrics — Support Purity,
  Within-slot Conflict Rate, Gold-support Coverage, Support Contamination. Again gold-relative.
- **Neo4j Agent Memory eval harness** (official docs) —
  [neo4j.com/labs/agent-memory/how-to/evaluation](https://neo4j.com/labs/agent-memory/how-to/evaluation/):
  exactly three dimensions — retrieval relevance (Recall@k), audit completeness, preference
  fidelity (F1). The page states plainly that it is "a scaffold, not a benchmark".

### 1.2 Where the construct *is* defined and scored: NLG decontextualization

The right construct exists, one field over.

**Choi et al., "Decontextualization: Making Sentences Stand-Alone", TACL 2021** —
[aclanthology.org/2021.tacl-1.27](https://aclanthology.org/2021.tacl-1.27/) ·
[arXiv:2102.05169](https://arxiv.org/html/2102.05169).
*(Note: the frequently-cited id 2102.05127 is an unrelated number-theory paper.)*

> "We isolate and define the problem of sentence decontextualization: taking a sentence
> together with its context and rewriting it to be interpretable out of context, while
> preserving its meaning."

The formal definition: *"Given a sentence-context pair (s,c), a sentence s′ is a valid
decontextualization of s if: (1) the sentence s′ is interpretable in the empty context; and
(2) the truth-conditional meaning of s′ in the empty context is the same as the
truth-conditional meaning of s in context c."*

Metrics: SARI as the main automatic metric, plus a human evaluation asking *"(b) is it
sufficiently and correctly decontextualized?"*, where "expert annotators marked as
'sufficient' those items for which all possible referential ambiguities had been resolved."
This is precisely the gate we want — and it has never been ported to memory records.

**Gunjal & Durrett, "Molecular Facts"** — [arXiv:2406.20079](https://arxiv.org/html/2406.20079v1)
extends it to extracted facts:

> "**Criterion 1 (Decontextuality):** When interpreted as a standalone statement, mᵢ must have
> the truth conditional meaning I(cᵢ, x, r). That is, it should uniquely specify entities,
> events, and other context such that the claim cᵢ is now interpretable."

Plus a **Minimality** criterion (don't over-specify). Evaluated by human ambiguity analysis,
not by an automatic score.

**The negative result that matters:** Newman et al., EMNLP 2023 —
[aclanthology.org/2023.emnlp-main.193](https://aclanthology.org/2023.emnlp-main.193/). Their
decontextualization framework decomposes into question generation → question answering →
rewriting, and "we conclude with analysis that finds, while rewriting is easy, **question
generation and answering remain challenging for today's models**."

### 1.3 The one production artefact that states the rule outright

Two shipped prompt files state self-containedness as a hard requirement — which is the
strongest evidence that practitioners hit this exact problem, and simultaneously evidence that
nobody verifies compliance.

**Mem0 OSS** — `mem0/configs/prompts.py` (verified by fetching the file):
[raw file](https://raw.githubusercontent.com/mem0ai/mem0/main/mem0/configs/prompts.py)

> "### Self-Contained
> Every memory must be understandable on its own. Replace all pronouns with specific names or
> "User.""

with the output contract *"text (string, required): A contextually rich, self-contained
factual statement (15-80 words)."* and a companion rule *"NEVER sacrifice a proper noun, title,
date, or specific detail to meet a word count — completeness beats brevity."* The extraction
prompt also supplies "Last k Messages … Use to resolve references and pronouns in New
Messages" — i.e. it feeds the antecedent in at write time.

**MemOS** — `src/memos/templates/mem_reader_prompts.py`:
[raw file](https://raw.githubusercontent.com/MemTensor/MemOS/main/src/memos/templates/mem_reader_prompts.py).
A post-extraction validator/rewriter that instructs the model to *"Resolve all pronouns,
aliases, and ambiguous references into full names or identities"* and to emit *"A detailed,
self-contained, and unambiguous memory statement"*, under the constraint *"Include only what
the user explicitly stated … no assumptions, interpretations, predictions, or generalizations
NOT supported by the text."*

**Letta's sleep-time agent system prompt — the most explicit statement of the rule found
anywhere, and it targets *deictic time* specifically.**
[`letta/prompts/system/sleeptime.txt` @ 0.7.0](https://raw.githubusercontent.com/letta-ai/letta/0.7.0/letta/prompts/system/sleeptime.txt):

> "You run in the background, organizing and maintaining the memories of an agent assistant who
> chats with the user. … When writing to memory blocks, make sure to be precise when referencing
> dates and times (for example, **do not write "today" or "recently", instead write specific
> dates and times, because "today" and "recently" are relative, and the memory is persisted
> indefinitely**). … You should continue memory editing until the blocks are organized and
> readable, and do not contain redundant and outdate information"

Its companion tool, `rethink_memory`, is documented as: *"Rewrite memory block for the main
agent, new_memory should contain all current information from the block that is not outdated or
inconsistent, integrating any new information, resulting in a new memory block that is
organized, readable, and comprehensive"*
([`letta/functions/function_sets/base.py` @ 0.7.0](https://raw.githubusercontent.com/letta-ai/letta/0.7.0/letta/functions/function_sets/base.py)).
This is the clearest published statement that a persisted memory must not carry relative time
references — and it is enforced by prompt compliance, with no verifier and no score.

**Anthropic's Contextual Retrieval** (official engineering post, 2024-09-19) —
[anthropic.com/engineering/contextual-retrieval](https://www.anthropic.com/engineering/contextual-retrieval).
This is the clearest published statement of D1 from a vendor, with the motivating example:

> *original_chunk = "The company's revenue grew by 3% over the previous quarter."*
> "this chunk on its own doesn't specify which company it's referring to or the relevant time
> period, making it difficult to retrieve the right information or **use the information
> effectively**."

The fix is index-time: prepend 50–100 tokens of generated context before embedding and before
building the BM25 index, using the prompt *"Please give a short succinct context to situate
this chunk within the overall document for the purposes of improving search retrieval of the
chunk. Answer only with the succinct context and nothing else."* **Claim (vendor-reported,
with appendix):** contextual embeddings reduced top-20 retrieval failure 5.7%→3.7% (−35%);
with contextual BM25 5.7%→2.9% (−49%); with reranking 5.7%→1.9% (−67%). The metric is
`1 − recall@20` — a retrieval metric, not a standalone-ness metric.

### 1.4 Summary table for Q1

| Benchmark / system | Grades the answer | Grades the retrieved item | Grades standalone-ness |
|---|---|---|---|
| LoCoMo | ✅ F1 / FactScore-F1 / MM-Relevance | Recall@k only | ❌ |
| LongMemEval | ✅ LLM-judge QA | Recall@k, NDCG@k | ❌ (requirement stated in an extraction prompt, unmeasured) |
| LongMemEval-V2 | ✅ accuracy + latency | — | ❌ |
| MemoryAgentBench | ✅ SubEM / EM / Recall@5 / F1 | — | ❌ (no contextual/abstraction axis exists) |
| MemConflict | ✅ AA | ✅ SEH@K, SRS | ❌ |
| AgentMemBench | ✅ F1, Faithfulness | ✅ Recall@k, MRR, nDCG@k | ❌ |
| PERMA | ✅ task perf | ✅ BERT-F1 + judge Memory Score | ❌ |
| DeMem audit | ✅ judge mean | ✅ purity/coverage/contamination | ❌ |
| Neo4j harness | — | ✅ Recall@k, recall, F1 | ❌ |
| **Choi et al. TACL 2021** | — | — | ✅ **but for sentence rewrites, not memories** |

---

## 2. Q2 — Retrieval-time techniques that repair an unresolvable reference

### 2.0 The load-bearing distinction

Techniques split cleanly, and the split determines whether they help D1/D2 at all:

- **Index/write time:** Anthropic Contextual Retrieval; GraphRAG community-summary
  construction; LongMemEval's fact-augmented key expansion; Cline Memory Bank.
- **Retrieval time:** query rewriting/expansion (HyDE, Query2doc, multi-query), conversational
  query rewriting (ConvDR, ConvSearch-R1), time-aware query expansion, agentic/multi-hop loops
  (Self-RAG, IRCoT, Auto-RAG, Iter-RetGen, FLARE, MemGPT paging), reranking, GraphRAG's
  map-reduce over pre-built community reports.

### 2.1 Query expansion and rewriting

- **HyDE** — [arXiv:2212.10496](https://arxiv.org/abs/2212.10496): *"Given a query, HyDE first
  zero-shot instructs an instruction-following language model … to generate a hypothetical
  document … Then, an unsupervised contrastively learned encoder … encodes the document into an
  embedding vector … the encoder's dense bottleneck filtering out the incorrect details."*
  Reported to match fine-tuned retrievers without training.
- **Query2doc** — [arXiv:2303.07678](https://arxiv.org/abs/2303.07678): pseudo-documents by
  few-shot prompting; *"boosts the performance of BM25 by 3% to 15% on ad-hoc IR datasets …
  without any model fine-tuning."*
- **Multi-query + RRF (RAG-Fusion)** — [repo](https://github.com/Raudaschl/RAG-Fusion);
  LangChain's `MultiQueryRetriever`. Note: the framework docs document the *pattern* but do not
  report measured gains, so treat HyDE/Query2doc as the quantitative evidence.
- **Anthropic evaluated HyDE and did not adopt it** — the contextual-retrieval post lists
  "hypothetical document embedding" among approaches that "differ from what is proposed in this
  post" and notes the alternatives they tried showed "very limited gains" or "low performance".
  That is a useful counterweight to treating HyDE as a default.

### 2.2 Conversational query rewriting / decontextualizing the *query*

These are the direct analogue of D1 — but applied to the query, not the stored memory.

- **ConvDR, "Few-Shot Conversational Dense Retrieval"** —
  [arXiv:2105.04166](https://arxiv.org/abs/2105.04166): *"we develop a teacher-student framework
  to train a student conversational query encoder to 'mimic' the representation of the oracle
  query rewrite"*; reported to *"outperform the previous state-of-the-art query rewriting based
  model by 9% and 48% in retrieval accuracy"* on CAsT-19. The authors also concede: *"the query
  reformulation step is not perfect and further reduces the conversational search accuracy."*
- **ConvSearch-R1** — [arXiv:2505.15776](https://arxiv.org/abs/2505.15776): RL-optimised
  reformulation with no external rewrite supervision; *"over 10% improvement on the challenging
  TopiOCQA dataset while using smaller 3B parameter models without any external supervision."*

### 2.3 Time-aware and key expansion (LongMemEval's own ablations)

From [arXiv:2410.10813v2](https://arxiv.org/html/2410.10813v2), §5.3–5.4 and Appendix D:

- **Fact-augmented key expansion.** Indexing a value under `key = value + extracted user facts`
  raises the RAG score from **0.524** (`K = value`) to **0.732** (`K = V + fact`). This is the
  benchmark's own evidence that appending *disambiguating context* to a stored item materially
  improves retrieval.
- **Time-aware indexing + query expansion.** Values are additionally indexed by the dates of
  the events they contain; at retrieval, *"an LLM M_T extracts a time range for time-sensitive
  queries, which is used to filter out a large number of irrelevant values."* Reported
  improvement: **+11.3% recall** with rounds as the value, **+6.8%** with sessions as the value.

### 2.4 Agentic / multi-hop retrieval over memory

- **IRCoT** — [arXiv:2212.10509](https://arxiv.org/abs/2212.10509): *"what to retrieve depends
  on what has already been derived, which in turn may depend on what was previously retrieved."*
  Reported: **up to +21 retrieval points and +15 QA points** across four datasets.
- **Self-RAG** — [arXiv:2310.11511](https://arxiv.org/abs/2310.11511): reflection tokens driving
  on-demand retrieval and self-critique.
- **Auto-RAG** — [arXiv:2411.19443](https://arxiv.org/abs/2411.19443): multi-turn dialogue with
  the retriever; *"can autonomously adjust the number of iterations based on the difficulty of
  the questions and the utility of the retrieved knowledge."*
- **Iter-RetGen** — [arXiv:2305.15294](https://arxiv.org/abs/2305.15294);
  **FLARE** — [arXiv:2305.06983](https://arxiv.org/abs/2305.06983).
- **MemGPT paging** — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560). The strongest
  memory-specific number: *"The LLM can request immediate follow-up LLM inference to chain
  function calls together by generating a special keyword argument (request_heartbeat=true) …
  function chaining is what allows MemGPT to perform multi-step retrieval to answer user
  queries."* On Deep Memory Retrieval: GPT-3.5 Turbo 38.7%→66.9%, GPT-4 32.1%→**92.5%**,
  GPT-4 Turbo 35.3%→93.4%. The authors also report the failure mode: *"we observe that MemGPT
  will often stop paging through retriever results before exhausting the retriever database."*
- **GraphRAG global search** —
  [arXiv:2404.16130](https://arxiv.org/abs/2404.16130) ·
  [Microsoft docs](https://microsoft.github.io/graphrag/query/global_search/). Community-summary
  *construction* is index time; *selection* + map-reduce + point rating/filtering is retrieval
  time. Evidence is qualitative ("substantial improvements … for both the comprehensiveness and
  diversity of generated answers") — no numeric effect size located.

### 2.5 Rehydrating an external pointer (D2) — the honest answer

**No primary source was found demonstrating that automatically fetching a referenced
file/commit/line at retrieval time repairs retrieval quality.** The evidence instead frames
reference staleness as a *write-time / CI* problem:

- **GitHub's own guidance** on permanent links —
  [docs.github.com](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files):
  *"The version of a file at the head of branch can change as new commits are made, so if you
  were to copy the normal URL, the file contents might not be the same when someone looks at it
  later."* The sanctioned fix is to put a commit ID in the URL.
- **Treude & Baltes, "Context Rot in AI-Assisted Software Development"** —
  [arXiv:2606.09090](https://arxiv.org/abs/2606.09090). They name the phenomenon *context rot*
  in exactly our setting (CLAUDE.md, AGENTS.md, .cursorrules): *"As software evolves, this
  context can become stale."* Their preliminary measurement: *"applying an existing README/wiki
  consistency checker to a statistically representative sample of 356 repositories identifies
  stale code element references in **23.0% of repositories**."* Their proposal is to reuse
  decades of documentation-consistency tooling as a detector — i.e. a cheap, deterministic,
  run-it-in-CI check, not a retrieval-time fetch.

**Synthesis for D2:** pin at write time (commit SHA + symbol, never a bare line number), verify
resolvability in a maintenance/CI pass, and give the agent a re-query tool. Do not assume a
silent auto-fetch will repair a dangling pointer — nobody has shown that it does, and a failed
fetch that degrades to nothing is indistinguishable from the bug you started with.

### 2.6 What the retrieval-time repair evidence actually supports

The strongest retrieval-time results — MemGPT's 35%→93% DMR jump, IRCoT's +21 points — come
from loops that **reformulate and re-query**, not from loops that **dereference a stored
pointer**. The second pattern's evidence base is a two-line GitHub docs note and a
consistency-checker paper. That asymmetry should drive the design.

---

## 3. Q3 — What reflection / distillation passes concretely do, and how often

### 3.1 Generative Agents (the original reflection pass)

[arXiv:2304.03442](https://arxiv.org/abs/2304.03442) ·
[repo](https://github.com/StanfordHCI/genagents) ·
[`genagents/modules/memory_stream.py`](https://raw.githubusercontent.com/StanfordHCI/genagents/main/genagents/modules/memory_stream.py)

**Trigger and frequency** (verbatim from §4.2):

> "Reflections are generated periodically; in our implementation, we generate reflections when
> the sum of the importance scores for the latest events perceived by the agents exceeds a
> threshold (**150** in our implementation). In practice, our agents reflected **roughly two or
> three times a day**."

**Step 1 — question generation.** Query the LLM with "the 100 most recent records in the
agent's memory stream" and prompt: *"Given only the information above, what are 3 most salient
high-level questions we can answer about the subjects in the statements?"*

**Step 2 — insight extraction.** The prompt file is
[`v2/insight_and_evidence_v1.txt`](https://raw.githubusercontent.com/joonspk-research/generative_agents/main/reverie/backend_server/persona/prompt_template/v2/insight_and_evidence_v1.txt)
and its entire body is:

> "What !<INPUT 1>! high-level insights can you infer from the above statements? (example
> format: insight (because of 1, 5, 3))"

producing e.g. *"Klaus Mueller is dedicated to his research on gentrification (because of 1, 2,
8, 15)"*. The paper: *"We parse and store the statement as a reflection in the memory stream,
including pointers to the memory objects that were cited."* Reflections feed back into
retrieval alongside observations, and can reflect on other reflections, forming trees.

**Importance scoring.** The prompt file is
[`v3_ChatGPT/poignancy_event_v1.txt`](https://raw.githubusercontent.com/joonspk-research/generative_agents/main/reverie/backend_server/persona/prompt_template/v3_ChatGPT/poignancy_event_v1.txt);
the paper quotes it as:

> "On the scale of 1 to 10, where 1 is purely mundane (e.g., brushing teeth, making bed) and 10
> is extremely poignant (e.g., a break up, college acceptance), rate the likely poignancy of the
> following piece of memory.
> Memory: buying groceries at The Willows Market and Pharmacy
> Rating: <fill in>"

Returns 2 for "cleaning up the room" and 8 for "asking your crush out on a date". Scored once,
at memory creation.

**Retrieval score** (§4.1): `score = α_recency·recency + α_importance·importance +
α_relevance·relevance`, all α = 1, components min-max normalised to [0,1]; recency is an
exponential decay with factor **0.995** per sandbox game hour since last retrieval; relevance
is cosine similarity between query and memory embeddings.

**Two code/paper discrepancies worth knowing before copying the design** (both checked in the
original repo): the code uses `self.recency_decay = 0.99`, not the paper's 0.995
([`scratch.py`](https://raw.githubusercontent.com/joonspk-research/generative_agents/main/reverie/backend_server/persona/memory_structures/scratch.py)),
and the code's retrieval weights are `gw = [0.5, 3, 2]` — relevance weighted 3×, contrary to
the paper's "all αs are set to 1". Also note the `StanfordHCI/genagents` repo referenced in the
paper does **not** contain the prompt files: it loads them from an external `LLM_PROMPT_DIR`,
and the verbatim text lives in
[joonspk-research/generative_agents](https://github.com/joonspk-research/generative_agents).

**What it does to a bad memory — and what it does not.** The reflection prompt asks for
*abstraction*, not *decontextualization*. The output format is `insight (because of 1, 5, 3)` —
indices into the numbered list currently in the prompt. The code resolves those indices back to
`record_ids` and stores them as `pointer_id`, so provenance survives. But **the reflection text
itself is never required to be standalone**, and a pronoun-bound reflection inherits exactly the
D1 defect while becoming *more* abstract. Reflection raises the level of the claim; it does not
close the reference.

### 3.2 MemGPT / Letta: promotion, pressure-driven summarization, sleep-time

**MemGPT** — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560).

- **Core memory (working context)** is *"a fixed-size read/write block of unstructured text,
  writeable only via MemGPT function calls … intended to be used to store key facts,
  preferences, and other important information about the user."* Promotion from archival/recall
  into it is model-directed: *"Memory edits and retrieval are entirely self-directed: MemGPT
  autonomously updates and searches through its own memory based on the current context."*
- **Recursive summarization fires on context pressure, not on a schedule.** When prompt tokens
  exceed the *warning token count* (~70% of the window) the system injects a *"memory pressure"*
  warning so the model can save what matters; when they exceed the *flush token count*
  (~100%), it evicts ~50% and *"generates a new recursive summary using the existing recursive
  summary and evicted messages."* This is a **lossy, compounding** distillation — each summary
  is built from the previous summary plus newly evicted messages, with no standalone-ness check
  anywhere in the loop.

**Sleep-time compute** — [Letta blog, 2025-04-21](https://www.letta.com/blog/sleep-time-compute) ·
[arXiv:2504.13171](https://arxiv.org/abs/2504.13171).

The architectural move: two agents. The primary agent *loses* the tools to edit its own core
memory; a sleep-time agent holds them. Verbatim:

> "Offloading memory to a sleep-time agent allows memory management to happen asynchronously.
> **Memory formation in MemGPT is incremental, so memories may become messy and disorganized
> over time.** Sleep-time agents on the other hand can continuously improve their learned
> context to generate **clean, concise, and detailed** memories."

**Frequency is a configuration knob, not a fixed trigger** — *"Sleeptime agents can also be
configured to run at different frequencies. The higher the frequency setting, the more tokens
your agent will use: but the more time the agent will have to revise its learned context."*
Archived Letta docs describe it as "triggered every N-steps (default 5)"; current docs say it
runs "after a set number of completed agent steps or when the context window is compacted", and
memory blocks are being superseded by "dreaming" background subagents
([docs.letta.com/configuration/memory](https://docs.letta.com/configuration/memory)). Writes are
"anytime": the primary agent can read the block without waiting for the sleep-time agent to
finish. The post also notes the sleep-time agent can afford a *stronger* model because it is not
latency-constrained.

The sleep-time agent's system prompt is the most explicit published statement of the D1 rule
(quoted in §1.3): do not write "today" or "recently", write specific dates and times, "because
'today' and 'recently' are relative, and the memory is persisted indefinitely." The paper
formalises the pass as *"During sleep-time we are given the context c but not the query q …
producing a re-represented context c′. We denote this process as: S(c) → c′"*, with
`rethink_memory` callable "up to 10 times" per pass
([arXiv:2504.13171](https://arxiv.org/abs/2504.13171)).

Note what is claimed vs. measured: the paper's demonstrated win is a cost/latency Pareto
improvement — roughly 5× less test-time compute, +13% GSM-Symbolic, +18% AIME. "Clean, concise,
and detailed memories" is a qualitative claim, not a measured memory-quality result.

### 3.3 Mem0: context *into* extraction, and ADD/UPDATE/DELETE/NOOP

[arXiv:2504.19413](https://arxiv.org/html/2504.19413v1).

- **Extraction is context-aware by construction.** The prompt is
  `P = (S, {m_{t−m}, …, m_{t−2}}, m_{t−1}, m_t)` where `S` is an asynchronously refreshed
  conversation summary: *"While S provides global thematic understanding across the entire
  conversation, the recent message sequence offers granular temporal context."* Mem0's answer
  to D1 is to feed the antecedent in at write time rather than repair later.
- **Update is a four-way classification per fact** (paper): ADD (no semantically equivalent
  memory), UPDATE (augment; fires when `InformationContent(f) > InformationContent(m_i)` and
  replaces the memory), DELETE (contradicted), NOOP. Operates over the top-`s` (s=10) similar
  memories with the last m=10 messages, via a function-calling interface, using GPT-4o-mini —
  i.e. it runs **per message pair**, on every turn. No schema is given in the paper.
- **The shipped repo no longer matches the paper, and the drift is instructive.** Current
  `main` names the no-op **NONE, not NOOP**, emits plain JSON rather than an OpenAI tool schema,
  and — most importantly — `mem0/memory/main.py` uses `ADDITIVE_EXTRACTION_PROMPT` whose stated
  contract is *"Your sole operation is ADD: identify every piece of memorable information and
  produce self-contained, contextually rich factual statements."* There is no consolidation pass
  in the current code path. The four-way update classifier that the paper is known for has been
  reverted to extraction-only.
- **Nowhere in the paper's pipeline is a memory required to be standalone.** That requirement
  appears only in the newer OSS extraction prompt quoted in §1.3, enforced by compliance.
- **Mem0's own results table is the strongest counter-argument to its own thesis.** With `J`
  as the metric, the paper reports Mem0 66.88 and Mem0^g 68.44 versus **full-context 72.90** —
  the no-consolidation baseline wins on accuracy. What Mem0 buys is latency (p95 1.44 s vs
  17.1 s). The paper's abstract says "91% lower p95 latency" while §4.3 says 92%; the numbers
  are the authors' own and are not peer-reviewed.

### 3.4 Zep / Graphiti

[arXiv:2501.13956](https://arxiv.org/abs/2501.13956). Graphiti is *"a temporally-aware knowledge
graph engine that dynamically synthesizes both unstructured conversational data and structured
business data while maintaining historical relationships."* It is bi-temporal (event time `T`
vs transaction time `T′`) and its invalidation rule is:

> "The system employs an LLM to compare new edges against semantically related existing edges to
> identify potential contradictions. When the system identifies temporally overlapping
> contradictions, it invalidates the affected edges by setting their t_invalid to the t_valid of
> the invalidating edge."

Extraction and invalidation run inside every `add_episode`; community refresh is
`update_communities=False` by default and the paper concedes *"periodic community refreshes
remain necessary."* Reported **claims** (vendor-authored preprint): 94.8% vs MemGPT's 93.4% on
DMR; LongMemEval_s 63.8% vs full-context 55.4% at 3.20 s vs 31.3 s. Admitted regression:
**−17.7% on single-session-assistant.** Graphiti's shipped dedup prompt is conflict-only —
*"NEVER mark facts as duplicates if they have key differences, particularly around numeric
values, dates, or key qualifiers"*
([dedupe_edges.py](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/dedupe_edges.py))
— and the README's rule is "when information changes, old facts are invalidated — not deleted."
No standalone-ness rule anywhere.

### 3.5 The measured counterweight: distillation can make memory *worse* than no memory

This is the most important 2026 result for anyone about to add a reflection pass.

**"Useful Memories Become Faulty When Continuously Updated by LLMs"** —
[arXiv:2605.12978](https://arxiv.org/abs/2605.12978):

> "As consolidation proceeds, memory utility first rises, then degrades, and can fall below the
> no-memory baseline. … even when consolidating from ground-truth solutions, GPT-5.4 fails on
> 54% of a set of ARC-AGI problems it had previously solved without memory."

Raw-episode control agents *"double the accuracy of their forced-consolidation counterparts"*,
and the authors' recommendation is to *"gate consolidation explicitly rather than firing it
after every interaction."*

**"Honest Lying"** — [arXiv:2605.29463](https://arxiv.org/abs/2605.29463) (ICML 2026 workshop):
*"0 of 121 reflections mention the correct target object"*, with mitigation lifting correct-object
mention from 0% to 86% and cutting a Reflection Repetition Rate from 0.64 to 0.10. Their
conclusion: *"reflective memory can reinforce false beliefs rather than correct them."*

**And the passes themselves are fragile by construction.** The survey
[arXiv:2603.07670](https://arxiv.org/abs/2603.07670) names the failure *summarization drift*:
*"The consolidation step … typically requires either explicit developer rules or periodic
LLM-driven summarization, both of which are fragile and hard to validate."*

**The counterweight to over-distillation in general** —
[arXiv:2603.02473](https://arxiv.org/abs/2603.02473): *"average accuracy spans 20 points across
retrieval methods (57.1% to 77.2%) but only 3-8 points across write strategies. Raw chunked
storage, which requires zero LLM calls, matches or outperforms expensive lossy alternatives,
suggesting that current memory pipelines may discard useful context that downstream retrieval
mechanisms fail to compensate for."*

Read together with LongMemEval's error analysis — **15–19% of all instances are "correct
retrieval yet wrong generation", 40–50% of all errors**
([v2 §E.5](https://arxiv.org/html/2410.10813v2)) — the conclusion is that the
scrap problem is substantially a *self-inflicted* one: the pipeline discarded the context that
would have made the item interpretable, and no downstream retriever can put it back.

### 3.6 How often the passes fire — the full table

| System | Trigger | Frequency | Gate |
|---|---|---|---|
| Generative Agents | sum of importance scores > **150** | "roughly two or three times a day" | LLM judgement |
| MemGPT | context > 70% (warn) / 100% (flush) | on pressure | none (compounding summary) |
| Letta sleep-time | configurable N-steps (archived default **5**) or on compaction | configured | LLM judgement |
| Mem0 (paper) | per message pair | every turn | LLM classification |
| Mem0 (current `main`) | — | every turn, **ADD-only** | none |
| Zep / Graphiti | per `add_episode`; communities **off by default** | every episode | LLM contradiction check |
| A-MEM ([arXiv:2502.12110](https://arxiv.org/abs/2502.12110), NeurIPS 2025) | per new note; rewrites *neighbouring* notes | every write | LLM judgement |
| MemoryBank ([arXiv:2305.10250](https://arxiv.org/abs/2305.10250), AAAI 2024) | on **recall** (`R = e^{−t/S}`, S increments on recall) | recall-driven | deterministic decay |
| MIRIX ([arXiv:2507.07957](https://arxiv.org/abs/2507.07957), not peer-reviewed) | memory > **90%** of capacity | on capacity | "controlled rewrite" |
| Vestige `dream` / `reflect` | >24h or >50 saves / weekly | scheduled | — |

Every LLM-gated pass in that table rewrites stored content with no standalone-ness check and no
post-hoc verification. [arXiv:2605.12978](https://arxiv.org/abs/2605.12978) is the evidence that
this is not a safe default.

---

## 4. Q4 — The failure taxonomy of agent memory, with numbers

### 4.1 The taxonomies that exist

| Source | Taxonomy | Names a "missing context" class? |
|---|---|---|
| [MemGuard, arXiv:2605.28009](https://arxiv.org/html/2605.28009v1) | **write-time contamination** ("stores incomplete, outdated, fabricated, or overgeneralized knowledge"), **retrieval-time contamination** ("semantically related but functionally unsuitable memories are retrieved together"), **composition failures** | Closest fit: "**incomplete**" and "**overgeneralized**" |
| [HaluMem, arXiv:2511.03506](https://arxiv.org/abs/2511.03506) | operation-level: **fabrication, errors, conflicts, omissions**; update-stage: "incorrect modification of old information, omission of new information, version conflicts or self-contradictions" | Closest fit: "**omissions**" |
| [arXiv:2602.19320 "Anatomy of Agentic Memory"](https://arxiv.org/abs/2602.19320) | four *evaluation* pain points: "benchmark saturation effects, metric validity and judge sensitivity, backbone-dependent accuracy, and the latency and throughput overhead introduced by memory maintenance" | No |
| [arXiv:2512.13564 "Memory in the Age of AI Agents"](https://arxiv.org/abs/2512.13564) | design axes: forms / functions / dynamics; §7.7 "Trustworthy Memory" | No |
| [arXiv:2504.15965 "From Human Memory to AI Memory"](https://arxiv.org/abs/2504.15965) | "3D-8Q Memory Taxonomy" over object × form × time | **No — and it has no error taxonomy at all** |
| [arXiv:2603.07670 "Memory for Autonomous LLM Agents"](https://arxiv.org/abs/2603.07670) | write–manage–read loop; three-dimensional taxonomy over temporal scope, representational substrate, control policy | No |

**This is its own finding: no published taxonomy has a category for "the entry is true but
uninterpretable without its source".** The nearest labels are MemGuard's *incomplete /
overgeneralized* and HaluMem's *omissions*, both of which are about content loss, not reference
loss. The closest named *process* failure is *summarization drift*, from the survey
[arXiv:2603.07670](https://arxiv.org/abs/2603.07670): *"The consolidation step … typically
requires either explicit developer rules or periodic LLM-driven summarization, both of which are
fragile and hard to validate."* The user's bug report is describing a failure mode the survey
literature does not currently name.

### 4.2 Numbers

- **The headline production number — MemUse, EMNLP 2026 Main**
  ([arXiv:2608.24189](https://arxiv.org/abs/2608.24189) ·
  [repo](https://github.com/ryuichi-sumida/memuse)). Deployment study: 40 users, 1,872
  sessions, 72 memory moments, 316 fact questions, four months. Memory moments are **~1.4% of
  user turns**. The result:
  > "the same system that scores **78.8%** on Direct QA references only **7.9%** of those facts
  > in conversation — a **71-point gap**."

  And: *"Existing-benchmark Direct QA varies from 19.7% to 70.1% across the 7 conditions, but
  satisfaction does not change."* Also, *"Natural Integration is associated with satisfaction,
  whereas Direct QA is not."* Human–human agreement κ = 0.57 for Natural Integration; Fleiss
  κ = 0.65 for fact questions. **This is the single best published measurement of memory
  *usefulness* rather than retrieval accuracy** — and it says the benchmark numbers are largely
  disconnected from whether the memory was usable.

- **Description–evidence mismatch — DeMem, [arXiv:2605.10870](https://arxiv.org/abs/2605.10870).**
  *"descriptive similarity is a statistically significant but weak predictor of evidence
  compatibility: the Spearman correlation is only ρ=0.103 (AUC = 0.548). Under a matched
  answer-time budget, description-based retrieval recovers only 66% of gold evidence, compared
  to 83% for DeMem. **Among the queries where description-based memory fails, 85% of errors
  trace directly to evidence miss or dilution** caused by this mismatch."*
  Audit table (§E.8): Feature Routing — support purity 0.215, contamination 0.533; RAG w/o
  certified split — 0.283 / 0.412; DeMem — 0.394 / 0.241. **Up to 53% of the memory content
  selected at answer time is not part of the gold support set.**

- **Contamination localisation — MemGuard** ([arXiv:2605.28009](https://arxiv.org/html/2605.28009v1)):
  unverifiability errors are *"predominantly associated with write-time contamination (97.7%)"*,
  factuality errors with *"retrieval-time contamination (63.8%)"*. Report claims +28.27%
  anti-hallucination accuracy and *"retrieving up to 5.8x fewer memory tokens"*.

- **Address rot — Context Rot** ([arXiv:2606.09090](https://arxiv.org/abs/2606.09090)): stale
  code-element references in **23.0% of 356 repositories**.

- **Harm — AgentPoison** ([arXiv:2407.12784](https://arxiv.org/abs/2407.12784)): *"an average
  attack success rate higher than 80% with minimal impact on benign performance (less than 1%)
  with a poison rate less than 0.1%."* Garbage in the store is cheap to plant and expensive to
  notice.

- **Benchmark ground truth is itself noisy.** Independent audit
  ([dial481/locomo-audit](https://github.com/dial481/locomo-audit), non-peer-reviewed but with
  SHA256-verified inputs): **99 of 1,540 LoCoMo questions (6.4%) have wrong golden answers**,
  giving a theoretical ceiling of 93.57%; **62.81% of intentionally wrong "vague-but-topical"
  answers were accepted by the LLM judge**; 446 adversarial questions (22.5% of the dataset)
  are never evaluated because the multiple-choice formatter is broken. One published system's
  single-hop score (95.96%) exceeds its own category ceiling (95.72%) — arithmetically
  impossible without credit from wrong gold answers.

- **No peer-reviewed paper reports "% of stored memory items never retrieved."** A blog's "85%"
  circulates but is not primary evidence. MemUse's 7.9% reference rate is the defensible proxy.

### 4.3 What a "useful memory" looks like, per product docs

Claude Code's memory documentation is the clearest published product-level standard
([code.claude.com/docs/en/memory](https://code.claude.com/docs/en/memory)). Four memory types —
`user` (role, expertise, working preferences), `feedback` (corrections and confirmed
approaches), `project` (ongoing work, deadlines, decisions), `reference` (where to find
information outside the project). Two rules matter here:

> "**Claude skips anything it can derive from the codebase**, such as architecture, file paths,
> or debugging fixes. It also skips anything your CLAUDE.md files already say."

> "keep one line per entry, move detail into topic files, and **merge or drop stale entries**";
> "if two rules contradict each other, Claude may pick one arbitrarily."

The published standard is *durable, non-derivable, one-idea-per-entry*. (OpenAI's memory FAQ
could not be retrieved — `help.openai.com` returned HTTP 403 for both article URLs in this
pass. Unverified.)

---

## 5. Q5 — What a cheap check would look like

### 5.1 What already ships

**`kernle` memory lint** — the most concrete item-level linter found
([kernle/lint.py v0.13.13](https://github.com/emergent-instruments/kernle/blob/v0.13.13/kernle/lint.py)).
Its docstring states the problem exactly as the user does:

> "The pipeline can produce truncated/low-quality statements … Once stored, they **contaminate
> retrieval forever**."

Its rules, which run *before* `save_belief` / `save_value`:

| Rule | Implementation |
|---|---|
| `too_short` / `too_long` | default min 10 chars, max 2000 |
| `prefix_artifact` | `^(value\|belief\|goal\|note\|episode\|statement\|name)\s*:\s*` — the model echoed a field label into the content |
| `templated_noise` | `^(worked on\|updated\|fixed\|changed\|modified\|edited\|added\|removed)\s+\.{2,}`, `^\.\.\.$`, `^(todo\|tbd\|fixme\|xxx\|placeholder\|n/a\|none\|null\|undefined)$`, repeated single char, whitespace/punctuation only |
| `trailing_fragment` | content ends with an incomplete phrase — " the", " a", " and", " with", " of", " is", " was", … |

**Notice what is absent: no rule for an unresolved pronoun, no rule for a deictic time
expression, no rule for a file/symbol reference that does not resolve.** That absence is the
gap this report exists to identify.

**`agent-memory-doctor`** — [repo](https://github.com/chenhz01/agent-memory-doctor) — is a
boot-time integrity check: `markers` ("a fingerprint pattern is no longer verbatim in the
memory file"), `memory_size` (char limit → `AUDIT_REQUIRED`), `archive_markers`, `hash:*`,
`freshness`, SQLite `session_integrity`. Its design rule is self-containedness stated as
tooling doctrine:

> "critical rules must stay **verbatim in the injected file** — **a pointer to an archive file
> is useless for a rule the agent must apply without reading anything**."

**Adjacent shipped rewrites** (write-time, so they *prevent* rather than detect):

- `localmem` `rewriter.rs` — *"every capture is rewritten to be self-contained before lex + vec
  indexing, so a retrieved chunk reads correctly in isolation — 'they prefer X' becomes 'Vijay
  prefers X', 'my email' becomes 'Vijay's email'."* Regex mode is first-person only, by design:
  *"We deliberately don't try to handle third-person rewrites ('they said X') because guessing
  the referent is exactly the case the LLM mode exists to handle."*
  ([raw](https://raw.githubusercontent.com/VJ-yadav/localmem-community/refs/heads/main/core/src/rewriter.rs))
- Mem0's extraction prompt and MemOS's validator (§1.3), and **Letta's sleep-time agent system
  prompt** ([`sleeptime.txt`](https://raw.githubusercontent.com/letta-ai/letta/0.7.0/letta/prompts/system/sleeptime.txt)),
  which is the only shipped instruction found that targets *deictic time* by name — "do not
  write 'today' or 'recently', instead write specific dates and times, because 'today' and
  'recently' are relative, and the memory is persisted indefinitely."

**Vendor eval harnesses that could host a new dimension:** Neo4j Agent Memory (three
dimensions, above); Mem0's `memory-reviewer` skill — dedup/conflict/staleness only
("*Near-duplicates: >60% noun overlap*", "*Stale: created_at older than 180 days*"); Letta
`/doctor` ("*audit placement, duplication, and system-prompt token usage*" — not semantics);
cognee `validate()` (structural integrity: `orphaned_edge`, `identity_id_mismatch`,
`missing_vector_entry`); Basic Memory `bm schema validate` (schema only).

### 5.2 The primitives that exist for the missing check

- **Coreference resolution metrics.** The CoNLL-2012 shared task
  ([aclanthology.org/W12-4501](https://aclanthology.org/W12-4501/)) defines the standard
  coreference evaluation (commonly reported as CoNLL-F1 = the mean of MUC, B³ and CEAF_e F1
  scores). *Caveat: the exact formula text was not verified verbatim in this pass — the
  anthology PDF is not fetchable by the tooling used here.* These metrics score a *system's*
  coreference decisions against annotation; they are not a one-item gate.
- **The decontextualization criterion** (§1.2) is the right formal test — Definition from Choi
  et al., decontextuality + minimality from Molecular Facts — and needs no new theory, only a
  port to the memory write/read path.
- **The round-trip / answerability test.** Generate a question from the item, then answer it
  from the item alone with no retrieval context. This is exactly the Newman et al. framework
  ([2023.emnlp-main.193](https://aclanthology.org/2023.emnlp-main.193/)) — and their finding
  that *"question generation and answering remain challenging"* is a warning that this test is
  noisier than it looks. Use it as a *flag for review*, never as an automatic rewrite trigger.
- **Documentation-consistency checkers.** Treude & Baltes
  ([arXiv:2606.09090](https://arxiv.org/abs/2606.09090)) argue these are "an immediate starting
  point" for detecting context rot; their 23.0% measurement is the proof that off-the-shelf
  checkers already surface the problem.
- **Commit-pinned references.** GitHub's permanent-link guidance
  ([docs](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)).

### 5.3 The verdict on Q5

**No published work — academic or OSS — defines a cheap, validated, automated test with
measured precision/recall for "this stored item will not make sense without the source
conversation."**

What exists is: (a) the academic *criterion*, unported and unscored for memory; (b) shipped
*prompt rules* with no verifier — Mem0's "Every memory must be understandable on its own",
MemOS's resolver, and Letta's "do not write 'today' or 'recently'"; (c) a shipped *linter* that
checks shape and truncation but not reference (kernle); (d) shipped *rewriters* that prevent
rather than detect (localmem); (e) a published *measurement* that the problem is real (23.0%
stale references, [arXiv:2606.09090](https://arxiv.org/abs/2606.09090)) and that usefulness
diverges wildly from benchmark accuracy (MemUse, [arXiv:2608.24189](https://arxiv.org/abs/2608.24189)).

The pattern across (b) is consistent and telling: **three independent teams wrote the rule into
a prompt, and none of them wrote a check.** Prompt rules drift, get ignored under length
pressure, and cannot be audited. That is the opening for a deterministic verifier.

### 5.4 A concrete cheap check (proposal — this is synthesis, not a citation)

Five checks, ordered by (cheapness × evidence):

1. **Reference resolvability (D2).** Extract every path / symbol / `path:line` from the item.
   Resolve each against the repo at HEAD, and against the memory's recorded commit SHA. Any
   unresolved reference is a flag. `path:line` should be recorded as `path@sha#symbol`, with the
   line number treated as a hint, never as the identity. Re-run over the whole store in the
   consolidation cycle. *Evidence: 23.0% of repos already fail this
   ([arXiv:2606.09090](https://arxiv.org/abs/2606.09090)).*
2. **Unbound-deixis (D1).** Flag items that contain a pronoun or demonstrative
   ("this", "that", "it", "they", "the above", "the second one") with no named entity anywhere in
   the same item; flag items whose main clause is pronoun-headed. *Evidence: the criterion is
   Choi et al. ([TACL 2021](https://aclanthology.org/2021.tacl-1.27/)); the failure class is
   named by MemGuard as "incomplete / overgeneralized"
   ([arXiv:2605.28009](https://arxiv.org/html/2605.28009v1)).*
3. **Unanchored temporal deixis.** Flag "yesterday", "today", "next Friday", "last week", "this
   morning" that were not converted to an absolute date. *Evidence: LongMemEval's time-aware
   expansion gains +11.3% recall ([v2 §5.4](https://arxiv.org/html/2410.10813v2)) — the anchored
   form is measurably more retrievable.*
4. **Entity requirement.** Require ≥1 extracted entity (person / project / file / component) per
   stored item, reusing the existing extractor as the gate. Items that carry none are almost
   always process narration ("fixed it", "deployed") rather than facts. *Evidence: A-MAC
   ([arXiv:2603.04549](https://arxiv.org/abs/2603.04549)) found "content type prior as the most
   influential factor for reliable memory admission" — cheap interpretable features beat opaque
   LLM policies.*
5. **Answerability probe.** Generate one question from the item, answer it from the item alone,
   and compare to the item's provenance-anchored answer. *Evidence: Newman et al.
   ([EMNLP 2023](https://aclanthology.org/2023.emnlp-main.193/)) — sound in principle, noisy in
   practice; route failures to review, not to automatic rewriting.*

**The one check to add first: #1, reference resolvability.** It is deterministic, needs no model,
has the strongest published evidence base, matches the most concrete half of the bug report
(`file:line` pointing into a codebase that moved), and it produces an actionable repair (pin to
a commit SHA + symbol, or inline the referenced content at retrieval time).

---

## 6. What this implies for Vestige

Vestige already has: contradiction detection, temporal supersession, FSRS-6 retention scoring, a
dream/consolidation cycle, a `reflect` metacognitive pass, entity extraction and relation
extraction, provenance metadata (session, agent, derivation chain), write-time coreference
rewriting ("He said X" → "John said X"), and write-time temporal anchoring. None of the
following restates those.

**1. Self-containedness is an unoccupied axis, and Vestige can own the definition.**
No memory benchmark scores it (§1). The construct exists only in TALC/TACL-style
decontextualization work aimed at sentence rewrites. A `self_containment` signal computed on
every stored item and exposed in search results would be, as far as this review can determine,
the first implementation of the Choi/Molecular-Facts criteria inside a memory engine. That is a
publishable position, not just a bug fix.

**2. `detail_level: "full"` is provenance *display*; the new thing is provenance *repair*.**
Vestige already returns provenance metadata on request. That tells the reader where a memory
came from — it does not close the hole in the memory. The new behaviour is: when a retrieved
item fails the self-containment check, *use* the provenance to fetch the missing antecedent
(the source turn, or the file at the recorded commit) and inline it into the result, marking
the result as repaired. `expandable` IDs currently exist as a token-budget overflow mechanism;
the upgrade is to make expansion *triggered by a failed check* rather than by budget.

**3. The `file:line` half of the bug has a published, quantified answer, and it is a
maintenance check — not a cleverer retriever.** 23.0% of repositories already carry stale code
references ([arXiv:2606.09090](https://arxiv.org/abs/2606.09090)), and GitHub's own
documentation says branch URLs rot by design. So: record `path@commit#symbol` at write time,
keep the line number only as a hint, and re-resolve every code reference during the existing
dream/consolidation cycle. A reference that has stopped resolving should demote the memory and
queue it for repair — that is a natural extension of the existing consolidation cycle, not a new
subsystem.

**4. Add a deictic lint, and note that the existing coreference rewriting does not cover it.**
Write-time coreference rewriting only rewrites pronouns that had an antecedent *inside the
ingested text*. The failure the user reports is precisely the case where the antecedent was in
the conversation and not in the text. That case cannot be fixed at write time with a regex; it
is detectable at write time (flag) and repairable at read time (fetch the turn). The regex
rewriter's own limitation is documented in the wild — `localmem`'s
[`rewriter.rs`](https://raw.githubusercontent.com/VJ-yadav/localmem-community/refs/heads/main/core/src/rewriter.rs)
concedes it handles first-person only, because "guessing the referent is exactly the case the
LLM mode exists to handle."

**5. The evidence now says: do not add another ungated LLM rewrite pass, and A/B the existing
one.** This is the sharpest update from the 2026 literature. Generative Agents' reflection
fires on an importance-sum threshold of 150, "roughly two or three times a day"
([arXiv:2304.03442](https://arxiv.org/abs/2304.03442)); its prompt asks for *abstraction*, not
standalone-ness, and its output format is index-based pointers into the current prompt. MemGPT's
recursive summarization is pressure-triggered and compounding
([arXiv:2310.08560](https://arxiv.org/abs/2310.08560)). And the measured result is damning:
*"As consolidation proceeds, memory utility first rises, then degrades, and can fall below the
no-memory baseline"* — with raw-episode control agents doubling the accuracy of their
forced-consolidation counterparts, and the authors recommending you *"gate consolidation
explicitly rather than firing it after every interaction"*
([arXiv:2605.12978](https://arxiv.org/abs/2605.12978)). A second study found *"0 of 121
reflections mention the correct target object"* — *"reflective memory can reinforce false
beliefs rather than correct them"* ([arXiv:2605.29463](https://arxiv.org/abs/2605.29463)). The
market has already noticed: Mem0's current `main` dropped its celebrated update phase for
ADD-only extraction, and Mem0's own table shows full-context (72.90) beating its pipeline
(66.88) on accuracy.

For Vestige this means two concrete things. First, `dream` and `reflect` should be measured with
a before/after self-containment score, not assumed to help — an abstraction pass that adds no
anchors can *widen* the gap the user is complaining about. Second, a consolidation pass that
"cleans up" a memory by shortening it is exactly the operation
[arXiv:2603.02473](https://arxiv.org/abs/2603.02473) warns about: pipelines "may discard useful
context that downstream retrieval mechanisms fail to compensate for." Prefer passes that **add
anchors** (names, absolute dates, pinned references, inlined antecedents) over passes that
**remove words**.

**6. Do not outsource the measurement to an LLM judge.** The LoCoMo audit found a judge
accepting 62.81% of intentionally wrong vague-but-topical answers, and 6.4% of gold answers
wrong ([audit](https://github.com/dial481/locomo-audit)). Prefer the deterministic checks
(§5.4 #1–#4) for anything that gates behaviour, and reserve judge-based scoring for a small,
human-validated eval set.

**7. Build the eval set, because no public benchmark will supply it.** A `standalone-ness` eval
is small and cheap: take N stored memories, strip the conversation, and ask a human or a
strictly-prompted judge the two Choi questions — "is it interpretable in the empty context?" and
"is its meaning the same as it was in context?" — plus the Molecular-Facts minimality check.
Vestige's own store is the dataset. This is the one place where doing the measurement is easier
than finding someone else's.

**8. What is *not* new, and should not be re-litigated.** Contradiction detection, supersession,
retention decay, and entity extraction are all covered by existing benchmarks and existing
Vestige features. The gap this report identifies is narrower and sharper: **nothing in the
literature scores whether a stored memory can be read by someone who was not there, and nothing
in Vestige currently repairs one that cannot.**

---

## Appendix — Sources

**Benchmarks and metrics**
- LoCoMo — [arXiv:2402.17753](https://arxiv.org/abs/2402.17753) · [HTML](https://arxiv.org/html/2402.17753v1)
- LongMemEval — [arXiv:2410.10813](https://arxiv.org/abs/2410.10813) · [v2 HTML](https://arxiv.org/html/2410.10813v2)
- LongMemEval-V2 — [arXiv:2605.12493](https://arxiv.org/abs/2605.12493) · [repo](https://github.com/xiaowu0162/LongMemEval-V2)
- MemoryAgentBench — [arXiv:2507.05257](https://arxiv.org/abs/2507.05257) · [v4 HTML](https://arxiv.org/html/2507.05257v4) · [repo](https://github.com/HUST-AI-HYZ/MemoryAgentBench) · [data](https://huggingface.co/datasets/ai-hyz/MemoryAgentBench)
- MemConflict — [arXiv:2605.20926](https://arxiv.org/abs/2605.20926)
- AgentMemBench — [arXiv:2608.00009](https://arxiv.org/abs/2608.00009)
- PERMA — [arXiv:2603.23231](https://arxiv.org/abs/2603.23231)
- DeMem (rate-distortion) — [arXiv:2605.10870](https://arxiv.org/abs/2605.10870)
- A-MAC admission control — [arXiv:2603.04549](https://arxiv.org/abs/2603.04549)
- MemUse — [arXiv:2608.24189](https://arxiv.org/abs/2608.24189) · [repo](https://github.com/ryuichi-sumida/memuse)
- Bottleneck diagnosis — [arXiv:2603.02473](https://arxiv.org/abs/2603.02473)
- LoCoMo independent audit — [github.com/dial481/locomo-audit](https://github.com/dial481/locomo-audit)

**Construct: decontextualization and standalone-ness**
- Choi et al., TACL 2021 — [aclanthology.org/2021.tacl-1.27](https://aclanthology.org/2021.tacl-1.27/) · [arXiv:2102.05169](https://arxiv.org/html/2102.05169)
- Gunjal & Durrett, Molecular Facts — [arXiv:2406.20079](https://arxiv.org/html/2406.20079v1)
- Newman et al., EMNLP 2023 — [aclanthology.org/2023.emnlp-main.193](https://aclanthology.org/2023.emnlp-main.193/)
- CoNLL-2012 shared task — [aclanthology.org/W12-4501](https://aclanthology.org/W12-4501/)

**Retrieval-time repair**
- HyDE — [arXiv:2212.10496](https://arxiv.org/abs/2212.10496)
- Query2doc — [arXiv:2303.07678](https://arxiv.org/abs/2303.07678)
- RAG-Fusion — [repo](https://github.com/Raudaschl/RAG-Fusion)
- ConvDR — [arXiv:2105.04166](https://arxiv.org/abs/2105.04166)
- ConvSearch-R1 — [arXiv:2505.15776](https://arxiv.org/abs/2505.15776)
- IRCoT — [arXiv:2212.10509](https://arxiv.org/abs/2212.10509)
- Self-RAG — [arXiv:2310.11511](https://arxiv.org/abs/2310.11511)
- Auto-RAG — [arXiv:2411.19443](https://arxiv.org/abs/2411.19443)
- Iter-RetGen — [arXiv:2305.15294](https://arxiv.org/abs/2305.15294) · FLARE — [arXiv:2305.06983](https://arxiv.org/abs/2305.06983)
- GraphRAG — [arXiv:2404.16130](https://arxiv.org/abs/2404.16130) · [global search docs](https://microsoft.github.io/graphrag/query/global_search/)

**Distillation and reflection**
- Generative Agents — [arXiv:2304.03442](https://arxiv.org/abs/2304.03442) · [repo](https://github.com/StanfordHCI/genagents) · [`memory_stream.py`](https://raw.githubusercontent.com/StanfordHCI/genagents/main/genagents/modules/memory_stream.py) · prompt files + [`scratch.py`](https://raw.githubusercontent.com/joonspk-research/generative_agents/main/reverie/backend_server/persona/memory_structures/scratch.py) in [joonspk-research/generative_agents](https://github.com/joonspk-research/generative_agents) · [reflection prompt](https://raw.githubusercontent.com/joonspk-research/generative_agents/main/reverie/backend_server/persona/prompt_template/v2/insight_and_evidence_v1.txt) · [importance prompt](https://raw.githubusercontent.com/joonspk-research/generative_agents/main/reverie/backend_server/persona/prompt_template/v3_ChatGPT/poignancy_event_v1.txt)
- MemGPT — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560)
- Sleep-time compute (Letta) — [blog](https://www.letta.com/blog/sleep-time-compute) · [arXiv:2504.13171](https://arxiv.org/abs/2504.13171) · [sleep-time prompt](https://raw.githubusercontent.com/letta-ai/letta/0.7.0/letta/prompts/system/sleeptime.txt) · [tool docstrings](https://raw.githubusercontent.com/letta-ai/letta/0.7.0/letta/functions/function_sets/base.py) · [memory config docs](https://docs.letta.com/configuration/memory)
- Mem0 — [arXiv:2504.19413](https://arxiv.org/html/2504.19413v1) · [shipped prompts](https://raw.githubusercontent.com/mem0ai/mem0/main/mem0/configs/prompts.py)
- Zep / Graphiti — [arXiv:2501.13956](https://arxiv.org/abs/2501.13956) · [dedupe prompt](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/dedupe_edges.py)
- A-MEM — [arXiv:2502.12110](https://arxiv.org/abs/2502.12110) · MemoryBank — [arXiv:2305.10250](https://arxiv.org/abs/2305.10250) · MIRIX — [arXiv:2507.07957](https://arxiv.org/abs/2507.07957) (not peer-reviewed)
- **Consolidation harms:** *Useful Memories Become Faulty When Continuously Updated by LLMs* — [arXiv:2605.12978](https://arxiv.org/abs/2605.12978) · *Honest Lying* — [arXiv:2605.29463](https://arxiv.org/abs/2605.29463) · atomic-fact-extraction trade-off — [arXiv:2605.28224](https://arxiv.org/abs/2605.28224)
- Anthropic Contextual Retrieval — [anthropic.com/engineering/contextual-retrieval](https://www.anthropic.com/engineering/contextual-retrieval)

**Failure taxonomies and surveys**
- [arXiv:2504.15965](https://arxiv.org/abs/2504.15965) · [arXiv:2512.13564](https://arxiv.org/abs/2512.13564) · [arXiv:2602.19320](https://arxiv.org/abs/2602.19320) · [arXiv:2603.07670](https://arxiv.org/abs/2603.07670)
- MemGuard — [arXiv:2605.28009](https://arxiv.org/html/2605.28009v1) · HaluMem — [arXiv:2511.03506](https://arxiv.org/abs/2511.03506)
- AgentPoison — [arXiv:2407.12784](https://arxiv.org/abs/2407.12784)
- Context Rot — [arXiv:2606.09090](https://arxiv.org/abs/2606.09090)

**Tooling and shipped rules**
- Mem0 extraction prompt — [`mem0/configs/prompts.py`](https://raw.githubusercontent.com/mem0ai/mem0/main/mem0/configs/prompts.py)
- MemOS reader/validator — [`mem_reader_prompts.py`](https://raw.githubusercontent.com/MemTensor/MemOS/main/src/memos/templates/mem_reader_prompts.py)
- kernle memory lint — [`lint.py`](https://github.com/emergent-instruments/kernle/blob/v0.13.13/kernle/lint.py)
- agent-memory-doctor — [repo](https://github.com/chenhz01/agent-memory-doctor)
- localmem rewriter — [`rewriter.rs`](https://raw.githubusercontent.com/VJ-yadav/localmem-community/refs/heads/main/core/src/rewriter.rs)
- Neo4j Agent Memory evaluation — [docs](https://neo4j.com/labs/agent-memory/how-to/evaluation/)
- GitHub permanent links — [docs](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)
- Claude Code memory — [docs](https://code.claude.com/docs/en/memory)

**Unverified / not retrievable in this pass (recorded so they are not mistaken for evidence)**
- OpenAI Memory FAQ — `help.openai.com` articles returned HTTP 403.
- CoNLL-F1 = mean(MUC, B³, CEAF_e) — the exact formula text was not quoted from the primary PDF.
- Barzilay & Lapata (2008) entity-grid coherence — not verified.
- The widely-repeated "85% of memories are never retrieved" — blog-only, non-primary.
- Letta's archival-memory docs page moved to `/v1-sdk/`; cited for tool existence only.

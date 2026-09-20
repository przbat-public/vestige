# Write-Time Extraction and Memory Formulation

**How known agent-memory systems turn a conversation into a stored item — and what their prompts actually say**

Scope: the *write path*. What gets extracted, what rules constrain the wording, what metadata rides along, and what is documented to fail.
Primary sources only where possible (repository prompt files, papers, official docs). Vendor marketing is labelled as a claim and given one line.

---

## 0. The problem, restated precisely

Vestige's report is that saved memories are "usually a useless scrap of text" that either

- **(a)** refers to pieces of code — a file and a line number, or
- **(b)** assumes the reader still has the agent's conversation in context.

These are two distinct defects and the literature treats them separately.

**(b) is a solved-in-principle problem with named techniques.** The umbrella term in the sources below is *self-containment* / *standalone-ness*: the stored item must be interpretable with no access to the episode it came from. The concrete mechanisms are pronoun elimination, explicit subjects, absolute dates, and preserved proper nouns.

**(a) is barely addressed at all.** General agent-memory systems do not model code. The one system found that does — `legendary-mcp`, a niche 2025 open-source project — anchors to `{file, symbol, commit, content_hash}` and explicitly ranks line numbers as the weakest anchor ([concepts](https://ashhadahsan.github.io/legendary/concepts/)). Everything else either ignores code or (Cognee) extracts file names and code blocks as retrieval keys without any staleness model.

A third finding cuts against the intuition that write-time work is the highest-leverage fix: two 2025–2026 studies find retrieval method dominates write strategy by a wide margin. That does **not** mean write-time formulation is worthless — it means the *measurable* benefit of write-time work is concentrated in a specific place (does a required item exist and is it findable at all), not in general answer quality. See §6.

---

## 1. What concrete rules make a stored memory self-contained?

### Mem0 — the most explicit decontextualization rule found anywhere

Mem0's `ADDITIVE_EXTRACTION_PROMPT` in `mem0/configs/prompts.py` is the current (v3-style) extraction prompt and contains the single most direct statement of the rule. Source: [mem0ai/mem0 `mem0/configs/prompts.py`](https://raw.githubusercontent.com/mem0ai/mem0/main/mem0/configs/prompts.py).

```
### Self-Contained
Every memory must be understandable on its own. Replace all pronouns with specific names or "User."
```

The same file fixes the granularity and several related wording rules:

```
### Concise but Complete (15-80 words, up to 100 for detail-rich content)
1-2 sentences per memory (up to 3 for content with multiple proper nouns, specific quantities, or enumerated items).
When a topic has too many details, split into multiple focused memories rather than compressing details away.
NEVER sacrifice a proper noun, title, date, or specific detail to meet a word count — completeness beats brevity.
```

```
### Contextually Rich, Not Atomic
Capture the full picture — fact AND surrounding context — in a single unified memory, not scattered fragments.
Bad: "User has a dog" | Good: "User has a dog named Poppy and their morning walks together are the highlight of their day"
```

And the anti-abbreviation rule that directly targets the "useless scrap" failure:

```
If the input is specific, the memory must be equally specific. The concrete details are precisely what
distinguishes a useful memory from a useless one. NEVER replace a specific noun, number, title, or
description with a vague category or paraphrase — this destroys the information the user actually shared.
```

Note the `MEMORY_INSTRUCTIONS`-style guidance also instructs what *not* to store — mem0's additive prompt has a `Do NOT extract` list including "Vague assistant characterizations", "Generic assistant acknowledgments", "Assistant meta-commentary about its own capabilities".

The older, still-shipped `FACT_RETRIEVAL_PROMPT` (same file) is much weaker and is the source of the well-known "Name is John" style fragments:

```
Input: Hi, my name is John. I am a software engineer.
Output: {"facts" : ["Name is John", "Is a Software engineer"]}
```

That few-shot example produces exactly the decontextualized-but-subjectless scrap the user is complaining about. **The upgrade path between these two prompts in the same repository is the most instructive artefact in this whole review.**

**Primary source:** repository file. Verified verbatim.

### Mem0's temporal-grounding rule (the "worthless a week later" argument, made by the vendor)

Same file, in the `## Observation Date` input description:

```
CRITICAL: "User went to Paris last week" is useless 6 months later.
"User went to Paris the week of May 15, 2023" is meaningful forever.
Always ground relative references to specific dates.
```

Also notable: Mem0 separates **Observation Date** (when the conversation happened) from **Current Date** (now) and warns the extractor not to use Current Date to resolve references — a direct guard against the retrospective-extraction bug where "yesterday" gets resolved against today.

### Graphiti / Zep — the explicit-subject rule, enforced by schema

Graphiti's edge-extraction prompt ([`graphiti_core/prompts/extract_edges.py`](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/extract_edges.py)) states:

```
- Facts should include entity names rather than pronouns whenever possible.
```

Its node-extraction prompt ([`graphiti_core/prompts/extract_nodes.py`](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/extract_nodes.py)) is far stricter and is essentially a long blocklist designed to prevent unfindable nodes:

```
NEVER extract any of the following:
- Pronouns (you, me, I, he, she, they, we, us, it, them, him, her, this, that, those)
...
- Bare relational or kinship terms (dad, mom, mother, father, sister, brother, husband, wife,
  spouse, son, daughter, uncle, aunt, cousin, grandma, grandpa, friend, boss, teacher, neighbor,
  roommate) and bare animal/pet words (dog, cat, pet, puppy, kitten). These are too generic on
  their own. Instead, qualify them with the possessor: extract "Nisha's dad" not "dad",
  "Jordan's dog" not "dog".
```

and the governing test:

```
Only extract entities that are specific enough to be uniquely identifiable. Ask: "Could this have its
own Wikipedia article or database entry, OR is it specific enough to distinguish from other items of the
same category within this conversation?"
...
If a phrase would not be distinguishable when read alone later, do NOT extract it.
```

That last line — *"if a phrase would not be distinguishable when read alone later, do NOT extract it"* — is the cleanest single sentence in the entire corpus for the user's complaint. It is a **write-time filter conditioned on a future read with no context**.

Graphiti also handles pronoun disambiguation as a transformation, not just a filter:

```
Pronoun references such as he/she/they or this/that/those should be disambiguated to the names of the
reference entities.
```

**Primary source:** repository prompt files. Verified verbatim.

### Cognee — self-containment as a numbered output contract

Cognee's summarization prompt ([`cognee/infrastructure/llm/prompts/summarize_content.txt`](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/infrastructure/llm/prompts/summarize_content.txt)) is a two-section output format whose second section is entirely about standalone facts:

```
Second section:
Facts:
- <self-contained fact>
- <self-contained fact>

Second-section rules:
1. Write complete sentences with clear subjects from the first section.
2. Each fact must stand alone without the chunk or the other facts.
3. Order facts by: time first, category second, entity/topic third.
4. Do not group all facts about one entity if that makes the facts jump backward or forward in time.
5. Make sure the facts cover the full content of the chunk.
6. Do not invent.
```

Two things stand out. Rule 1 makes the required subject explicit ("from the first section" — i.e. the entity list in section 1 is the allowed subject vocabulary). Rule 3 is unusual: facts are ordered by *time first*, then category, then entity, which is a retrieval-oriented layout choice rather than a prose one. Rule 6 is the anti-hallucination guard.

Cognee's graph prompt ([`generate_graph_prompt.txt`](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/infrastructure/llm/prompts/generate_graph_prompt.txt)) carries a section literally titled **Coreference Resolution**:

```
# 3. Coreference Resolution
- **Maintain Entity Consistency**: When extracting entities, it's vital to ensure consistency.
  If an entity, is mentioned multiple times in the text but is referred to by different names or pronouns,
  always use the most complete identifier for that entity throughout the knowledge graph.
```

It also forbids the vice of generic nodes and requires human-readable IDs:

```
**Node IDs**: Never utilize integers as node IDs.
  - Node IDs should be names or human-readable identifiers found in the text.
...
Don't use too generic terms like "Entity".
```

and constrains edge *descriptions* to use the endpoint names:

```
Every edge should include a description when the text supports relevant information about the endpoints.
The description must use the endpoint names, stay dry and efficient, and may include useful qualifiers
from the source text.
Do not add outside knowledge.
  - Good: Alice works at Acme as a platform engineer on the search team.
  - Bad: This edge describes an employment relationship.
```

The Good/Bad pair is a direct instruction against meta-description — the same failure as Vestige storing "the bug we discussed".

**Primary source:** repository prompt files. Verified verbatim.

### LangMem — self-containment delegated to the user, with a confidence escape hatch

LangMem's memory schema field description ([`src/langmem/knowledge/extraction.py`](https://raw.githubusercontent.com/langchain-ai/langmem/main/src/langmem/knowledge/extraction.py)):

```python
class Memory(BaseModel):
    """Call this tool once for each new memory you want to record..."""
    content: str = Field(
        description="The memory as a well-written, standalone episode/fact/note/preference/etc."
        " Refer to the user's instructions for more information the preferred memory organization."
    )
```

and its default system instructions `_MEMORY_INSTRUCTIONS` (same file):

```
1. **Extract & Contextualize**
   - Identify essential facts, relationships, preferences, reasoning procedures, and context
   - Caveat uncertain or suppositional information with confidence levels (p(x)) and reasoning
   - Quote supporting information when necessary

2. **Compare & Update**
   - Attend to novel information that deviates from existing memories and expectations.
   - Consolidate and compress redundant memories to maintain information-density; strengthen based on
     reliability and recency; maximize SNR by avoiding idle words.
   - Remove incorrect or redundant memories while maintaining internal consistency

3. **Synthesize & Reason**
   - What can you conclude about the user, agent ("I"), or environment using deduction, induction, and abduction?
...
As the agent, record memory content exactly as you'd want to recall it when predicting how to act or respond.
Prioritize retention of surprising (pattern deviation) and persistent (frequently reinforced) information,
ensuring nothing worth remembering is forgotten and nothing false is remembered.
Prefer dense, complete memories over overlapping ones.
```

LangMem's distinguishing move is **framing**: memories are written in the voice the agent will read them in ("record memory content exactly as you'd want to recall it"), and uncertainty is carried *in the record* via an explicit `p(x)` rather than being silently asserted. It also explicitly tempts inference ("what can you conclude... using deduction, induction, and abduction") while requiring probabilistic qualification — worth contrasting with the systems below that forbid inference outright.

**Primary source:** repository source. Verified verbatim.

### MemMachine — the anti-example

`MemMachine`'s profile-extraction prompt ([`semantic_prompt_template.py`](https://raw.githubusercontent.com/MemMachine/MemMachine/main/packages/server/src/memmachine_server/semantic_memory/util/semantic_prompt_template.py)) has good atomicity language:

```
How to construct profile entries:
- Entries should be atomic. They should communicate a single discrete fact.
- Entries should be as short as possible without corrupting meaning. Be careful when leaving out
  prepositions, qualifiers, negations, etc.
```

but then explicitly mandates inference, with few-shot examples that invent demographic attributes from a single message:

```
Further Guidelines:
- Not everything you ought to record will be explicitly stated. Make inferences.
```

```
Query: I'm planning a dinner party for 8 people next weekend ...
[
  { "tag": "Financial Profile", "feature": "upper_class",
    "value": "User entertains guests at dinner parties, suggesting affluence." }
]
```

```
Query: my boss (for the summer) is totally washed...
[
  { "tag": "Demographic Information", "feature": "summer_job", ... },
  { "tag": "Demographic Information", "feature": "young_adult",
    "value": "User is young, possibly still in college" }
]
```

This is a documented, prompt-level generator of exactly the class of low-value memory the user is complaining about: an inference dressed as a fact, with no provenance and no confidence. **Cited as a negative result, not a recommendation.** Primary source: repository file. Verified verbatim.

### A-MEM — the note has a *generated context field* as a first-class attribute

A-MEM (NeurIPS 2025; paper [arXiv:2502.12110](https://ar5iv.labs.arxiv.org/html/2502.12110v2), code [`WujiangXu/A-mem`](https://github.com/WujiangXu/A-mem), prompts live in [`memory_layer.py`](https://raw.githubusercontent.com/WujiangXu/A-mem/main/memory_layer.py), not a `prompts.py`). Its note schema is:

```
m_i = {c_i, t_i, K_i, G_i, X_i, e_i, L_i}
```
where `c_i` = raw content, `t_i` = timestamp, `K_i` = keywords, `G_i` = tags, `X_i` = **LLM-generated contextual description**, `e_i` = embedding, `L_i` = links.

The construction prompt (`MemoryNote.analyze_content`) is:

```
Generate a structured analysis of the following content by:
    1. Identifying the most salient keywords (focus on nouns, verbs, and key concepts)
    2. Extracting core themes and contextual elements
    3. Creating relevant categorical tags

    Format the response as a JSON object:
    {
        "keywords": [
            // several specific, distinct keywords that capture key concepts and terminology
            // Order from most to least important
            // Don't include keywords that are the name of the speaker or time
            // At least three keywords, but don't be too redundant.
        ],
        "context":
            // one sentence summarizing:
            // - Main topic/domain
            // - Key arguments/points
            // - Intended audience/purpose
        ,
        "tags": [
            // several broad categories/themes for classification
            // Include domain, format, and type tags
            // At least three tags, but don't be too redundant.
        ]
    }
```

The structural idea worth stealing: **the stored unit is not the raw sentence**. It is raw content *plus* a generated one-sentence context *plus* keywords *plus* tags, and the retrieval embedding is computed over the concatenation:

```
e_i = f_enc[ concat(c_i, K_i, G_i, X_i) ]
```

Keywords and tags are explicitly excluded from containing the speaker name or the time, which forces them to be topical rather than incident-specific.

**Primary source:** paper + repository. Verified verbatim.

### MemoryOS — knowledge-base entries are extracted, but the *page* is the retrieval unit

MemoryOS ([arXiv:2506.06326](https://ar5iv.labs.arxiv.org/html/2506.06326), [BAI-LAB/MemoryOS](https://github.com/BAI-LAB/MemoryOS)) stores a three-tier hierarchy: STM dialogue pages `{Q_i, R_i, T_i}`, MTM topic segments grouping pages, and LPM persona (User Profile + User KB + User Traits, 90 trait dimensions). The write path is paging-driven, not extraction-driven — pages move between tiers by queue pressure (STM→MTM is FIFO) and a heat score `Heat = α·N_visit + β·L_interaction + γ·R_recency`. Extraction happens at the MTM→LPM step, where "factual information relevant to the user and agent assistant is extracted and recorded into the User KB". The User KB is a **fixed-size FIFO queue of 100 entries**.

The earlier claim that the paper does not publish the extraction prompt was wrong — the prompt is in the repository, not the paper. From [`memoryos-chromadb/prompts.py`](https://raw.githubusercontent.com/BAI-LAB/MemoryOS/main/memoryos-chromadb/prompts.py):

```
KNOWLEDGE_EXTRACTION_SYSTEM_PROMPT = """You are a knowledge extraction assistant. Your task is to extract user private data and assistant knowledge from conversations.

Focus on:
1. User private data: personal information, preferences, or private facts about the user
2. Assistant knowledge: explicit statements about what the assistant did, provided, or demonstrated

Be extremely concise and factual in your extractions. Use the shortest possible phrases.
"""

KNOWLEDGE_EXTRACTION_USER_PROMPT = """...
【User Private Data】
Extract personal information about the user. Be extremely concise - use shortest possible phrases:
- [Brief fact]: [Minimal context(Including entities and time)]
...
【Assistant Knowledge】
Extract what the assistant demonstrated. Use format "Assistant [action] at [time]". Be extremely brief:
- Assistant [brief action] at [time/context]
..."""
```

This is worth reading carefully against Mem0, because it is the **exact opposite** policy: `"Be extremely concise and factual"`, `"Use the shortest possible phrases"`, and a bracket asking only for `[Minimal context(Including entities and time)]`. There is no pronoun rule, no self-containment requirement, and no anti-generalization clause. The instruction to be *shortest* while omitting any rule about what makes a short phrase interpretable is a direct, documented generator of the fragment class the user is complaining about. Note also the stilted output format it produces (`"Assistant [brief action] at [time/context]"`) — a template that is structurally incapable of carrying the context a later reader needs.

**Code/paper divergence — do not trust the paper's mechanism description.** The paper says eviction is by lowest heat. The shipped code selects the victim by **LFU**, ignoring the computed heat entirely ([`memoryos-chromadb/mid_term.py`](https://raw.githubusercontent.com/BAI-LAB/MemoryOS/main/memoryos-chromadb/mid_term.py)):

```python
def evict_lfu(self):
    ...
    lfu_sid = min(self.access_frequency, key=lambda k: self.access_frequency[k])
    print(f"MidTermMemory: LFU eviction. Session {lfu_sid} has lowest access frequency.")
```

The heat `max-heap` is built and maintained (`heapq.heappush(self.heap, (-session_obj["H_segment"], session_id))`) but never consulted to pick an eviction victim. The shipped constants also disagree with the paper: `RECENCY_TAU_HOURS = 24` (paper: `μ = 1e+7` seconds) and `HEAT_GAMMA = 1` as an integer. **LRU appears in the paper only as prior OS art, and is not implemented.**

The relevant lesson is architectural rather than prompt-level: MemoryOS keeps the *conversational* form as the retrieval unit and only distils to facts in a deliberate, separate consolidation pass. That is a valid answer to "the memory doesn't make sense alone" — don't store the fragment alone; store it with its episode. But its own extraction prompt, when it does run, generates the fragment problem; the architecture is what saves it, not the prompt.

**Primary sources:** paper for architecture and formulas; repository prompt and code files for the extraction prompt and the LFU divergence. The paper's ablation numbers are rasterized in a figure — see §6.9.

---

## 2. Does anyone explicitly solve "the memory refers to something outside itself"?

Yes — five distinct strategies appear, and they are not equivalent.

### (i) Rewrite it away — pronoun/antecedent substitution

- **Mem0**: `Replace all pronouns with specific names or "User."`
- **Graphiti**: `Pronoun references ... should be disambiguated to the names of the reference entities.` Plus a list of antecedents that must be *attached* rather than resolved: bare kinship and pet terms become `"Nisha's dad"`, `"Jordan's dog"`.
- **Cognee**: Coreference Resolution section, `always use the most complete identifier for that entity throughout the knowledge graph`.
- **MemReader / MemOS**: the most thorough statement of this rule found, and it is systemic across that project's prompt variants rather than a single lucky line — [`mem_reader_prompts.py`](https://raw.githubusercontent.com/MemTensor/MemOS/main/src/memos/templates/mem_reader_prompts.py):
  ```
  2. Resolve all time, person, and event references clearly:
     - Convert relative time expressions (e.g., "yesterday," "next Friday") into absolute dates using the message timestamp if possible.
     - Clearly distinguish between event time and message time.
     - If uncertainty exists, state it explicitly (e.g., "around June 2025," "exact date unclear").
     - Include specific locations if mentioned.
     - Resolve all pronouns, aliases, and ambiguous references into full names or identities.
     - Disambiguate people with the same name if applicable.
  3. Always write from a third-person perspective, referring to user as
  "The user" or by name if name mentioned, rather than using first-person ("I", "me", "my").
  For example, write "The user felt exhausted..." instead of "I felt exhausted...".
  ```
  with the required output field `"value": <A detailed, self-contained, and unambiguous memory statement ...>`. The named failure modes are the strongest part: `clearly distinguish between event time and message time`, `disambiguate people with the same name`, and — importantly — an **explicit uncertainty escape hatch** (`"around June 2025", "exact date unclear"`) so the model is not forced to fabricate a definite date in order to satisfy an absolutization rule. The two core rules (`Resolve all pronouns, aliases, and ambiguous references into full names or identities` and third-person-only writing) recur verbatim in the doc-reader and general-string-reader variants in the same file, so this is a project-wide policy rather than one prompt.
- **Vestige already does this** for the third-person case.

Critical observation: **all of these operate within a single extraction window.** Mem0 supplies "Last k Messages (up to 20) preceding New Messages. Use to resolve references and pronouns in New Messages." Graphiti supplies `<PREVIOUS_MESSAGES>` with the constraint `You may use information from the PREVIOUS MESSAGES only to disambiguate references or support continuity.` MemReader scopes the rule to `the message timestamp`. So the antecedent must be *inside the window*. Nothing in any of these systems handles an antecedent that only exists in the agent's own head, in a tool result that scrolled out of the window, or in an earlier session's unextracted tail.

### (ii) Forbid the reference by requiring an explicit subject and a "read alone later" test

- **Graphiti**: `If a phrase would not be distinguishable when read alone later, do NOT extract it.` and `When in doubt, do NOT extract.`
- **Cognee**: `Write complete sentences with clear subjects from the first section.` / `Each fact must stand alone without the chunk or the other facts.`
- **Mem0** (additive prompt): `Every memory must be understandable on its own.`

This is the strongest and cheapest rule, and it is a **gate, not a rewrite**. The distinction matters: rewriting preserves coverage, gating loses information. Graphiti chooses gating; Mem0 chooses rewriting.

### (iii) Store the antecedent alongside — keep the retrieval unit larger than the fact

- **MemoryOS**: the dialogue page (query + response + timestamp) is the unit; facts are a separate, slower, lossy tier.
- **A-MEM**: `X_i`, the generated contextual description, is stored *with* the content and is part of the embedded text. The retrieved note carries its own gloss.
- **LightMem** ([arXiv:2510.18866](https://ar5iv.labs.arxiv.org/html/2510.18866)): the stored LTM entry is `{topic, embedding(sum_i), user_i, model_i}` — a topic label *and* the original user and model turns *and* a summary. The topic label is a decontextualization key: it tells a later reader what the summary is about without reading it.
- **Letta / MemGPT** (§6.8): the strongest version of this strategy. Memory *blocks* are "always visible - no retrieval needed" — they are prepended to the system prompt, so a block never has to justify itself against a source the reader cannot see. Letta's own docs state the routing rule: *"Information that should always be visible → Use memory blocks. Information that needs frequent modification → Use memory blocks."* The tradeoff is explicit: archival fragments "cannot be pinned to the context window, and must be queried on-demand", and for those Letta offers no formulation rule at all.

### (iv) Store a pointer to the source — the two-granularity answer

This is the most direct answer to the user's complaint (a) and (b), and it comes from **TriMem** ([arXiv:2605.19952](https://ar5iv.labs.arxiv.org/html/2605.19952)):

> "we add an additional dimension `f_src` to the extraction prompt to obtain a source dialogue identifier `e_i.src ≜ {r_t, u_t ∈ w_i}` for each entry `e_i` to address the lossy storage problem."

Each extracted fact carries an anchor into the raw dialogue it came from, so retrieval hits the fact (efficient) and then *resolves* to the raw dialogue (fidelity). The paper's measured motivation: **"the extracted fact lose 14.5% more information than original dialogue"** in reference-answer token coverage, with a worked example (`trans` modifier dropped). TriMem also builds incremental **entity profiles** — a synthesized understanding layer over the scattered facts — precisely because "reasoning for many real-world questions relies on understanding rather than simple fact matching".

The design lesson for a code memory: a memory that says `foo.rs:412` is *using* strategy (iv) but with a pointer that rots. Strategy (iv) is correct; the anchor choice is what's broken.

### (v) Defer the write, or repair it afterwards — the strategies Vestige most lacks

Two mechanisms that appear in none of the systems above, both from MemReader/MemOS, and worth distinguishing carefully because they change *when* the problem is handled rather than *how* the text is phrased.

**Deferral — search instead of guessing.** MemReader's teacher (ReAct) prompt, quoted from the paper's appendix ([arXiv:2604.07877](https://ar5iv.labs.arxiv.org/html/2604.07877), Appendix B.1) — *not* from the prompt file linked above, which does not contain it:

```
# Principles:
- Do not distinguish between the user or the assistant; extract if it has long-term value.
- References must be explicit; do not use "I" or "me"; must include specific names or roles.
- Normalize time (combine with current time {session_time}) and facts.

# Preferences:
- Do not be hasty to `add`. If there is potential background information that needs to be supplemented, prioritize `search`.
- When encountering ambiguous information (e.g., "he", "that thing"), you MUST prioritize `search` to try and find the answer in the history.
- Only choose `buffer` when `search` cannot resolve the issue, or when it is obvious the user hasn't finished speaking (a new topic).
- Do not choose `buffer` out of laziness. If you can complete the information via `search` and `add` immediately, that is the highest priority behavior.
```

The served memory-management prompt states the same policy more compactly: `Process the dialogue and decide: add (extract), buffer (wait), ignore, or search for context.` and `Prioritize search when encountering ambiguous info ("he", "that thing")` / `Do NOT buffer out of laziness`.

The design: when a reference cannot be resolved from the current window, **do not write a memory containing that reference**. Run a retrieval against history first; write only if the antecedent is found; otherwise *buffer* (hold the item) rather than commit a scrap. `SIMPLE_STRUCT_ADD_BEFORE_SEARCH_PROMPT` in the prompt file is the related guard that evaluates candidate memories against existing ones before an add. This is the only *deferral* design found, and it is conceptually different from (i)–(iv): it treats an unresolvable reference as a reason to **not write yet**, which is the right response when the alternative is a permanently uninterpretable memory. Vestige's write path is synchronous and always-commit, so it has no equivalent.

**Repair — a post-write validating rewriter.** The same project ships a validation pass:

```
You are a strict, language-preserving memory validator and rewriter.

Your task is to eliminate hallucinations and tighten memories by grounding them strictly in the user's
explicit messages. Memories must be factual, unambiguous, and free of any inferred or speculative content.
```

with an explicit source-attribution requirement:

```
3. **Source Attribution Requirement**:
   - Every memory must be clearly traceable to its source:
     - If a fact appears **only in [assistant] messages** and **is not affirmed by [user]**, label it as "[assistant] memory".
     - If [assistant] states something and [user] explicitly contradicts or denies it, label it as
       "[assistant] memory, but [user] [brief quote or summary of denial]".
```

and — the subtle, load-bearing part — an exception protecting legitimate temporal grounding:

```
4. **Timestamp Exception**: Memories may include timestamps (e.g., "On December 19, 2026") derived from
conversation metadata. If such a date likely reflects the conversation time (even if not in the `messages`
list), do NOT treat it as hallucinated
```

That exception exists because a naive "ground everything in the source text" validator flags every correctly-absolutized date as hallucinated — the absolute date never literally appears in the transcript. Any repair pass that insists on **both** strict grounding **and** absolute dates must carve out exactly this exception, or the two rules destroy each other. This is a concrete, non-obvious interaction that only shows up once you try to build the pass, and it is worth copying deliberately rather than rediscovering.

### What is *not* solved anywhere

No system found handles:
- an antecedent that was never in the extraction window (tool output, earlier session, agent's own reasoning) — MemReader's deferral is the closest thing, and it defers rather than resolves;
- a reference to a mutable external artefact (file, line, symbol, commit) with a staleness model — except `legendary-mcp`, see §5;
- the requirement that a stored item be understandable to a reader who has **neither** the conversation **nor** the linked memories, i.e. checking resolvability of the links themselves. Mem0's `linked_memory_ids` and A-MEM's `L_i` create links but no prompt requires that the link target be present at read time.

---

## 3. Granularity — and what evidence backs it

The sources disagree, and this is the most important disagreement in the review.

| System | Recommended granularity | Basis |
|---|---|---|
| Mem0 `FACT_RETRIEVAL_PROMPT` | one short fact per item; splits compound sentences into multiple facts | few-shot examples |
| Mem0 `ADDITIVE_EXTRACTION_PROMPT` | **15–80 words, 1–2 sentences, "contextually rich, not atomic"**; split only when detail-rich | explicit prompt rules |
| Graphiti | one triple per edge, but with an explicit anti-generalization clause | schema + prompt |
| A-MEM | one atomic Zettelkasten note (raw content + keywords + tags + context) | paper §3.1 |
| Cognee | numbered list of self-contained facts, topic-ordered | prompt |
| LangMem | "Prefer dense, complete memories over overlapping ones" | prompt |
| MemMachine | "Entries should be atomic. They should communicate a single discrete fact." | prompt |
| MemoryOS | the whole dialogue page is the unit; facts are a separate LPM tier | paper §3.2 |
| TriMem | all three at once: raw window + atomic fact + synthesized profile | paper §3 |

**The two Mem0 prompts are the natural experiment.** The same vendor ships a "one short fact" extractor and an "15–80 word contextually rich" extractor. The newer one is explicitly justified inside the prompt:

```
### Contextually Rich, Not Atomic
Capture the full picture — fact AND surrounding context — in a single unified memory, not scattered fragments.
Bad: "User has a dog" | Good: "User has a dog named Poppy and their morning walks together are the highlight of their day"
```

and for transitions specifically:

```
Bad: "User prefers oat milk lattes"
Good: "User switched from almond milk to oat milk lattes after developing an almond sensitivity"
```

This is an admission that pure atomicity destroys the context that makes an item interpretable. A bare "Prefers oat milk lattes" is a scrap; the same fact with its predecessor and rationale is a memory.

Graphiti takes the opposite tack — keep the triple atomic — but compensates with **entity summaries**: node summaries are written to 2–6 dense sentences under the instruction *"Be exhaustive within the evidence. Prefer retaining a supported concrete detail over omitting it for brevity."* The triple gives precision; the entity summary gives context. TriMem formalizes this as a first-class three-level architecture.

**Vestige's "one fact per memory / compound_content_warning" rule is the Mem0 v1 position.** The evidence in Mem0's own repository suggests the v3 position (context-rich single memory, split only when there are genuinely multiple topics) is the deliberate replacement.

### Evidence that granularity is hard to hold constant

TriMem's third documented failure of fact-centric pipelines is **unstable extraction granularity**:

> "Realistic long-term interactions involve highly diverse information styles, expression patterns and content categories, whereas conventional systems rely on static hand-written extraction prompts. Such fixed prompts fail to adaptively accommodate heterogeneous dialogue content, making it impossible to maintain consistent and rational fact extraction granularity."

Worked example from the paper: *"the Pomodoro technique is sometimes explicitly mentioned by name, while in other cases it is implicitly described as 25 minutes on and 5 minutes off. The fixed prompt cannot recognize such high-level semantic concepts, resulting in inconsistent extraction granularity."*

Their fix is **TextGrad-based prompt optimization** — treating the extraction and profile prompts as trainable natural-language parameters and back-propagating answer-quality failures into prompt edits. That is a 2026 result and is the only mechanism found that adapts granularity without a human rewriting the prompt.

---

## 4. Metadata a later reader needs

| Field | System(s) | Notes |
|---|---|---|
| `valid_at` / `invalid_at` (ISO 8601, Z) | Graphiti schema `Edge` | extracted by a **separate** `extract_timestamps` call; `Leave both fields null if no explicit or resolvable time is stated` and `Do NOT hallucinate or infer dates from unrelated events` |
| `t_valid`/`t_invalid` **and** `t'_created`/`t'_expired` (bi-temporal) | Zep / Graphiti paper | event time vs. transaction time — lets you answer "what did we believe on date X" |
| observation date vs. current date, passed as separate inputs | Mem0 | guards retrospective extraction |
| `episode_indices` / source episode | Graphiti `Edge`, `ExtractedEntity` | traceability to the episode(s) a fact came from |
| `f_src` raw-dialogue identifier | TriMem | the pointer that makes lossy facts recoverable |
| `linked_memory_ids` | Mem0 additive prompt | for same-entity, updated-preference, continuation, contradiction |
| `L_i` link set | A-MEM | plus a memory *evolution* pass that rewrites neighbours' context and tags |
| `attributed_to` (user \| assistant) | Mem0 | explicit attribution of who the memory is about |
| `p(x)` confidence + reasoning | LangMem `_MEMORY_INSTRUCTIONS` | confidence carried in the record text, not a separate column |
| keywords / tags / generated context | A-MEM | part of the embedded text, not just filters |
| `supersedes` + anchor coverage | `legendary-mcp` | see §5 |
| heat (visits, interactions, recency) | MemoryOS | drives tier migration and eviction, not retrieval |

**Supersession semantics.** Graphiti's dedup prompt ([`dedupe_edges.py`](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/dedupe_edges.py)) separates the two operations cleanly and has a sharp example:

```
EXISTING FACT: idx=1, "Alice works at Acme Corp as a software engineer"
NEW FACT: "Alice works at Acme Corp as a senior engineer"
Result: duplicate_facts=[], contradicted_facts=[1] (same relationship but updated title — contradiction, NOT a duplicate)
```

with the guard `NEVER mark facts as duplicates if they have key differences, particularly around numeric values, dates, or key qualifiers.` This distinction — *same relationship, updated qualifier* = contradiction, not duplicate — is the one most systems get wrong, and it is exactly the distinction Vestige needs if it is to avoid either silently merging or silently duplicating a decision that changed.

Important caveat: Zep is a **vendor-authored paper** ([arXiv:2501.13956](https://arxiv.org/abs/2501.13956)). The architecture claims are checkable against the Apache-2.0 Graphiti code, which corroborates the prompt text; the benchmark numbers are the vendor's own.

---

## 5. Code specifically

This is the weakest-covered area in the literature and the strongest match to the user's complaint (a).

### What the general systems do

- **They do not model code at all.** No extraction prompt found in Mem0, Graphiti, A-MEM, Cognee's general graph prompt, LangMem, MemoryOS, TriMem, LightMem, or MemMachine contains any notion of a file, symbol, commit, or line.
- **Cognee is the partial exception.** It has a code-retrieval prompt ([`codegraph_retriever_system.txt`](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/infrastructure/llm/prompts/codegraph_retriever_system.txt)) that extracts file names and code blocks from a *query* as retrieval keys:

```
1. **Identify File Names:** Extract filenames from inline text, headers, or markdown formatting.
   Empty list of filenames is completely normal.
2. **Extract Code:** Extract code pieces that are in the text (do not add additional content) and
   maintain their indentation and formatting.
```

  This tells us filenames are treated as a retrieval signal, not as a memory anchor, and there is no hashing, no staleness check, no commit pinning.

- **Cognee also has a rule-generalization prompt** ([`coding_rule_association_agent_system.txt`](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/infrastructure/llm/prompts/coding_rule_association_agent_system.txt)) which is the closest thing to guidance against the user's defect (a):

```
Suggest rules that are general and not specific to the current text, strictly technical, add value
and improve the future Cursor agent behavior.
Do not suggest rules similar to the existing ones or rules that are not general and dont add value.
It is acceptable to return an empty rule list.
```

  *"It is acceptable to return an empty rule list"* is the explicit permission to store nothing, which most extractors lack. And *"general and not specific to the current text"* is the direct instruction that a memory referencing *this* file at *this* line is not a memory.

### What a code-aware memory system does

`legendary-mcp` (v0.2.1, MIT, 2025–2026) is a small local-first MCP memory server built specifically for coding agents, and its `Concepts` documentation is the most concrete published anchor design found:

```
An anchor is {file, symbol?, lines?, commit, content_hash}. At write time the symbol is resolved
with tree-sitter and the region is hashed. At recall time it is re-resolved and re-hashed.
```

with the verdict table:

| Verdict | Meaning |
|---|---|
| `fresh` | the region hashes the same — rendered `(verified against current code)` |
| `stale` | the region changed — rendered with an instruction to verify |
| `orphaned` | the file or symbol is gone |

and the crucial ordering rule:

```
Whitespace-only edits do not invalidate a memory, and a symbol that merely moves stays fresh,
because anchors re-resolve by symbol before hashing.
```

**That sentence is the direct answer to the line-number problem.** `lines?` is present in the anchor but is *not* the identity: resolution is by symbol first, then by content hash of the resolved region. A line number alone rots on the next insertion above it; a symbol plus a content hash survives a move and degrades gracefully on a change.

Two further design decisions are documented with their failure rationale:

```
An episode without a verbatim error string cannot be matched against a future failure, so it can only
be found by search — the weakest channel. legendary rejects such writes with an actionable message
rather than saving something that will never resurface.

Good: triggers: ["sqlite3.OperationalError: database is locked"]
Useless: triggers: ["database problem"]
```

and, on supersession:

```
`supersedes` requires the replacing memory to cover every anchor of the memory it replaces. This was
added after observing a real agent deprecate a broad, two-file memory with a narrower one, leaving a
file with no active memory at all. If you cannot cover the anchors, use `deprecate(reason=...)` instead.
```

The documented failure is worth quoting as-is: **an agent replaced a two-file memory with a narrower one and silently orphaned a file.** That is a coverage-regression bug in the write path that no general memory system guards against.

Staleness is also given a rationale that applies directly to Vestige:

```
Delivering a confident "do it this way" for code that has since changed is worse than delivering
nothing — it is the error-propagation failure mode that memory systems without verification cannot avoid.
```

**Provenance class:** this is a small open-source project's own documentation. It is primary (it describes the shipped implementation) but it is not peer-reviewed and has no published evaluation. There is a `Benchmark` page; I did not verify its methodology. Treat the design as a well-argued proposal, not as validated evidence.

### Claude Code: the counter-position, from the largest deployed system

Anthropic's Claude Code memory documentation ([code.claude.com/docs/en/memory](https://code.claude.com/docs/en/memory)) takes the opposite approach for the same problem — it does not store code-adjacent facts at all, it stores *instructions*, and it enforces locality by file path rather than by anchor:

```
**Specificity**: write instructions that are concrete enough to verify. For example:
* "API handlers live in `src/api/handlers/`" instead of "Keep files organized"
```

```
**Size**: target under 200 lines per CLAUDE.md file.
```

and it scopes rules by glob rather than by line:

```
Rules can be scoped to specific files using YAML frontmatter with the `paths` field. These conditional
rules only apply when Claude is working with files matching the specified patterns.
```

and it names the code-specific failure mode explicitly:

```
**Consistency**: if two rules contradict each other, Claude may pick one arbitrarily. Review your
CLAUDE.md files ... periodically to remove outdated or conflicting instructions.
```

This system deliberately avoids the rot problem by anchoring to **directory/glob granularity** (which almost never changes) rather than to files or lines, and by keeping the whole memory set small enough that periodic human review is feasible. It is a legitimate alternative to hashing.

---

## 6. Documented failure modes, and evidence on whether write-time work pays

### 6.1 The strongest negative result: write strategy barely matters relative to retrieval

**"Diagnosing Retrieval vs. Utilization Bottlenecks in LLM Agent Memory"** ([arXiv:2603.02473](https://ar5iv.labs.arxiv.org/html/2603.02473), UCSD/CMU/UNC, ICLR 2026 workshop; code at [boqiny/memory-probe](https://github.com/boqiny/memory-probe)) runs a controlled 3×3 factorial on LoCoMo (1,540 non-adversarial questions, GPT-5-mini backbone, k=5):

| Write strategy | Cosine | BM25 | Hybrid+Rerank |
|---|---|---|---|
| Basic RAG (raw 3-turn chunks, 0 LLM calls) | 77.9 | 59.2 | **81.1** |
| Extracted Facts (Mem0-style) | 72.2 | 49.4 | 77.3 |
| Summarized Episodes (MemGPT-style) | 70.1 | 62.7 | 73.3 |

Their conclusions, verbatim:

> "retrieval method is the dominant factor: average accuracy spans 20 points across retrieval methods (57.1% to 77.2%) but only 3–8 points across write strategies. Raw chunked storage, which requires zero LLM calls, matches or outperforms expensive lossy alternatives, suggesting that current memory pipelines may discard useful context that downstream retrieval mechanisms fail to compensate for."

> "Retrieval precision is near-perfectly correlated with downstream accuracy (r = 0.98)."

> "Retrieval failure is the dominant error mode, accounting for 11–46% of all questions depending on configuration... utilization failures remain stable at 4–8% and hallucinations at 0.4–1.4%."

**Their extraction prompt is published in full in Appendix C.1** and is a third independent statement of the self-containment rule:

```
You are a memory extraction agent. Given a conversation session between two people, extract ALL
important facts, events, preferences, and details mentioned. For each fact, provide:
- fact: A concise, self-contained statement (should make sense without the original context)
- speakers: Which speaker(s) this fact is about
- type: One of [event, preference, relationship, plan, personal_detail, opinion]
```

with the conflict-resolution prompt:

```
- ADD: The new fact contains genuinely new information not covered by existing memories.
- UPDATE: The new fact updates/supersedes one of the existing memories (provide the ID to update).
- NOOP: The new fact is redundant --- the information is already fully captured.
```

**How to read this against the user's complaint.** This study measures *answer quality on LoCoMo*, a benchmark of benign personal-chat facts. It does not measure whether the one fact a user needed was *stored at all*, and it explicitly acknowledges the limitation: *"The advantage of raw chunking may also diminish under tighter context budgets where compression is required."* The correct inference is not "write-time work is pointless". It is:

- The measurable, general-purpose gains are in retrieval (hybrid + rerank), which Vestige already has.
- A write-time defect that makes an item **unfindable or uninterpretable** is not visible in aggregate accuracy on a benchmark where the raw transcript is still in the corpus. It becomes fatal only when the source is gone — which is exactly the user's situation, where the "source" is a past conversation that no longer exists.
- Therefore, write-time work should be justified on **interpretability and resolvability**, not on expected benchmark uplift. Any claim that better write-time prompts will move retrieval metrics is not supported by this study.

### 6.2 Lossy storage

From TriMem ([arXiv:2605.19952](https://ar5iv.labs.arxiv.org/html/2605.19952)):

> "we calculate the coverage rate of reference answer token, it can be seen that the extracted fact loss 14.5% more information than original dialogue. For example, the modifier trans in the original dialogue is omitted during fact extraction."

Their fix is the source identifier (strategy iv above), not a better prompt.

### 6.3 Shallow reasoning over scattered facts

Same paper:

> "The results reveal that the reasoning performance on multi-evidence questions is considerably inferior to that on single-evidence ones... multi-evidence questions demand deep understanding of dispersed facts, such as emotional inference and logical induction. This phenomenon adequately demonstrates that reasoning mechanisms solely relying on extracted fact completely lack the ability of deep comprehension towards entity semantic portraits or behavioral tendency modeling."

Fix: an incremental **profile** layer aggregated per entity. This is the argument for a synthesis layer *on top of* atomic memories, not *instead of* them.

### 6.4 Label pollution and reasoning leakage in structured fields

Graphiti's attribute-extraction prompt documents, in its `BAD` examples, a failure mode that any schema-constrained extractor will recognize:

```
BAD  → "phones": "415-555-0142 (implied by original entity, but no new information in
        messages, retaining original value as per instruction...)"
BAD  → "industry": "Content platform, SaaS (implied by usage context, though not stated
        explicitly as industry classification...)"
```

The countermeasures are unusually blunt and transferable:

```
2. NEVER write reasoning, justification, or commentary into any field.
3. Each attribute schema description ... tells you the FORMAT a real value should take.
   The description text is NEVER itself a value. NEVER copy schema description text into the field.
4. The literal strings "null", "N/A", "Not specified", "unknown", "none", "not provided",
   or any sentence describing absence are NOT valid values.
```

### 6.5 Inference dressed as fact (and its poisoning consequence)

MemMachine's prompt mandates inference (§1). The security literature shows why this is dangerous at the write path. **MINJA** ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)) demonstrates that an attacker can inject malicious records "by only interacting with the agent via queries and output observations", using "a sequence of bridging steps to link victim queries to the malicious reasoning steps" and "a progressive shortening strategy that gradually removes the indication prompt, such that the malicious record will be easily retrieved when processing later victim queries". In other words: the extractor's willingness to record inferred content is the injection surface. An extractor that records only what is traceable to the source is a defence.

### 6.6 Extraction volume / trivia

Mem0's additive prompt contains a defensive section that names the failure directly:

```
# CRITICAL: Exhaustive Extraction Checklist
...
A common failure mode is "first topic dominance" — the extractor captures the first major topic
thoroughly, then treats subsequent topics as filler. This is WRONG.
```

and, in the opposite direction, a `Do NOT extract` list (greetings, filler, vague acknowledgments, assistant meta-commentary). The interesting thing is that Mem0 explicitly resolves the over/under-extraction tension toward over-extraction:

```
**When in doubt, extract.** A slightly redundant memory is far less costly than a missing one.
The deduplication system downstream will handle true duplicates
```

while Graphiti resolves it the other way:

```
When in doubt, do NOT extract.
```

These are opposite policies, both stated in primary prompt text. The choice depends on whether the store has a reliable dedup/invalidation path. **Vestige does** (prediction-error gating, `find_duplicates`, temporal invalidation), which puts it on Mem0's side of that trade — with the caveat that a redundant *uninterpretable* memory is still a cost, because it consumes retrieval slots.

### 6.7 Episodic memory under context pressure (MemGPT)

The MemGPT paper ([arXiv:2310.08560](https://arxiv.org/abs/2310.08560)) documents the mechanism by which memories get written at all, and it is not extraction-driven:

> "When the prompt tokens exceed the 'warning token count' of the underlying LLM's context window (e.g. 70% of the context window), the queue manager inserts a system message into the queue warning the LLM of an impending queue eviction (a 'memory pressure' warning) to allow the LLM to use MemGPT functions to store important information contained in the FIFO queue to working context or archival storage... When the prompt tokens exceed the 'flush token count' (e.g. 100% of the context window), the queue manager flushes the queue to free up space"

> "including any runtime errors that occur (e.g. trying to add to main context when it is already at maximum capacity), are then fed back to the processor by MemGPT."

**This is a documented failure mechanism for memory quality, and it maps onto the user's complaint.** A memory written *under eviction pressure* — "save this before it's lost" — is written by an agent in a hurry with no formulation budget. That is a structural generator of scraps. The mitigation is architectural: decouple the "don't lose it" write from the "formulate it well" write (Vestige's `dream`/consolidation loop is the right shape for this; TriMem's TextGrad prompt optimization and MemoryOS's heat-gated MTM→LPM pass are the same idea).

### 6.8 Letta: the write-time instruction is almost absent — and the *description field* is the real mechanism

Letta (formerly MemGPT) is the most-cited "self-editing memory" system, and its tool docstrings carry almost no formulation guidance. From [`letta/functions/function_sets/base.py`](https://raw.githubusercontent.com/letta-ai/letta/28514da5df44570002cc4a2fa8384fd74f75101f/memgpt/functions/function_sets/base.py) at a pinned commit:

```python
def core_memory_append(self, name: str, content: str):
    """
    Append to the contents of core memory.

    Args:
        name (str): Section of the memory to be edited (persona or human).
        content (str): Content to write to the memory. All unicode (including emojis) are supported.
    """

def core_memory_replace(self, name: str, old_content: str, new_content: str):
    """
    Replace to the contents of core memory. To delete memories, use an empty string for new_content.

    Args:
        name (str): Section of the memory to be edited (persona or human).
        old_content (str): String to replace. Must be an exact match.
        new_content (str): Content to write to the memory. All unicode (including emojis) are supported.
    """

def archival_memory_insert(self, content: str):
    """
    Add to archival memory. Make sure to phrase the memory contents such that it can be easily queried later.

    Args:
        content (str): Content to write to the memory. All unicode (including emojis) are supported.
    """
```

`"Make sure to phrase the memory contents such that it can be easily queried later"` is the **entire** write-time formulation instruction the MemGPT model receives for archival memory. Everything else is a structural constraint (`old_content` must be an exact match, which forces the model to *read* before it writes and makes edits surgical rather than wholesale rewrites).

**But Letta's actual decontextualization mechanism is architectural, not prompt-level, and it is the most interesting idea in this system.** From the current official docs ([docs.letta.com/v1-sdk/memory/memory-blocks](https://docs.letta.com/v1-sdk/memory/memory-blocks)):

```
Memory blocks are structured sections of the agent's context window that persist across all interactions.
They are always visible - no retrieval needed.

Under the hood, memory blocks are simply prepended to the agent's prompt in an XML-like format.
```

```
Memory blocks represent a section of an agent's context window. ... A memory block consists of:
[an id], which is a unique identifier for the block
[a description], which describes the purpose of the block
[the value], which is the contents/data of the block
[a limit], which is the size limit (in characters) of the block

Section titled "The importance of the description field"
When making memory blocks, it's crucial to provide a good description field that accurately describes
what the block should be used for. [The description] is the main information used by the agent to
determine how to read and write to that block. Without a good description, the agent may not
understand how to use the block.
```

The default descriptions are themselves mini write-time prompts:

```
The persona block: Stores details about your current persona, guiding how you behave and respond.
This helps you to maintain consistency and personality in your interactions.

The human block: Stores key details about the person you are conversing with, allowing for more
personalized and friend-like conversation.
```

Two structural properties follow, both directly relevant to complaint (b):

1. **A block is always in context, so it never needs its provenance to be interpretable.** Archival memory is the opposite, and Letta's own docs state the tradeoff plainly ([archival-memory](https://docs.letta.com/v1-sdk/memory/archival-memory)):

   ```
   Unlike memory blocks, archival memory fragments cannot be pinned to the context window, and must be
   queried on-demand via tools.
   - Agents cannot easily modify or delete archival memories (though developers can via SDK)
   ```
   ```
   Information that should always be visible → Use memory blocks
   Information that needs frequent modification → Use memory blocks
   ```

   The docs suggest this routing rule for choosing between the two tiers, based on an "institutional knowledge" example:

   ```
   Use archival for structured knowledge, conversation search for historical context.
   ```

2. **The `description` field is a declarative write contract.** The agent is told, at read time and at write time, what belongs in the block. That is a different control point from an extraction prompt: instead of instructing a *separate extractor model* how to phrase a memory, Letta instructs the *writing agent* what the bucket is for, and lets it decide. It is the mechanism that makes "one bucket per concern" enforceable without a schema.

**Why this matters for a store like Vestige, and what it does not solve.** Vestige's `node_type` (`fact | concept | event | person | place | note | pattern | decision`) is a *classifier label*. Letta's `description` is an *instruction to the writer*. Converting the former into the latter — attaching to each node type a sentence saying what belongs in it and how an entry should read — is a cheap, unclaimed improvement. But note the honest limit: Letta's approach works because blocks are small, few, always-visible, and human-curated. It does not scale to thousands of archival fragments, and Letta offers **no formulation rule at all for archival memory** beyond "phrase it so it can be queried later". So for a large retrieval-backed store, Letta's answer is "route the important things into a curated always-visible tier" — which is a real option (Vestige's `precompute_for_context` and dream digests are the same idea) but not a substitute for a write-time gate on the archival tier.

Caveat on provenance: Letta has since moved active development to [`letta-ai/letta-code`](https://github.com/letta-ai/letta-code); the docstrings quoted are from an earlier pinned commit of the archived `letta-ai/letta` tree and may not reflect current behaviour. The `description`-field guidance is from current official docs and is verified verbatim.

---

### 6.9 Two cautions about the evidence base itself

**MemoryOS's ablation is not verifiable.** The paper reports `-MTM` / `-LPM` / `-Chain` ablations qualitatively ("the Mid-Term Memory (MTM) has the most significant impact, followed by the Long-Term Memory (LPM), while the Chain has the least impact"), but the numbers are rasterized in a figure. A numeric table appears in the arXiv v1 LaTeX source only as a **commented-out block** (`Full Model 91.8/82.3/90.5; w/o Conversation Chain 87.3/80.5/88.3; w/o MTM 46.4/44.6/66.0; w/o Persona Module 77.1/70.5/72.3`). Combined with the code/paper divergence documented in §1 (LFU vs. heat), MemoryOS's mechanism claims should be treated as **unreproduced**. Do not cite its ablation as evidence that tiering works.

**Cognee explicitly declines to make a write-time ablation claim.** Its own BEAM report ([`cognee/eval_framework/beam/REPORT.md`](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/eval_framework/beam/REPORT.md)) states:

```
The components appeared useful for different reasons, but were not isolated. ... we did not run
controlled ablations. The scores therefore describe the combined pipeline, not the contribution
of each component.
```

So the tally on §6.1's question is: **one controlled factorial study** ([arXiv 2603.02473](https://ar5iv.labs.arxiv.org/html/2603.02473)) finds retrieval dominates write by roughly 3×; **three vendor-authored reports** (Mem0, Zep, MemReader) claim write-time structure helps; and **Cognee, the one system with a public eval harness, refuses the comparison.** There is no independent replication of any of them in the sources reviewed. This is a genuinely unsettled question, and §6.1's counter-evidence should be carried alongside the vendor claims rather than treated as an outlier.

---

## 7. What this implies for Vestige

Vestige already does: atomic-memory enforcement, coreference rewriting (`He said X` → `John said X`), temporal anchoring, relation extraction, and provenance tagging. Those cover §1, §2(i), and part of §4. The genuinely new material is elsewhere.

### 7.1 Genuinely new — adopt these

**A. Make the self-containment requirement a *verifiable gate*, not a regex.** The coreference rewriter substitutes pronouns. It cannot detect that `"the retry logic"`, `"the above"`, `"that file"`, `"as discussed"`, or `"the fix"` are dangling references, because they are noun phrases, not pronouns. Every strong system in §1 uses a **semantic test** instead: Graphiti's `"If a phrase would not be distinguishable when read alone later, do NOT extract it"`, Mem0's `"Every memory must be understandable on its own"`, Cognee's `"Each fact must stand alone without the chunk or the other facts."`

The new move for Vestige: at write time, ask *"does this memory contain a definite reference whose antecedent is not inside the memory?"* — a bounded, cheap classifier over the stored text (definite articles with no prior mention, deictics, `as discussed/above/earlier`, unresolved `it/that/this` with no in-text antecedent). Flag it as a `dangling_reference` warning the way `compound_content_warning` already works. **Vestige has a precedent for exactly this pattern** — a failing write-time check that returns a warning the caller must fix — so this is an extension of an existing mechanism, not a new subsystem.

**B. Store the source span, not just a reference — and make the memory resolvable without it.** TriMem's `f_src` is the design to copy: every fact carries an identifier into the raw episode it came from (`docs/review/attachments/...` references here are the analogous idea done badly). The key constraint TriMem adds that Vestige does not have: **the fact must be usable without dereferencing the source.** Their measured problem was the opposite (facts too lossy, so the source is needed); Vestige's problem is that the source is *assumed*. Concretely: a memory may carry a `source_span` for audit, but the `content` must pass the §7.1-A gate on its own, and nothing in retrieval should require the span to be present.

**C. A code anchor that is not a line number.** This is the direct fix for complaint (a), and it is the one area where the general systems offer nothing and a small dedicated project offers a complete design. Adopt the `legendary-mcp` anchor shape — `{file, symbol, commit, content_hash}` with **symbol resolved before hashing** so that a moved symbol stays fresh and a whitespace-only edit does not invalidate — and the three-state verdict `fresh | stale | orphaned` rendered into the retrieved text as an instruction (`(verified against current code)` vs `[stale - code changed since this was written; verify before trusting]`).

Two things make this more than a copy:

1. **Vestige can compute the anchor with what it already has.** It already extracts file paths as entities (the `entity:john-smith`-style auto-tagging is described in `AGENTS.md` as covering "file paths"). An anchor is that entity plus a symbol plus a hash — no new dependency if the symbol comes from the codebase tool's existing `files` field (`codebase(action="remember_pattern", files=[...])` already carries file lists).
2. **The policy question Vestige must answer that legendary does not.** `legendary` *rejects* a write with no trigger (`"legendary rejects such writes with an actionable message rather than saving something that will never resurface"`). That is a defensible rule for a code-only store and would be wrong for a general memory server. The transferable rule is narrower: **a memory that anchors to code must either carry a resolvable anchor or state its claim in a way that survives the code changing.** A memory that says "line 412 is the bug" satisfies neither.

**D. The "same relationship, updated qualifier" distinction.** Graphiti's dedup prompt separates `duplicate` from `contradicted` and gives the rule: *same relationship, changed qualifier → contradiction, NOT a duplicate* (`"Alice works at Acme Corp as a software engineer"` vs `"...as a senior engineer"`). Vestige has contradiction detection and temporal invalidation but the literature's specific failure — treating a qualifier update as either a merge or a new memory instead of a supersession — is worth a targeted check when memories are near-duplicates with differing numbers or dates. Graphiti's guard is worth copying verbatim into whatever prompt or heuristic does this: `NEVER mark facts as duplicates if they have key differences, particularly around numeric values, dates, or key qualifiers.`

**E. Write-time formulation needs a *second, calm* pass.** §6.7 is the finding most likely to explain the user's experience in practice. MemGPT documents that writes happen under eviction pressure; Vestige's `SESSION_END` save gate is structurally the same situation — an agent at the end of a session, saving a batch, with no budget to formulate. Vestige already has the right machinery (`dream`, inline consolidation, reconsolidation windows) but the current design uses it for *ranking and insight*, not for *reformulation*. The new move: treat an under-specified memory as a **repairable** artifact — a consolidation pass that rewrites a dangling memory into a self-contained one (or marks it `needs_context` and suppresses it from retrieval rather than serving a scrap). TriMem's TextGrad loop ([arXiv:2605.19952](https://ar5iv.labs.arxiv.org/html/2605.19952)) is the research-grade version; a much simpler version is a nightly pass that runs the §7.1-A gate over memories written under `SESSION_END` and rewrites the failures.

**F. Turn `node_type` from a label into an instruction.** This is the cheapest idea in the review and it comes from Letta (§6.8). Vestige's `node_type` (`fact | concept | event | person | place | note | pattern | decision`) tells a *reader* how to classify a memory after the fact. Letta's block `description` tells the *writer* what belongs in the bucket, and Letta's docs are explicit that this is the load-bearing part: *"[the description] is the main information used by the agent to determine how to read and write to that block. Without a good description, the agent may not understand how to use the block."*

Concretely: give each node type a sentence of the form *"a `decision` records what was chosen, the alternative rejected, and the reason — not the fact that a decision happened"*, expose it in the `smart_ingest` tool schema next to the `node_type` enum, and return it in the `compound_content_warning` message when the gate fires. This costs one string per type and gives the writing agent a formulation target instead of a category. It also pairs naturally with §7.1-A: the gate detects a dangling reference, the node-type instruction tells the writer how to fix it.

**G. Route interpretability-critical memories into an always-visible tier.** Letta's architectural answer to complaint (b) is not a prompt at all — it is to keep the memories that must be understood without provenance *pinned in context*, so the question of whether they stand alone never arises. Vestige has the primitive (`session_context` returns a markdown context block; `precompute_for_context` builds topic digests with a TTL). The gap is that nothing distinguishes "a memory that must be readable with no lookups" from "a memory that is fine as a retrieval candidate". A `standalone_critical` flag — set by the §7.1-A gate on the memories that *pass* it, for the small set that keep coming back in retrieval — would let `session_context` pin exactly those and stop pinning the scraps. This is the one intervention that makes the problem structurally impossible rather than merely detected.

**H. Give the write path a *defer* outcome, not just commit-or-warn.** MemReader's deferral mechanism (§2-v) is the design with no analogue in Vestige, and it addresses the highest-value case the gate in (A) cannot fix: a memory that fails the standalone test and whose antecedent is genuinely recoverable from history. Today Vestige's options are "warn and store" or "split and store" — both commit. A third outcome is available: on a `dangling_reference` warning, run a bounded retrieval for the missing antecedent (Vestige already has the search stack and the entities), rewrite if found, and if not found, store the memory **quarantined** — present for audit, excluded from retrieval — rather than serving a scrap. The `buffer` state in MemReader is the same idea. This is what converts (A) from a detector into a fix, and it is the one change here that plausibly reduces the number of bad memories rather than merely labelling them.

Two cautions if (A) and (H) are built together: apply MemReader's uncertainty escape hatch, and its timestamp exception. A strict "ground everything / no inference" validator will flag every correctly-absolutized date as hallucinated, because the absolute date is not literally in the transcript. The repair pass must explicitly exempt metadata-derived dates or it will undo Vestige's temporal anchoring.

### 7.2 Reconsider — evidence contradicts the current default

**Granularity.** Vestige enforces one fact per memory and warns on compound content, citing "compound content degrades search recall by 40-60%". The two most mature write paths found both moved *away* from this: Mem0's current prompt says `"Contextually Rich, Not Atomic"` and `"split into multiple focused memories rather than compressing details away"` only when detail-rich; LangMem says `"Prefer dense, complete memories over overlapping ones."` TriMem stores three granularities simultaneously and reports that the atomic-only layer loses 14.5% of reference-answer tokens.

This does not mean the atomic rule is wrong — it is well-suited to a store with FSRS retention and graph relations. But the *unit of a memory* and the *unit of a search hit* need not be the same. TriMem's three-level architecture suggests the target shape: keep the atomic fact as the retrieval key (Vestige's strength), and add a synthesis layer (an entity/problem profile) that the atomic memories roll up into. Vestige's `dream` and `precompute_for_context` already produce digests; the gap is that they are *derived on demand* rather than being a durable, addressable tier that a retrieved fact can resolve to.

**Whether write-time work should be prioritized at all.** [arXiv:2603.02473](https://ar5iv.labs.arxiv.org/html/2603.02473) is a real caution: retrieval method moved accuracy 20 points, write strategy 3–8, and raw chunks beat fact extraction. If Vestige's roadmap is being set by expected retrieval gains, write-time prompts are the wrong place to spend. If the goal is the user's stated one — *a memory that is still worth something a week later* — then the relevant metrics are not LoCoMo accuracy but: (i) what fraction of stored memories pass a standalone-readability check, (ii) what fraction of retrieval hits are judged useful by the reader, and (iii) how many code-anchored memories are stale. **Those three are not measured by any system reviewed here**, and `memory-probe`'s probe-2 (utilization) and probe-3 (failure classification) are the closest published instrumentation to copy.

### 7.3 Adopt first — ranked by expected effect on the reported complaint

1. **A dangling-reference gate at write time** (§7.1-A), reusing the existing `compound_content_warning` return path. This is the smallest change that addresses complaint (b) directly, and it turns an invisible defect into a visible one. It also produces the baseline metric for (i) above at zero extra cost.
2. **A code anchor of `{file, symbol, content_hash}` with a three-state freshness verdict rendered into the retrieved text** (§7.1-C), replacing any current practice of storing a bare path (or path + line). This addresses complaint (a) and is the only intervention in this review with a documented, shipped design behind it.
3. **`node_type` descriptions in the `smart_ingest` schema, plus a `standalone_critical` flag that `session_context` pins** (§7.1-F, §7.1-G). These are two strings-and-a-boolean changes that give the writing agent a formulation target and give retrieval a way to distinguish a memory that stands alone from one that only works as a candidate. Together they are the cheapest structural answer to complaint (b).
4. **A defer/quarantine outcome on the write path when the gate fires** (§7.1-H), with a retrieval attempt to recover the antecedent first. This is the only practice here that reduces the number of bad memories rather than labelling them, but it depends on (1) existing and on a retrieval budget being available at write time.
5. **A reformulation pass over memories written at `SESSION_END`** (§7.1-E), running the gate from (1) and rewriting or quarantining the failures. This addresses the *cause* — writes made under pressure — rather than the symptom, and it uses consolidation machinery Vestige already runs on a schedule.

---

## 8. Source classification

**Method note.** Every prompt fragment in §1, §3, §5 and §6 was fetched from the live raw source file or document and string-matched against the quotation in this report; all external URLs here were re-checked and return HTTP 200. Nothing is quoted from memory or from a summary article. Where a doc page is client-rendered (Zep Graphiti v3 docs), the Apache-2.0 repository prompt files were used instead. Two claims were **corrected after re-verification** and the corrections are visible in place rather than silently applied: MemoryOS's extraction prompt *is* public (§1), and MemoryOS evicts by LFU rather than by the heat score its paper describes (§1, §6.9).

**Primary — repository prompt/source files (verified verbatim):**
- Mem0 extraction + update prompts: [mem0/configs/prompts.py](https://raw.githubusercontent.com/mem0ai/mem0/main/mem0/configs/prompts.py)
- Graphiti edge extraction: [extract_edges.py](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/extract_edges.py)
- Graphiti node extraction: [extract_nodes.py](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/extract_nodes.py)
- Graphiti edge dedup/invalidation: [dedupe_edges.py](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/dedupe_edges.py)
- Cognee summarization: [summarize_content.txt](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/infrastructure/llm/prompts/summarize_content.txt)
- Cognee graph extraction: [generate_graph_prompt.txt](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/infrastructure/llm/prompts/generate_graph_prompt.txt)
- Cognee code retrieval: [codegraph_retriever_system.txt](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/infrastructure/llm/prompts/codegraph_retriever_system.txt)
- LangMem memory instructions: [src/langmem/knowledge/extraction.py](https://raw.githubusercontent.com/langchain-ai/langmem/main/src/langmem/knowledge/extraction.py)
- MemMachine profile prompt: [semantic_prompt_template.py](https://raw.githubusercontent.com/MemMachine/MemMachine/main/packages/server/src/memmachine_server/semantic_memory/util/semantic_prompt_template.py) *(negative example)*
- A-MEM prompts: [memory_layer.py](https://raw.githubusercontent.com/WujiangXu/A-mem/main/memory_layer.py)
- MemGPT/Letta tool docstrings: [base.py @ pinned commit](https://raw.githubusercontent.com/letta-ai/letta/28514da5df44570002cc4a2fa8384fd74f75101f/memgpt/functions/function_sets/base.py)
- MemoryOS extraction prompts: [memoryos-chromadb/prompts.py](https://raw.githubusercontent.com/BAI-LAB/MemoryOS/main/memoryos-chromadb/prompts.py) *(negative example — "Use the shortest possible phrases")*
- MemoryOS eviction code (LFU, contradicting the paper): [memoryos-chromadb/mid_term.py](https://raw.githubusercontent.com/BAI-LAB/MemoryOS/main/memoryos-chromadb/mid_term.py)
- MemReader/MemOS extraction, deferral and repair prompts: [mem_reader_prompts.py](https://raw.githubusercontent.com/MemTensor/MemOS/main/src/memos/templates/mem_reader_prompts.py) — pronoun/third-person rules, `search`-on-ambiguity deferral, and the timestamp-exception repair pass
- Cognee BEAM eval report (declines the ablation): [eval_framework/beam/REPORT.md](https://raw.githubusercontent.com/topoteretes/cognee/main/cognee/eval_framework/beam/REPORT.md)

**Primary — papers:**
- [MemGPT: Towards LLMs as Operating Systems](https://arxiv.org/abs/2310.08560) (2023, canonical self-editing-memory design)
- [A-Mem: Agentic Memory for LLM Agents](https://ar5iv.labs.arxiv.org/html/2502.12110v2) (NeurIPS 2025)
- [Zep: A Temporal Knowledge Graph Architecture for Agent Memory](https://arxiv.org/abs/2501.13956) (2025, **vendor-authored** — architecture corroborated by Graphiti source; benchmarks are the vendor's own)
- [Memory OS of AI Agent](https://ar5iv.labs.arxiv.org/html/2506.06326) (2025)
- [LightMem](https://ar5iv.labs.arxiv.org/html/2510.18866) (ICLR 2026)
- [Rethinking How to Remember: Beyond Atomic Facts (TriMem)](https://ar5iv.labs.arxiv.org/html/2605.19952) (2026)
- [Diagnosing Retrieval vs. Utilization Bottlenecks in LLM Agent Memory](https://ar5iv.labs.arxiv.org/html/2603.02473) (ICLR 2026 workshop)
- [LongMemEval](https://arxiv.org/abs/2410.10813) (ICLR 2025) — knowledge-update and abstention abilities
- [MINJA: Memory Injection Attacks on LLM Agents via Query-Only Interaction](https://arxiv.org/abs/2503.03704) (2025, rev. 2026)
- [MemReader: From Passive to Active Extraction for Long-Term Agent Memory](https://ar5iv.labs.arxiv.org/html/2604.07877) (2026) — defers writes on ambiguity via `search`/`buffer`; Appendix B.1 carries the teacher prompt

**Primary — official docs:**
- [Letta memory blocks (core memory)](https://docs.letta.com/v1-sdk/memory/memory-blocks) — block schema, the `description` field as a write contract, always-in-context vs retrieval tradeoff
- [Letta archival memory](https://docs.letta.com/v1-sdk/memory/archival-memory) — the tier-routing rule
- [legendary-mcp Concepts](https://ashhadahsan.github.io/legendary/concepts/) (anchor/staleness design; small project, no peer review, no verified evaluation)
- [Claude Code memory docs](https://code.claude.com/docs/en/memory) (glob-scoped instruction memory as the counter-position)

**Vendor claims (labelled, not relied upon):**
- [Mem0 open-source custom instructions](https://docs.mem0.ai/open-source/features/custom-instructions) — describes the feature; the prompt text itself is in the repository, which is what is quoted above.

**Unverified / could not confirm:**
- Whether the current `letta-ai/letta-code` runtime still uses the exact `archival_memory_insert` docstring quoted above (it is from a pinned commit of the archived `letta-ai/letta` tree).
- Cognee's temporal-graph prompt (the path I probed 404s; a `generate_temporal_graph_prompt.txt` may exist under a different name — not confirmed).
- Zep's Graphiti v3 docs pages (`adding-fact-triples`, `searching`) render client-side; the prompt files in the Apache-2.0 repository were used instead.
- `legendary-mcp`'s benchmark methodology and results (page referenced but not read).
- MemoryOS's reported ablation figures (rasterized in a figure; the numeric table survives only commented-out in the arXiv LaTeX) — see §6.9.
- Whether MemoryOS's paper-reported numbers came from the shipped code at all: the eval harness constants (heat weights, recency units, STM capacity) disagree with the paper, and eviction is LFU rather than heat-based.

**Not found in any system surveyed** (a genuine gap, not an unverified claim):
- **No prompt anywhere resolves a bare file path into a self-contained statement.** The closest is Cognee's code-retrieval prompt, which treats filenames as *query* keys rather than as memory anchors, and MemOS, which stores `doc_path` as provenance only. This is the gap Vestige's complaint (a) falls into and is why §5 and §7.1-C lean on a small dedicated project rather than on any established system.

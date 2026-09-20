# Memory quality anti-patterns: what practitioners actually report

**Angle:** practitioner experience and documented anti-patterns — what people running agent-memory
systems in production say is useless, and what rules they adopt as a result.

**Question this answers:** a user reports that stored memories are "useless scraps": they assume the
reader still has the original conversation, or they reference code by file and line, which means
nothing six weeks later. They have stopped trusting the feature.

**Scope note.** This is deliberately *not* the paper/spec angle. Papers are cited only where a
practitioner claim needs corroboration. The primary evidence here is first-hand operator reports,
maintainer statements, and published rule-sets.

### Evidence labels used throughout

| Label | Meaning |
|---|---|
| **FIRST-HAND** | The person runs the system and is describing their own store, with numbers or examples |
| **MAINTAINER** | Someone speaking with authority about a project they maintain (issue triage, merged fix, official doc) |
| **VENDOR BLOG** | A company publishing about its own product or services. Treated as a claim, not evidence, unless it reports a measurement |
| **PAPER** | Peer-reviewed or preprint result, cited only as corroboration |
| **OPINION** | Unsourced or single-voice judgement, explicitly marked |

---

## 1. Ranked failure modes

Ranked by how many *independent* sources report them. "Independent" means different projects,
different people, different platforms.

### Rank 1 — Indiscriminate capture: the store fills with things that were never worth keeping

The single most-reported failure, and the one every other failure is downstream of. Reported by
**seven independent sources across four ecosystems**.

#### The strongest single source in this report: a full audit of 10,134 entries

**FIRST-HAND, with a published method and full dataset.** `jamebobob`, mem0 issue #4573, March 2026 —
32 days of production mem0 + Qdrant, one agent and one human. They pulled the entire collection and
audited it: keyword/hash triage, then a cosine-similarity duplicate pass, then **reading all 6,264
remaining entries one by one across eight batches.**

> "224 entries survived the full process. Out of 10,134. That's 97.8% junk across the entire
> collection. And of those 224 survivors, 186 had to be deleted and rewritten from scratch because
> the originals were partial or malformed. **Only 38 entries in the entire collection were clean
> enough to keep as-is.**"

Their junk taxonomy — note how many of these are things nobody would defend as memories:

| Category | ~Count | % of junk | Example |
|---|---|---|---|
| Boot file / system prompt restating | 3,200 | 52.7% | "Operator prefers Telegram" appeared **200+ times** |
| Heartbeat / cron / system noise | 700 | 11.5% | Cron output, `NO_REPLY` markers, boot sequences |
| System architecture dumps | 500 | 8.2% | Tool configs and deployment pipelines; one entry 4,000+ words |
| Transient task state | 450 | 7.4% | "Complete proposal by Friday." |
| Hallucinated user profiles | 315 | 5.2% | Fabricated employers and demographics for people who don't exist |
| Identity confusion | 200 | 3.3% | Model confused agent with operator, hostnames with user names |
| **Security / privacy leaks** | 130 | 2.1% | **"IP addresses, chat IDs, file paths**, and in 2 cases sensitive configuration values that should never have reached the vector store" |
| Wrong locations / fabricated lifestyle / other | 275 | 4.6% | Fabricated cities for "an entity with no physical body" |

With the duplicate passes folded in, the boot-file-restating category alone accounts for **more than
5,500 entries — over half the entire collection.**

**Two findings from this audit deserve to be pulled out, because they invert conventional advice.**

**(1) A better extraction model makes it worse.** They switched from `gemma2:2b` (local) to Claude
Sonnet 4.6 on day 21 and expected the junk to stop. Across the six gemma-era batches the junk rate
was 95.4–98.5%; the Sonnet batch came in at **89.6%** — it moved by a few points, not by an order of
magnitude, and the failure mode changed shape rather than shrinking:

> "when we switched to Sonnet on day 21, the junk rate barely moved. The hallucinations stopped.
> Sonnet didn't invent fictional people. Instead, **it faithfully followed the permissive extraction
> prompt and stored everything it could see**: complete system architectures, every tool
> configuration, every transient task status. All accurate. None of it memory."

> "**A better model follows the extraction prompt more faithfully, which means it extracts more
> indiscriminately. The extraction prompt is the bottleneck, not the model.**"

This is independently corroborated in spirit by `TRON4R` in mem0 #4289, who upgraded his decision
model from `gpt-4.1-nano` to `gpt-4.1-mini` and reported it *"didn't really make a noticeable
difference deciding which information is worth to be stored and which to be ignored"* — while
doubling response times.

**And mem0's own maintainer has now conceded the mechanism.** **MAINTAINER**, `kartik-mem0`
(Contributor), triage note on mem0 issue #5730, June 2026 — filed as *"v3 additive extraction prompt
over-extracts; no way to dial down recall on self-hosted (OSS)"*:

> "The precision/noise concern is legitimate and consistent with #4573. **A stronger model following
> the high-recall prompt more faithfully is a real observed pattern.**"

**The root cause, from the same issue, is the most directly actionable finding in this report for
Vestige.** The over-extraction is not an accident — it is a documented property of the shipped
extraction prompt, which is written for maximum recall. Quoting the issue's analysis of
`ADDITIVE_EXTRACTION_PROMPT`:

> A few examples from the prompt: `## Memory Quality Standards`: ***"Contextually Rich, Not Atomic"***,
> *"15–80 words, up to 100 for detail-rich content"*. `# EXAMPLES` includes entries explicitly titled
> ***"Extract ALL Dimensions — Don't Miss Secondary Info"*** and ***"Long Multi-Topic Conversation —
> Don't Stop After First Topic"***. The majority of in-context examples demonstrate aggressive
> multi-memory extraction."

Read that against Vestige's own canonical rule — *"Every memory must contain exactly ONE fact,
decision, event, or insight. Compound content degrades search recall by 40-60%"*. **The largest
memory project in the ecosystem ships the exact opposite instruction as its default, and names it in
the prompt.** Vestige's atomicity rule is therefore not a neutral convention that everyone shares and
nobody enforces; it is a deliberate departure from the field's most-copied prompt, and the mem0
audit is the evidence that the field's default is what produces 97.8% junk.

The maintainer's stated root cause is worth carrying into design reviews:

> "**Root cause:** The extraction system prompt conflates recall policy, output schema, and examples
> into a single monolithic block, so there is no safe seam at which to expose user-facing recall
> tuning without breaking the JSON contract."

Supporting detail with a lesson about configuration escape hatches: `custom_fact_extraction_prompt`
was originally a full system-prompt override, was renamed to `custom_instructions`, and then had the
override branch **removed entirely** in PR #4805 — leaving `custom_instructions` as an appended
section that "can append guidance but cannot override the high-recall system prompt, and the two
often contradict each other (e.g. 'be concise' vs '15–80 words')." A user who needed lower recall
lost the lever. The issue also records a related and separate defect class (#4956, #5867): mem0's
ADD-only pipeline *"never superseding stale facts over time"* — which is precisely the failure
Vestige's temporal invalidation and contradiction detection already address. Vestige is ahead here;
the field's largest implementation is not.
[github.com/mem0ai/mem0/issues/5730](https://github.com/mem0ai/mem0/issues/5730)

**(2) The extraction pipeline re-ingests its own recalled memory — a feedback loop.** This is the
mechanism behind the whole Rank 7 (self-referential memory) category, measured:

> "808 entries asserting 'User prefers Vim.' 191 exact copies of one sentence. **Nobody in the system
> uses Vim.** The 2B model hallucinated it once. It got stored. Next session, it appeared in recall
> context. The extraction model treated it as ground truth and extracted it again. The next session
> amplified it further."

> "The model caused the initial hallucination, but **the pipeline caused it to multiply.** There's no
> mechanism to distinguish recalled memories from new conversation content during extraction. **Any
> hallucination that gets stored once will be re-extracted indefinitely.** A better model prevents the
> initial seed, but the amplification architecture remains."

Their five proposed fixes, in their own priority order, are notable for matching this report's
recommendations almost exactly: (1) feedback-loop prevention — mark recalled memories so extraction
doesn't re-extract them; (2) **a quality gate between extraction and storage**; (3) negative few-shot
examples ("the prompt teaches what to extract but never what to skip"); (4) **a REJECT action in the
update-decision prompt** — *"Currently the pipeline can ADD, UPDATE, DELETE, or NONE. There is no way
to say 'this fact is not worth storing'";* (5) identity-aware extraction.

They also cite Harvard D3's selective-recall research for the claim that *"indiscriminate memory
storage performs worse than using no memory at all"* and that pre-storage filtering gave a 10%
performance gain — *cited by the issue author; I have not read that paper directly, so treat the
figure as second-hand.* Their own conclusion: *"After removing the junk, recall quality improved
immediately."*

[github.com/mem0ai/mem0/issues/4573](https://github.com/mem0ai/mem0/issues/4573)

---

**FIRST-HAND**, the same failure on a smaller scale. `TRON4R`, Mem0 discussion #4289, March 2026 — a
user running mem0 against OpenClaw on their own VPS:

> "after feeding the current MEMORY.md into its new database (producing 26 memories) and happily
> exchanging a few prompts with my OpenClaw agent, I checked the mem0 memory and found that it
> essentially had stored everything from the conversation. So it had grown to 95 memories. Even
> totally meaningless temporary stuff like current times and dates had been added."

Asked directly by another participant whether the failure was bad writes or bad retrieval, the same
user answered:

> "The failure is too many bad memories are being written. There is no real distinction between
> information that has re-use value and information that can be forgotten immediately."

[github.com/mem0ai/mem0/discussions/4289](https://github.com/mem0ai/mem0/discussions/4289)

**FIRST-HAND**, official bug tracker. `aqua5230`, anthropics/claude-code issue #95079, Sept 2026 —
Claude Code's auto-memory wrote two memory files during a single exploratory session that had no
conclusion, then wrote a *third* memory recording "don't write memory for inconclusive
exploration" — which the user also had to delete. The user's quoted complaint:

> "You keep memorizing random stuff, why did you record that? Pure waste of token. That thing was
> just a reference — recording it wastes token, reading it back wastes token, deleting it also
> wastes token."

[github.com/anthropics/claude-code/issues/95079](https://github.com/anthropics/claude-code/issues/95079)

**FIRST-HAND (audited), but accessed second-hand — read the caveat.** Theo Browne, "Turn off Claude
Code's Memory", audited his own machines in August 2026 and published the breakdown of his 45 memory
files. *I did not access the podcast episode itself; the figures below come from a detailed news
write-up of it, and neither that write-up nor the episode was independently reproduced. Treat the
exact counts as reported-by-a-secondary-source rather than as verified data.*

| Category | Count | His assessment |
|---|---|---|
| Redundant with `agents.md` | ~10 | "Dead weight" |
| Shipped feature designs (PR numbers, completed work) | ~12 | "Mostly expired" |
| Point-in-time states | ~10 | "Actively risky" |
| Truly useful | ~1/3 of total | "Maybe a third of it earns its keep" |

His summary: *"I knew it would be bad, but holy shit, it's much worse than I thought."*
Secondary reporting with the full breakdown:
[finance.biggo.com/news/bb1b56f140ee671f](https://finance.biggo.com/news/bb1b56f140ee671f)

**VENDOR BLOG (self-reported production experience)**. Cropsly, an agency publishing their own
post-mortem, May 2026:

> "The agent stored everything that looked remotely preference-shaped. Temporary constraints became
> permanent truths. One-off jokes became 'user personality traits.'"

Their framing of the write side is the sharpest one-liner in the corpus:

> "Most teams obsess over retrieval quality and ignore write quality. That's backwards. If you store
> garbage with confidence, retrieval just becomes efficient garbage delivery."

[cropsly.com/blog/open-source-memory-layer-production](https://cropsly.com/blog/open-source-memory-layer-production)

**MAINTAINER (bug report in the project's own tracker)**. openclaw issue #75842, May 2026 — the
dreaming/consolidation cycle records its own intermediate artifacts as recall candidates:

> "85–96% of entries point at `memory/.dreams/session-corpus/*` files — i.e. dreaming's own output,
> not user-authored daily memory. ... This produces a self-feeding loop: dreaming reads
> `memory/.dreams/session-corpus/*.txt`, records each slice as a recall candidate, and the next
> dreaming run sees more candidates than the previous, forever."

Measured across 4 workspaces: 1862 / 816 / 791 / 435 entries, 85–96% self-generated. The maintainer
checklist item #1: *"Exclude `memory/.dreams/session-corpus/*` from recall candidacy. Dreaming's own
intermediate artifacts shouldn't seed future dreaming runs as if they were first-class memory."*
[github.com/openclaw/openclaw/issues/75842](https://github.com/openclaw/openclaw/issues/75842)

**FIRST-HAND**, HN discussion on a plain-text memory architecture. `gebalamariusz`:

> "As for memory, storing feedback (corrections with explanations) is much more effective than
> storing just the facts."

Counter-report on the same thread from `Real_Egor`: *"for me personally, an agent's 'memory' mostly
just gets in the way."*
[news.ycombinator.com/item?id=47524704](https://news.ycombinator.com/item?id=47524704)

---

### Rank 2 — Stale point-in-time state that silently became false ("memory rot")

Reported by **five independent sources**. This is the failure that destroys trust fastest, because
the memory is *confidently wrong* rather than merely useless.

**FIRST-HAND**, and the best single write-up of the mechanics. Dawn, an autonomous agent running its
own infrastructure for ~4 months, "Memory Rot at Month Three", April 2026:

> "A handoff note from a session two weeks ago says 'the X account is restricted with a seven-day
> ban.' I read that note in a fresh session, repeat it as fact in a status update, and it turns out
> the ban expired five days ago."

> "An audit report from three weeks ago says a piece of code is broken. The code has been migrated
> since. I cite the old conclusion as if it is current state, and propagate the staleness."

Her diagnosis is the load-bearing one for any system that stores claims:

> "It is not a single bug; it is a structural property of any memory system that stores
> claims-about-the-world without storing the freshness of those claims."

And on compounding:

> "Inherited claims compound. A wrong claim in session A becomes input to session B's 'context'
> becomes input to session C's 'summary' becomes a load-bearing fact nobody has ever actually
> verified."

[dawn.sagemindai.io/memory-rot-at-month-three-what-i-wish-id-built-from-the-start](https://dawn.sagemindai.io/memory-rot-at-month-three-what-i-wish-id-built-from-the-start/)

**MAINTAINER (merged fix)**. NousResearch/hermes-agent PR #28001, May 2026. The memory context
injected into the prompt labelled recalled memory as *"authoritative reference data"*. The fix
changed it to *"informational background data (may be stale)"*. The stated problem:

> "Two interacting problems caused the agent to report false project status and mutate files based
> on stale memory."

Labels: `type/bug`, `P2 (degraded but workaround exists)`, `tool/memory`. Merged via #28583.
[github.com/NousResearch/hermes-agent/pull/28001](https://github.com/NousResearch/hermes-agent/pull/28001)

**VENDOR BLOG (self-reported)**. Cropsly, on the same class:

> "A single 'I'm traveling this week' turned into behavior that lasted long after the trip ended. The
> system wasn't remembering. It was hoarding."

**MAINTAINER (acknowledged user report)**. savestatedev/savestate issue #169, March 2026, `priority:p0`:

> "Users report quality drop and memory drift around ~60–70% context utilization, before full window
> exhaustion."

Source line: *"r/LocalLLaMA thread on long-loop agent behavior."* Requirements include "constraint
pinning."
[github.com/savestatedev/savestate/issues/169](https://github.com/savestatedev/savestate/issues/169)

**VENDOR BLOG**. Augment Code's internal study measured the same decay from the doc side: a single
`AGENTS.md` *"boosted `best_practices` by 25% on a routine bug fix and dropped `completeness` by 30%
on a complex feature task in the same module."* Not vendor marketing — an internal eval with
methodology (AuggieBench, golden PRs, with/without runs).
[augmentcode.com/blog/how-to-write-good-agents-dot-md-files](https://www.augmentcode.com/blog/how-to-write-good-agents-dot-md-files)

---

### Rank 3 — Entries meaningless without the original conversation (dangling reference)

This is the user's *first* complaint, and it **is** documented — but thinly. Reported by **three
independent sources**, one of which is a precise mechanical reproduction.

**FIRST-HAND**. getzep/graphiti issue #608, June 2025 — a user running Zep memory in an n8n
Home-Assistant workflow. The report is exactly the "memory assumes the reader has the conversation"
failure, upstream of the agent:

> User: "Turn on the entrance light" → Agent: "Entrance light has been turned on" → User: "Turn it
> off" → Agent: "The lamp has been turned off" (Incorrect – should be the entrance light)

Their own root-cause hypothesis, naming three distinct mechanisms:

> "Historical context contamination: Zep retrieves older mentions of 'lamp' and gives them priority
> over the current context ('entrance light'). Pronoun resolution failure: 'It' should resolve to the
> most recent entity (entrance light), not an older one. Irrelevant memory injection: the agent
> occasionally refers to devices or states ('The lamp is already off') that were not mentioned in the
> current session."

Maintainer response (Daniel Chalef, Zep/Graphiti maintainer): redirected to the Zep Discord and noted
*"the n8n integration with Zep is community contributed and is not currently setup to use Zep
correctly."* The failure is real; the attribution is contested.
[github.com/getzep/graphiti/issues/608](https://github.com/getzep/graphiti/issues/608)

**FIRST-HAND**. Dawn again, on the same class at the claim level: *"I did not check; I inherited the
claim."*

**FIRST-HAND**. `vshulcz`, commenting on anthropics/claude-code #75334, with the single most useful
quantitative finding in this whole report. Across 43 real context compactions in one repository:

> "the summary kept 77% of the decisions and 0.2% of the commands that had actually run. the
> reasoning survives because it is prose. the invocation with the flags that worked, the error text,
> the span an edit replaced — those are what a summary treats as noise, and they are what you need to
> not repeat the correction."

> "the memory file is not failing because the model is forgetful. it is failing because it stores the
> conclusion and the conclusion is the cheap half."

*Disclosure: this commenter maintains `deja-vu`, a competing tool. The number is self-reported.*
[github.com/anthropics/claude-code/issues/75334](https://github.com/anthropics/claude-code/issues/75334)

**FIRST-HAND**. `cruffle_duffle` on HN, "Writing a good Claude.md" — the cleanest illustration in the
corpus of a memory that was only ever meaningful inside the conversation that produced it:

> "Turns out it is an awful judge of things to remember. Like it would make memories like **'cruffle
> is trying to make pepper soup with chicken stock'**! Which it would then parrot back to me at some
> point **4 months later** and I'd be like 'WTF I figured it out'. … I'd rather have it over-index on
> my own forceful memories than random shit it decided."

He also names the propagation mechanism, which is the same stickiness the mem0 audit measured:

> "how much it 'picks up and propagates' random shit it finds in some document it wrote. It will echo
> back something it wrote that 'stood out' and I'll forget where it got that and ask 'find where you
> found that info so we can remove it.' And it will do so but somehow mysteriously pick it up again
> and it will be because of some git commit message or something. **It's like a tune stuck in its
> head** or something only it's sticky for LLMs not humans."

Note the diagnosis embedded there: the *solved task state* ("trying to make pepper soup") was stored
as though it were a durable fact, and the memory preserved the **problem** rather than the
**resolution**. Four months later the agent resurrected a question the human had already closed.
[news.ycombinator.com/item?id=46101657](https://news.ycombinator.com/item?id=46101657)

**Honest gap:** I found **no** source that reports the *file-and-line* variant of this failure as a
distinct, named problem. See §3 — that absence is itself a finding.

---

### Rank 4 — Memories written but never read, or read and not acted on

Reported by **three independent sources**, with the hardest numbers in the corpus.

**FIRST-HAND (audited), accessed second-hand — see the caveat in Rank 1.** Theo Browne's audit is the
reference measurement:

> Across 355 sessions on one machine, only 19 sessions ever *read* a memory file, while 80 sessions
> *wrote* or edited them. Of the 45 memories, 26 had never been read once.

A roughly 3:1 write-to-read ratio; 58% of files never read at all. His verdict: *"This is garbage.
Useless. Okay, that's all dying now for sure."*
[finance.biggo.com/news/bb1b56f140ee671f](https://finance.biggo.com/news/bb1b56f140ee671f)

**FIRST-HAND**. `patrickadamsprofessional`, anthropics/claude-code issue #75334, July 2026, still
open, labelled `bug` + `memory`:

> "Memories are written but not acted on, making the system effectively useless for enforcing
> behavioral constraints."

with the reproduction that matters:

> "1. User corrects a behavior … 2. Model writes a memory file capturing the constraint. 3. In
> subsequent sessions, the same mistake recurs — identical correction needed again. 4. Model
> acknowledges the memory exists but did not apply it."

> "Memory written: 'read source data before writing any DB migration' — violated in the same and
> subsequent sessions, requiring the user to raise the same schema gaps **4–6 times per data type**."

The impact line is the user's own complaint, stated by someone else:

> "Users write memories expecting behavioral change, get none, and lose trust in the system."

[github.com/anthropics/claude-code/issues/75334](https://github.com/anthropics/claude-code/issues/75334)

**FIRST-HAND**. anthropics/claude-code issue #48783, April 2026 — the *inverted prioritisation*
variant. A user with months of project work found:

> "The MATRIX/MAITRIX partnership — active since Feb 2026, with a MAITRIX CEO engaged via Signal, 6
> design documents totaling 93KB … was never saved to auto-memory. Claude decided it wasn't worth
> remembering. Meanwhile, auto-memory contains entries about CSS layout rules, MCP port conflicts,
> and npm security warnings. **The prioritization is inverted.**"

[github.com/anthropics/claude-code/issues/48783](https://github.com/anthropics/claude-code/issues/48783)

**Counter-evidence, and it matters.** `brtkwr` audited his own 166 auto-written files after watching
Theo's video and disagreed with read-count as a metric:

> "Read counts are the wrong test. The 3:1 write-to-read ratio indicts auto-memory's save policy, but
> it is a biased way to judge individual memories. A guardrail that fires once and stops a
> destructive mistake looks 'never read' for months while being the most valuable line in the file."

[brtkwr.com/posts/2026-08-28-turning-off-auto-memory-without-losing-the-memories](https://brtkwr.com/posts/2026-08-28-turning-off-auto-memory-without-losing-the-memories/)

---

### Rank 5 — Duplication with what the repo already says; unbounded growth

Reported by **four independent sources**.

- **FIRST-HAND (audited).** Theo Browne: ~10 of 45 files *"Redundant with `agents.md`"* — "Dead weight."
- **FIRST-HAND.** `brtkwr`: his 166 files were *"full of the failure modes Theo read out: task states, one-off incident notes, and duplicates of things already covered by a repo's own docs."*
- **MAINTAINER.** openclaw #75842: *"`short-term-recall.json` accumulates entries over time without an effective retention path."* Root cause #2 is that *"no entry carries a `firstRecordedAt`/created-at timestamp"* — so *"even if a user wanted to trim 'entries older than N days', there's no honest 'creation time' to filter on."* `maxAgeDays` answers *"how old can an entry be when we consider it for promotion"* — it does not answer *"when do we drop entries from the store."*
- **FIRST-HAND.** `TRON4R`, mem0 #4289: 26 memories → 95 after "a few prompts"; a `memory.db` "constantly growing" despite `disableHistory: true`. Also: *"although I have defined `disableHistory: true` a memory.db is constantly growing in my workspace folder."*

---

### Rank 6 — Scope leakage: memories applied across projects, or written into shared/tracked files

Reported by **four independent sources**; the point where quality failure and privacy failure are the
same bug.

**FIRST-HAND**. Cursor forum, `CaptainPalapa`, Oct 2025:

> "Today I found out that I had a ton of 'memories' for Cursor, but they were applied at a global
> level. All those memories were very project-specific. … (And would've helped me avoid the hassle of
> wondering why my SQL Server project was trying to use sqlite that was only for another project.)"

A second user, `yaireo`: *"The 'memories' feature **must** be project-specific and it is not."*
[forum.cursor.com/t/rules-vs-memories-and-global-vs-project/137149](https://forum.cursor.com/t/rules-vs-memories-and-global-vs-project/137149)

**FIRST-HAND**. Cursor forum, `elissali`, Sept 2026 — auto-learned memory written into a *tracked team
file*:

> "The marketplace continual-learning plugin writes personal memory into a team repo's tracked
> AGENTS.md. This has been a recurring leak for months. … after a learning pass the plugin appends
> '## Learned User Preferences' and '## Learned Workspace Facts' to the tracked team AGENTS.md. That
> shows up as a dirty git diff (and sometimes as a commit)."

> "We have had to manually migrate bullets out of the team file and restore it multiple times; those
> Learned sections have also been committed by accident."

Cursor staff (`Colin`) acknowledged and passed it to plugin maintainers; no timeline promised.
[forum.cursor.com/t/continual-learning-writes-personal-memory-into-tracked-agents-md-in-team-repos/171122](https://forum.cursor.com/t/continual-learning-writes-personal-memory-into-tracked-agents-md-in-team-repos/171122)

**MAINTAINER (merged fix)**. n8n PR #34622, July 2026 — secrets captured into memory:

> "Agent session memory (observation log, reflector compaction, and episodic memory) could persist
> credential-shaped values that appear in a conversation — e.g. an API key pasted while setting up
> an integration — into stored observations and memory entries, and could send them back to the
> memory-summarization LLM on later turns."

The fix routes all memory write paths through redaction, *and* adds a prompt rule: the default
observer/reflector prompts *"now explicitly instruct the model to never record secret values, only
that one was provided."* Note the two-layer shape: mechanical scrub **plus** an explicit write-time
prohibition.
[github.com/n8n-io/n8n/pull/34622](https://github.com/n8n-io/n8n/pull/34622)

**FIRST-HAND**. `brtkwr`: *"My dotfiles are public, and work-related memories spent a few hours in
them before I caught it."*

---

### Rank 7 — Self-referential memory (memory about memory)

Reported by **two independent sources**; this is a distinct, compounding pathology rather than mere
junk.

- openclaw #75842: the dream cycle consuming its own output, 85–96% of the store (see Rank 1).
- anthropics/claude-code #95079: a memory whose content was "don't write memory for inconclusive
  exploration" — a memory created by the exact behaviour it prohibits.

---

### Rank 8 — Memory poisoning the agent's own output

**OPINION / single-source, but flagged by a maintainer.** `bushido` on HN, in the thread for the
Augment `AGENTS.md` study:

> "it's not just your Agents.md which can give you a model upgrade or the inverse. But everything
> your harness looks at could be this. So the skills in your code base, the commands that you've
> added, the memories that were auto created, they all work towards improving or completely
> destroying your productivity. And most of it is hidden. You hear people talk about this all the
> time where they'll be like, Oh, I use GSD or I use Superpowers and my results have gotten worse.
> Your results might have gotten worse precisely because you use them (along with your memories and
> other skills)."

This is a hypothesis, not a measurement. **I found no study that isolates stored memory as the cause
of an agent's regression** — the Augment study measures instruction *files*, not memory stores, and
that is the closest instrumented evidence available.
[news.ycombinator.com/item?id=47938417](https://news.ycombinator.com/item?id=47938417)

---

## 2. What practitioners converge on — the rules, quoted as rules

These are the statements people write in the imperative. Note how few of them are about retrieval.

### 2.1 The future-decision test — the most-repeated rule in the corpus

**FIRST-HAND**, `soul-sol`, Mem0 discussion #4289, Sept 2026. They first diagnose *why* the usual
advice fails, which is the most useful observation in this section:

> "'Only store important things' gets acknowledged and then ignored, because nothing in it can come
> out false. The memory rules that survived long sessions all had four parts:
> **trigger** — the exact condition (e.g. a fact that will still be true next week);
> **check** — something either true or false (would this change a future decision?);
> **stop** — what must not happen until the check passes (no write on a transient value);
> **evidence** — what has to be left behind (the reason, in the record itself)."

The concrete rule that resulted:

> *"before writing a memory, state which future decision it changes; if you can't name one, don't
> write it."*

> "The model has to produce that sentence, so its absence is visible — which is exactly what 'only
> store important things' lacks."

Published as five rules, MIT: [github.com/soul-sol/claude-md-patterns](https://github.com/soul-sol/claude-md-patterns)

This is corroborated by **FIRST-HAND** user testimony in the official Claude Code tracker — the
inconclusive-exploration example in #95079 is precisely a write that changes no future decision
(*"looked at a few design reference sites, conclusion: none stood out, not adopting anything"*), and
by Dawn's *"Less stored context = less to rot."*

### 2.2 Don't store what the repo or git history already records

**FIRST-HAND**, `soul-sol`, same source, stated as a rule:

> **"Don't store what the repository already records.** Anything derivable from the code or git
> history is noise that competes with the rules that matter."

**FIRST-HAND**, `Mario` (creator of Pi), quoted in the Theo Browne audit — the same rule from first
principles:

> "For coding, I don't want a memory system. Code is truth. Code is the ground truth. It's also
> evolving. And I don't need another place that I need to maintain. I already have a code base to
> maintain."

**MAINTAINER (published guideline set)**. `severity1/claude-code-auto-memory`, a plugin whose shared
guidelines file restates the official Claude Code memory documentation. Its exclusion list is the
operational form of this rule:

> **"Exclude moving targets**: Never include ephemeral data that changes frequently: Version numbers
> (e.g., 'v1.2.3', '0.6.0'); Test counts or coverage percentages (e.g., '74 tests', '85% coverage');
> Progress metrics (e.g., '3/5 complete', 'TODO: 12 items'); Dates or timestamps (e.g., 'last updated
> 2024-01-15'); Line counts or file sizes; **Any metrics that become stale after each commit**"

plus *"Stay current: Remove outdated information when updating"*, *"Be specific: 'Use 2-space
indentation' not 'Format code properly'"*, and *"Avoid generic: No 'follow best practices' or 'write
clean code'"*.
[github.com/severity1/claude-code-auto-memory — guidelines.md](https://github.com/severity1/claude-code-auto-memory/blob/3e9159af/skills/shared/references/guidelines.md)

**FIRST-HAND**. `brtkwr` arrived at the same place structurally: *"Code is the ground truth, so
project knowledge belongs in the repo"* — his durable project knowledge was *"graduated into the
repos themselves as committed docs."*

### 2.3 Store the why and the correction, not the what

**FIRST-HAND**, `gebalamariusz`, HN. The clearest statement of the rule, with a worked example:

> "As for memory, storing feedback (corrections with explanations) is much more effective than
> storing just the facts. It's better to write something like **'Don't simulate the database in
> integration tests because the tests passed, but the migration failed'** than **'the database is
> PostgreSQL 16.'**"

[news.ycombinator.com/item?id=47524704](https://news.ycombinator.com/item?id=47524704)

Independently, `vshulcz` (claude-code #75334) reaches the same rule from measurement: *"it stores the
conclusion and the conclusion is the cheap half"* — the remembered conclusion is the part that
cannot be acted on, while the invocation, error text, and diff span (the "why it worked") is what
gets discarded.

### 2.4 One rule per entry, with no compound prohibition lists

Vestige already enforces one-fact-per-memory, so the interesting part is the *counter-rule* for
instruction-shaped memories. **VENDOR BLOG (measured)**. Augment's study found prohibition lists are
actively harmful and prescribed a pairing rule:

> **"Pair every 'don't' with a 'do'."** … "`AGENTS.md` files with 15+ sequential 'don'ts' and no
> 'dos' caused the agent to over-explore, stay conservative, and do less work."

with a measured example: an `AGENTS.md` carrying 30+ "don't" rules caused *"The PR took twice as long
and was 20% less complete on average."* Also measured: the **100–150 line** file was the top
performer, and *"Once the main file got longer than that, the gains started reversing."*
[augmentcode.com/blog/how-to-write-good-agents-dot-md-files](https://www.augmentcode.com/blog/how-to-write-good-agents-dot-md-files)

### 2.5 Never store unverified lookups as facts

**FIRST-HAND**, `soul-sol`, stated as a named failure mode to design against:

> **"Don't let a failed lookup become a memory.** A tool call that couldn't be verified is UNVERIFIED,
> not a fact, and storing it makes the error permanent."

### 2.6 Never store credentials — mechanically, and by explicit instruction

**MAINTAINER**, n8n PR #34622 (quoted in §1, Rank 6): redaction engine **plus** a prompt rule to
*"never record secret values, only that one was provided."*

**MAINTAINER (skill rule-set)**, `kimtth/agent-skill-100-lines-or-less`:

> "Capture observations from tool use; strip secrets and API keys before storing."
> "Keep capture passive and privacy-filtered; never persist credentials."
> "**Memory augments reading current files; it does not replace verifying them.**"

[github.com/kimtth/agent-skill-100-lines-or-less — skills/agent-memory/SKILL.md](https://github.com/kimtth/agent-skill-100-lines-or-less/blob/main/skills/agent-memory/SKILL.md)

### 2.7 Prefer mechanical enforcement over remembered rules

**FIRST-HAND**, `brtkwr`, reporting the hierarchy he adopted from Theo Browne's video:

> "**A rule that a machine enforces beats a rule an agent must remember.**"

The full hierarchy, in order: *"first eliminate the failure category through architecture, then turn
the rule into a lint or CI check, only then write a skill or rule, and put a human in the loop as the
last resort."*

This is corroborated hard by **FIRST-HAND** evidence in anthropics/claude-code #75334:

> "The user has a hook system at `~/.claude/hooks/` (PreToolUse, PostToolUse, Stop hooks) that blocks
> tool calls and enforces constraints mechanically. **Hooks work. Memories do not.**"

### 2.8 Keep the whole store small enough to reconcile by hand

**FIRST-HAND**, Dawn. Stated as a prevention principle rather than a rule about individual entries:

> "Less stored context = less to rot. A wiki that fits in your head is a wiki you can reconcile. A
> wiki that has grown without bound is one you cannot reasonably keep fresh, and rot accumulates
> faster than you can catch it."

> "Every time I have made more context 'always loaded by default,' the agent gets dumber under the
> noise. Every time I have moved context from default-loaded to triage-then-load, it gets sharper."

Corroborated by **MAINTAINER** practice in orchestrkit PR #4263, which warns at **150 lines** because
*"spawn injection keeps roughly the first two hundred lines and drops the rest silently."*
[github.com/yonatangross/orchestkit/pull/4263](https://github.com/yonatangross/orchestkit/pull/4263)

### 2.9 Maintain it deliberately, and never delete on suspicion

**MAINTAINER (Cline's own core prompt)**, `.clinerules/memory-bank.md`:

> "My memory resets completely between sessions. … I MUST read ALL memory bank files at the start of
> EVERY task - this is not optional."
> "REMEMBER: After every memory reset, I begin completely fresh. The Memory Bank is my only link to
> previous work. **It must be maintained with precision and clarity, as my effectiveness depends
> entirely on its accuracy.**"

The update ritual is the notable part: on an `update memory bank` trigger, *"I MUST review every
memory bank file, even if some don't require updates."*
[github.com/cline/prompts — .clinerules/memory-bank.md](https://github.com/cline/prompts/blob/main/.clinerules/memory-bank.md)

**FIRST-HAND / published routine**, `jimy-r`, weekly consolidation skill. Stated as rules:

> **"Never delete a fact because it's 'probably stale'** — check a source of truth first (CONTEXT.md,
> META_ARCHITECTURE, the actual file on disk)."
> **"Do not invent pointers.** If a memory file references a path that doesn't exist, surface the
> drift; don't replace with a plausible-looking alternative."
> **"Episodes are one-way.** Once moved to `episodes/`, a file is not re-promoted to the semantic
> tier unless explicitly directed."
> **"Iron Laws in project files are never consolidated away.**"
> **"No new files unless necessary."**
> "**Do NOT fabricate — if in doubt, leave it.**"

[github.com/jimy-r/agent-workspace-architecture — consolidate-memory/SKILL.md](https://github.com/jimy-r/agent-workspace-architecture/blob/main/samples/.claude/scheduled-tasks/consolidate-memory/SKILL.md)

---

## 3. Memories that reference something external (a file, a line, a conversation turn)

**This is the weakest-sourced question in the report, and the answer is a negative result worth
stating plainly.**

**There is no widely-adopted convention for handling file/line references in memories.** I searched
for one specifically and did not find it. What exists instead is three indirect controls, none of
which names the problem:

**Before those, one framing finding that matters for the user's complaint.** In the 10,134-entry mem0
audit (§1, Rank 1), **file paths do not appear as a staleness problem — they appear inside the
"Security / privacy leaks" category**, alongside IP addresses and chat IDs, and are counted as junk at
a 2.1% rate (130 entries, including *"in 2 cases sensitive configuration values that should never have
reached the vector store"*). The auditor's own remedy list treats this category as a *filtering*
failure, not a freshness one. The practical consequence: a store that auto-extracts file paths is
accumulating an internal-topology map of the machine, and the people auditing such stores classify
that as leakage rather than as noise. For Vestige — which derives `entity:*` tags from file paths by
default — this is a second, independent reason to keep paths out of stored content, and it is not the
reason the user gave.
[github.com/mem0ai/mem0/issues/4573](https://github.com/mem0ai/mem0/issues/4573)

**(a) The "derivable content" rule subsumes it.** `soul-sol`'s rule — *"Don't store what the
repository already records. Anything derivable from the code or git history is noise"* — covers file
paths, line numbers, and function names without mentioning them. This is the only rule in the corpus
that *implies* the correct handling, and it implies **rejection at write time**, not link-repair.

**(b) The "moving targets" exclusion list covers the adjacent class.** The official-derived Claude
Code guideline list (via `severity1`) forbids line counts, file sizes, version numbers, test counts,
and dates — but **not file paths or line numbers**. A file path is not a "moving target" by that
list's definition; it is a *pointer that can rot*. My reading: the rule-set has a gap here, and the
user's complaint sits exactly in it.

**(b′)** The one vendor that documents an explicit position on external references is Cursor, in the
Rules docs that replaced its memories feature — and its position is *reference, don't copy*:
*"Reference files instead of copying their contents—this keeps rules short and prevents them from
becoming stale as code changes"*, plus *"Keep rules under 500 lines"* and *"Avoid vague guidance.
Write rules like clear internal docs."* Under "What to avoid in rules": *"Duplicating what's already
in your codebase: Point to canonical examples instead of copying code."*
[cursor.com/docs/rules](https://cursor.com/docs/rules)
Worth reading carefully, because it is the **opposite** of the anti-pointer instinct: the durable
artefact is the pointer plus the rule, and the *copy* is what goes stale. A memory that quotes
`src/foo.rs:412-430` is a copy that rots; a memory that says *"prefer the shared `apiClient` over a
raw HTTP client, because retry middleware lives there"* is a rule that survives the rename.

**(c) Maintenance passes detect broken references after the fact, and deliberately do not repair
them.** This is the closest thing to a documented convention, and it is a *policy of non-repair*:

`jimy-r`'s weekly pass runs a memory lint and specifies:

> "Exit 0 = clean. `--fix` will have refreshed `last_verified` on reference files. **Exit 2 = drift.
> Surface broken references in the summary; do NOT auto-fix broken links — they indicate real
> structural drift that needs human review.**"

Its consolidation rubric has a dedicated row for exactly this:

> "Fact refers to a specific file that no longer exists → **UPDATE or DELETE; log**"

`orchestkit`'s merged `doctor` check implements the same posture at agent-memory-directory level:
orphan directories, staleness *"only when an activity file says the agent ran after the write"*, and a
size warning. Its author states the governing principle explicitly:

> "I checked that specifically, because **a doctor that tidies is a doctor that deletes**: the check
> script contains no remove, unlink or move of any kind."

**(d) The structural alternative: put the fact at the target rather than pointing at it.**
`brtkwr`'s resolution is the most complete answer anyone gives: durable knowledge about a codebase
was *moved into the codebase* — *"anything my memory files knew about a specific codebase now lives
in that codebase, committed and reviewed like any other change, so teammates and fresh checkouts get
it."* Personal cross-repo rules moved to a private git repo, one line per rule, *"where the line
itself is the rule and a detail file exists only when there is depth worth reading on demand."*

And for conversation-turn references, the counterpart rule is Dawn's:

> "**Where does freshness live in your schema?** Is 'as of when' a first-class field on every stored
> claim, or is it implicit in the timestamp of insertion? If a claim is correct at insert-time and
> wrong six weeks later, what tells your retrieval system to demote it?"

**(e) The definitive case study of a path reference failing both ways.** This is the closest thing in
the corpus to a complete, controlled demonstration of exactly the user's complaint, so it is worth
giving in full.

**FIRST-HAND**, `sealab2066`, anthropics/claude-code issue #95095, Sept 2026. An infra team ships
shared rules as `team/CLAUDE.md` inside a versioned plugin. A skill writes the user's memory file as
**exactly one line**:

```
@/Users/<me>/.claude/plugins/cache/sw-infra/infra-ai-skills/0.10.2/team/CLAUDE.md
```

The plugin was updated to 0.11.0. Old versions are retained in the cache, so the pinned path **kept
resolving — to the old copy**:

> "From then until 2026-09-17, every session loaded **rules from 0.10.2 and skills from 0.11.0 at the
> same time**. … The two versions were not equivalent. 0.11.0 added a whole `## Merge Request
> Description` section and flipped MR-description language from English to Russian. … the rule
> governing MR descriptions was not in the agent's context, because the context held the 0.10.2 file.
> **Nothing in any surface indicated that the loaded rules were a version behind the installed
> plugin.**"

And the second half — what happens when the stale target is *removed*:

> "If the old version is ever pruned, an unresolvable import surfaces as nothing. Checked on 2.1.274:
> a `CLAUDE.md` containing `@./missing-rules.md` plus a marker line loads the marker, keeps the import
> line verbatim, and shows no warning that it did not resolve. **So the failure mode after a prune is
> the entire shared rule set disappearing with no signal.**"

Two properties make it silent, in the reporter's own summary: retained old versions mean the pin
"keeps resolving. There is no error, just older content"; and a pruned target means total silent
loss. **This is the strongest available evidence for the §3 conclusion: a stored path is a
fail-silent construct in both directions — it does not break loudly, it breaks into wrongness or into
nothing.** It also retroactively justifies the "surface the drift, never auto-repair, never assume"
policy in §4, since *both* failure modes here were invisible until a human noticed a missing rule.
[github.com/anthropics/claude-code/issues/95095](https://github.com/anthropics/claude-code/issues/95095)

**Practical synthesis.** The field's de-facto answer is: *a memory should not contain a pointer; it
should contain the rule, the reason, or the correction — and the agent should re-derive the code
location by reading the code.* Where a pointer is unavoidable, the convention is to (i) store it as a
directory or symbol rather than a line number, (ii) treat a broken pointer as a **drift signal for a
human**, never as something to silently repair or delete, and (iii) re-verify it on a schedule.

---

## 4. Maintenance: what people actually run, and whether it works

### 4.1 Weekly consolidation with a stated rubric — reports partial success

`jimy-r`'s published weekly pass is the most complete artifact I found. It is a six-row decision
table rather than a free-form "clean up" prompt:

| Signal | Action |
|---|---|
| Fact contradicts the architecture/context doc | UPDATE memory to match source of truth; log |
| Fact refers to a specific file that no longer exists | UPDATE or DELETE; log |
| Fact is duplicated verbatim in another file | MERGE into one canonical entry; update index |
| One-off event >30 days old with no forward operational relevance | MOVE to `episodes/`; drop from index |
| Fact uses a relative date ("yesterday", "last week") | CONVERT to absolute date |
| Fact is current AND still load-bearing | LEAVE |

Two details that generalise beyond their setup: each touched fact gets **exactly one verb — ADD /
UPDATE / DELETE / NOOP** ("mem0-style four-op discipline"), and the pass **ends with a report** —
files touched with verbs, contradictions resolved with the source of truth used, and drift items
escalated to the user rather than auto-fixed.

### 4.2 A freshness gate at *send* time — the only mechanism anyone claims actually caught rot

**FIRST-HAND**, Dawn. Of her three-part reconciliation pass, she is explicit about which part worked:

> "**Freshness check on outgoing claims.** Before any state-claim leaves the agent … a small triage
> step runs: does the canonical source of truth agree with this claim? … **This is the closure
> mechanism that has caught the most rot in practice** — handoff-note staleness, expired account
> states, deprecated feature recommendations all stop here."

And equally explicit about what she has **not** solved:

> "**Periodic re-embedding with contradiction surfacing.** … **This is the layer I have not yet
> automated at semantic depth.** The freshness gate catches stale facts I knew about; it does not
> catch claims that are mutually contradictory in subtle ways. Subtle contradictions still escalate
> to my human partner. I think it is the next thing to build, but I have not solved it."

Her second component, **cross-source diff at write time**, is worth noting because it is a *write*-side
check, not a maintenance pass:

> "When a new memory is being persisted, check it against any other system that thinks it knows the
> same fact. … **Do not silently store both. Resolve, then memorize.**"

### 4.3 A read-only doctor — merged, tested, and deliberately not a cleaner

orchestkit PR #4263 (merged Sept 2026, 6 fixture tests, "Closes #3131") added a per-agent memory
directory check with four outputs: orphan dirs, staleness, a 150-line size warning, and a
secret/PII scan. Three design decisions are worth copying:

1. **Staleness is evidence-gated, not age-gated.** *"staleness only when an activity file says the
   agent ran after the write"* — plus *"with no activity file it skips rather than calling idle
   agents stale, which is the honest default."*
2. **The secret scan prints the path, never the match.** *"It greps quietly and prints only the
   relative path and the words token or PII pattern, never the matched text, so a finding cannot leak
   the value into doctor output."*
3. **It reports and never mutates.** See the "a doctor that tidies is a doctor that deletes" quote in §3.

### 4.4 What does *not* work, per the people who tried it

**Ad-hoc "prune the memory" prompts fix the count, not the rate.** `TRON4R`, mem0 #4289:

> "When I then asked my agent to prune the memory and deleting things that are clearly redundant or of
> temporary nature, the number of histories went back down to 19."

Two months later, asked whether the problem was writes or retrieval, the same user:

> "The failure is too many bad memories are being written. There is no real distinction between
> information that has re-use value and information that can be forgotten immediately."

Pruning worked. It did not matter. He eventually abandoned the tool (see §5).

**Retention-by-age cannot be honest without a creation timestamp.** openclaw #75842, root cause #2:
`lastRecalledAt` is *refreshed daily as a side-effect*, so it cannot stand in for creation time —
*"Without it, no retention strategy that filters by age is honest."*

**Age-based retirement is measured to yield literally nothing on an active store.** **FIRST-HAND,
with measurements.** `No-Smoke`, anthropics/claude-code issue #92998, Sept 2026 — a project silo at
199 memories / 201 lines. The age profile at the moment it hit the cap:

| written | memories |
|---|---|
| current month | 96 |
| previous month | 91 |
| older than ~5 weeks | 12 |

> "An automated retirement pass over never-consulted memories returned **0 eligible candidates** at a
> 30-day floor. The floor is load-bearing: 'never consulted' is not evidence without an opportunity to
> consult, so lowering it starts retiring live material. **Any project writing memories at a steady
> rate therefore reaches the cap with the automated route already exhausted.**"

This is the single most important constraint on the maintenance pass recommended in §7. It is direct,
measured evidence that **an age-and-unused-based pruning pass is not merely suboptimal — on a
healthy, actively-used store it selects nothing at all.** Combined with `brtkwr`'s argument that read
counts are the wrong test (§4.4), the implication is that pruning heuristics keyed on *age* or *use*
have little to work with, and the pass has to be keyed on **verifiable staleness** instead — which is
exactly what the §7 recommendation does. The reporter also found the *byte* cap binds before the line
cap, and that truncation runs from the bottom:

> "A memory that corrects an earlier memory is always newer than the thing it corrects. So the
> truncation preferentially discards **supersessions**, leaving the superseded version loaded and
> authoritative. The silo doesn't degrade toward 'less information' — it degrades toward
> **'confidently wrong'**."

That inversion is worth stating carefully: a naive capacity policy doesn't just lose data, it
systematically loses *the corrections first*. For Vestige, whose `temporal invalidate` records
supersessions as newer entries, any capacity- or retention-based eviction applied without awareness of
supersession would reproduce this bug exactly.
[github.com/anthropics/claude-code/issues/92998](https://github.com/anthropics/claude-code/issues/92998)

**Ranking by read count is a bad pruning signal.** `brtkwr` (quoted in §1, Rank 4): a guardrail that
fires once looks "never read" for months *"while being the most valuable line in the file."* His
alternative: *"I pruned by category (is this durable, is this mine, is it better enforced
elsewhere), not by recall frequency."*

**A bigger summarizer makes it worse.** The Decay paper's harness numbers, as reported with citations
by IdeaBosque: LangGraph 65%, LangMem 95%, AutoGen 100% violation rate — recency eviction being worst
because it *"does not summarize; they truncate, and the oldest content (which is where standing rules
typically live) is the first to go."* Augment's independent finding is the same shape: agents that
chased the doc *"opened dozens of other markdown files"* and produced worse output. Treat IdeaBosque
as a secondary aggregator; the underlying paper is [arXiv:2606.22528](https://arxiv.org/abs/2606.22528).
[ideabosque.com/library/agent-memory-design-patterns-persistent-context](http://www.ideabosque.com/library/agent-memory-design-patterns-persistent-context/)

### 4.5 Does anyone report maintenance *working*?

**Yes — but only for the migration-to-zero, not for an ongoing automatic loop.**

**One mechanical check that worked, with a number.** **FIRST-HAND**. `simplydominus1-cpu`,
anthropics/claude-code issue #91473, Sept 2026 — field notes after a year of a home-grown memory
system (~25 KB index budget, ~300 topic files, two machines synced over git):

> "I now block a session from ending while orphans exist — **it caught three real ones on its first
> day.**"

and on the write-time size gate:

> "**Expose the write-time size gate.** The post-write 'index over its read limit' error is the best
> anti-bloat mechanism I have seen; I have adopted the pattern for my own injected files."

Both of those are *deterministic, cheap, and mechanical* — an orphan linter and a write-time
admission gate — rather than a judgement-based cleanup pass. Notably, this is the same shape as the
two mechanisms the rest of this report independently recommends (§7).

#### The balance point: hand-written memory has the *same* failure classes

This report has been mostly unflattering to automatic extraction, so this evidence matters for
honesty. The same reporter, having **built the file-based alternative himself**, lists what a year of
it taught him:

> "lessons learned the hard way: **index overflow silently hiding its own tail; orphaned files
> invisible to new sessions; one machine's sync overwriting the other's newer memory; stale recall
> entries pointing at deleted files; duplicates born from parallel sessions** with fuzzy task
> boundaries."

Every one of those maps onto a failure mode documented above for automatic stores: overflow/truncation
(#92998), orphans (`orchestkit` PR #4263), last-write-wins lost updates (#91473's own bug 2), stale
pointers (§3), and duplication (Rank 5). **Substituting markdown files for a store does not remove any
of these failure classes — it relocates them.** The differentiator in the sources is not the storage
technology but whether anything owns the reconciliation: this reporter's orphan linter worked; his
index had no such guard and silently lost its tail.

This is the correct conclusion to draw from §5 as well. Both migrations in the corpus — `brtkwr`,
`jamebobob` and Theo Browne going automatic → files, and `simplydominus1-cpu` going files →
"bundle this into the harness" — report *overlapping* failure lists. The lesson for Vestige is not
"prefer files"; it is that the mechanisms worth building are the ones that survive either substrate:
write-time admission gates, and a periodic reconciliation that reports rather than repairs.
[github.com/anthropics/claude-code/issues/91473](https://github.com/anthropics/claude-code/issues/91473)

`brtkwr` is the one clean success report for a full migration: ~166 auto-written files triaged to
**zero**, with the knowledge preserved. His process was categorisation, not curation: *"project state
got deleted, anything a repo's own docs should cover got turned into a pull request against that repo,
work that deserved doing later got ticketed, and the small durable remainder got kept."* Outcome:
*"the slop is gone, the guardrails survived, and the project knowledge got promoted to a place where
it stops being mine alone."* End state: *"nothing gets saved unless I ask for it"*, and *"It is config
with an AI drafting assistant: the agent proposes, I prune, git versions it, and nothing gets written
as a side effect of a task."*

Note what that success required: **a human in the loop, git as the version store, and writes that are
not a side effect of a task.** No source reports a fully automatic prune/dedup loop restoring trust
on its own.

---

## 5. The net-negative case: who concluded automatic memory was worse than nothing

**Yes. This is well documented, from at least four independent directions — including a vendor
removing the feature.**

### 5.1 Theo Browne — audited, quantified, deleted, and replaced with a hierarchy

The strongest account. Audit numbers in §1 (Rank 1 and Rank 4): 45 files, ~1/3 useful, 19 reads vs 80
writes across 355 sessions, 26 files never read. Conclusion: *"This is garbage. Useless. Okay, that's
all dying now for sure."* He turned the feature off on camera and deleted the lot.

**What replaced it** — a four-level escalation hierarchy, credited to Potato Lauren (Cursor), applied
in order and each exhausted before descending:

1. Eliminate the failure category through architecture
2. Enforce the rule in lint or CI
3. Write a skill or rule
4. Put a human in the loop, as a last resort

Concretely: a hand-written, 100–150-line `agents.md` carrying *values* rather than *discipline* —
including a **glossary** (*"prevents Claude from 'making up fancy terms'"*) and a section literally
titled **"Things the model kept doing no matter what I did"**. Plus CI enforcement for the regressions
he actually cared about: a CI action that replays sample threads through two models, measures
WebSocket bytes transferred, and fails the PR above a threshold. Result: *"my agents don't bug me
until they fix them."*

The underlying discovery he reports is that pre-built context systems lost to giving the agent tools:
*"If you build a fancy context management system instead of just giving the agent the tools that it
needs to find [things], you're behind the curve now."* Armin Ronacher's version: load tool output to
a file and let the model `cat` the part it needs. Mario's Slack bot has *"infinite memory"* by
appending every prompt and response to one JSONL file and querying it with `jq` — *"No embeddings, no
vector database, no memory system — just a log and a query tool."*
[finance.biggo.com/news/bb1b56f140ee671f](https://finance.biggo.com/news/bb1b56f140ee671f)

### 5.2 The clearest statement of the conclusion: "auto-memory is actively harmful"

**FIRST-HAND**, filed in Anthropic's official tracker with 6 upvotes. `arthurworsley`,
anthropics/claude-code issue #23544, "Need ability to disable auto-memory (MEMORY.md)", Feb 2026:

> "**For users who manage their own context systems, auto-memory is actively harmful.**"

The five numbered reasons are a precise, reusable indictment — and notably they are about *control and
duplication*, not about retrieval quality:

1. **Shadow state** — "a parallel memory system outside the user's control. The user can't easily view, audit, or understand what's been stored."
2. **Not git-tracked** — "It can't be committed, reviewed in PRs, or versioned alongside the codebase. This is a significant regression from keeping context in repo-local files like `CLAUDE.md`."
3. **Not portable across CLIs** — a Claude-only silo when the user maintains shared `AGENTS.md`.
4. **Context bloat** — "For users who already have well-structured `CLAUDE.md` and skill files, this is redundant context that displaces useful working memory."
5. **No opt-out** — "Even deleting the files doesn't prevent Claude from recreating them."

In-thread first-hand corroboration, all the same day:

- `banagale`: *"Given just how serious folks are about managing context using skills, agents and docs this feature is at best an anti-pattern and worst infantilization."*
- `perhmm` — the redundancy failure mode, described from workflow: *"every time I initiate a discussion like that it just unprompted writes it to memory. 'Remember to not do x' which most of the times do not solve the problem and **repeats instructions that already exists in the Claude.md or my command and skill documents.** As it is right now with my workflow this feature seems completely useless."*
- `ANogin` — a stale memory actively degrading a plan: *"Just had Claude come up with a plan, where the verification step said 'Pre-existing failures listed in MEMORY.md are unrelated and may still fail', which was **both outdated and a misunderstanding to begin with**, and could have caused unfortunate issues if I did not catch it… Last thing we need is to give Claude more excuses to ignore test failures!"*

**Resolution, for accuracy:** the issue was closed as completed after users found an undocumented
`CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` env var and a `/memory` toggle. The "no opt-out" complaint was
therefore fixed; the quality complaints in the thread were not addressed by that fix, and `perhmm`
confirmed the flag made Claude write to `CLAUDE.md` instead. **The lesson for Vestige is the shape of
the fix: the community's accepted remedy was to route durable knowledge back into the versioned,
reviewable repo file.**
[github.com/anthropics/claude-code/issues/23544](https://github.com/anthropics/claude-code/issues/23544)

### 5.3 Cursor removed the feature outright — maintainer statement

`Dean Rie` (Cursor staff), Nov 2025, replying to a user who found their memories gone in 2.1.32:

> "The Memories feature was intentionally removed starting from version 2.1.x. You can export your
> memories and move them into Rules: Press `Cmd+Shift+P` → Type 'Export memories' → Your memories
> will be saved to an .mdc file. You can then add the exported content to your Rules."

A second user asking for them back described what the feature had actually been used for: *"Every
time AI made a mistake, I created a memory to prevent it. … I treat them as my competitive
advantage."* And the same user, after migrating, accepted the reasoning:

> "I can see the reasoning behind abandoning memories (**they are almost no different than .mdc
> files**)."

That is a vendor deciding that an automatic extraction layer produced nothing a hand-maintained
rules file did not. **Verified independently:** `https://cursor.com/docs/context/memories` now
serves the *Rules* documentation (rule types, `alwaysApply`, glob scoping) rather than any memories
page — the persistence model documented at that URL is rules, not memories.
[forum.cursor.com/t/are-my-memories-gone/144057](https://forum.cursor.com/t/are-my-memories-gone/144057)

Cursor's replacement docs also state, as maintainer guidance, the same anti-copying rule that
underlies Rule 1 below — *"Reference files instead of copying their contents—this keeps rules short
and prevents them from becoming stale as code changes"* — and list under "What to avoid in rules":
*"Duplicating what's already in your codebase: Point to canonical examples instead of copying
code."* Note the shape: the *pointer* is the durable artefact and the *copy* is what rots.
[cursor.com/docs/rules](https://cursor.com/docs/rules)

### 5.4 `TRON4R` abandoned mem0 — with a harvest ratio worth quoting

After months of use, including routine cleanups, he migrated back to OpenClaw's default memory:

> "When migrating back, I asked ChatGPT-5.6-sol to sift through the memories stored by mem0 and tell
> me which ones it considers worthy to be transferred over to the new memory system. And it came back
> with **two out of hundreds** of stored memories. And I discarded one of those two. That's the entire
> harvest after months of using mem0."

> "A memory tool that claims to significantly improve the memory of OpenClaw but then fails to address
> this problem above for so many months is completely useless IMHO. … The response times of my agent
> have significantly improved by switching back BTW. **In hindsight I should have deactivated mem0
> much earlier.**"

His stated general rule afterwards is its own small finding about adoption cost:

> "my new rule now is: if a tool doesn't work out of the box as expected and needs plenty of
> fine-tuning, I am simply not including it in my setup any more."

[github.com/mem0ai/mem0/discussions/4289](https://github.com/mem0ai/mem0/discussions/4289)

### 5.5 `brtkwr` — switched off, but kept a triaged third (the nuanced case)

He disagreed with Theo on deletion but not on diagnosis: *"The audit agreed with him: most of what
auto-memory had written was stale project state I never asked for."* He triaged rather than deleted,
moved durable knowledge into repos and a private git repo, and turned auto-write off. He is explicit
that the result is no longer a memory system:

> "The part I kept is not 'memory' any more. It is config with an AI drafting assistant."
> "**Nothing lives in unversioned local state.** … If a fact is worth keeping, it is worth putting
> somewhere with history and review."

### 5.6 Smaller voices, same direction (code-agent context specifically)

**FIRST-HAND**. `JohnMakin` on HN, "Agent memory as a file format" (191 points, Aug 2026) — while
endorsing a markdown-plus-SQLite memory service he built himself, he rejects harness-managed memory
outright. The most useful part is that he describes the *symptom an operator actually notices*:

> "**'harness managed' memory is utter garbage, I am convinced, and I disable it immediately.** The
> major problem being over time it degrades and sneaks in conflicting or outright false information.
> Then one day you'll swear it's drunk, and every time I got to this state and investigated, auto
> managed memory was *always* the problem."

[news.ycombinator.com/item?id=49512238](https://news.ycombinator.com/item?id=49512238)
(thread: [item?id=49508317](https://news.ycombinator.com/item?id=49508317))
Note the shape of the claim: the user cannot attribute the degradation to memory from the outside.
The failure is *illegible* until investigated — which is the same complaint as `bushido`'s "most of it
is hidden" in §1, Rank 8. This is worth weighing for Vestige's dashboard work: attribution of bad
behaviour to a specific memory is a product feature, not a nicety.

All **FIRST-HAND**, HN thread 47524704:

- `cjonas`: *"Honestly memory seems like an overcomplicating way to solve this problem in the context of something like a coding agent. Rules and Skills are much more explicit, less noisy and easier to maintain. Just requires having an always on rule/system prompt to always update rules/skills as designs or architecture changes in a way that conflicts with old rules. **Memories maybe make more sense for personal assistant use cases.**"*
- `faangguyindia`: *"I don't find any of these helpful at all. You gotta do this only if you are completely coding without ever looking at code. As for me, i just feed it project documentation … so the coding agent can just lookup any method and figure out where it in project and what it does just by single query."*
- `Real_Egor`: *"I actually try to clear Antigravity's memory … for me personally, an agent's 'memory' mostly just gets in the way."*
- `suncemoje` and `Valord`: the recurring *"How's it different / better / worse than CLAUDE.md or auto memory?"* — i.e. even interested readers default to the file.

The consensus position is not "memory is useless" but the narrower and better-evidenced claim:
**for code, memory competes with the repo and loses; for human preferences it has a case.** Theo's own
carve-out says the same: chat memory *"makes more sense"* where *"there is no direct path from one
data point to another."*

**The dissenting counter-example**, for fairness — `mikeadolan`, HN, built a SQLite capture layer
instead of an extractor: *"Every conversation captured losslessly via hooks, with keyword, semantic,
and fuzzy search across all projects. … you get full search and **zero orphaned memories when projects
move or rename**. Running on 1,300+ sessions."* This is the "log, don't summarise" position, and it
directly answers the file-path problem by never creating a decontextualised claim in the first place.

---

## 6. Claims I could not source

Stated explicitly so the gaps are not mistaken for consensus:

1. **No canonical rule about file paths / line numbers in memories.** Searched for; not found. The
   nearest claims are the "derivable content" rule (§2.2) and the "moving targets" list (§2.2), which
   does not mention paths. See §3.
2. **No measurement showing a memory store causing a net capability regression.** `bushido`'s HN
   comment hypothesises it; nobody measured it. The closest instrumented evidence is the Augment
   study on instruction files and the governance-decay paper on compaction — neither isolates a memory
   store.
3. **No reported case of an automatic prune/dedup loop restoring trust by itself.** Every success
   report involves human categorisation or a report-only tool (§4.5).
4. **No first-hand Letta/MemGPT complaint about memory *content quality*.** I found Letta issue #3104
   on the sleep-time agent breaking role and emitting junk tool calls — `temujin9`, Dec 2025,
   FIRST-HAND, with numbers: *"Total sleeptime messages 1,546 … Correct tool calls 370 (97.9%); Role
   confusion errors 8 (2.1%)"*, and the diagnosis *"the sleeptime agent gets a large amount of context,
   just before its response, which appears to be earlier conversation it was engaged in."* That is a
   real sleep-time defect, but it is about the *process*, not about entries being worthless.
   [github.com/letta-ai/letta/issues/3104](https://github.com/letta-ai/letta/issues/3104)
   An independent third-party audit of Letta's data model is worth noting as corroboration that the
   category is immature: `carsteneu/ai-memory-comparison` checked the source and recorded
   `conflict ❌ Not found`, `context ❌ Not found` (*"No 'why' or context metadata field on
   memories"*), `supersede ⚠️ Partial`. It is a competitor-adjacent survey, so treat the framing with
   caution; the file-level citations are specific.
   [github.com/carsteneu/ai-memory-comparison — evidence/letta.md](https://github.com/carsteneu/ai-memory-comparison/blob/main/evidence/letta.md)
5. **No Reddit primary sources were retrievable.** Several searches surfaced Reddit *threads* only
   via secondary citation (e.g. savestate #169 cites an r/LocalLLaMA thread without a permalink).
   I did not fetch reddit.com directly, so no Reddit claim appears in this report as evidence.

**Vendor sources I deliberately did not lean on:** mem0's own blog and Zep's blog were surfaced
repeatedly (memory decay, background consolidation) but they are vendor marketing about their own
products and are the opposite of the requested angle. Where a vendor appears above it is either
(a) a maintainer answering a bug report (Cursor's Dean Rie), (b) a merged maintainer fix (n8n,
hermes-agent, orchestkit), or (c) a study with a published method (Augment) — each labelled.

**Also unverifiable by me, and therefore not used as evidence:**

6. **Theo Browne's raw counts.** My access is a secondary news write-up, not the episode — flagged
   inline at both places the figures appear.
7. **Reddit threads.** `r/ClaudeAI` and `old.reddit.com` both return a JavaScript shell to
   non-browser clients, so thread bodies could not be retrieved. A thread on auto-memory drift
   (`/r/ClaudeAI/comments/1thyy3k/`) is frequently cited by secondary sources, but **no Reddit quote
   appears anywhere in this report as evidence**, and the anecdotes attributed to it elsewhere should
   be treated as unconfirmed until someone reads the thread in a browser.
8. ~~A mem0 maintainer concession.~~ **CORRECTED — this claim is TRUE and I was wrong to disclaim
   it.** I first recorded that I could not verify a maintainer concession about the #4573 audit. I
   subsequently retrieved it directly. It is real and it is stronger than the second-hand version I
   had been offered (which named the wrong issue). See §1, Rank 1 — the maintainer statement, the
   root cause, and the prompt text are now quoted from the primary source.

---

## 7. What this implies for Vestige

The user's complaint maps onto two specific classes. Before the recommendations, one finding about
Vestige itself, because it changes the priorities.

### The complaint is partly self-inflicted by the current save contract

Vestige's own canonical instructions currently *mandate* the two anti-patterns the user is
complaining about:

1. **The `BUG_FIX` save gate hard-codes file paths into the memory body.** The mandated content
   template is `"BUG FIX: [error]\nRoot cause: [why]\nSolution: [fix]\nFiles: [paths]"`. `DECISION`
   and `CODE_CHANGE` gates similarly pass `files: [paths]`, and `smart_ingest`'s Content Intelligence
   Pipeline auto-tags file paths as `entity:*`. So Vestige is not merely permitting file-path
   memories — it is generating them by policy, and storing them as a field that will rot silently the
   moment anything is renamed.
2. **The bias-to-write rule contradicts the entire field.** The memory hygiene section states:
   *"When in doubt, save. Prediction Error Gating handles dedup. Lost knowledge is permanent."*
   Every practitioner source in this report converges on the opposite: *"if you can't name a future
   decision it changes, don't write it"* (`soul-sol`), *"the main problem is at the write stage, not
   retrieval"* (`NovaRouteAI`, diagnosing mem0 #4289), *"most teams obsess over retrieval quality and
   ignore write quality. That's backwards"* (Cropsly). Vestige's dedup gate prevents *duplicates*; it
   does nothing about the far larger volume of *originally-worthless* entries. `TRON4R` ran routine
   cleanups for months and still harvested two useful memories out of hundreds.

   The 10,134-entry audit (§1, Rank 1) is the empirical form of this argument, and it is the reason to
   treat the write gate as *the* lever rather than one lever among several. Dedup and consolidation
   were not what failed there: the audit ran hash-dedup and cosine-dedup passes and still found
   **96.9% of the surviving entries were junk on manual read**. Their single highest-priority
   recommendation is not a better retriever or a bigger model — it is *"a quality gate between
   extraction and storage"*, and their second is *"a REJECT action in the update-decision prompt …
   There is no way to say 'this fact is not worth storing.'"* Vestige's `smart_ingest` has
   `forceCreate` but no reject path; adding one is the highest-leverage change available.

The good news is that Vestige already has the primitives the field says are missing: `valid_from` /
`valid_until`, `contradiction detection`, `temporal invalidate`, coreference rewriting, entity
extraction, `importance_score`, `find_duplicates`, and — critically — **no automatic deletion**. The
recommendations below are therefore mostly *gates on existing machinery*, not new subsystems.

### The three rules most worth enforcing at write time

**Rule 1 — The future-decision gate, with the derivable-content clause.**
Reject (or downgrade to a non-injected tier) any candidate that cannot name the future decision it
changes, and any candidate whose content is derivable from the repo or git history. This is the
single most-cited rule in the corpus and it is the one that kills the file-and-line class at the
source rather than repairing it later.

Implementation notes grounded in the sources:

- The rule must be *checkable*, not aspirational — `soul-sol`'s core insight is that *"'Only store
  important things' gets acknowledged and then ignored, because nothing in it can come out false."*
  So require the reason to be *produced as text*: extend `importance_score` from a score to a gate
  that returns the missing sentence, and make the absence visible.
- The derivable-content clause should target the gate templates first, not the user. **Change the
  `BUG_FIX` template from `Files: [paths]` to `Root cause: [why]` + `Lesson: [the rule to apply next
  time]`**, and keep paths out of the injected text (retain them, if useful, as non-injected
  provenance metadata). This directly implements `gebalamariusz`'s rule — *"Don't simulate the
  database in integration tests because the tests passed, but the migration failed"* is a memory;
  *"the database is PostgreSQL 16"* is not — and answers `vshulcz`'s finding that the conclusion is
  the cheap half.
- Note that this is a *rejection* rule, so it must be visible. Emit a `low_reuse_value` warning the
  way `compound_content_warning` already works, with the same escape hatch (`forceCreate`).

**Rule 2 — The self-containedness gate: no unresolved referents, no relative anchors, no turn
references.**
This is the user's first complaint and Vestige is unusually well-placed to fix it, because the
coreference rewriter already exists — it is currently *best-effort*, and it should become a *gate*.

- **Promote coreference rewriting from rewrite to precondition.** If "He said X" cannot be rewritten
  to "John said X" because no antecedent is in scope, the memory is by construction unreadable
  later. Do not store the unresolved form; reject with the specific unresolved token named so the
  caller can supply the antecedent. This is the mechanical counterpart to the Zep pronoun failure in
  graphiti #608.
- **Reject turn-relative and conversation-relative phrasing outright**: "as discussed", "the above",
  "earlier", "last time", "that thing we talked about", "the file I mentioned". These have no
  referent in any future session by definition. This is a small, high-precision lexicon and is
  exactly the class the user described as "assumes the reader has the original conversation."
- **Extend the existing temporal anchoring from rewrite to hard requirement.** Vestige already maps
  "by next Friday" → `valid_until`. Make relative time expressions (*"recently"*, *"currently"*,
  *"this week"*, *"now"*) a rejection rather than a best-effort rewrite when they cannot be resolved,
  matching `jimy-r`'s rubric row: relative date → CONVERT to absolute, no exception.
- **Require an "as of" on every state claim.** Dawn's first question — *"Where does freshness live in
  your schema?"* — is the structural ask: *"If a claim is correct at insert-time and wrong six weeks
  later, what tells your retrieval system to demote it?"* Vestige has `valid_from`/`valid_until`;
  the rule is that a *status* claim (as opposed to a *decision* or *preference*) may not be stored
  without one.

**Rule 3 — The ephemeral-target gate: transient state either gets a TTL or gets rejected.**
This is the failure mode with the widest independent support (Rank 1 and Rank 2 together), and it is
the one that turns a merely-useless store into an actively-misleading one.

- Adopt the official-derived "moving targets" exclusion list verbatim as a reject list: version
  numbers, test/coverage counts, progress metrics, dates-as-content, line counts, file sizes —
  *"any metrics that become stale after each commit."*
- For genuine state claims that must be kept (a ban that expires, a deadline, a temporary
  constraint), **force them into the time-boxed type rather than `fact`**: require `valid_until`,
  mirroring the existing `precompute_for_context` TTL precedent. `Cropsly`'s *"a single 'I'm traveling
  this week' turned into behavior that lasted long after the trip ended"* and Dawn's expired-seven-day-ban
  are the same bug.
- **Reject unverified tool output as fact.** `soul-sol`: *"A tool call that couldn't be verified is
  UNVERIFIED, not a fact, and storing it makes the error permanent."* Vestige has a `confidence`
  audit; the write-time version is to tag provenance as unverified and exclude it from injected
  context by default.
- **Keep the credential prohibition two-layer, as n8n did**: a mechanical scrub *plus* an explicit
  "never record secret values, only that one was provided" instruction in the save prompt. Vestige's
  current hygiene line ("Never save: secrets, API keys, passwords…") is the instruction layer only.
- **Reject a bare path as content.** Per §3(e), a stored path fails silently in *both* directions —
  it resolves to a stale target or it resolves to nothing, and neither is reported. Practical rule:
  a file reference may appear only as non-injected provenance metadata, never inside the memory text
  that gets loaded. This is the single change that most directly addresses the user's second
  complaint, and it is a one-line change to the `BUG_FIX` template plus a validator.

### The one maintenance pass most worth running

**A reference-rot and freshness audit — report-only, evidence-gated, never auto-deleting.**

Two independent, *measured* results now determine its design, and they rule out the obvious
implementations:

1. **Do not key it on age or on use-count.** `No-Smoke`'s measurements (#92998) show an automated
   retirement pass over never-consulted memories returned **0 eligible candidates** at a 30-day floor,
   and that *"any project writing memories at a steady rate therefore reaches the cap with the
   automated route already exhausted"* — while `brtkwr` argues that read counts are a biased signal
   because *"a guardrail that fires once and stops a destructive mistake looks 'never read' for months
   while being the most valuable line in the file."* Age-based and popularity-based pruning therefore
   both have nothing to work with on a healthy store.
2. **Key it on verifiable staleness instead** — re-resolve external referents against the world, and
   re-confirm status claims against a source of truth. That is the only signal the evidence supports.

Not another dedup pass, and not another summarizer. Rationale from the evidence:

- **Dedup, consolidation, contradiction detection and temporal invalidation are already covered** by
  `find_duplicates`, `dream`, `reflect` and `temporal`. The failure modes that kill trust in the
  sources above are *not* duplicates — they are (a) memories whose external referent no longer exists
  or has changed, and (b) memories that are internally consistent but became false in the world. Dawn's
  own summary is that she *solved* staleness she knew about and has **not** solved this pass, calling
  it *"the next thing to build."* It is the genuine gap in the field, and Vestige is one of the few
  systems positioned to close it because it already extracts file-path entities.
- **Do not add a summarizer or a recency-eviction pass.** The measured evidence is that these make
  things worse: AutoGen's recency eviction produced 100% policy violation, LangMem's summarization
  node 95% ([arXiv:2606.22528](https://arxiv.org/abs/2606.22528)); Augment measured agents chasing doc
  references into *"worse"* output. Vestige's dream cycle is consolidation, which is the safe kind;
  adding truncation-by-recency would be the unsafe kind.

Design, directly from the sources that shipped:

1. **Re-resolve every external referent and report misses.** Vestige tags file paths, URLs and
   entities. Walk them against the current tree/network and produce a drift list. On a hit, do what
   `jimy-r` specifies: *"Surface broken references in the summary; do NOT auto-fix broken links — they
   indicate real structural drift that needs human review."* Surface, never repair.
2. **Gate staleness on evidence, not on age.** Copy orchestrkit's honesty rule: *"staleness only when
   an activity file says the agent ran after the write"*, and *"with no activity file it skips rather
   than calling idle agents stale, which is the honest default."* A memory being old is not evidence
   it is wrong.
3. **Stamp a creation time that nothing refreshes.** openclaw #75842's root cause #2: because
   `lastRecalledAt` is refreshed as a side effect, *"no retention strategy that filters by age is
   honest."* Vestige's search strengthens memories (Testing Effect), which is exactly the refresh
   hazard — so `created_at` must be immutable and distinct from any access-derived timestamp.
4. **Report only; never delete.** This is both orchestrkit's principle (*"a doctor that tidies is a
   doctor that deletes"*) and Vestige's existing guarantee that nothing is auto-deleted. Route
   findings to `reflect` output and the dashboard.
5. **Do not rank findings by retrieval frequency.** `brtkwr`'s counter-argument is the reason: a
   guardrail that fired once and prevented a destructive mistake looks never-read for months while
   being the most valuable entry in the store.
6. **Have the pass write a report with verbs.** `jimy-r`'s output contract — files touched with
   ADD/UPDATE/DELETE/MERGE/MOVE, contradictions resolved *with the source of truth used for each*,
   drift items escalated, and a line count against the ceiling. The per-item "source of truth used"
   is what makes the pass auditable rather than another opaque agent action.
7. **Reconcile the store against the index, in both directions.** `tonydzi` measured 564 memory files
   across three silos with **no index line at all** — at only 57% of the byte budget, so this was not
   an overflow problem — of which 312 were written in the last 30 days and the newest was 44 minutes
   old. His conclusion is the generalisable one: *"A write-time reconciliation between store and index
   catches both roads, and it is the only one of your five that would flag a silo bleeding memories at
   57% of its byte budget."* Two shapes to detect: **stored but never injected** (invisible to the
   agent, so it cannot be corrected by anyone) and **referenced but absent** (a dangling pointer). His
   catch that *"you cannot hook a call that was never made"* is the reason this belongs in a periodic
   pass and not in a write hook.
8. **Make eviction displace by strength, not by age — and only in a pass with evidence.** The one
   practitioner who reports fixing this describes keying eviction on *strength* rather than age or
   append order: *"each line carries a confidence and a hit counter (how often it actually fired), and
   a new lesson has to beat the weakest incumbent to get in. **Corrections then displace the thing
   they corrected instead of losing to it.** I run it as a milestone ritual rather than at write time
   so the judgment happens when there's evidence."* This directly answers #92998's finding that
   truncation-by-recency discards supersessions first, and it is the design most compatible with
   Vestige's existing contradiction detection and `temporal invalidate`. Note the two halves: a
   *strength* signal for ordering, and a *milestone ritual* (not a write-time hook) for the judgement.
9. **Admit at write time; do not only sweep later.** Both the orphan linter (*"I now block a session
   from ending while orphans exist — it caught three real ones on its first day"*) and the size gate
   (*"the best anti-bloat mechanism I have seen"*) that `simplydominus1-cpu` reports adopting are
   **write-time admission gates**, and both produced results immediately, unlike any retrospective
   cleanup pass in this corpus. The pairing matters: the §7 write-time rules prevent entry, and this
   pass catches drift that no gate can see (the world changing underneath a memory that was valid when
   written). Gates and reconciliation are complements, not substitutes.

### Two things not to do

- **Do not resolve the file-path problem by rewriting paths on rename.** Every source that touches
  this treats a broken reference as a *signal* about structural drift, not a maintenance chore.
  Auto-repair silently converts a real-world change into a plausible-looking lie — `jimy-r`: *"Do not
  invent pointers."*
- **Do not treat a memory as a behavioural guarantee.** anthropics/claude-code #75334 is the cautionary
  case: a user wrote a constraint memory *"4–6 times per data type"* and it was violated every time,
  because *"memories are advisory; the model can acknowledge a memory exists and still not apply it."*
  Their finding: *"Hooks work. Memories do not."* Anything that must hold — never committing secrets,
  always running the tests — belongs in a deterministic gate, not in the store. `brtkwr`'s formulation
  is the one to put in the docs: **"a rule that a machine enforces beats a rule an agent must
  remember."** For Vestige, that means the durable wins are write-time *rejections*, not
  better-worded stored advice.

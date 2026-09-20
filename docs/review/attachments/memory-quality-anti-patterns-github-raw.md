# Anti-patterns in Agent Memory Systems — First-Hand GitHub Evidence (2024–2026)

**Retrieval method.** Every finding below was retrieved in full through the **authenticated `gh` CLI** (`gh api repos/OWNER/REPO/issues/N` + `.../comments?per_page=100 --paginate`), which returned complete issue bodies and every comment thread. Quotes are copied verbatim from that retrieved text. Dates are the ISO `created_at` values returned by the API. All items are **issue reports or issue comments**; PR review comments were not separately mined.

**Known gaps / partial retrieval — stated up front:**
- **GitHub Discussions were NOT retrieved for any repository.** The REST `/discussions` endpoint is not available unauthenticated and I did not use GraphQL. Any discussion-only evidence in these projects is absent from this report.
- **`mem0ai/mem0` #4573** is the single richest source and is cited under five different failure modes; where I cite it more than once I use a different verbatim passage each time so quotes remain independently checkable.
- Items I saw only as **search-result titles** (not fetched) are listed in the final section and are explicitly labelled as snippets.
- I excluded the "Pre-publication review" series (`mem0#7002`, `graphiti#1771`, `langmem#180`, `letta#3431`) as primary evidence: those are **static source readings by a competing vendor's staff**, not first-hand operational reports, and Letta's maintainer publicly disputed them.

---

## 1. Memory stored trivia / junk / noise; extraction too indiscriminate; memory made the agent WORSE

### 1.1 — A 32-day production audit: 97.8% of 10,134 stored memories were junk
- **URL:** https://github.com/mem0ai/mem0/issues/4573
- **User:** `jamebobob` · **Date:** 2026-03-27 · **Class:** FIRST-HAND (operator running mem0 OSS + Qdrant in production, 32 days)
- **Quote:** *"224 entries survived the full process. Out of 10,134. That's 97.8% junk across the entire collection."*
- **Quote 2:** *"A better model follows the extraction prompt more faithfully, which means it extracts more indiscriminately."*
- **Evidence strength:** Very strong. Issue closed, **24 comments**, with four further independent first-hand operators confirming the same pattern in-thread (`farrrr`, `kevhiggins1-cmd`, `DomLynch`, `jwade83`). Includes a per-batch junk table and a nine-category breakdown (boot-file restating 52.7%, heartbeat/cron noise 11.5%, architecture dumps 8.2%, transient task state 7.4%, hallucinated profiles 5.2%).

### 1.2 — High-recall extraction is the *design*; a better model makes it worse; no OSS lever to dial it down
- **URL:** https://github.com/mem0ai/mem0/issues/5730
- **User:** `hcsum` · **Date:** 2026-06-21 · **Class:** FIRST-HAND (self-hosted OSS user)
- **Quote:** *"In practice a capable LLM follows this faithfully and extracts transient/operational/low-value facts alongside durable ones."*
- **Quote 2 (same issue, `linked_memory_ids` contract):** *"There is no way to dial recall down without either losing the extraction contract or rewriting the entire prompt."*
- **MAINTAINER confirmation (same issue, `kartik-mem0`, 2026-06-22):** *"The precision/noise concern is legitimate and consistent with #4573. A stronger model following the high-recall prompt more faithfully is a real observed pattern."*
- **Evidence strength:** Strong. Issue open, 4 comments; a maintainer independently reproduces the diagnosis and names the root cause (recall policy, output schema and examples fused into one monolithic prompt block).

### 1.3 — "You keep memorizing random stuff" — auto-memory writing files for inconclusive tasks
- **URL:** https://github.com/anthropics/claude-code/issues/95079
- **User:** `aqua5230` (filing on a user's behalf, quoting them) · **Date:** 2026-09-17 · **Class:** FIRST-HAND (end user of Claude Code auto-memory; issue text states the quoted words are the user's, *translated from Chinese*)
- **Quote:** *"You keep memorizing random stuff, why did you record that? Pure waste of token. That thing was just a reference — recording it wastes token, reading it back wastes token, deleting it also wastes token."*
- **Detail:** The model wrote a memory for an exploratory task with no actionable conclusion, then — after the user deleted it — wrote a *second* memory whose content was "don't write memory for inconclusive exploration", which the user also deleted.
- **Evidence strength:** Moderate. Open, 0 comments, one reporter — but the quoted user text is direct and the failure mode (memory about not writing memory) is self-demonstrating.

### 1.4 — Working-directory-scoped memory made the agent apply one project's facts to an unrelated project
- **URL:** https://github.com/anthropics/claude-code/issues/91738
- **User:** `RHerAle` · **Date:** 2026-09-03 · **Class:** FIRST-HAND
- **Quote:** *"The model used facts from project B (a business/legal context) when analysing project A, a personal project with no relation to B. The user had never connected the two."*
- **Detail:** Over ~3 months of sessions launched from `$HOME`, 23 markdown files plus `MEMORY.md` accumulated into one shared store. The user read the cross-project bleed as a *privacy* failure because both claude.ai memory toggles were off.
- **Evidence strength:** Strong for a single report — concrete 3-month timeline, 23-file store, reproducible steps (open, 1 comment).

---

## 2. Memory entries meaningless without the original conversation (severed provenance, dangling references, conversational residue)

### 2.1 — Once written, there is no way to tell which message produced a memory, or whether it was invented
- **URL:** https://github.com/mem0ai/mem0/issues/7047
- **User:** `OfficialAbhinavSingh` · **Date:** 2026-08-20 · **Class:** FIRST-HAND
- **Quote:** *"Once written, there is no way to answer: Which message produced this memory? Is this extracted fact actually supported by what the user said, or did the model invent it?"*
- **Detail:** Source messages *are* persisted, but `_create_memory` writes no reference to the `messages` row that produced the memory, and `save_messages` deletes everything past the most recent 10 rows per scope — so *"a memory written today can have its source row deleted by tomorrow's `add()`."*
- **Evidence strength:** Strong. Open, 1 comment (a ping, no maintainer response). Code-level citations with file:line.

### 2.2 — "The link to the original conversation is severed"
- **URL:** https://github.com/mem0ai/mem0/issues/4573 (comment) · **User:** `jamebobob` · **Date:** 2026-03-30 · **Class:** FIRST-HAND
- **Quote:** *"Once a fact is extracted, the link to the original conversation is severed. 'User prefers PostgreSQL' lives in the store with no trace of when they said it, why, or what they were comparing against."*
- **Evidence strength:** Strong — same 24-comment thread; this passage is the reporter's own architectural post-mortem after the audit.

### 2.3 — Retrieval chunks lose the note identity they came from
- **URL:** https://github.com/basicmachines-co/basic-memory/issues/1335
- **User:** `gkhnelbstn` · **Date:** 2026-08-26 · **Class:** FIRST-HAND (running a team vault, v0.22.1, sqlite + FastEmbed)
- **Quote:** *"every later chunk — which for an observations-style note means every single `- [fact] …` bullet — is embedded with no indication of which note it belongs to."*
- **Quote 2:** *"For a vault whose notes are mostly bullet lists of discrete facts, that means most vectors carry no note identity at all."*
- **MAINTAINER confirmation:** `phernandez`, 2026-08-28 — *"Confirmed by reading `src/basic_memory/repository/..."* (thread comment).
- **Evidence strength:** Strong. Closed with maintainer confirmation; the reporter also self-corrects an unrelated claim in-thread, which raises credibility.

### 2.4 — Extraction misattributes relationships to third parties, with no faithfulness check before persistence
- **URL:** https://github.com/getzep/graphiti/issues/1880
- **User:** `DataAlchmesit` · **Date:** 2026-09-11 · **Class:** FIRST-HAND (analysing their own Graphiti ingestion pipeline)
- **Quote:** *"there is no deterministic middleware hook or validation callback phase between the resolution of these graph elements and their final bulk persistence to verify if the extracted triples remain semantically faithful to the raw input string."*
- **Detail:** *"During multi-party dialogues or entries involving third parties (e.g., 'My brother is a lawyer'), upstream LLMs can occasionally misattribute entity relationships during the initial extraction pass."*
- **Evidence strength:** Moderate — open, 2 comments, but the second comment (`renezander030`) adds an independent first-hand implementation of the same gate, including the point that *"a refused triple should leave a record, otherwise the same triple comes back on the next extraction pass."*

### 2.5 — A store that is never read still produces confident wrong answers, and corrections land in the wrong place
- **URL:** https://github.com/anthropics/claude-code/issues/95308
- **User:** `riannone-doxee` · **Date:** 2026-09-18 · **Class:** FIRST-HAND
- **Quote:** *"The model writes project-scoped notes that contradict global ones. Not knowing the global store exists, it wrote an access note that said the opposite of the global one. From then on the wrong copy wins, because it is the only one injected."*
- **Detail:** The user's `~/.claude/memory/MEMORY.md` holds ~41 cross-project notes and is never injected nor even named in the system instructions. The reporter's second point: the correction loop cannot work — *"the user says 'fix your memory'; the model fixes the project store, which is not where the truth lives; the next session repeats the same mistake identically."*
- **Evidence strength:** Moderate–strong. Open, 0 comments, but very specific, falsifiable claims with a named mechanism.

---

## 3. Entries referencing file paths / line numbers / function names / commit SHAs that went stale

### 3.1 — A memory file pinned to a versioned plugin path silently loaded rules one version behind for three days
- **URL:** https://github.com/anthropics/claude-code/issues/95095
- **User:** `sealab2066` · **Date:** 2026-09-17 · **Class:** FIRST-HAND
- **Quote:** *"Old versions are retained in the cache, so `0.10.1`, `0.10.2` and `0.11.0` all sit on disk and the pinned import keeps resolving — to the old copy."*
- **Quote 2:** *"every session loaded rules from 0.10.2 and skills from 0.11.0 at the same time."*
- **Detail:** The memory file contained exactly one line: `@/Users/<me>/.claude/plugins/cache/sw-infra/infra-ai-skills/0.10.2/team/CLAUDE.md`. On 2026-09-17 the rule governing MR descriptions was simply absent from context. Second failure mode: if the old version is pruned, *"an unresolvable import surfaces as nothing"* — the whole rule set disappears with no warning.
- **Evidence strength:** Strong. Open, 0 comments, but it re-files a prior report (#82068, closed by stale-bot) and **demonstrates the failure that report predicted**, with a two-version transcript as proof.

### 3.2 — Stale path/number references inside a hand-maintained rule file contradicted each other; nothing detects it
- **URL:** https://github.com/anthropics/claude-code/issues/90542
- **User:** `paddykopp` · **Date:** 2026-08-29 (follow-up comment 2026-08-29T11:51:48Z) · **Class:** FIRST-HAND
- **Quote:** *"The rule file itself was wrong, and it had misled me."*
- **Quote 2:** *"The contradiction had been sitting in the file for days — nothing detects that."*
- **Detail:** `TOOLS.md` stated "Ports: Live 9400, Demo 9410" on one line and "since #175 there is no second port" six lines later; the model asserted the stale half. The 700-line contract also required `file:line` evidence chains, which the model fabricated nine times.
- **Evidence strength:** Strong. Open, **33 comments**, long verbatim audit trail.

### 3.3 — Moving a file silently forks the memory: the edit reports success and changes nothing
- **URL:** https://github.com/basicmachines-co/basic-memory/issues/1479
- **User:** `sammywachtel` · **Date:** 2026-09-05 · **Class:** FIRST-HAND
- **Quote:** *"Everything that reads the note by path, or by listing the directory, keeps reading the moved file, so the edit appears to have been made and changes nothing. The caller is told the write succeeded."*
- **Detail:** Reproduced on shipped default `update_permalinks_on_move: false`, public MCP tools only, on commit `6d6e7fe`. Result is two notes with the same title in the same directory.
- **Evidence strength:** Strong. Closed, 1 comment; includes literal tool output and a 6-step transcript.

### 3.4 — Graph memory stores commands and paths as entities — meaningless fragments with no later referent
- **URL:** https://github.com/mem0ai/mem0/issues/4573 (comment) · **User:** `farrrr` · **Date:** 2026-03-30 · **Class:** FIRST-HAND (operator, ~5 weeks production, FalkorDB + pgvector)
- **Quote:** *"Our graph has 12,083 relationships, and some of them are junk: SSH commands stored as entities (`ssh -L 8899:127.0.0.1:8899`), file paths (`.env`, `.agents/skills/`), CSS selectors."*
- **Evidence strength:** Strong — quantified (12,083 relationships) and specific; the same commenter notes graph structure prevents *mass duplication* but not a single bad triple.

### 3.5 — Memory paths become unreachable when the session's working directory moves
- **URL:** https://github.com/anthropics/claude-code/issues/94796
- **User:** `petertanis` · **Date:** 2026-09-16 · **Class:** FIRST-HAND
- **Quote:** *"this becomes permanently unreachable via the UI for that session once the working directory diverges, even though memory is documented as persisting 'across conversations.'"*
- **Detail:** The memory store stays pinned to `~/.claude/projects/<hash-of-A>/memory/` for the session lifetime while the preview pane restricts to the current cwd; the absolute path printed is valid and the file exists on disk. Labelled `has repro`.
- **Evidence strength:** Moderate. Open, 0 comments; clear repro, narrower impact (in-app preview only).

---

## 4. Unbounded growth, duplicate explosion, self-reinforcing loops (memory extracted from memory)

### 4.1 — 808 copies of one hallucination, amplified by the recall→extract loop
- **URL:** https://github.com/mem0ai/mem0/issues/4573 · **User:** `jamebobob` · **Date:** 2026-03-27 · **Class:** FIRST-HAND
- **Quote:** *"808 entries asserting 'User prefers Vim.' 191 exact copies of one sentence. Nobody in the system uses Vim."*
- **Quote 2:** *"There's no mechanism to distinguish recalled memories from new conversation content during extraction. Any hallucination that gets stored once will be re-extracted indefinitely."*
- **Detail:** Junk category "Boot file / system prompt restating" — 3,200 entries (52.7% of junk); *"Operator prefers Telegram" appeared 200+ times*. Including hash duplicates and cosine clusters, the true total for that category is over 5,500 entries.
- **Evidence strength:** Very strong — the canonical first-hand account of a self-reinforcing extraction loop, corroborated by four other operators in-thread.

### 4.2 — `improve()` re-processes all history every run; content-hash dedup never fires because the LLM rephrases
- **URL:** https://github.com/topoteretes/cognee/issues/4996
- **User:** `VerusK` · **Date:** 2026-09-09 · **Class:** FIRST-HAND
- **Quote:** *"It ran for roughly a day on my machine and cost ~$18 in OpenAI charges before I noticed. Graph growth was a steady ~600 nodes/min with no sign of tapering."*
- **Quote 2:** *"The LLM rephrases and picks different facts each pass, so the generated text is never byte-identical and `content_hash` dedup never fires."*
- **Detail:** Measured on the reporter's own store: `session_learnings documents: 1534`, `unique by content_hash: 1534`, `exact duplicates: 0`. Root cause traced to `last_n_steps=None` at `cognee/api/v1/improve/improve.py:542`.
- **Evidence strength:** Very strong — first-hand, quantified in dollars and node rate, with the dedup failure measured rather than asserted.

### 4.3 — A capped memory file silently discards the NEWEST entries, i.e. preferentially discards corrections
- **URL:** https://github.com/anthropics/claude-code/issues/92998
- **User:** `No-Smoke` · **Date:** 2026-09-09 · **Class:** FIRST-HAND
- **Quote:** *"So overflow discards the most recently written entries, with no error at write time — the write reports success."*
- **Quote 2:** *"A memory that corrects an earlier memory is always newer than the thing it corrects. So the truncation preferentially discards supersessions, leaving the superseded version loaded and authoritative."*
- **Detail:** Measured on a real silo: 199 memories at 201/200 lines. An automated retirement pass over never-consulted memories returned **0 eligible candidates** at a 30-day floor — the age profile was 96 written this month, 91 last month, 12 older. The reporter also maintains a `PreToolUse` guard that *did not fire during the overflow*, because auto-memory's write path bypasses the `Write`/`Edit` tools: *"no user-installed hook can guard it."*
- **Evidence strength:** Very strong — re-files five previously auto-closed reports (#25006, #33143, #38452, #39811, #57574); includes capacity tables and a control experiment. Open, 3 comments.

### 4.4 — A forgetting system that structurally cannot reach the stale tail
- **URL:** https://github.com/doobidoo/mcp-memory-service/issues/1124
- **User:** `doobidoo` · **Date:** 2026-09-05 (migrated from Codeberg #327, 2026-08-27) · **Class:** MAINTAINER (about own project's behaviour on a real deployment)
- **Quote:** *"~7,900 live memories, of which 7,687 are stale by the 30-day definition, and the median age of the corpus is far past a year. Those are permanently out of reach now."*
- **Quote 2:** *"36 archived, all of them never-accessed automation noise (20 'Memory Consolidation Report … Mode: DRY-RUN' notes and 15 auto-captured session summaries, `access_count = 0` on every one)."*
- **Evidence strength:** Strong — a maintainer documenting, with production numbers, that controlled forgetting is bounded to a 365-day window and cannot see the backlog it exists to remove. Closed, 2 comments with an explicit contributor brief.

### 4.5 — Stale prior-session messages leak into the extraction context and generate irrelevant memories
- **URL:** https://github.com/mem0ai/mem0/issues/7195
- **User:** `SilenceSik` · **Date:** 2026-09-01 · **Class:** FIRST-HAND (self-hosted mem0 2.0.19 + SQLite)
- **Quote:** *"the extraction prompt's `## Last k Messages` section mixes unrelated prior-session content into the current extraction — resulting in irrelevant memories and, in some upstream gateway setups, content-filter rejections."*
- **Detail:** The buffer is evicted by pure time-FIFO; a new session with fewer than N messages retains the previous session's tail. The reporter has a gateway capture showing another user's intimate content at positions 1–2 of the extraction prompt.
- **Evidence strength:** Strong. Open, 1 comment — and that comment (`yashdoke7`, 2026-09-15) independently traces the underlying ordering bug to `ORDER BY created_at DESC LIMIT 10` in `mem0/memory/storage.py:286` and notes a maintainer-endorsed fix (#5632) that never landed.

---

## 5. Abandoning automatic extraction for hand-written memory files (and the reverse)

### 5.1 — "It's files, not a database. And it works better than any of these."
- **URL:** https://github.com/mem0ai/mem0/issues/4573 (comment) · **User:** `jamebobob` · **Date:** 2026-03-30 · **Class:** FIRST-HAND
- **Quote:** *"That's narrative memory. It's files, not a database. And it works better than any of these."*
- **Stated reasoning (immediately preceding):** *"the closest thing to a reconstruction-based memory system I've found is what my bot already has: SOUL.md (identity), USER.md (understanding of you), MEMORY.md (curated state), daily notes (episodic memory), LCM (compressed conversation history with on-demand expansion back to source)."*
- **Evidence strength:** Very strong — an explicit, reasoned switch from automatic extraction to hand-curated markdown, from the operator of the 10,134-entry audit.

### 5.2 — Migrating off the extraction-and-store model entirely
- **URL:** https://github.com/mem0ai/mem0/issues/4573 (comment) · **User:** `jamebobob` · **Date:** 2026-04-18 · **Class:** FIRST-HAND
- **Quote:** *"I pulled mem0 out within a few days of posting here. Not because the thread convinced me, but because the audit data kept looking worse the more I looked, and trying to patch around it felt like the wrong shape of work."*
- **Quote 2:** *"I moved off the extraction-and-store model entirely and put the long-term side on a knowledge-brain that's closer to a markdown-backed typed-link graph."*
- **Detail:** Three weeks later, in the same thread, the reporter concedes the counter-argument from `farrrr` — that synthesis belongs at the agent layer, not the memory layer — so the switch is presented as a bet on *where to spend complexity*, not a refutation of memory systems per se.
- **Evidence strength:** Very strong — a dated, self-reported migration with the reasoning and the counter-argument both recorded.

### 5.3 — A year of hand-built memory: index injected by a hook, topic files, git sync
- **URL:** https://github.com/anthropics/claude-code/issues/91473
- **User:** `simplydominus1-cpu` · **Date:** 2026-09-02 · **Class:** FIRST-HAND
- **Quote:** *"Before I knew auto-memory existed, I built my own persistent memory for Claude Code: an index a hook injects into every prompt — guaranteed in context, not merely readable on request; topic files loaded just in time; semantic recall over the whole store; automatic two-machine sync over git."*
- **Stated pain that drove it (his own list):** *"index overflow silently hiding its own tail; orphaned files invisible to new sessions; one machine's sync overwriting the other's newer memory; stale recall entries pointing at deleted files; duplicates born from parallel sessions with fuzzy task boundaries."*
- **Evidence strength:** Strong. Open, 2 comments; ~25 KB index budget, ~300 topic files, daily use for a year. Notably the hand-rolled system converges on the same *shape* as the product's later auto-memory, which is itself evidence that the file-based design is the attractor.

### 5.4 — When the curated index is the source of truth, the harness's compaction reminder becomes an adversary
- **URL:** https://github.com/anthropics/claude-code/issues/91188
- **User:** `niels-roest` · **Date:** 2026-09-01 · **Class:** FIRST-HAND
- **Quote:** *"An agent session that follows the reminder literally will compress away ~2KB of curated content to satisfy a target below the user's own norm."*
- **Quote 2:** *"We ended up writing standing doctrine into memory itself telling future sessions to measure and ignore the reminder — which works, but means the product is training the agent to distrust a harness message."*
- **Detail:** The team measured and settled on a ~22 KB curated index; the hardcoded reminder fires above ~17.5 KB and has fired *"8+ times over several weeks without ever being actionable."*
- **Evidence strength:** Strong. Open, **52 comments**; `pm25coder`'s reply is a second independent hand-designed alternative (keep `MEMORY.md` a pure index, facts in per-topic files, so compaction *"can never delete a fact, because the facts are not in the index"*).

### 5.5 — The reverse direction: users deliberately switching automatic memory OFF, and finding it still on
- **URL:** https://github.com/anthropics/claude-code/issues/91738 · **User:** `RHerAle` · **Date:** 2026-09-03 · **Class:** FIRST-HAND
- **Quote:** *"The user had both claude.ai memory toggles OFF on purpose and only discovered the local memory when the model referenced an unrelated project in a new session."*
- **Evidence strength:** Strong — concrete evidence of opt-out intent being defeated by a second, unadvertised memory system. Open, 1 comment.

---

## 6. Long-horizon reports: drift, staleness, rot over months, "actively harmful"

### 6.1 — Seven-day drift with no failed write: three conventions contradicted the file that referenced them
- **URL:** https://github.com/anthropics/claude-code/issues/82056
- **User:** `shawnacason` · **Date:** 2026-07-28 · **Class:** FIRST-HAND
- **Quote:** *"Over seven days it drifted three times, including two conventions that contradicted the written process inside the same file that referenced it."*
- **Quote 2:** *"No individual write failed. Every component behaved as designed."*
- **Detail:** A process ruling was saved to auto-memory but not to the working tree; the tree resolved to a different project path, so later sessions never saw it. Reconstructing the drift took *"a dedicated audit pass of roughly 118k subagent tokens and produced no product output."* The reporter also notes the write-time size warning goes to the model, not the user, and fires only on write.
- **Evidence strength:** Very strong. Open, **51 comments**, at least 7 distinct commenters. The core claim — *"A store that fails partially and silently produces false negatives that read as ordinary answers"* — is the cleanest statement of the long-horizon failure in this corpus.

### 6.2 — Months of daily use: the rule existed, was indexed, and had no behavioural effect
- **URL:** https://github.com/anthropics/claude-code/issues/69044
- **User:** `klausrattenbacher` · **Date:** 2026-06-17 (update comment 2026-07-07) · **Class:** FIRST-HAND
- **Quote:** *"The specific file containing the exact applicable rule was never opened before the task, so it had no behavioral effect despite 'existing.'"*
- **Quote 2:** *"Writing new memory entries after each failure doesn't help if those entries aren't retrieved with full content next time."*
- **Detail:** A structured multi-month German-language log of recurring failures explicitly maintained *because* "the same error types repeat across sessions despite explicit instructions, hooks, CLAUDE.md files, Auto-Memory, and documented rules."
- **Evidence strength:** Very strong. Open, **57 comments**; the update comment documents the exact failure mode recurring weeks after it was written down.

### 6.3 — Time-blindness: conflicting "truths" with no recency signal
- **URL:** https://github.com/mem0ai/mem0/issues/5352
- **User:** `Ardem2025` · **Date:** 2026-06-03 · **Class:** FIRST-HAND (self-described team production deployment, mem0 OSS + Qdrant)
- **Quote:** *"Over time, old configuration values, outdated user preferences, or transient error states result in duplicate/contradictory facts in Qdrant."*
- **Quote 2:** *"vector search returns them sorted by similarity rather than recency — and doesn't include timestamps — the LLM receives multiple conflicting 'truths' and gets confused."*
- **Detail:** The reported workaround is a weekly cron hygiene script that merges clusters at cosine ≥ 0.82 — i.e. manual, scheduled cleanup of a store that cannot age. Production result: *"with a 90-fact DB, the first run safely purged 8 duplicate entries and merged 6 others."*
- **Evidence strength:** Very strong. Open, **47 comments**; a detailed first-hand counter-analysis in-thread warns the "newer date wins" merge heuristic breaks when *"a transient error state gets written last and then 'wins'."*

### 6.4 — Errors that outlive the session because they were persisted as fact
- **URL:** https://github.com/anthropics/claude-code/issues/95436
- **User:** `brunosantoscodexforgeceo` · **Date:** 2026-09-18 · **Class:** FIRST-HAND
- **Quote:** *"Persist the conclusion to memory, so the error outlives the session."*
- **Detail:** From one iOS-release session: the agent asserted an SDK nullability change as the cause of a compile error and *"wrote that explanation into memory"*; the real cause was a compatibility shim ~200 lines below in the same file. It then deleted that shim after validating on only one of two toolchains, breaking the release archive. Reporter's framing: *"This is a report about a behavioural failure mode, not a crash."*
- **Evidence strength:** Moderate–strong. Open, 0 comments, but the four-step pattern is stated crisply and each instance is concrete.

### 6.5 — A rule contract that governed nothing, over a 4.5-hour session
- **URL:** https://github.com/anthropics/claude-code/issues/90542 · **User:** `paddykopp` · **Date:** 2026-08-29 · **Class:** FIRST-HAND
- **Quote:** *"The rule file was loaded into **every** context window of the session. It was read. It was quoted. It governed nothing."* (emphasis is the author's own markdown, preserved)
- **Detail:** ~700-line `CLAUDE.md`, 30 numbered rules, a 25-point self-check, two formal prompt contracts, `PreToolUse` hooks that actually block. The user's own contract *"predicts this gap in writing, and he was right."* Recorded cost: the user missed a live-streamed endurance race; the rule file already contained a self-contradiction nobody detected (see 3.2).
- **Evidence strength:** Strong. Open, 33 comments, verbatim session record.

---

## 7. Privacy / secret leakage: credentials, tokens, private data captured into long-term memory

### 7.1 — 130 security/privacy leaks in one 32-day corpus, including live configuration values
- **URL:** https://github.com/mem0ai/mem0/issues/4573 · **User:** `jamebobob` · **Date:** 2026-03-27 · **Class:** FIRST-HAND
- **Quote:** *"IP addresses, chat IDs, file paths, and in 2 cases sensitive configuration values that should never have reached the vector store."*
- **Detail:** Tabulated as a named junk category — *"Security / privacy leaks | 130 | 2.1%"* — inside a 10,134-entry audit. The same audit found a further 200 entries of *"Identity confusion | Model confused agent with operator, hostnames with user names."*
- **Evidence strength:** Very strong — quantified in a public dataset audit with a full category table.

### 7.2 — Agent episodic memory persisted pasted credentials in plaintext
- **URL:** https://github.com/n8n-io/n8n/issues/34740
- **User:** `KhairnarLokesh` · **Date:** 2026-07-22 · **Class:** FIRST-HAND
- **Quote:** *"the agent's observation log and episodic memory indexer persist these details to the database so they can be referenced/summarized across conversational sessions."*
- **Quote 2 (repro):** *"Observe that the provided secret (e.g., `xoxb-1234567890-abcdefghij`) is stored in plaintext rather than being redacted."*
- **Detail:** Affected fields named explicitly: `content`, `evidenceText`, observation log entries, reflection records. The reporter notes the pre-fix redaction was *"too narrow (relying on simple custom regex patterns that only caught a subset of inline keys) and did not run systematically across episodic memory indexers and observation log reflectors."*
- **Evidence strength:** Very strong — labelled `team:ai`, `status:in-linear`, reproduced on n8n 2.32.0 / Windows / sqlite, and confirmed fixed by maintainer `burivuhster`: *"Was fixed in https://github.com/n8n-io/n8n/pull/34622"* (2026-08-03). This is a **confirmed, reproduced memory-store secret leak**, not a hypothesis.

### 7.3 — Memory content, raw messages and extracted entity values written to application logs
- **URL:** https://github.com/mem0ai/mem0/issues/6915
- **User:** `Aryan-Pillai7` · **Date:** 2026-08-11 · **Class:** FIRST-HAND
- **Quote:** *"`Memory`/`AsyncMemory` write user content to application logs. `_update_memory` logs the full memory text at **INFO**, twice per update."* (emphasis is the author's own markdown, preserved)
- **Quote 2:** *"so this is personal data flowing into log aggregators — systems with different retention, access control and residency than the memory store the operator actually vetted."*
- **Detail:** A sweep found **18 log sites** across three categories (memory content, raw conversation dicts, entity values). The `{message_dict}` case logs an entire rejected message verbatim at WARNING *including its `content`*. The reference server sets `logging.basicConfig(level=logging.INFO)` at `server/main.py:46`, so the INFO lines fire in mem0's own default deployment. Repro uses a health fact: `"I have type 2 diabetes and take metformin daily"`.
- **Evidence strength:** Strong. Open, 0 comments (no maintainer response retrieved), but file:line citations for every site and a reproducible script.

### 7.4 — One client's project memory injected into a different client's session, under NDA
- **URL:** https://github.com/anthropics/claude-code/issues/93960
- **User:** `intermap74` · **Date:** 2026-09-13 · **Class:** FIRST-HAND
- **Quote:** *"`MEMORY.md` is loaded automatically at the start of every session, so one client's accumulated project notes are injected into an unrelated client's session context, with no user action and no indication. For contract work under NDA that is the entire problem."*
- **Detail:** The store directory is derived by replacing every non-alphanumeric character with a single `-`, which is not injective. Controlled repro with a control group: `가나다` and `라마바` both map to `C--Users-USER-AppData-Local-Temp-clash----`, and a "secret code" written in the first was returned verbatim in the second. Cross-listed as failure mode 1 (memory contamination) and 4 (dedup/keying failure).
- **Evidence strength:** Strong. Open, 1 comment; `stonianua` independently confirms the encoding shape on Windows stores. Labelled `has repro`, `platform:windows`, `area:core`.

### 7.5 — A live service credential printed in plaintext, in a session whose memory convention forbade it
- **URL:** https://github.com/anthropics/claude-code/issues/93740
- **User:** `coverageandrew` · **Date:** 2026-09-11 · **Class:** FIRST-HAND
- **Quote:** *"Secret exposed in plaintext directly in the conversation (not written to a file, but fully disclosed): one live Doppler service credential — this credential should be treated as compromised and rotated."*
- **Memory-relevant quote:** *"Claude then attempted to write a new file into a personal memory-notes directory using the Write tool, despite a standing prior instruction that this project does not use that memory convention. The write was only stopped by a permission hook."*
- **Detail:** The agent also *"queried a long-term knowledge-graph memory tool (repo-memory) that only stores merged repo/architecture facts and has no session/chat history — a tool that structurally could not answer the question."* The reporter counts the pattern repeating *"across at least six distinct instances in a single session (secret handling, memory-tool selection, sub-agent dispatch, unprompted file reads…)."*
- **Evidence strength:** Moderate–strong. Open, 0 comments; but the secret exposure is concrete and the report includes a "should have" list distinguishing model behaviour from harness design. The credential reached the *conversation*; the memory-file write was blocked.

---

## Cross-cutting notes for the report author

1. **The strongest single datapoint in the whole corpus is `mem0ai/mem0#4573`** — a public, dated, quantified audit with a taxonomy, and a thread in which four independent operators confirm the same failure classes and two of them publish fork-level fixes.
2. **The "memory made it worse" case is stated outright, not inferred.** `jamebobob`, 2026-03-30, in the same thread: *"We were close to sunsetting mem0 entirely this week after the junk findings, realizing we may have been better off with no memory system at all."* He then did migrate (see 5.2).
3. **The two failure modes with the weakest first-hand coverage in this corpus are mode 2 (context-free entries) and mode 3 (stale code references).** Mode 2 evidence is mostly *provenance-absence* reports rather than examples of dangling stored text. Mode 3 evidence clusters in Claude Code / Codex *instruction-file* stores (markdown), not in vector or graph memory — the closest vector-store analogue is `farrrr`'s graph entities that are shell commands and `.env` paths (3.4).
4. **Maintainer statements used here are only those that concede a limitation of their own design**: `kartik-mem0` on mem0 #5730 (recall/noise is a real, design-level regression in configurability), `phernandez` on basic-memory #1335 (confirmed chunk-context loss), `doobidoo` on mcp-memory-service #1124 (forgetting cannot reach the stale tail), `burivuhster` on n8n #34740 (fix confirmed).
5. **Vendor-adjacent material was excluded.** Many search hits in this space are competing-vendor proposals filed as issues (`vgudur-dev`/OWASP Agent Memory Guard across mem0, letta, langchain, chroma, autogen, n8n; `vnbochkarev-netizen`/ViBo; `izgorodin`/Mnemoverse). They are not user reports and are not cited above.

---

## Appendix — seen only as search-result snippets, NOT retrieved (do not cite as findings)

These titles appeared in `gh api search/issues` results. I did **not** fetch their bodies, so the following is a **search-result snippet, not a verified finding**, and the URL is the canonical issue URL constructed from the API's `number` field:

- `mem0ai/mem0#7123` — *"[mem0-ts] add() dedup search hardcoded to top-10 with no similarity threshold → duplicate memories on large corpora"* (`ilndboy`, 2026-08-26, open, 1 comment) — **snippet only.**
- `mem0ai/mem0#6515` — *"bug(memory): hash-dedup TOCTOU race in add() creates permanent duplicate memories under concurrency"* (`OfficialAbhinavSingh`, 2026-07-22, open, 0 comments) — **snippet only.**
- `mem0ai/mem0#5205` body/comment by `Sololololo` (2026-05-21) — title and a Chinese-language comment were retrieved, but I have **not** verified a clean 10–40 word English quote, so it is not used above. The gist (search mixes preference memories with no ranking, weighting or dedup, which "只会对Agent造成混乱" — only creates confusion for the Agent) is corroborated by maintainer `kartik-mem0` in that thread: *"The real gap is in search: `score_and_rank` … combines semantic, BM25, and entity boost only, no recency term, so old and new conflicting facts tie on relevance."* — that maintainer quote **was** retrieved in full.
- `basicmachines-co/basic-memory#1275` — *"file_path lookups are byte-wise, so NFC/NFD filename variants create duplicate entities (macOS + Syncthing)"* (`SloNN`, 2026-08-18, open, 4 comments) — **snippet only.**
- `microsoft/autogen`, `run-llama/llama_index`, `chroma-core/chroma`, `getzep/zep` — searched; **no first-hand end-user complaint matching failure modes 1–7 surfaced** in the queries run. `llama_index#23090` ("Memory truncates highest-priority memory blocks first", `Harsh23Kashyap`, 2026-09-17) was retrieved in full but is a pure logic-inversion bug with no user-impact narrative, so it is not presented as an anti-pattern finding.

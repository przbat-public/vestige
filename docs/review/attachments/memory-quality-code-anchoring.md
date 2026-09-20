# Anchoring memory to code: what coding agents and dev tools store, and what they refuse to store

**Date:** 2026-09-20 · **Scope:** how established coding agents and developer tools anchor knowledge to
source code, what they tell users/models *not* to persist, and what that implies for Vestige's
`smart_ingest` / `codebase` tools.

**Problem under investigation.** A memory saved as *"the bug is in `src/search/query.rs:412`"* is
unreadable a week later and probably wrong, because the line moved. This report collects primary
sources on how other tools avoid or accept that failure.

**Source labelling used throughout:**

- **[OFFICIAL]** — vendor documentation or a specification.
- **[REPO]** — a file in the vendor's own repository (primary artifact).
- **[RESEARCH]** — a published technical report with a method section.
- **[INFERENCE]** — my reasoning, not documented by any source.
- **[NOT DOCUMENTED]** — I looked and could not find a source; stated explicitly.

> **Method note.** Every claim below carries a URL I actually loaded; block quotes are verbatim from
> the fetched text. Pages that failed to load, that loaded only partially, or whose URL in my original
> brief turned out to be wrong are listed in the appendix — nothing is cited from a page I did not
> retrieve, and claims that rest only on my reading are marked **[INFERENCE]**. Where a vendor's docs
> are silent, I say so rather than substituting a plausible-sounding practice.

---

## 0. The headline findings

1. **Every tool surveyed keeps durable knowledge in the repository, not in a private memory store.** The
   documented pattern is consistent even though the emphasis differs: Anthropic keeps *"instructions
   and rules"* in `CLAUDE.md` and scopes auto memory to *"context Claude can't derive from the code"*;
   Cursor's rules are *"version-controlled"* in `.cursor/rules` and must not duplicate the codebase;
   Cognition/Windsurf states outright that durable knowledge belongs in a Rule or `AGENTS.md`
   *"rather than relying on auto-generated Memories"*; OpenAI/Codex defines the layered, repo-rooted
   `AGENTS.md` chain; and the cross-vendor `agents.md` standard exists precisely to be *"a README for
   agents"* in the repo (§1).
2. **The strongest documented answer to "a fact that only makes sense with code" is GitHub's
   read-time citation verification.** Store the fact *"with citations: references to specific code
   locations that support each fact"*, then **verify the citations in real time against the current
   branch before using the memory**; if a citation is invalid, *"store a corrected version"*; if it
   checks out, re-store to refresh the timestamp. GitHub's stated principle: *"Information retrieval is
   an asymmetrical problem: It's hard to solve, but easy to verify."* They **explicitly rejected**
   offline curation (dedup, conflict resolution, expiry) as too complex and costly
   ([OFFICIAL engineering blog](https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/) ·
   [docs](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)).
3. **Storing raw content is explicitly discouraged; storing a reference is explicitly encouraged.**
   Cursor: *"Reference files instead of copying their contents—this keeps rules short and prevents
   them from becoming stale as code changes"* ([OFFICIAL](https://cursor.com/docs/rules)).
   Anthropic: agents *"maintain lightweight identifiers (file paths, stored queries, web links, etc.)
   and use these references to dynamically load data into context at runtime"*
   ([OFFICIAL](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)).
4. **"Information that changes frequently" is on Anthropic's explicit exclude list for memory**
   ([OFFICIAL](https://code.claude.com/docs/en/best-practices)). A line number is information that
   changes frequently by construction.
5. **GitHub's own docs state the failure mode verbatim:** a file URL with a branch name is unstable
   because *"The version of a file at the head of branch can change as new commits are made, so if
   you were to copy the normal URL, the file contents might not be the same when someone looks at it
   later"* ([OFFICIAL](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)).
6. **A vendor that ships automatic memory tells users not to rely on it for durable knowledge.**
   Cognition/Windsurf: *"For knowledge you want Cascade to reliably reuse, write it as a Rule or add
   it to `AGENTS.md` in your repo rather than relying on auto-generated Memories"*
   ([OFFICIAL](https://docs.devin.ai/desktop/cascade/memories)).
7. **Outside the agent world, the standard is: pin the version for reproducibility, name the thing
   for currency — and stop there.** Zenodo: *"You should normally always use the DOI for the specific
   version of your record in citations. This is to ensure that other researchers can access the exact
   research artefact you used"* ([OFFICIAL](https://zenodo.org/help/versioning)). FORCE11's software
   citation principles specify down to *"version numbers, revision numbers"* and, when a reviewer
   asked for finer granularity *within* a code base, the authors **declined**: *"we do not believe
   this rises to the level of what should be specified…"* (§8.2).
8. **Reference rot is measured, and the mechanism matches the reported bug.** *"It is precisely
   because their domains are fixed that their deep links are fragile"* — 25% of deep links in a
   4.5M-article NYT sample were completely inaccessible, 13% of the intact ones had drifted
   ([primary, CJR 2021](https://www.cjr.org/analysis/linkrot-content-drift-new-york-times.php)).
9. **The term "line-number rot" is not a term of art in any vendor documentation I could find**
   **[NOT DOCUMENTED]**. The academic terms are *link rot* and *reference rot*
   ([2014 *Perma*, 127 Harv. L. Rev. F. 176](https://harvardlawreview.org/forum/vol-127/perma-scoping-and-addressing-the-problem-of-link-and-reference-rot-in-legal-citations/)).
   Use those, not the colloquial label, in anything citable.

---

## 1. What the tools tell users and models to store — and not to store

### 1.1 Claude Code — two systems, one of which is told to skip code

Claude Code documents **two** memory mechanisms with different rules
([OFFICIAL, "How Claude remembers your project"](https://code.claude.com/docs/en/memory)):

| | `CLAUDE.md` files | Auto memory |
|---|---|---|
| Who writes it | You | Claude |
| What it contains | Instructions and rules | Learnings and patterns |
| Scope | Project, user, or org | Per repository, shared across worktrees |
| Use for | Coding standards, workflows, project architecture | *"Your preferences, corrections you give Claude, project context Claude can't derive from the code"* |

The auto-memory taxonomy is four types, recorded as a `type` field in each memory file's frontmatter
([OFFICIAL](https://code.claude.com/docs/en/memory)):

- `user` — role, expertise, working preferences
- `feedback` — corrections you give Claude and approaches you confirm
- `project` — *"ongoing work, deadlines, and decisions that Claude can't derive from the code or git history"*
- `reference` — *"where to find information outside the project, such as an issue tracker or dashboard"*

**The explicit refusal is the important part** — verbatim, same page:

> *"Claude skips anything it can derive from the codebase, such as architecture, file paths, or
> debugging fixes. It also skips anything your CLAUDE.md files already say."*

Note that **"file paths" and "debugging fixes"** are named as *skippable* — i.e. Claude Code's auto
memory is documented to refuse precisely the two things Vestige's mandatory `BUG_FIX` save gate
requires the model to write (see §7.1).

The best-practices page turns that into a two-column include/exclude table
([OFFICIAL](https://code.claude.com/docs/en/best-practices)):

| ✅ Include | ❌ Exclude |
|---|---|
| Bash commands Claude can't guess | **Anything Claude can figure out by reading code** |
| Code style rules that differ from defaults | Standard language conventions Claude already knows |
| Testing instructions and preferred test runners | **Detailed API documentation (link to docs instead)** |
| Repository etiquette (branch naming, PR conventions) | **Information that changes frequently** |
| **Architectural decisions specific to your project** | Long explanations or tutorials |
| Developer environment quirks (required env vars) | **File-by-file descriptions of the codebase** |
| Common gotchas or non-obvious behaviors | Self-evident practices like "write clean code" |

Plus the pruning heuristic: *"For each line, ask: 'Would removing this cause Claude to make
mistakes?' If not, cut it."*

There is also a **mechanical pruning tool**: `/doctor` *"proposes trims for a checked-in CLAUDE.md:
it cuts content Claude can derive from the codebase, such as directory layouts, dependency lists,
and architecture overviews, and keeps pitfalls, rationale, and conventions that differ from tool
defaults"* ([OFFICIAL](https://code.claude.com/docs/en/memory)). Read that as an explicit priority
ordering: **derivable → cut; rationale and pitfalls → keep.**

Storage facts that matter for scoping: auto memory lives at
`~/.claude/projects/<project>/memory/`, where *"The `<project>` path is derived from the git
repository, so all worktrees and subdirectories within the same repo share one auto memory
directory"*, and *"Auto memory is machine-local. Files are not shared across machines or cloud
environments."* The index is `MEMORY.md`, of which *"The first 200 lines … or the first 25KB,
whichever comes first, are loaded at the start of every conversation"*
([OFFICIAL](https://code.claude.com/docs/en/memory)).

### 1.2 Cursor — reference, don't copy; encode domain knowledge, not code

Cursor's Rules page is the most quotable anti-duplication guidance of any vendor
([OFFICIAL](https://cursor.com/docs/rules)):

> *"Reference files instead of copying their contents—this keeps rules short and prevents them from
> becoming stale as code changes."*

Under **"What to avoid in rules"** ([OFFICIAL](https://cursor.com/docs/rules)):

- *"**Copying entire style guides**: Use a linter instead. Agent already knows common style conventions."*
- *"**Documenting every possible command**: Agent knows common tools like npm, git, and pytest."*
- *"**Adding instructions for edge cases that rarely apply**: Keep rules focused on patterns you use frequently."*
- *"**Duplicating what's already in your codebase**: Point to canonical examples instead of copying code."*

What rules *are* for: *"Encode domain-specific knowledge about your codebase / Automate
project-specific workflows or templates / Standardize style or architecture decisions."* Sizing:
*"Keep rules under 500 lines."* Adoption: *"Start simple. Add rules only when you notice Agent
making the same mistake repeatedly."*

**Cursor Memories — a partial negative finding.** Cursor announced Memories in the 1.0 changelog:
*"With Memories, Cursor can remember facts from conversations and reference them in the future.
Memories are stored per project on an individual level, and can be managed from Settings … We're
rolling out Memories as a beta feature. To enable from Settings → Rules"*
([OFFICIAL, Cursor 1.0 changelog, 2025-06-04](https://cursor.com/changelog/1-0)).

**[NOT DOCUMENTED — verified by absence, 2026-09-20]** I could not find a current Cursor Memories
concept page. A fetch of Cursor's docs index (`https://cursor.com/llms.txt`, which lists every
documented page) returns no page matching "memor", and `https://cursor.com/docs.md` contains zero
occurrences of the string `memor` (verified by downloading the page: HTTP 200, 36 090 bytes, `grep -ic
memor` → `0`). Two candidate URLs (`/docs/context/memories.md`, `/docs/agent/memories.md`) return
404. **Conclusion: Cursor's *current* published guidance for Memories — scope rules, what to store,
retention — could not be verified from official docs. Only the 2025 changelog description and the
Rules page's general anti-duplication guidance are citable.** Do not attribute detailed Memories
semantics to Cursor without a fresh source.

**This absence is itself a finding, and it has an explanation:** a Cursor staff member states that
*"The Memories feature was intentionally removed starting in version 2.1.x, so the UI to manage them is
no longer available. Even though this feature was removed from Cursor, it still works, just without a
UI"* ([maintainer statement, Cursor forum, 2026-01-07](https://forum.cursor.com/t/cant-clear-memories/148254)).
The docs went away; the memory store did not. See §5.4 for the user reports and the official
workaround (roll back to 2.0.77 to reach the deletion UI).

### 1.3 Cline Memory Bank — a hand-curated, in-repo alternative to automatic memory

Cline's Memory Bank is a *documentation methodology*, explicitly not a database
([OFFICIAL](https://docs.cline.bot/best-practices/memory-bank)):

```
memory-bank/
├── projectbrief.md      # Foundation document
├── productContext.md    # Why this project exists
├── activeContext.md     # Current work focus
├── systemPatterns.md    # Architecture & patterns
├── techContext.md       # Tech stack & setup
└── progress.md          # Status & milestones
```

| File | Documented purpose |
|---|---|
| `projectbrief.md` | *"Foundation document with core requirements and goals"* |
| `productContext.md` | *"Why the project exists, problems it solves, UX goals"* |
| `activeContext.md` | *"Current focus, recent changes, next steps (updates most frequently)"* |
| `systemPatterns.md` | *"Architecture, design patterns, component relationships"* |
| `techContext.md` | *"Tech stack, setup, constraints, dependencies"* |
| `progress.md` | *"What works, what's left, known issues"* |

Three design decisions are directly relevant:

1. **The files live in the repository.** *"Memory Bank files are regular markdown files in your
   project that both you and Cline can access."* They are therefore versioned with the code and
   reviewed like code.
2. **Updates are explicit and human-triggered,** not automatic: the documented commands are
   `"initialize memory bank"`, `"update memory bank"`, `"follow your custom instructions"`. The
   instruction text says *"When user requests with **update memory bank** (MUST review ALL files)"*.
3. **Frequent change is a feature, not a bug, of the *content*:** the FAQ contrasts Memory Bank with a
   README — *"It includes files for active context and progress tracking that change frequently,
   unlike a typical README"* ([OFFICIAL](https://docs.cline.bot/best-practices/memory-bank)).

The framing sentence is the design rationale: *"I am Cline, an expert software engineer with a unique
characteristic: my memory resets completely between sessions. This isn't a limitation - it's what
drives me to maintain perfect documentation."*

**[INFERENCE]** Memory Bank solves the same problem as an automatic memory store but chooses the
opposite trade-off: no retrieval, no embeddings, no staleness policy — instead, a small fixed set of
files that a human re-reads and re-writes at known checkpoints. Its weakness is that it is entirely
manual; its strength is that every entry is curated by someone who knows the code is still there.

### 1.4 GitHub Copilot — instructions vs prompt files, and the citation model

Copilot supports three kinds of repository instructions
([OFFICIAL](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/add-custom-instructions/add-repository-instructions)):

- **Repository-wide** — `.github/copilot-instructions.md`, applies to all requests in the repo.
- **Path-specific** — one or more `NAME.instructions.md` under `.github/instructions/`, gated by an
  `applyTo` glob in frontmatter. *"If the path you specify matches a file that Copilot is working on,
  and a repository-wide custom instructions file also exists, then the instructions from both files
  are used."*
- **Agent instructions** — `AGENTS.md` (nearest file in the tree wins), or a root `CLAUDE.md` / `GEMINI.md`.

**The instructions/prompt-files split** is the clearest documented boundary between *persistent
rules* and *reusable tasks* ([OFFICIAL cheat sheet](https://docs.github.com/en/copilot/reference/customization-cheat-sheet)):

| | Custom instructions | Prompt files |
|---|---|---|
| What it is | *"Always-on context that automatically applies to every interaction within its defined scope"* | *"Reusable, standalone prompt template with input variables"* |
| Filename | `.github/copilot-instructions.md`, `.github/instructions/*.instructions.md`, `AGENTS.md` | `.github/prompts/*.prompt.md` |
| How triggered | *"Automatic"* | *"Manual: reference directly in chat or use the prompt file picker"* |
| Best for | *"Standards, guidelines, or expectations that apply broadly across a context"* | *"Focused single tasks you run once with different inputs each time"* |

VS Code states the same split from the other direction: *"Unlike custom instructions that are applied
automatically, you invoke prompt files manually in chat"*
([OFFICIAL](https://code.visualstudio.com/docs/agent-customization/prompt-files)).

The size rationale is explicit: *"Because the instructions are sent with every chat message, they
should be broadly applicable to most requests you will make in the context of the repository"*
([OFFICIAL](https://docs.github.com/en/copilot/concepts/prompting/response-customization)).

**A caution worth carrying over:** GitHub's docs warn that some instruction styles backfire in large
repositories, listing *"Requests to refer to external resources when formulating a response"* as a
pattern that *"may cause problems"* and giving this as an example that *"may not have the intended
results"*: *"Always conform to the coding styles defined in styleguide.md in repo my-org/my-repo when
generating code"* ([OFFICIAL](https://docs.github.com/en/copilot/concepts/prompting/response-customization)).
**[INFERENCE]** The lesson is not "never point at files" — it is that a *pointer to a large external
document* is a weak instruction, whereas a *pointer to a canonical example* (as Cursor recommends) is
a strong one. A file path alone is not an anchor; a file path plus a symbol is.

### 1.5 Windsurf / Devin Desktop (Cascade) — the vendor says don't trust auto-memory for durable facts

This is the most direct statement from any vendor that automatic memory is the wrong home for
knowledge you care about ([OFFICIAL](https://docs.devin.ai/desktop/cascade/memories)):

> *"**Recommendation:** For knowledge you want Cascade to reliably reuse, write it as a Rule or add
> it to `AGENTS.md` in your repo rather than relying on auto-generated Memories. Rules are
> version-controlled, shareable with your team, and give you explicit control over activation."*

and, in a callout on the same page:

> *"Auto-generated memories live only on your machine. If you want Cascade to remember something
> durably — and share it with your team — ask Cascade to write it to a Rule in `.devin/rules/` … or to
> your repo's `AGENTS.md` instead."*

Documented mechanics ([OFFICIAL](https://docs.devin.ai/desktop/cascade/memories)):

- *"Cascade's autogenerated memories are associated with the workspace they were created in and are
  stored locally in `~/.codeium/windsurf/memories/`. … Memories generated in one workspace are not
  available in another, and they are not committed to your repository."*
- Two mechanisms only: **Memories** ("automatically generated by Cascade") and **Rules**
  ("manually defined by the user").
- Rule activation modes carry an explicit **context cost** column: `always_on` = *"Every message"*;
  `model_decision` = *"Description always; full content on demand"*; `glob` = *"Only when matching
  files are touched"*; `manual` = *"Only when @mentioned"*.
- Hard limits: global rules file 6 000 characters, workspace rule files 12 000 characters each.
- **Auto-memory applies only to the legacy agent:** *"Memories apply to the legacy Cascade agent
  only. The Devin Local agent — the default agent for new tabs — does not persist memories."*

That last point is significant: the vendor's *newer* agent does not persist automatic memories at
all, and directs users to migrate the ones they rely on to skills.

### 1.6 Devin — Knowledge as curated, trigger-gated, repo-pinnable snippets

Devin's Knowledge feature is a vendor memory store with unusually explicit content guidance
([OFFICIAL](https://docs.devin.ai/product-guides/knowledge)):

- **Content shape:** *"**Content** should be a handful of sentences with relevant information."*
- **Retrieval is trigger-gated:** *"Your **Trigger Description** will help Devin recall relevant
  Knowledge at the right times … Devin will retrieve a Knowledge item when its current work is
  related to the specified triggers, and all Knowledge requires a trigger description."*
- **Repo scoping is a first-class field:** *"Pinning to **no repo**: The Knowledge is only retrieved
  when Devin decides it's relevant to your current context. Pinning to **a specific repo**: The
  Knowledge is always used whenever Devin is working in that specific repo. Pinning to **all repos**:
  The Knowledge automatically applies to every repo."*
- **What belongs:** *"We recommend including the aspects of your prompts or playbooks you find
  yourself repeating regularly. Examples include common bugs and their associated solutions, code
  conformance practices, deployment workflows, testing workflows…"*
- **Atomicity and freshness:** *"Create specific Knowledge that is targeted at one workflow or
  action. Devin will read the entire Knowledge contents, so keep it all relevant and up-to-date! … 
  Split up your Knowledge into smaller ones where possible."*
- **Suggestions are edited before saving:** *"Devin will automatically suggest Knowledge to remember
  based on your feedback in chat. **Edit the suggested Knowledge before saving**, or dismiss the
  Knowledge if it's not helpful."*

**[INFERENCE]** Devin's design accepts automatic *proposals* but keeps a human in the loop for
*commits*. That is a middle path between Cline's fully manual Memory Bank and a fully automatic
store, and it is the closest published analogue to a well-governed `smart_ingest`.

### 1.7 Aider — one small, read-only conventions file

Aider has no memory feature; it has a conventions file
([OFFICIAL](https://aider.chat/docs/usage/conventions.html)):

> *"The easiest way to do that with aider is to simply create a small markdown file and include it in
> the chat."*

and

> *"It's best to load the conventions file with `/read CONVENTIONS.md` or `aider --read
> CONVENTIONS.md`. This way it is marked as read-only, and cached if prompt caching is enabled."*

**[INFERENCE]** `--read` matters more than it looks: marking the file read-only means the model
cannot *rewrite its own rules* mid-task. A memory server with full write access has no equivalent
guard, which is one reason a memory entry and an instruction file should not share a namespace.

### 1.8 Codex / `AGENTS.md` — a layered, project-rooted instruction chain

OpenAI's Codex docs describe a three-level instruction chain
([OFFICIAL](https://learn.chatgpt.com/docs/agent-configuration/agents-md.md)):

1. **Global scope:** `~/.codex/AGENTS.override.md` if present, else `~/.codex/AGENTS.md` — *"Create
   persistent defaults in your Codex home directory so every repository inherits your working
   agreements"* (e.g. *"Prefer `pnpm` when installing dependencies"*).
2. **Project scope:** walk from the project root (usually the Git root) down to the CWD, taking at
   most one file per directory (`AGENTS.override.md` → `AGENTS.md` → configured fallbacks).
3. **Merge:** *"Codex concatenates files from the root down … Files closer to your current directory
   override earlier guidance because they appear later in the combined prompt."*

With a hard size cap: *"Codex skips empty files and stops adding files once the combined size reaches
the limit defined by `project_doc_max_bytes` (32 KiB by default)."* And no caching to invalidate:
*"Codex rebuilds the instruction chain on every run (and at the start of each TUI session), so there
is no cache to clear manually."*

The cross-vendor standard fills in the conventions ([OFFICIAL](https://agents.md/)):
*"Think of AGENTS.md as a **README for agents**"*; *"The closest AGENTS.md to the edited file wins;
explicit user chat prompts override everything"*; *"Place another AGENTS.md inside each package.
Agents automatically read the nearest file in the directory tree … at time of writing the main OpenAI
repo has 88 AGENTS.md files."* It is stewarded by the Agentic AI Foundation under the Linux
Foundation.

**The Codex repo's own `AGENTS.md` is itself a primary artifact** and contains the single best
example I found of a real code-anchored instruction
([REPO, `openai/codex` AGENTS.md](https://github.com/openai/codex/blob/main/AGENTS.md)):

> *"Discourage both `#[async_trait]` and `#[allow(async_fn_in_trait)]` in Rust traits. — Prefer native
> RPITIT trait methods with explicit `Send` bounds on the returned future, as in `3c7f013f9735` /
> `#16630`."*

That is the pattern in one line: **a rule stated in prose, anchored to a commit SHA and a PR number,
with no file path and no line number.** The commit and the PR are immutable; the prose is
self-contained; and a reader can `git show 3c7f013f9735` to find the code without the memory needing
to know where it lives today.

The same file also encodes a *context-budget* discipline for what the agent may put in front of the
model ([REPO](https://github.com/openai/codex/blob/main/AGENTS.md)):

> *"Model visible context … 1. No history rewrite … 3. No unbounded items - everything injected in
> the model context must have a bounded size and a hard cap. 4. No items larger than 10K tokens. 5.
> Highlight new individual items that can cross >1k tokens as P0."*

---

## 2. How each tool handles a fact that only makes sense with code

| Tool | Documented mechanism for a code-adjacent fact |
|---|---|
| **Claude Code** | Push it into the repo as `CLAUDE.md` / `.claude/rules/*.md` with a `paths:` glob, or import the file with `@path`. Auto memory is explicitly told to skip *"architecture, file paths, or debugging fixes"*. ([OFFICIAL](https://code.claude.com/docs/en/memory)) |
| **Cursor** | *"Reference files instead of copying their contents"*; *"Point to canonical examples instead of copying code"*. Rules carry `globs` so they attach when a matching file enters context. ([OFFICIAL](https://cursor.com/docs/rules)) |
| **Cline** | The fact lives in a repo markdown file next to the code; the model re-reads all of them at the start of every task. ([OFFICIAL](https://docs.cline.bot/best-practices/memory-bank)) |
| **GitHub Copilot Memory** | **Store the fact *with citations pointing to the supporting code*; re-validate the citations against the current branch on retrieval; use only validated facts.** ([OFFICIAL](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)) |
| **Windsurf / Devin Desktop** | Auto-memory is workspace-local and explicitly not for durable knowledge; write a Rule (globbed) or `AGENTS.md` instead. ([OFFICIAL](https://docs.devin.ai/desktop/cascade/memories)) |
| **Devin Knowledge** | Curated short prose + trigger description + repo pinning; *"a handful of sentences"*. ([OFFICIAL](https://docs.devin.ai/product-guides/knowledge)) |
| **Aider** | `CONVENTIONS.md`, loaded read-only. ([OFFICIAL](https://aider.chat/docs/usage/conventions.html)) |
| **Codex / AGENTS.md** | The rule goes in the nearest `AGENTS.md`; when it must cite code, cite a **commit SHA + PR number**. ([OFFICIAL](https://learn.chatgpt.com/docs/agent-configuration/agents-md.md), [REPO](https://github.com/openai/codex/blob/main/AGENTS.md)) |

### 2.1 The Copilot Memory citation model in detail

This is the only *documented* automatic memory store I found that solves the staleness problem
structurally rather than by hoping. Verbatim
([OFFICIAL](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)):

> *"Repository-level facts are stored with citations pointing to the code that supports them. When
> Copilot finds a fact relevant to its current work, it checks those citations against the current
> branch to confirm the information is still accurate. **Only validated facts are used.**"*

Three further design choices from the same page are worth copying or deliberately rejecting:

1. **Scope is enforced, not advisory:** *"those facts can only be used in operations on the same
   repository. This keeps what Copilot learns about a repository scoped to that repository."*
2. **Staleness has a clock:** *"To prevent stale information from lingering, any stored fact or
   preference that goes unused is automatically deleted after 28 days. The 28-day timer may reset
   whenever Copilot successfully validates and uses an entry."*
3. **Facts learned from abandoned work are still gated by validation:** *"Facts can also be captured
   from pull requests that were closed without merging. In those cases, the validation step ensures
   that Copilot's behavior is unaffected unless the current codebase still substantiates the
   information."*

Two kinds of memory are treated differently, and this is the key distinction for Vestige: repository
facts get **code citations re-checked against the branch**, while user preferences get *"citations
that may include direct user quotes"* and are confirmed by *"best judgment"* rather than by machine
validation. **Code-anchored knowledge needs a verifier; preference knowledge does not.**

GitHub's engineering blog on the same system adds the design rationale and the rejected alternative
([OFFICIAL](https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/)) —
*"Information retrieval is an asymmetrical problem: It's hard to solve, but easy to verify"* — and
describes the correction path: *"If the code contradicts the memory, or if the citations are invalid
(e.g. point to nonexistent locations), the agent is encouraged to store a corrected version of the
memory reflecting the new evidence."* Their evaluation deliberately seeded *"adversarial memories–facts
that contradicted the codebase–with citations pointing to irrelevant or nonexistent code locations"*
and over-represented *"memories from branches that were abandoned or closed without merging"*; the
reported result was that *"The memory pool self-healed as agents stored corrected versions based on
their observations."* Full treatment and a granularity caveat in §5.1.

**[CAVEAT]** Neither the blog nor the docs state what a citation *is* — the phrase "line numbers" and
the word "symbol" do not appear in the blog. Only *"references to specific code locations"* is
documented. The anchor format is a black box; the verification loop is the documented part.

---

## 3. Documented practice for making a code-adjacent note survive refactoring

### 3.1 Pin a revision, or do not call the link permanent

GitHub's permanent-links documentation states both the problem and the fix
([OFFICIAL](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)):

> *"The version of a file at the head of branch can change as new commits are made, so if you were to
> copy the normal URL, the file contents might not be the same when someone looks at it later."*

> *"For a permanent link to the specific version of a file that you see, instead of using a branch
> name in the URL (i.e. the `main` part in the example above), put a commit ID. This will permanently
> link to the exact version of the file in that commit."*

Note the precise guarantee: pinning a **commit ID** makes the *file contents* immutable. It does not
make the *line number* meaningful — it makes it meaningful *only relative to that commit*. The
snippet-permalink page documents the line-range form (`?plain=1#L14`) as scoped to *"a specific
version of a file or pull request"* and notes a further constraint: *"This type of permanent link
will render as a code snippet only in the repository it originated in. In other repositories, the
permalink code snippet will render as a URL. This does not work in Markdown files, only in comments."*
([OFFICIAL](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-a-permanent-link-to-a-code-snippet))

**[INFERENCE]** A commit-pinned permalink is *readable* but not *checkable*: it tells a future reader
what the code looked like, never whether it still looks that way. Sitting inside a memory store, it
is a frozen quote with better provenance — which is strictly better than a bare `file:line`, and
still strictly worse than Copilot's re-validation, because nothing ever notices when it goes out of
date.

### 3.2 Store the identifier, fetch the content

Anthropic's context-engineering post gives the general principle that a memory store should follow
([OFFICIAL, 2025-09-29](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)):

> *"Rather than pre-processing all relevant data up front, agents built with the 'just in time'
> approach maintain lightweight identifiers (file paths, stored queries, web links, etc.) and use
> these references to dynamically load data into context at runtime using tools."*

> *"Claude Code is an agent that employs this hybrid model: CLAUDE.md files are naively dropped into
> context up front, while primitives like glob and grep allow it to navigate its environment and
> retrieve files just-in-time, **effectively bypassing the issues of stale indexing and complex
> syntax trees**."*

And the design rule that governs how much to store at all: *"good context engineering means finding
the smallest possible set of high-signal tokens that maximize the likelihood of some desired
outcome"*, with context treated as *"a finite resource with diminishing marginal returns."*

That same post is where Anthropic endorses the finding that long context degrades retrieval:

> *"Studies on needle-in-a-haystack style benchmarking have uncovered the concept of context rot: as
> the number of tokens in the context window increases, the model's ability to accurately recall
> information from that context decreases."*

The underlying study is Chroma's technical report
([RESEARCH, 2025-07-14](https://www.trychroma.com/research/context-rot)):

> *"We observe that model performance varies significantly as input length changes, even on simple
> tasks. … models do not use their context uniformly; instead, their performance grows increasingly
> unreliable as input length grows."*

Two of its findings have direct consequences for what a memory system should emit:

- **Distractors are worse than irrelevant text:** *"Even a single distractor reduces performance
  relative to the baseline (needle only), and adding four distractors compounds this degradation
  further."* A memory that is *almost* right about a file is a distractor, not noise.
- **Presentation matters as much as presence:** *"Whether relevant information is present in a
  model's context is not all that matters; what matters more is how that information is presented."*

**[INFERENCE]** The combination is an argument for *small, verifiable* code references over *long,
frozen* ones: a paraphrased snippet is both a token cost and a potential distractor, whereas a symbol
name is one line and can be checked.

### 3.3 Symbols, not positions — evidence status

**[NOT DOCUMENTED as a general rule by the vendors surveyed.]** None of the official docs I fetched
states "refer to a symbol rather than a line number" in those words. What *is* documented is the
equivalent practice for instruction files:

- Cursor: *"Reference files instead of copying their contents"* and *"Point to canonical examples
  instead of copying code"* ([OFFICIAL](https://cursor.com/docs/rules)).
- Copilot: path-scoped instructions attach by **glob**, and the trigger is *"If the path you specify
  matches a file that Copilot is working on"* — i.e. attachment is by path pattern and file identity,
  never by line ([OFFICIAL](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/add-custom-instructions/add-repository-instructions)).
- Claude Code: path-scoped rules *"trigger when Claude reads files matching the pattern"*, with glob
  budgets and escaped-bracket edge cases documented at length
  ([OFFICIAL](https://code.claude.com/docs/en/memory)).
- Codex, in practice: a commit SHA + PR number for the example, and prose for the rule
  ([REPO](https://github.com/openai/codex/blob/main/AGENTS.md)).

**The strongest evidence *against* line numbers is GitHub's own framing of its file URLs:** a
branch-relative file URL is explicitly described as unstable, and its fix is a commit pin — never a
line pin. Line-range links exist, but the documentation scopes them to *"a specific version of a
file"* ([OFFICIAL](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)).

**Tooling evidence: names are addressed where positions are avoided.** Three primary sources make the
name/position distinction concrete without ever stating it as a rule:

- **`git log -L` offers a function name *as an alternative to a line range*** — the single clearest
  first-party expression of the idea. The option is documented as
  *"`-L:<funcname>:<file>`: Trace the evolution of the line range given by `<start>,<end>`, or by the
  function name regex _<funcname>_, within the _<file>_"*, with the worked example
  *"`git log -L '/int main/',/^}/:main.c`: Shows how the function `main()` in the file `main.c`
  evolved over time"* ([OFFICIAL](https://git-scm.com/docs/git-log)). History can be traced by
  *identity* or by *position*; git ships both and documents the name form.
- **`git log -S` is content-addressed, not position-addressed:** *"Look for differences that change
  the number of occurrences of the specified `<string>` … It is useful when you're looking for an
  exact block of code (like a struct), and want to know the history of that block since it first came
  into being"* ([OFFICIAL](https://git-scm.com/docs/git-log)).
- **Rust RFC 1946 documents path-based URLs as fragile and replaces them with item paths:** *"Currently,
  these links are plain Markdown links, and the URLs are the (relative) paths of the items' pages in
  the rendered Rustdoc output. **This is sadly very fragile in several ways**"* and *"Should Rustdoc's
  file name scheme ever change (it has change before…), all manually created links need to be
  updated."* The accepted fix is name-based: *"Add a notation how to create relative links in
  documentation comments (**based on Rust item paths**) and extend Rustdoc to automatically turn this
  into working links"*
  ([OFFICIAL](https://rust-lang.github.io/rfcs/1946-intra-rustdoc-links.html),
  [resulting docs](https://doc.rust-lang.org/rustdoc/write-documentation/linking-to-items-by-name.html)).

**The closest thing to a direct statement about line-keyed references breaking** comes from
Sourcegraph's code-navigation docs, which explain why a *position-keyed index* returns stale results:

> *"You may occasionally see results from search-based code navigation even when you have uploaded an
> index. This can happen in the following scenarios: - **The line containing the symbol was created or
> edited between the nearest indexed commit and the commit being browsed.**"*

([OFFICIAL/archived](https://raw.githubusercontent.com/sourcegraph/sourcegraph-public-snapshot/56bf5f946338e0e5a9b5c542680d952d313aa6ff/doc/code_navigation/explanations/precise_code_navigation.md))

That is exactly the failure mode the user reported, stated by a vendor about its own index: the index
keys on a line, the line moves, the answer is silently wrong. Sourcegraph's fix is also instructive —
*"Cross-repository code navigation works out-of-the-box when both the dependent repository and the
dependency repository have indexes _at the correct commits or versions_"* — i.e. **bind the index to a
revision, and you know when it is stale.**

**[INFERENCE]** "Quote the smallest unit that survives refactoring" is a reasonable engineering maxim
and it follows from the sources above, but **I could not find it stated in those words in any
official or first-party source**. Treat it as a derived heuristic, not as a cited practice. The
sourced version of it is narrower and safer: *store the identifier, not the content.*

### 3.4 Decisions outlive locations: the ADR pattern

Claude Code's best-practices table explicitly keeps *"Architectural decisions specific to your
project"* and cuts *"File-by-file descriptions of the codebase"*
([OFFICIAL](https://code.claude.com/docs/en/best-practices)). Cursor's rules are for *"Encoding
domain-specific knowledge"* and *"architecture decisions"*
([OFFICIAL](https://cursor.com/docs/rules)). Copilot stores *"architectural decisions"* as
repository-level facts, subject to citation re-validation
([OFFICIAL](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)). Devin lists *"code
conformance practices"* among things worth storing
([OFFICIAL](https://docs.devin.ai/product-guides/knowledge)).

**The pattern across all four: a decision is keepable; a location is not.** A decision has no line
number to rot.

The provenance for that pattern is Michael Nygard's original ADR post
([OFFICIAL/primary, 2011-11-15](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)),
which defines the four headings and the supersede-don't-delete rule:

> *"**Context** This section describes the forces at play, including technological, political,
> social, and project local. These forces are probably in tension, and should be called out as such.
> The language in this section is value-neutral."*

> *"**Decision** This section describes our response to these forces. It is stated in full sentences,
> with active voice. 'We will …'"*

> *"**Status** A decision may be 'proposed' if the project stakeholders haven't agreed with it yet, or
> 'accepted' once it is agreed. If a later ADR changes or reverses a decision, it may be marked as
> 'deprecated' or 'superseded' with a reference to its replacement."*

> *"**Consequences** This section describes the resulting context, after applying the decision. All
> consequences should be listed here, not just the 'positive' ones."*

> *"If a decision is reversed, we will keep the old one around, but mark it as superseded. (It's still
> relevant to know that it _was_ the decision, but is _no longer_ the decision.)"*

> *"ADRs will be numbered sequentially and monotonically. Numbers will not be reused."*

And the motivating sentence, which is the same problem this report is about: *"One of the hardest
things to track during the life of a project is the motivation behind certain decisions."* Note what
the template does **not** contain: no file paths, no line numbers, no code. The MADR 4.0 template
adds front-matter status such as `superseded by ADR-0123` and a *"Considered Options"* section —
*"We think that the considered options with their pros and cons are crucial to understand the reasons
for choosing a particular design"*
([OFFICIAL](https://adr.github.io/adr-templates/), [template](https://raw.githubusercontent.com/adr/madr/4.0.0/template/adr-template.md)).
The `adr.github.io` project describes the unit as *"An Architectural Decision Record (ADR) captures a
single AD and its rationale"* ([OFFICIAL](https://adr.github.io/)).

**[INFERENCE]** The ADR form is the proof that a code-adjacent note can be durable: it records the
*decision* and its *consequences* — both of which survive any refactor — and it records the code's
identity only through the decision it forced. **A memory system that stores decisions this way has
almost nothing left to anchor to a line number.**

---

## 4. Why project-scoped files instead of one global memory store

Every tool surveyed scopes durable knowledge to a repository, and the reasons given are consistent.

**1. Instructions compete for a shared context budget, so scope limits what is loaded.**
Claude Code loads `CLAUDE.md` from the working directory and every directory above it at launch, and
subdirectory files *"load on demand when Claude reads files in those directories"*; the docs target
*"under 200 lines per CLAUDE.md file"* because *"Longer files consume more context and reduce
adherence"*, and offer `claudeMdExcludes` for *"other teams' CLAUDE.md files"* in a monorepo
([OFFICIAL](https://code.claude.com/docs/en/memory)). Copilot's path-specific instructions exist *"to
avoid overloading your repository-wide instructions with information that only applies to files of
certain types, or in certain directories"*
([OFFICIAL](https://docs.github.com/en/copilot/concepts/prompting/response-customization)).

**2. The nearest file wins, because locality is a proxy for correctness.** `agents.md`: *"The closest
AGENTS.md to the edited file wins"* ([OFFICIAL](https://agents.md/)). Codex concatenates root→CWD so
*"Files closer to your current directory override earlier guidance"*
([OFFICIAL](https://learn.chatgpt.com/docs/agent-configuration/agents-md.md)). Cursor merges nested
`AGENTS.md` *"with more specific instructions taking precedence"* and supports `globs`-scoped rules
([OFFICIAL](https://cursor.com/docs/rules)). Windsurf's `glob` trigger costs context *"Only when
matching files are touched"* ([OFFICIAL](https://docs.devin.ai/desktop/cascade/memories)).

**3. Shared knowledge must be reviewable in the same PR as the change it describes.** Claude Code's
scope table marks project instructions as *"Shared with: Team members via source control"*
([OFFICIAL](https://code.claude.com/docs/en/memory)). Cursor: *"Check your rules into git so your
whole team benefits. When you see Agent make a mistake, update the rule."* and *"Project rules live
in `.cursor/rules` … and are version-controlled"* ([OFFICIAL](https://cursor.com/docs/rules)).
Windsurf contrasts auto-memories (*"not committed to your repository"*) with Rules
(*"version-controlled, shareable with your team"*)
([OFFICIAL](https://docs.devin.ai/desktop/cascade/memories)).

**4. Personal preferences are the one thing that legitimately stays global — and they are kept in a
separate container.** Claude Code has `~/.claude/CLAUDE.md` (*"Personal preferences for all
projects"*) and `~/.claude/rules/` (*"apply to every project on your machine"*)
([OFFICIAL](https://code.claude.com/docs/en/memory)); Copilot has personal instructions, and the
precedence list is *"Personal → Repository (path-specific → repo-wide → agent) → Organization"*
([OFFICIAL](https://docs.github.com/en/copilot/concepts/prompting/response-customization)); Codex has
`~/.codex/AGENTS.md` for *"reusable preferences"*
([OFFICIAL](https://learn.chatgpt.com/docs/agent-configuration/agents-md.md)). In every case, global
scope is reserved for things that are *about the person*, not *about the code*.

**5. Auto-memory is per-repository even when it is not in the repository.** Claude Code derives the
memory directory from the git repository so *"all worktrees and subdirectories within the same repo
share one auto memory directory"*; Windsurf associates memories *"with the workspace they were
created in"*; Copilot restricts repository facts to *"operations on the same repository"*; Devin
offers explicit repo pinning. All four URLs as above.

### 4.1 What this implies for a memory that references code from one repository

**[INFERENCE — derived from the five points above.]** A memory entry that references code is not a
general-purpose memory. It is a *repository-scoped artifact* with three properties the sources
support:

1. **It must carry the repository identity** (Copilot: *"facts can only be used in operations on the
   same repository"*; Devin: repo pinning).
2. **It must be suppressible outside that repository**, or it becomes a distractor in unrelated work
   (Chroma's distractor finding; Windsurf's workspace scoping).
3. **It must be invalidated by the repository's changes**, which is exactly Copilot's citation
   re-validation step. A global store that cannot observe the repository cannot do this, which is
   the structural argument for why instructions live in the repo: **the repo is the only component
   that knows when the repo changed.**

---

## 5. Evidence that automatic dev-memory goes stale or produces low-value entries

### 5.1 The centrepiece: GitHub's engineering account of *why* they verify citations at read time

The GitHub Blog post describing how Copilot Memory was built is the single most useful source in this
report, because it is a vendor stating the design problem and the rejected alternative in its own
words ([OFFICIAL engineering blog](https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/)):

> *"The core challenge for memory systems isn't about information retrieval, but ensuring that any
> stored knowledge remains valid as code evolves across branches and time. In practice, this means a
> memory system must handle changes to code, abandoned branches, and conflicting observations—all
> while ensuring that agents only act on information that's relevant to the current task and code
> state."*

They name the concrete example the user reported:

> *"For example, a logging convention observed in one branch may later be modified, superseded, or
> never merged at all. One option would be to implement an offline curation service to deduplicate,
> resolve conflicts, track branch status, and expire stale information. At GitHub's scale, however,
> such an approach would introduce significant engineering complexity and LLM costs, while still
> requiring mechanisms to reconcile changes at read time."*

**They explicitly rejected offline curation and chose read-time verification**, on this principle:

> *"Information retrieval is an asymmetrical problem: It's hard to solve, but easy to verify. By using
> real-time verification, we gain the power of pre-stored memories while avoiding the risk of outdated
> or misleading information. Instead of offline memory curation, we store memories with citations:
> references to specific code locations that support each fact. When an agent encounters a stored
> memory, it verifies the citations in real-time, validating that the information is accurate and
> relevant to the current branch before using it."*

The write path on validation failure is a **correction**, not a deletion:

> *"Before applying any memory, the agent is prompted to verify its accuracy and relevance by checking
> the cited code locations. If the code contradicts the memory, or if the citations are invalid (e.g.
> point to nonexistent locations), the agent is encouraged to store a corrected version of the memory
> reflecting the new evidence. If the citations check out and the memory is deemed useful, the agent
> is encouraged to store it again in order to refresh its timestamp."*

And they stress-tested the mechanism adversarially, including by manufacturing the exact rot this
report is about:

> *"Our biggest concern was the impact of outdated, incorrect, or even maliciously injected memories.
> To test the system's resilience, we deliberately seeded repositories with adversarial memories–facts
> that contradicted the codebase–with citations pointing to irrelevant or nonexistent code locations.
> Across all test cases, agents consistently verified citations, discovered contradictions, and
> updated incorrect memories. **The memory pool self-healed as agents stored corrected versions based
> on their observations.** The citation verification mechanism robustly prevented the risk of
> misleading memories."*

> *"To simulate worst-case conditions, we overrepresented memories from branches that were abandoned or
> closed without merging, ensuring realistically noisy memories."*

**[CORRECTION — important for anyone quoting this source.]** GitHub's blog does **not** say what a
citation consists of. I grepped the full page: the phrase `line numbers` does not occur, and neither
does `symbol`. The only description is *"citations: references to specific code locations that support
each fact"*; the docs page says *"citations pointing to the code that supports them"*. **The
granularity of a Copilot Memory citation — line, path, or symbol — is not documented.** A secondary
wiki-style site renders it as *"file paths + line numbers"*; that phrasing is not in the primary
source and I do not repeat it as fact.

**[INFERENCE]** Note what this implies for the user's complaint. GitHub runs the largest documented
automatic dev-memory system, has a citation-verification mechanism, publishes that the mechanism
works — and still does not promise that a citation points at a *stable* thing. The durable part of
their design is not the anchor format; it is **the read-time check that makes a bad anchor harmless.**

The docs page for the same feature adds the operational defences
([OFFICIAL](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)):

1. **Citation re-validation against the current branch** before a repository fact may be used.
2. **A 28-day unused-entry deletion sweep** — *"To prevent stale information from lingering…"*.
3. **A guard for facts learned from rejected work** — facts *"captured from pull requests that were
   closed without merging"* are only used *"unless the current codebase still substantiates the
   information"*.

### 5.2 Corroboration: a vendor tells users not to rely on its own auto-memory

Cognition/Windsurf, quoted in full in §1.5: *"For knowledge you want Cascade to reliably reuse, write
it as a Rule or add it to `AGENTS.md` in your repo rather than relying on auto-generated Memories"*,
and *"Auto-generated memories live only on your machine."* The same page notes that the newer Devin
Local agent *"does not persist memories"* at all
([OFFICIAL](https://docs.devin.ai/desktop/cascade/memories)).

### 5.3 Corroboration: Anthropic ships a tool to delete the content a naive memory would save

- Auto memory is documented to *skip* architecture, file paths, and debugging fixes because they are
  derivable from the code ([OFFICIAL](https://code.claude.com/docs/en/memory)).
- `/doctor` exists to *"cut content Claude can derive from the codebase, such as directory layouts,
  dependency lists, and architecture overviews"* ([OFFICIAL](https://code.claude.com/docs/en/memory)).
- Best practices list *"Information that changes frequently"* and *"File-by-file descriptions of the
  codebase"* as things to exclude ([OFFICIAL](https://code.claude.com/docs/en/best-practices)).
- Anthropic recommends stripping *"redundant tool outputs or messages"* during compaction while
  preserving *"architectural decisions, unresolved bugs, and implementation details"*
  ([OFFICIAL](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)).

**[INFERENCE]** Anthropic ships a tool whose job is to delete from memory the exact category of
content (derived architecture, file-by-file description, frequently-changing detail) that a naive
"save what you saw" pipeline produces most of.

### 5.4 Field reports: the recurring shape is "confidently superseded", not "wrong"

All of the following are **[USER REPORTS]** on public issue trackers unless marked otherwise. They are
first-hand, dated, and reproducible by the reporter; they are not vendor statements.

| Source | Reported failure | What the author did |
|---|---|---|
| [claude-code #88886](https://github.com/anthropics/claude-code/issues/88886) | *"a memory file DELETED from disk at 12:50 was still present in the subagent's context, and its index still pointed at the (now nonexistent) path"*; *"the deleted file contained a rule that had been reversed; the subagent would have acted on the reversed version"*; measured drift **8h39m** | wrote a workaround script the subagent runs first to diff context files against git |
| [claude-code #85075](https://github.com/anthropics/claude-code/issues/85075) | *"We found a MEMORY.md whose index was last written ~3 months ago… Its top-level description of the repository — the first thing every session would read — described a project that no longer exists in that form… Every new session would have been primed with a stale world-model."* | *"We only avoided impact because `autoMemoryEnabled: false` had been set explicitly"* |
| [claude-code #92998](https://github.com/anthropics/claude-code/issues/92998) | Overflow truncation drops the **newest** entries, so *"the truncation preferentially discards supersessions, leaving the superseded version loaded and authoritative. The silo doesn't degrade toward 'less information' — it degrades toward 'confidently wrong'."* | reported; the dropped entry *"specifically contradicted an earlier instruction that was still inside the cap and still being loaded"* |
| [claude-code #90604](https://github.com/anthropics/claude-code/issues/90604) | Memory cache directories were stale: *"both were significantly behind (202 files missing combined)"*. Concrete harm: a rule *"never to `git push origin main`"* was missing from the cache, and *"The session never saw it and pushed to main three times in one session."* | filed a bug requesting the resolved cache path be surfaced |
| [claude-code #91738](https://github.com/anthropics/claude-code/issues/91738), [#93743](https://github.com/anthropics/claude-code/issues/93743), [#28037](https://github.com/anthropics/claude-code/issues/28037), [#83758](https://github.com/anthropics/claude-code/issues/83758) | Cross-project contamination: path-slug collisions make unrelated projects inherit *"even long-deleted"* memory; the model *"used facts from project B… when analysing project A"*; worktree memories are orphaned on worktree deletion | reported |
| [codex #11757](https://github.com/openai/codex/issues/11757) | *"Instruction boundary was violated at prompt-construction level: model followed unrelated AGENTS guidance from another workspace context."* Expected behaviour: instructions *"should be recomputed from the active workspace (`cwd`)"* | reported |
| [copilot-cli #3945](https://github.com/github/copilot-cli/issues/3945) | *"Out of the nowhere, it mumbled about some 'facts stored in the memory'… that seem to be part of a different (similar named) repository"* — on a fresh repo | reported |
| [cline #2097](https://github.com/cline/cline/issues/2097), [#1911](https://github.com/cline/cline/issues/1911) | Memory Bank protocol adherence collapses after task completion; agent *claimed* it had updated the memory-bank files and had not (*"You did not actually update the files."*) | reported/closed |
| [Cursor forum, "Can't clear memories"](https://forum.cursor.com/t/cant-clear-memories/148254) + staff reply | **Maintainer statement.** Cursor staff (Dean Rie): *"The Memories feature was intentionally removed starting in version 2.1.x, so the UI to manage them is no longer available. Even though this feature was removed from Cursor, it still works, just without a UI."* A user: *"the system uses memories from a long time ago and I have to tell at the beginning of every chat to not use them."* Official deletion workaround: roll back to 2.0.77, delete in Settings, upgrade again | users migrated memories into hand-written project files: *"I asked the agent to take those memories and create a file with them and later had it also incorporate those memories into my current markdown files… anyone that is using their memories, they should probably migrate them to rules because you lose the ability to remove or update them"* ([companion thread](https://forum.cursor.com/t/agents-have-lost-access-to-memory-capability/143310)) |

**This resolves the §1.2 negative finding.** The reason Cursor's docs index contains no Memories page
is not that I failed to find it — the feature was removed from the UI in 2.1.x while continuing to run
without a management UI. **The published documentation disappeared; the memory store did not.** For a
memory system, that is the worst of both worlds: entries persist, users cannot see or delete them, and
the docs that would tell you what they contain are gone.

### 5.5 A practitioner account of turning it off, and what replaced it

The most complete first-hand report found is a named operator's audit of automatic memory across
21 projects ([practitioner blog, author discloses selling rule/skill packs — commercially interested,
treat as opinionated first-hand evidence](https://dev.to/rulestack/auto-memory-on-21-projects-17-empty-and-3-repos-learned-the-same-fix-separately-4k4d)):

- **Yield was near zero:** *"17 empty"* memory directories out of 21; 9 topic files in total.
- **Silent schema drift:** *"none of the nine topic files carries a `type:` field… the memory files
  that exist do not look like the ones the current docs describe, and nothing in a session surfaces
  that difference."*
- **Redundant learning:** three of four projects *"had independently saved a note about the same
  thing… Same correction, learned three times, worded three ways."*
- **What they did:** *"We turned the feature off"* — `{"autoMemoryEnabled": false}` in the repo's
  `.claude/settings.local.json`. Critically, the migration came first: *"Before flipping the switch we
  read what auto memory had accumulated for that repository, found six notes that no committed file
  yet reflected, moved each into CLAUDE.md or a skill, and only then turned it off."*
- **Stated reason — auditability:** memory is machine-local, so scheduled CI jobs never see it, and
  *"it is written by the agent on its own judgment, which is precisely the kind of unrecorded decision
  the ledgers exist to prevent."*

The same author's follow-up documents a commit gate for dangling references — *"fails the commit if
CLAUDE.md references a skill directory that doesn't exist"* — which is a hand-rolled, CI-time version
of the same validation Copilot does at read time
([practitioner blog](https://dev.to/rulestack/we-cut-our-claudemd-from-548kb-to-34kb-what-loads-when-measured-and-the-commit-gate-that-keeps-1kpk)).

### 5.6 Retention decay as a designed-for outcome

Copilot's 28-day sweep (§5.1) and Chroma's finding that *"model performance consistently degrades
with increasing input length"* across 18 models
([RESEARCH](https://www.trychroma.com/research/context-rot)) both point the same way: an accumulating
store without pruning is expected to get *worse* over time, not better. Note the tension with
Vestige's documented policy that *"Nothing is ever deleted automatically"* (`AGENTS.md`,
"Automatic (server handles it)") — Vestige decays retention via FSRS-6 rather than deleting, which is
a defensible alternative, but it means **stale code facts remain retrievable indefinitely unless
something explicitly invalidates them.**

Chroma also supplies the vocabulary for *why* a stale entry is worse than a missing one. Their
definition of a distractor is *"topically related to the needle, but do not quite answer the
question"* — which is precisely what a superseded memory looks like, and they measured that *"Even a
single distractor reduces performance relative to the baseline (needle only)"*
([RESEARCH](https://www.trychroma.com/research/context-rot)).

### 5.7 Negative results, and sources excluded

**The specific sub-claim I could not verify — stated plainly.** **[NOT VERIFIED]** Despite fetching
the trackers named above, **I found no first-hand report of a coding-agent memory entry that recorded
a line number which later pointed at the wrong code after a refactor.** The failure mode is documented
at the *vendor design* level (§5.1: citations *"point to nonexistent locations"*; abandoned branches;
superseded conventions) and proven for *file-level* staleness (§5.4), but the precise `file:line` rot
the user described has no verified public instance that I or my search agents could find. The
mechanism is certain; a citation for it is not. Do not describe it as "widely reported".

**Excluded as not credible for this purpose:**

- A Hacker News "Show HN" thread that surfaced in searches was fetched and contains **no staleness
  complaint** — only the submitter's own marketing plus competitor plugs. Not used.
- A blog post analysing Claude Code's memory via a leaked source map was fetched but is **excluded as
  a source**: the author is affiliated with a competing memory product, and the leaked-source claims
  are not independently verifiable. Its quoted list of failure modes ("Functions that got refactored
  away still linger in memory") is *consistent* with everything above, but it is not evidence.
- One search result rendered Copilot's citation format as *"file paths + line numbers"*. I fetched the
  primary source it cites: the phrase is absent from the GitHub blog (§5.1). Not repeated as fact.

**Other negative results in this section:**

- **[NOT DOCUMENTED]** There is no surviving first-party Cursor documentation on Memories — the
  feature's docs were removed along with its UI while the feature kept running (§1.2, §5.4). The only
  citable Cursor sources are the 2025 1.0 changelog and the forum staff statement.
- No maintainer statement on `AGENTS.md` length limits was found. Search snippets suggesting PRs about
  "silently truncated" AGENTS.md files were **not** verified and are not cited.

---

## 6. What is documented vs. what is inference — an explicit ledger

| Claim | Status |
|---|---|
| Vendors instruct users to keep durable knowledge in repo files, not memory stores | **Documented** — Claude Code, Cursor, Windsurf, Devin, Codex, agents.md (§1) |
| Claude Code's auto memory skips architecture, file paths, debugging fixes | **Documented, verbatim** ([link](https://code.claude.com/docs/en/memory)) |
| Copilot Memory re-validates code citations against the current branch | **Documented, verbatim** ([link](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)) |
| GitHub recommends pinning a commit ID rather than a branch name | **Documented, verbatim** ([link](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)) |
| A position-keyed code index returns wrong answers when lines move | **Documented, verbatim, by Sourcegraph** (§3.3) |
| A shipped automatic dev-memory system verifies code citations at read time and rejects offline curation | **Documented, verbatim, GitHub engineering blog** (§5.1) |
| What a Copilot Memory citation actually contains (line? path? symbol?) | **NOT documented** — "line numbers"/"symbol" absent from both the blog and the docs (§2.1, §5.1) |
| Automatic dev-memory goes stale, leaks across repos, and loses supersessions | **Documented first-hand** across Claude Code, Cursor, Cline, Codex, Copilot CLI (§5.4) |
| A `file:line` memory entry going wrong after a refactor | **NOT VERIFIED** — no first-hand public instance found (§5.7) |
| Version identifiers are for reproducibility; concept identifiers for currency | **Documented, verbatim, by Zenodo** (§8.1) |
| Software-citation standards specify down to the revision and *no further* | **Documented — FORCE11 explicitly declined line-level specificity** (§8.2) |
| References measurably rot; "reference rot" is distinct from "link rot" | **Documented, measured** — 2014 *Perma*; 2021 CJR (§8.3) |
| Line numbers are an unstable anchor | **Documented by implication and by tooling design** (branch URLs unstable; path/glob scoping everywhere; no vendor uses line-scoped retrieval; git offers `-L:<funcname>` as the alternative to a line range) |
| "Line-number rot" is an established term of art | **NOT documented** — no first-party source uses the phrase (§0.9). The documented terms are *link rot* and *reference rot* (§8.3) |
| "Quote the smallest unit that survives refactoring" | **NOT documented verbatim** — a derived heuristic (§3.3); the documented near-equivalents are Cursor's "reference, don't copy" and Anthropic's "lightweight identifiers" |
| Symbols are strictly better than line numbers as anchors | **Infrastructure supports it, no vendor states it as policy** — the closest are git's `-L:<funcname>`, Rust RFC 1946, GitHub's commit-pin rule (§3.3, §8.5) |
| A memory system should refuse to store code-derivable facts | **Strongly implied by four vendor doc sets** (§1.1, §1.2, §7.2); not phrased as a hard rule anywhere |

---

## 7. What this implies for Vestige

### 7.1 The current design has the exact shape the sources warn about

Three concrete facts from this repository:

1. **`AGENTS.md` mandates file-anchored saves with no revision.** The `BUG_FIX` gate (`AGENTS.md:318`)
   requires `smart_ingest` with content
   `"BUG FIX: [error]\nRoot cause: [why]\nSolution: [fix]\nFiles: [paths]"`. Anthropic's documented
   auto-memory behaviour is to *skip* *"debugging fixes"* and *"file paths"* as derivable
   ([OFFICIAL](https://code.claude.com/docs/en/memory)). **Vestige mandates what Claude Code
   refuses.** (`AGENTS.md:342` already has a *"Never save"* list — secrets, API keys, temporary
   debugging state — so the refusal mechanism exists; it just does not cover code-derivable content.)
2. **The data model already carries a line number.** `CodeEntity` defines
   `pub line_number: Option<u32>` at `crates/vestige-core/src/codebase/types/code_entity.rs:21` — the
   only positional field in the code model.
3. **`files` is not even a field at retrieval time — it is markdown inside the memory body.**
   `crates/vestige-mcp/src/tools/codebase_unified.rs:102` documents `files` as *"Files where this
   pattern is used or affected by this decision"*, and the three write paths
   (`:191`, `:291`, `:427`) flatten it into prose:
   `content.push_str("\n\n## Files:\n")` followed by `- {path}` bullets. The repository identity is
   likewise prose-adjacent: `codebase` is converted into the *tag* `codebase:{name}` (`:203`, `:307`,
   `:443`) and read back as a tag filter (`:498`).
   **Consequence:** there is no queryable, machine-checkable code reference anywhere in the record —
   no commit, no symbol, no content hash, and no validation step between ingest and retrieval. A
   citation-based check (as in §2.1) is not merely missing; the data needed to perform one is not
   stored.

**[INFERENCE]** The reported symptom — "a useless scrap of text that refers to a file and a line
number, unreadable alone and probably wrong a week later" — is the predictable output of a schema
that can express a path and a position but *cannot express a revision or an identity*.

### 7.2 What Vestige should simply refuse to store

Each item below is justified by a source in §1–§5. "Repository is already the source of truth" is the
governing test — Claude Code's `/doctor` phrasing (*"content Claude can derive from the codebase"*)
and Cursor's (*"Duplicating what's already in your codebase"*) are the operative rules.

**Refuse outright:**

| Category | Reason / source |
|---|---|
| File contents, function bodies, code blocks copied out of the repo | Cursor: *"Reference files instead of copying their contents—this keeps rules short and prevents them from becoming stale as code changes."* |
| Architecture overviews, directory layouts, dependency lists | Claude Code `/doctor` cuts exactly these; Claude Code auto memory skips *"architecture"* |
| File-by-file descriptions of the codebase | Anthropic exclude list |
| Anything already stated in the repo's `AGENTS.md` / `CLAUDE.md` / `.cursor/rules` | Claude Code: *"It also skips anything your CLAUDE.md files already say."* |
| Build/test/lint commands that are already documented in-repo | Anthropic excludes *"Anything Claude can figure out by reading code"*; Codex/`AGENTS.md` and Cline Memory Bank are the designated home |
| *"How X works"* answers reachable by reading one file | Cursor: *"Point to canonical examples instead of copying code"* |
| **Any `file:line` recorded as the sole locator of a fact** | GitHub: branch URLs are unstable; no vendor uses line-scoped retrieval; Copilot instead validates citations |
| Facts learned while working on changes that were abandoned | Copilot: facts from PRs closed without merging are gated on the current codebase substantiating them |
| Secrets, keys, credentials (already in `AGENTS.md`) | Unchanged — keep this refusal |

**Accept only in a transformed form** (the "anchoring scheme" below):
a bug fix's *root cause and rationale*, a decision's *context and consequences*, a non-obvious *gotcha*
that cost real time, and a *pointer* to a canonical example — never the example itself.

### 7.3 The anchoring rule to implement

**Rule, one sentence:**
> A code-anchored memory must identify code by **revision + repository + symbol**, never by position;
> it must carry a **machine-checkable citation** so retrieval can re-validate it; and if the citation
> cannot be checked, the memory must be marked degraded rather than returned as fact.

Concretely, for every memory whose content depends on code:

1. **Replace `file:line` with a five-field reference.**
   `{ repo_remote, commit_sha, path, symbol (fully-qualified, e.g. `Storage::embeddings_fingerprint`),
   content_hash (optional) }`. Rationale: the Codex repo's own `AGENTS.md` anchors a Rust rule to
   *"as in `3c7f013f9735` / `#16630`"* — a commit and a PR, no path, no line
   ([REPO](https://github.com/openai/codex/blob/main/AGENTS.md)). GitHub's fix for an unstable file
   URL is likewise a commit pin, not a line pin.
   *A line number may be stored as a non-authoritative `hint` — never as the locator, and never the
   only anchor. Mark it as a hint in the schema so nothing downstream treats it as verified.*
2. **Adopt GitHub's read-time validation loop, and make the failure visible.** GitHub's stated principle
   is *"Information retrieval is an asymmetrical problem: It's hard to solve, but easy to verify"*, and
   their implementation is: on every read, *"the agent verifies the citations in real-time, validating
   that the information is accurate and relevant to the current branch before using it"*; if a citation
   is invalid, *"store a corrected version"*; if it checks out, *"store it again in order to refresh its
   timestamp"* ([OFFICIAL](https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/)).
   **They explicitly rejected the alternative Vestige currently has** — *"an offline curation service to
   deduplicate, resolve conflicts, track branch status, and expire stale information"* — on the grounds
   that it adds *"significant engineering complexity and LLM costs, while still requiring mechanisms to
   reconcile changes at read time."* Vestige already has the offline half (`reflect`, `dream`,
   `consolidate`, `gc`, FSRS decay); it has **no read-time half**. Concretely: resolve the symbol
   (ripgrep / tree-sitter / `git show`) when a code-anchored memory is retrieved. If it resolves,
   return normally. If it does not, **do not silently return it as fact** — return it flagged
   (`unverified` / `citation_broken`) and route it to `reflect` / `temporal invalidate`. This converts
   "probably wrong" into "knowably wrong", which is the whole point.
3. **Copy the two behaviours that make GitHub's loop self-correcting.** (a) A **correction write path**:
   when the code contradicts the memory, write the corrected version rather than only flagging the old
   one. (b) A **refresh on successful validation**, so "recently confirmed true" is distinguishable from
   "recently written" — Vestige's FSRS-6 retention already models recency of *use*, but not recency of
   *confirmation*. The `modified`-timestamp idea is also present in Claude Code's auto memory, which
   records an ISO-8601 `modified` field *"show[ing] how current the fact is, both to you and to Claude
   when it reads the memory back"* ([OFFICIAL](https://code.claude.com/docs/en/memory)).
4. **Scope code-anchored memories to a repository and hide them elsewhere.** Copilot restricts
   repository facts to the same repository; Devin pins knowledge to a repo; Windsurf scopes memories
   to a workspace. Five separate trackers report cross-repo or cross-workspace leakage as a bug —
   Codex *"followed unrelated AGENTS guidance from another workspace context"*, Copilot CLI *"mumbled
   about some 'facts stored in the memory'… that seem to be part of a different (similar named)
   repository"*, and several Claude Code issues report project-path-slug collisions (§5.4). Vestige's
   `codebase` field already exists — it needs to become a *retrieval filter* that suppresses
   code-anchored memories when the session's repo does not match, not just a tag.
5. **Push durable facts back into the repo instead of hoarding them.** Cursor and Copilot both describe
   an agent-side path to write the rule into `.cursor/rules` / `AGENTS.md`; the Windsurf and Cursor
   users quoted in §5.4 migrated their memories into repo files *by hand* after discovering they could
   not manage them. A memory server that detects "this fact belongs in `AGENTS.md`" and *offers* that
   write is doing what four vendors tell their users to do manually. At minimum, `smart_ingest` should
   refuse the duplicate and point at the file when the fact is already there.
6. **Build on the ADR primitives Vestige already has.** `remember_decision_v2` in
   `crates/vestige-mcp/src/tools/codebase_unified.rs` already accepts `criteria`, `choices`,
   `scoreMatrix`, **`supersedes`** (*"IDs of decisions this one replaces"*, `:96`) and **`validUntil`**
   (*"When the date passes the decision is auto-flagged as stale by reflect"*, `:91`). That is
   Nygard's *"keep the old one around, but mark it as superseded"* in production order handling. The
   gap is that the legacy `remember_decision` and the `BUG_FIX` gate do **not** go through it, and
   `files` carries no revision.
7. **Prefer the smallest stable unit — and label it as a heuristic, not a citation.** "Quote the
   smallest unit that survives refactoring" is not documented by any source I could find (§3.3). The
   defensible, sourced version is: *store the identifier, not the content* (Anthropic's *"lightweight
   identifiers"*) and *reference, don't copy* (Cursor). Concretely: `Storage::embeddings_fingerprint`
   is an anchor; eight lines of its body is a liability.
8. **Do not make preference memory and code memory one thing.** Copilot validates repository facts
   against code but confirms preferences by "best judgment"; Vestige's `smart_ingest` currently
   applies one pipeline (entity extraction, temporal anchoring, dedup) to both. Preferences
   (`node_type: person`, `preference`) do not need citation validation; code claims do. Splitting
   them is cheaper than validating everything.

### 7.4 Rough sequencing

Ordered by value-per-unit-of-work, not by ease. Step 1 is first because it is the only one that
actually fixes the reported complaint; the schema change is a prerequisite for it, not an end in
itself.

| Step | Change | Source it answers to |
|---|---|---|
| **1** | **Read-time citation validation with an `unverified` / `citation_broken` flag on retrieval; route failures to `reflect` / `temporal invalidate`.** Start with "does the path still exist, and does the symbol still resolve" — that alone catches whole-file moves and renames | GitHub's *"hard to solve, but easy to verify"* loop (§5.1) |
| 2 | Add a `code_ref` field (`repo_remote`, `commit_sha`, `path`, `symbol`, `hint_line`) to `CodeEntity`, `remember_pattern`, `remember_decision`, and the `BUG_FIX` gate; stop emitting bare `Files: [paths]` | Codex `AGENTS.md` SHA anchoring; GitHub commit pins |
| 3 | Make `line_number` explicitly a *hint* in the schema and stop using it to locate anything | GitHub permalink docs; §3.3 |
| 4 | Correction write path + refresh-on-validated-use, so "confirmed recently" ≠ "written recently" | GitHub's corrected-version path; Claude Code's `modified` field |
| 5 | Enforce repository scoping as a retrieval filter on code-anchored memories | Copilot repo-scoping; Devin repo pinning; the five leakage reports in §5.4 |
| 6 | Add a refusal list to `smart_ingest` for code-derivable content, with a "put it in `AGENTS.md` instead" suggestion | Claude Code `/doctor` + auto-memory skip list; Cursor anti-duplication list |
| 7 | Split validation policy: preferences need no citation; code claims do | Copilot's two-tier validation (§2.1) |

**The one-sentence rule, restated for whoever implements this:** *store what the code cannot tell you,
anchor it to a revision and a name rather than a position, and check the anchor every time you use
it.*

---

## 8. Durable references outside coding agents: version DOIs, and measured rot rates

The user's complaint is an instance of a problem with a literature. Two bodies of work are directly
applicable.

### 8.1 Version identifier vs. concept identifier — Zenodo's rule

Zenodo registers two DOIs per upload, and states the citation rule
([OFFICIAL](https://zenodo.org/help/versioning)):

> *"When you publish an upload for the first time, we register two DOIs: a DOI representing the
> **specific version** of your record. a DOI representing **all of the versions** of your record."*

> *"You should normally always use the DOI for the **specific version** of your record in citations.
> This is to ensure that other researchers can access the **exact** research artefact you used for
> reproducibility. By default, we use the specific version to generate citations."*

> *"You can use the Concept DOI representing all versions in citations when it is desirable to cite an
> evolving research artifact, without being specific about the version."*

> *"Including semantic information such as the version number in a DOI is bad practice, because this
> information may change over time, while DOIs must remain persistent and should not change."*

**[INFERENCE]** This is the cleanest available statement of the anchoring trade-off a memory system
faces. A **version identifier** (commit SHA, version DOI) is exact and reproducible but frozen: it
answers "what did it look like then", never "what is true now". A **concept identifier** (repo remote
+ symbol, concept DOI) is stable and always current but not reproducible at a point in time. Cody's
practice — the Codex `AGENTS.md` example cited above — is to carry **both**: a commit SHA *and* a PR
number *and* prose that is true independent of either. A memory that carries only one of the two is
either unverifiable or unfindable.

### 8.2 The software-citation community considered line-level citation and declined to standardize it

The FORCE11 Software Citation Principles (Smith, Katz, Niemeyer, *PeerJ CS* 2:e86) are the canonical
statement of how to cite code. Three principles apply directly
([primary, DOI 10.7717/peerj-cs.86](https://doi.org/10.7717/peerj-cs.86); quotes obtained via a text
proxy because the publisher's site returned HTTP 403):

> **"Unique identification:** A software citation should include a method for identification that is
> machine actionable, globally unique, interoperable, and recognized by at least a community of the
> corresponding domain experts…"

> **"Persistence:** Unique identifiers and metadata describing the software and its disposition should
> persist—even beyond the lifespan of the software they describe."

> **"Specificity:** Software citations should facilitate identification of, and access to, the specific
> version of software that was used. Software identification should be as specific as necessary, such
> as using version numbers, revision numbers, or variants such as platforms."

The review appendix contains the most useful negative finding in this report. A reviewer asked for
finer granularity, and the authors declined:

> *"The citation/url should therefore allow for greater specificity within a code base."* — reviewer
> *"**Our response:** We agree that greater specificity is desirable in some cases, but we do not
> believe this rises to the level of what should be specified or discussed in the principles at this
> time."* — authors

**[INFERENCE]** The people who thought hardest about durable references to code drew the line at
**version/revision**, and explicitly refused to go below it. That is a strong argument that
"which line" is not the right question for a memory system to answer, and that "which revision, and
which identifiable unit within it" is.

### 8.3 Measured rates: references rot, and stable containers are exactly what makes deep links fragile

The empirical literature quantifies both failure modes — **link rot** (target gone) and **reference
rot** (target still resolves but no longer says what was cited)
([primary, Zittrain, Albert & Lessig, *Perma*, 127 Harv. L. Rev. F. 176, 2014](https://harvardlawreview.org/forum/vol-127/perma-scoping-and-addressing-the-problem-of-link-and-reference-rot-in-legal-citations/)):

> *"Link rot refers to the URL no longer serving up any content at all. Reference rot, an even larger
> phenomenon, happens when a link still works but the information referenced by the citation is no
> longer present, or has changed."*

> *"more than 70% of the URLs within the above mentioned journals, and 50% of the URLs within U.S.
> Supreme Court opinions suffer reference rot"*

> *"Of the 353 '200 status' links within the Supreme Court corpus that we viewed and coded, only 76%
> still led to the cited material, indicating that reference rot independent of link rot is a major
> problem."*

**[CORRECTION TO A COMMON MISATTRIBUTION]** The frequently-cited "over 50% of URLs in US Supreme Court
opinions are broken" figure is this **2014** paper (49.9%), not a 2021 study. The 2021 study is a
separate New York Times analysis
([primary, Bowers, Stanton & Zittrain, CJR, 2021-05-21](https://www.cjr.org/analysis/linkrot-content-drift-new-york-times.php)):

> *"Of these deep links, 25 percent of all links were completely inaccessible."*

> *"6 percent of links from 2018 had rotted, as compared to 43 percent of links from 2008 and 72
> percent of links from 1998."*

> *"Thirteen percent of intact links from that sample of 4,500 had drifted significantly since the
> Times published them."*

And the sentence that is the exact structural analogue of `file:line`:

> *"It is precisely because their domains are fixed that their deep links are fragile."*

**[INFERENCE]** Substitute "file path" for "domain" and "line number" for "deep link" and the sentence
describes the reported bug precisely: the *stable* part of the reference (the repository, the file
path) is what makes the *unstable* part (the position inside it) dangerous, because the stable part
keeps resolving to something. A memory entry that 404s is harmless — it is obviously broken. A memory
entry that resolves to the *wrong code* is the failure mode that matters, and it is the one the user
described ("probably wrong, because the line moved").

### 8.4 Design guidance worth stealing: omit what changes

The oldest source here is still the most quotable
([OFFICIAL, W3C, Tim Berners-Lee](https://www.w3.org/Provider/Style/URI)):

> *"A cool URI is one which does not change."* … *"URIs don't change: people change them."*

> *"Designing mostly means leaving information out."*

The page's list of things to leave out of an identifier includes authors' names, subject, status,
access method, file name extension, software mechanisms and **disk name** — i.e. every component that
encodes *current implementation detail*. **[INFERENCE]** A line number is a disk name for a memory
entry: it encodes where something happened to sit at the moment of observation, which is precisely
the information most likely to change and least likely to be needed again.

### 8.5 What was searched and not found

Recorded so that absence is not mistaken for a finding:

- **No fetched primary source literally states "cite symbols, not line numbers."** The closest are
  git's `-L:<funcname>` alternative, Rust RFC 1946's "very fragile" critique plus name-based
  replacement, and GitHub's commit-SHA permalink rule. Each is labelled as such above.
- **GitHub never warns about `#L` rot explicitly.** Its stability statement is at *file* granularity;
  the inference from "file contents can change" to "line numbers definitely move" is mine.
- **No GitHub changelog entry** about line-range links breaking was found.
- **Diátaxis** (`https://diataxis.fr/`) was loaded but its home page covers only the four documentation
  forms; it says nothing about identifiers vs. positions. Not a refutation — simply not consulted at
  depth.
- **The LSP `textDocument/documentSymbol` spec could not be verified** (the single-page spec truncated
  before that section), so no LSP claim is made.
- **Write the Docs** yielded no authoritative page on stable code references.
- Two URLs given in my original brief were wrong and are corrected here:
  `https://help.zenodo.org/docs/deposit/describe-records/doi/` is a 404 (live page:
  `https://zenodo.org/help/versioning`), and the rustdoc intra-doc-links page is
  `https://doc.rust-lang.org/rustdoc/write-documentation/linking-to-items-by-name.html`
  (`.../attributes/documentation.html` is a 404). The FORCE11 paper was reachable only through a text
  proxy because `peerj.com` returned HTTP 403.

---

## Appendix: all sources loaded for this report

Vendor documentation:

- Claude Code — How Claude remembers your project — https://code.claude.com/docs/en/memory
- Claude Code — Best practices — https://code.claude.com/docs/en/best-practices
- Anthropic — Effective context engineering for AI agents — https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Claude Platform — Memory tool — https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool
- Cursor — Rules — https://cursor.com/docs/rules
- Cursor — Changelog 1.0 (Memories announcement) — https://cursor.com/changelog/1-0
- Cline — Memory Bank — https://docs.cline.bot/best-practices/memory-bank
- GitHub — Adding repository custom instructions — https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/add-custom-instructions/add-repository-instructions
- GitHub — About customizing Copilot responses — https://docs.github.com/en/copilot/concepts/prompting/response-customization
- GitHub — Copilot customization cheat sheet — https://docs.github.com/en/copilot/reference/customization-cheat-sheet
- GitHub — About GitHub Copilot Memory — https://docs.github.com/en/copilot/concepts/agents/copilot-memory
- VS Code — Use custom instructions — https://code.visualstudio.com/docs/agent-customization/custom-instructions
- VS Code — Use prompt files — https://code.visualstudio.com/docs/agent-customization/prompt-files
- VS Code — Use memory with agents — https://code.visualstudio.com/docs/agents/run/memory
- Devin — Knowledge — https://docs.devin.ai/product-guides/knowledge
- Devin Desktop (Windsurf/Cascade) — Memories & Rules — https://docs.devin.ai/desktop/cascade/memories
- Devin — AGENTS.md — https://docs.devin.ai/onboard-devin/agents-md
- Aider — Specifying coding conventions — https://aider.chat/docs/usage/conventions.html
- Codex — Custom instructions with AGENTS.md — https://learn.chatgpt.com/docs/agent-configuration/agents-md.md
- AGENTS.md (open format, Agentic AI Foundation) — https://agents.md/
- GitHub Blog — Building an agentic memory system for GitHub Copilot — https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/

Field reports and maintainer statements (all fetched; labelled **[USER REPORT]** or
**[MAINTAINER]** at each point of use):

- Claude Code — [issue #88886](https://github.com/anthropics/claude-code/issues/88886) (deleted memory still in subagent context) · [#85075](https://github.com/anthropics/claude-code/issues/85075) (months-old `MEMORY.md`) · [#92998](https://github.com/anthropics/claude-code/issues/92998) (overflow discards supersessions) · [#90604](https://github.com/anthropics/claude-code/issues/90604) (stale cache path, pushed to `main`) · [#91738](https://github.com/anthropics/claude-code/issues/91738) · [#93743](https://github.com/anthropics/claude-code/issues/93743) · [#28037](https://github.com/anthropics/claude-code/issues/28037) · [#83758](https://github.com/anthropics/claude-code/issues/83758) (cross-project contamination) · [#75334](https://github.com/anthropics/claude-code/issues/75334) · [#85591](https://github.com/anthropics/claude-code/issues/85591)
- Cursor forum — [Can't clear memories](https://forum.cursor.com/t/cant-clear-memories/148254) (maintainer statement: feature removed from UI in 2.1.x, still runs) · [Agents have lost access to memory capability](https://forum.cursor.com/t/agents-have-lost-access-to-memory-capability/143310) (user migrated memories to rules)
- Cline — [issue #2097](https://github.com/cline/cline/issues/2097) · [issue #1911](https://github.com/cline/cline/issues/1911)
- OpenAI Codex — [issue #11757](https://github.com/openai/codex/issues/11757) (stale `AGENTS.md` from another workspace)
- GitHub Copilot CLI — [issue #3945](https://github.com/github/copilot-cli/issues/3945) (memory leaking between repositories)
- Practitioner audit of auto-memory across 21 projects — https://dev.to/rulestack/auto-memory-on-21-projects-17-empty-and-3-repos-learned-the-same-fix-separately-4k4d · companion post on the hand-curated replacement — https://dev.to/rulestack/we-cut-our-claudemd-from-548kb-to-34kb-what-loads-when-measured-and-the-commit-gate-that-keeps-1kpk *(author sells rule/skill packs — labelled as commercially interested at point of use)*

Repository files:

- `openai/codex` AGENTS.md — https://github.com/openai/codex/blob/main/AGENTS.md
- Vestige `AGENTS.md` (mandatory save gates, `codebase` tool contract)
- `crates/vestige-core/src/codebase/types/code_entity.rs` (`CodeEntity.line_number`)
- `crates/vestige-mcp/src/tools/codebase_unified.rs` (`files`, `remember_decision_v2` schema)

Reference / research:

- GitHub — Getting permanent links to files — https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files
- GitHub — Creating a permanent link to a code snippet — https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-a-permanent-link-to-a-code-snippet
- Chroma — Context Rot: How Increasing Input Tokens Impacts LLM Performance — https://www.trychroma.com/research/context-rot
- Nygard — Documenting Architecture Decisions (2011) — https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions
- adr.github.io — https://adr.github.io/ · ADR templates — https://adr.github.io/adr-templates/ · MADR 4.0 template — https://raw.githubusercontent.com/adr/madr/4.0.0/template/adr-template.md
- Zenodo — DOI versioning — https://zenodo.org/help/versioning
- FORCE11 Software Citation Principles (Smith, Katz, Niemeyer, *PeerJ CS* 2:e86) — https://doi.org/10.7717/peerj-cs.86
- Zittrain, Albert & Lessig — *Perma* (2014), 127 Harv. L. Rev. F. 176 — https://harvardlawreview.org/forum/vol-127/perma-scoping-and-addressing-the-problem-of-link-and-reference-rot-in-legal-citations/
- Bowers, Stanton & Zittrain — CJR, 2021-05-21 — https://www.cjr.org/analysis/linkrot-content-drift-new-york-times.php
- W3C — Cool URIs don't change — https://www.w3.org/Provider/Style/URI
- git — `git log` (incl. `-L:<funcname>`, `-S<string>`) — https://git-scm.com/docs/git-log
- Rust RFC 1946 — Intra-rustdoc links — https://rust-lang.github.io/rfcs/1946-intra-rustdoc-links.html · resulting docs — https://doc.rust-lang.org/rustdoc/write-documentation/linking-to-items-by-name.html
- Sourcegraph — Precise code navigation (position-keyed index staleness) — https://raw.githubusercontent.com/sourcegraph/sourcegraph-public-snapshot/56bf5f946338e0e5a9b5c542680d952d313aa6ff/doc/code_navigation/explanations/precise_code_navigation.md

Pages that failed to load or were corrected (recorded for honesty):

- `https://cursor.com/docs/context/memories.md` → HTTP 404
- `https://cursor.com/docs/agent/memories.md` → HTTP 404
- `https://docs.windsurf.com/windsurf/cascade/memories` → cross-origin redirect to `docs.devin.ai`;
  resolved via https://docs.devin.ai/desktop/cascade/memories
- `https://help.zenodo.org/docs/deposit/describe-records/doi/` → HTTP 404 (live page is
  https://zenodo.org/help/versioning)
- `https://doc.rust-lang.org/reference/attributes/documentation.html` → HTTP 404
- `https://peerj.com/articles/cs-86/` → HTTP 403 (Cloudflare); full text obtained through a text proxy
- LSP 3.17 `textDocument/documentSymbol` — page loaded but truncated before the relevant section;
  **not cited**
- `https://diataxis.fr/` — loaded, but covers only the four documentation forms; no identifier guidance

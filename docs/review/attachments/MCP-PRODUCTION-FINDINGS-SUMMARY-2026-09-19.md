# MCP production deployment — key findings for the parent agent

> **Status:** COMPLETE — all 8 research areas covered. Full Polish report: `docs/research/mcp-production-best-practices-2026-09-19.md`.
> **Date:** 2026-09-19. **Delivery note:** `send_message` to parent agent `f8fda318-fdb8-4d86-94e4-2e928b8cf9a6` failed with "direct parent is not live" — this file is the durable handoff.

---

## 1. BREAKING: MCP is now a stateless protocol (`2026-07-28`)

Current revision is **`2026-07-28`** (released 2026-07-28, https://blog.modelcontextprotocol.io/posts/2026-07-28/). Vestige's current transport implementation is **legacy**.

| Change | Detail |
|---|---|
| `Mcp-Session-Id` | **REMOVED** |
| `initialize` / `notifications/initialized` | **REMOVED** |
| Per-request `_meta` | `io.modelcontextprotocol/protocolVersion` (**MUST**), `.../clientCapabilities` (**MUST**), `.../clientInfo` (**SHOULD**) |
| New required RPC | **`server/discover`** (MUST) — advertises `supportedVersions`, `capabilities`, `serverInfo` |
| HTTP GET endpoint | **REMOVED** → replaced by `subscriptions/listen` |
| `resources/subscribe` / `unsubscribe` | **REMOVED** → `subscriptions/listen` with `resourceSubscriptions` |
| SSE `Last-Event-ID` resumability | **REMOVED** — broken stream = lost request |
| Required HTTP headers | `MCP-Protocol-Version`, `Mcp-Method` (all), `Mcp-Name` (tools/call, resources/read, prompts/get) |
| Header/body mismatch | **400** + JSON-RPC **`-32020` HeaderMismatch** |
| Required result field | **`resultType`** (`"complete"` \| `"input_required"`); absent → treat as `"complete"` |
| Required cache fields | `ttlMs` + `cacheScope` on `server/discover`, `tools/list`, `prompts/list`, `resources/list`, `resources/templates/list`, `resources/read` |
| Removed RPCs | `ping`, `logging/setLevel`, `notifications/roots/list_changed` |
| Server→client requests | Replaced by **MRTR** (`InputRequiredResult` + `inputRequests` + `requestState`) |
| Error codes | `-32020` HeaderMismatch, `-32021` MissingRequiredClientCapability, `-32022` UnsupportedProtocolVersion; resource-not-found `-32002` → `-32602` |
| Deprecated (12-mo window, removal ≥ 2027-07-28) | Roots, Sampling, Logging, DCR (RFC 7591), `includeContext: thisServer\|allServers`, HTTP+SSE transport |
| Deprecated **and removed** | ⚠️ `logging/setLevel` is **removed outright**; the Logging *feature* is only deprecated. Commonly conflated. |

**Dual-era is explicitly allowed:**
> "A dual-era server **MAY** serve both eras concurrently on the same endpoint or process."
> — https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning.md

So migration does **not** have to be a flag-day. A modern-only server receiving legacy traffic SHOULD: `405` for GET/DELETE, ignore `Mcp-Session-Id`, ignore `Last-Event-ID`.

**Servers MUST NOT mutate the tool list as a side effect of another request** (SEP-2567). "Call `session_context`, then new tools appear" is now illegal.

**Governance:** MCP donated to the **Agentic AI Foundation (AAIF)**, a Linux Foundation project, December 2025.

---

## 2. Migration cost — measured, not guessed

[SEP-2567](https://modelcontextprotocol.io/seps/2567-sessionless-mcp.md) contains an automated survey of a **1,000-repo random sample** of open-source MCP servers:

| Category | Share | Migration |
|---|---:|---|
| No application-level session-ID reference | **90.0%** | none |
| `Map<sessionId, Transport>` (TS SDK boilerplate) | 3.5% | removed by sessionless SDK transport |
| Transport setup only (`sessionIdGenerator`, never read) | 2.8% | delete one constructor option |
| **Session-keyed application state** | **2.5%** | explicit handles or auth principal |
| **Proxy/gateway sticky routing** | 0.7% | needs a designed replacement |
| **Auth artifacts bound to session ID** | 0.5% | server-generated nonce or token subject |

If Vestige keeps state in SQLite rather than in a session-keyed map, it is in the 90%. If any logic depends on connection identity (e.g. a "current conversation" notion), it is in the 2.5%.

Handle design guidance (SEP-2567): opaque; **≥128 bits** of CSPRNG entropy if unauthenticated; validate `(handle, auth_context)` on every call; **document the lifetime in the `create_*` tool's `description`** (a policy only in server docs is invisible to the model); specific expiry errors.

---

## 3. OAuth 2.1 — and why stdio needs none

**The spec is explicit:**
> "Implementations using an **STDIO transport SHOULD NOT follow this specification**, and instead retrieve credentials from the environment."

Required by `2026-07-28`: RFC 9728 (MUST for servers), RFC 8414 + OIDC Discovery (servers MUST offer ≥1, clients MUST support both), RFC 8707 (clients MUST), RFC 9207 `iss` (SHOULD from AS, MUST-validate in clients — a future revision will upgrade AS side to MUST).
**Deprecated:** RFC 7591 DCR → **CIMD** (`draft-ietf-oauth-client-id-metadata-document-00`, still an Internet-Draft, not an RFC).
**⚠️ Not required:** **RFC 8693 token exchange** is *not* in `2026-07-28`. It appears only in the roadmap (2026-08-22) and in auth extensions. **RFC 7636 PKCE** is not listed in the spec's "Standards Compliance" section, though it appears in the flow diagram — treat it as required by OAuth 2.1, not by a standalone MCP norm.

For a local-first server: `Origin` validation (403 on invalid) + loopback bind + bearer token from a `0600` file, or **unix domain socket** (explicitly named in the security best practices). Full OAuth only if you expose HTTP beyond loopback.

---

## 4. Observability — the semconv moved

**OTel GenAI + MCP semconv left `open-telemetry/semantic-conventions` for `open-telemetry/semantic-conventions-genai`** in semconv **v1.42.0 (2026-06-16)**. Most indexed pages, including opentelemetry.io, still point at the old (now "Moved"/Deprecated) location. MCP semconv first shipped in **v1.39.0 (2026-01-14)**. ⚠️ The new repo has `## Schema URL` → `TODO` and an empty CHANGELOG — **there is no pinnable version for `mcp.*` today**.

**Exactly 4 official metrics, all Histogram, unit `s`, status `Development`:**
`mcp.client.operation.duration`, `mcp.server.operation.duration`, `mcp.client.session.duration`, `mcp.server.session.duration`
Buckets: `[0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 30, 60, 120, 300]`

**Explicit negatives — do not invent these:**
- No metric named "tool call duration" → derive via `mcp.method.name="tools/call"`
- No `mcp.*` request-count or error-rate counters → use histogram `_count` and `error.type`
- No token metrics in `mcp.*` → `gen_ai.usage.*` lives on GenAI inference spans, **client side only**

Other rules: `error.type` SHOULD be `tool_error` when `CallToolResult.isError == true`; `gen_ai.operation.name` = `execute_tool` for tool calls and SHOULD NOT be set otherwise; don't put `mcp.resource.uri` in span names by default (cardinality); don't create a duplicate span if outer GenAI instrumentation already traces the tool execution.

**⚠️ Two conflicts:**
1. `mcp.session.id` still exists in semconv (Recommended, links to `2025-06-18`) but `2026-07-28` **has no sessions**. Unresolved; don't set it for stateless HTTP.
2. `opentelemetry-instrumentation-mcp` (traceloop — **not** the OTel org) logs prompts/results to span attributes **by default**, inverting the semconv rule ("SHOULD NOT capture them by default"). Set `TRACELOOP_TRACE_CONTENT=false`.

**SDK support:** Python `mcp` 2.2.0 (2026-09-07) native + on-by-default; C# native (`ActivitySource`/`Meter` = `"Experimental.ModelContextProtocol"`); TypeScript 1.30.0 none (verified negative); **Go/Java/Rust unverified**. **No instrumentation package is published BY the MCP project.** ⚠️ For the Rust SDK (2026-07-28 in beta) there is **no official OTel documentation** — expect to hand-roll `tracing-opentelemetry` plus manual `traceparent` injection/extraction from `params._meta`.

---

## 5. Conformance — the concrete migration gate

**`npx @modelcontextprotocol/conformance server --url http://localhost:PORT/mcp --requirements 2026-07-28`**

| Revision | Server scenarios | Client scenarios | `not_scored` |
|---|---:|---:|---:|
| **`2026-07-28`** | **37** | **32** | 20 |
| `2025-11-25` | 30 | 18 | 10 |

- GitHub Action: `modelcontextprotocol/conformance@v0.1.11`
- Exit-code contract with `--expected-failures`: fail+baselined=0; fail+not-baselined=**1**; pass+baselined=**1 (stale baseline)**; pass+not=0
- ⚠️ Two npm lines: `latest` = 0.1.16 (2026-03-30) vs `alpha` = 0.2.0-alpha.11 (2026-08-07). `--requirements`/`tier-check` are **alpha-only**. Pin explicitly.
- ⚠️ **Dual-era requires TWO runs** — a scenario in both revisions runs at a different wire each time. "Passing it on one wire says nothing about the other."
- ⚠️ `http-header-validation` and `http-custom-header-server-validation` are `pending` → **not scored**. The official suite will **not** enforce the MUST-level `Mcp-Param-*` validation. Test it yourself.
- ⚠️ `x-mcp-header` mirroring is **skipped by the SDK in the browser** — test from Inspector CLI/TUI only.

**Coverage gaps to state honestly:** no official guidance for unit-testing tool handlers; **no load-testing harness or guidance anywhere.**

**Inspector:** `latest` 2.7.0 (2026-09-16), `next` 2.0.0-rc.3, `v1-latest` 1.0.2; Node 22.19.0+; exit codes 0/1/2/3/4/5; use `--stored-auth-only` in CI; JSON error envelope on stderr parsable via `2>&1 | tail -1 | jq .error`. "Protocol era" (`legacy` default / `auto` / `modern`) is **per-server and orthogonal to transport** — CI needs it as its own axis.
**mcpjam** is third-party (2,222★, npm 3.8.1, 2026-09-18); its "conformance" is its own, separate from the official suite.

---

## 6. Sandboxing

**Official posture: SHOULD, not MUST.** Only consent is MUST (SEP-1024, Final, 2025-07-22 — and it mandates *only* consent; sandboxing is non-normative in it).
⚠️ **Verified negative: neither 2026-07-28 security page has any tool-poisoning or prompt-injection section.** The only hook is one sentence: clients "MUST consider tool annotations to be untrusted unless they come from trusted servers."

**Docker MCP Gateway v0.43.3 (2026-07-16):**
- ⚠️ **The container DOES see the plaintext secret, as an env var** (`secrets: [{name: …, env: …}]`; Docker Desktop resolves the `se://` URI at container runtime, then injects the value into the server's environment).
- ⚠️ **No `--runtime` flag exists** in the complete CLI reference → plain Docker/runc + `no-new-privileges` + `--cpus 1` / `--memory 2Gb`. **"Docker MCP Gateway uses gVisor" is unsupported by Docker's own docs.**
- Network egress is **NOT** denied by default; Docker's threat model **explicitly disclaims prompt injection** as out of scope.
- v0.43.1 (2026-06-25) was a breaking security release (Bearer auth required by default for HTTP/SSE/streaming; digest-pinned `mcp/` images; name-collision rejection; no raw arg values in logs). CVE-2026-55887 fixed in v0.42.2.
- **Docker Sandboxes** (2026-04) uses a purpose-built VMM (Hypervisor.framework/WHPX/KVM) and injects credentials into HTTP headers (value never enters the VM) — but ⚠️ **local stdio MCP servers run on the HOST, not in the VM**.

**Others:** gVisor `20260914.0` (2026-09-16) — no MCP-specific guidance exists. **WASM/WASI is technically ready but officially unsupported**: WASI 0.3.0 (2026-06-11), Wasmtime 48.0.2 (2026-09-10, now denies TCP/UDP sockets by default); **the MCP Registry has no WASM package type** (exactly npm, pypi, nuget, cargo, oci, mcpb) and no WASM SEP exists. macOS `sandbox-exec` is **deprecated per a 2017-03-09 man page yet Anthropic ships on it in v0.0.77 (2026-09-18)**; Claude Code uses Seatbelt on macOS, bubblewrap+seccomp on Linux; documented macOS breakage includes Apple Events (-600) and Go CLIs failing TLS verification under Seatbelt.
**MCP SDKs do NO sandboxing** — verified from source (TS and Python implement env allowlists + process teardown only).

⚠️ **Most important honest gap:** sandboxing is universally recommended but **no published controlled measurement exists of its effect on tool-poisoning ASR**. The widely-quoted "84%" is *permission-prompt reduction*, not injection resistance. MCPTox (arXiv:2508.14925) reports 36.5% avg / 72.8% max ASR, but those figures reach me via a CSA summary rather than the paper body — flagged for verification.

---

## 7. Tool design at ~28 tools

**You are at the threshold, not comfortably below it.**
- Anthropic: **tool-selection accuracy degrades past 30–50 available tools** (https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool).
- Cost: **600–1,900 tokens per tool definition** → 28 tools ≈ **17k–53k tokens**.
- Published anchors: GitHub 35 tools ≈ **26k tokens**; Slack 11 tools ≈ **21k** (~1,900/tool); Sentry/Grafana ~600/tool; Anthropic hit **134k** internally.
- Anthropic's trigger for tool search is **">10K tokens" of definitions** — already exceeded.
- Tool search reduces definition cost **>85%**, loading 3–5 tools. Accuracy gains: Opus 4 **49%→74%**, Opus 4.5 **79.5%→88.1%**.
- Code execution with MCP: **150,000 → 2,000 tokens (98.7% saving)** in the Drive→Salesforce example.

**There is NO on-demand tool discovery mechanism in the spec.** SEP-1821 (Dynamic Tool Discovery) and SEP-1300 (Tool Filtering with Groups and Tags) are **unadopted issues**. "Progressive discovery" is a roadmap deliverable (roadmap updated 2026-08-22), not a feature.

**Free wins available today:**
1. **Deterministic `tools/list` ordering** — a spec SHOULD whose stated purpose is improving LLM prompt-cache hit rates.
2. High **`ttlMs`** (e.g. 300000) + correct **`cacheScope`**.
3. Trim `description` strings — they dominate the token cost.
4. ⚠️ **If you filter the catalog by scope, you MUST use `cacheScope: "private"`** — `"public"` lets different access tokens share the cache.

---

## 8. Rate limiting

**The spec has no numbers and no request/response size limit at all.** It only requires: "Servers MUST … Rate limit tool invocations."

Best public source with concrete values is **agentgateway** (Solo.io):
- Key insight: **"count sessions, not raw requests"** — ~5 POSTs per tool-call session in the legacy era (initialize → tools/list → tools/call)
- Published config: **5 req/s + burst 10** on the MCP route; **10000/min** gateway-wide ceiling; **per-tool** limits of 3/min (expensive) vs 10/min (rest), each with an independent Redis counter
- Response headers: `x-ratelimit-limit`, `x-ratelimit-remaining`, `x-ratelimit-reset`

**Where to apply limits now that sessions are gone:** per-IP, per-token/tenant, per-method (`Mcp-Method`), per-tool (`Mcp-Name`) — the last two **without parsing the JSON body**, which is exactly why SEP-2243 added those headers.

Real-world bug worth avoiding: LiteLLM's MCP Gateway silently capped `tools/list` at 100 tools and ignored `nextCursor` (https://github.com/BerriAI/litellm/issues/32229).

**Tasks** moved to the `io.modelcontextprotocol/tasks` extension: `tasks/get` polling + `tasks/update`; `tasks/list` removed; server-directed with no per-request opt-in; cancellation is cooperative. ⚠️ **A server MUST NOT return a task to a client that did not declare the extension** — so long-running tools need **both** a synchronous and a task code path, selected from per-request `clientCapabilities`. Client support is limited ("Host support varies by client").

---

## 9. Section 8 — memory-server production (SQLite, backup, licensing, token budget)

**🚨 LICENSING LANDMINE — verified two independent ways.** `jinaai/jina-reranker-v2-base-multilingual` is **CC-BY-NC-4.0 = NON-COMMERCIAL**. The model card says verbatim: *"This model repository is licenced for research and evaluation purposes under CC-BY-NC-4.0. For commercial usage, please refer to Jina AI's APIs, AWS Sagemaker or Azure Marketplace offerings."* HF front-matter: `license: cc-by-nc-4.0`. **fastembed shipping it does not relicense it** — fastembed's Apache-2.0 covers code, not weights, and "downloads at runtime" is not a loophole because CC-BY-NC restricts *use*. **Vestige's only licensing problem is the reranker; the embedder (nomic-embed-text-v1.5, Apache-2.0) is clean.** Fixing the reranker clears the whole issue.

**🔴 SQLite WAL-reset corruption bug — directly hits Vestige's deployment shape.** Present in **3.7.0 (2010-07-21) – 3.51.2 (2026-01-09)**, fixed in **3.51.3 (2026-03-13)**, backported to 3.44.6 / 3.50.7. Trigger: *"two or more database connections open on the same file, in separate threads or processes, and when those two connections attempt to write or checkpoint at the same instant."* Phil Eaton published a non-test-harness reproducer **2026-08-23**, so it no longer requires pathological timing.
**✅ Vestige is currently OK — I verified from source:** `libsqlite3-sys 0.37.0/sqlite3/sqlite3.h:149` → `#define SQLITE_VERSION "3.51.3"`. Bundled SQLite is the fixed version. **But this is not permanently guaranteed** — building without the `bundled` feature links system SQLite, which on macOS is older than 3.51.3 and therefore vulnerable. Add a CI assertion on `rusqlite::version()`.

**🔴 Prompt caching is structurally broken by the current injection design.** Cache hashes are cumulative over `tools → system → messages (in that order)`, so *"changing any block at or before the breakpoint produces a different hash."* `session_context` injects freshly-retrieved, query-dependent memory early → **the prefix changes every turn → the cache never hits.** Fix: stable durable-memory block *before* a breakpoint, volatile retrieval *after*. Also **verify `catalog.rs::build_tools_list` emits deterministic order** — Rust `HashMap` iteration would make all 28 tool definitions part of a changing hash on every request. This is the same problem MCP's deterministic-`tools/list` SHOULD exists to solve.
- **Minimum cacheable length is model-dependent and the brief's premise was outdated:** **512** (Fable 5.1, Mythos 5.1, Opus 5, Fable 5, Mythos 5), **1024** (Opus 4.8, Sonnet 5/4.6/4.5, Opus 4.1/4, Sonnet 4), **2048** (Mythos Preview, Opus 4.7), **4096** (Opus 4.6, Opus 4.5). `token_budget: 2000` is above the 1024 floor but **below the 4096 floor for Opus 4.5/4.6** → budget presets should be model-aware. Cost: write 5-min **1.25×**, write 1-h **2×**, read **0.1×**. Max **4 breakpoints**; **20-block lookback window**; a cache entry only becomes available after the first response begins.

**🚩 CORRECTION to a widely-miscited claim — Mem0's own Table 2 shows full-context SCORING HIGHEST.** full-context = 26,031 tokens, J = **72.90%**; Mem0 = 1,764 tokens, J = **66.88%**. The paper says so explicitly: *"a full-context method that ingests a chunk of roughly 26 000 tokens still achieves the highest J score (approximately 73%)."* **Mem0's win is cost and latency, not accuracy.** Token reduction is 93.2% (my arithmetic), not the abstract's ">90%" for the graph variant (86.1%). The paper is also internally inconsistent on latency (91% abstract vs 92% §4.3), and the "26% over OpenAI" headline is against a baseline the authors admit wasn't measured fairly. **The mem0 README (April 2026) reports entirely different numbers (LoCoMo 92.5, 7.0K tokens) and discloses they come from the managed platform with proprietary optimizations not in the OSS SDK — do not mix the two.**

**✅ Best available anchor for an injection budget: Zep, 1.6k tokens.** LongMemEval: full-context/gpt-4o = 60.2%, 28.9 s, **115k tokens**; Zep/gpt-4o = **71.2%, 2.58 s, 1.6k tokens** → **98.6% fewer tokens, +18.5% accuracy**. Unlike Mem0, Zep beats full-context on accuracy — but it's also a vendor self-evaluation, Mem0's paper shows Zep *losing* on LOCOMO, and there's no independent replication. **1.6k corroborates Vestige's 2,000-token default as well-supported.**

**✅ "Lost in the Middle" gives the retrieval cap.** TACL 2023: *"using 50 documents instead of 20 retrieved documents only marginally improves performance (∼1.5% for GPT-3.5-Turbo and ∼1% for claude-1.3)."* → **top-50 buys 1–1.5 points for 2.5× the context; a relevance threshold plus a hard top-k of 10–20 is the evidence-supported default.** Caveat: 2023 paper, 2023 models — it does *not* support claims about current frontier models.

**✅ Mem0's chunk-size sweep empirically validates Vestige's "atomic memory" rule:** J by chunk size at k=2 — 128→59.56, **256→60.97 (peak)**, 512→58.19, 1024→50.68, 2048→48.57, 4096→51.79, 8192→60.53. **~256-token retrieval units beat 1024–4096-token chunks.** (Note: AGENTS.md's "degrades search recall by 40-60%" figure — I could not verify that number in primary sources.)

**✅ Anthropic endorses exactly Vestige's shape:** *"retrieving some data up front for speed, and pursuing further autonomous exploration at its discretion"* — small up-front injection + cheap agent-driven drill-down (i.e. `expandable` IDs + `get_batch`), not a large up-front dump. Anthropic's term is **"context rot"**, and it publishes **no numeric token budget** — don't attribute one. ("Context poisoning/distraction/confusion" is Drew Breunig's taxonomy, not Anthropic's.)

**🔴 Nomic task prefixes are mandatory and currently OFF.** Model card: *"the text prompt **must** include a **task instruction prefix**"* (`search_query:` / `search_document:`). `VESTIGE_NOMIC_PREFIXES` defaults to `off` → **the model is running off-label**, degrading retrieval quality. Fixing requires a full `regenerate_embeddings` (mixing prefixed and unprefixed vectors in one index silently degrades recall). Nomic publishes MTEB for 768 (62.28) → 512 (61.96) → 256 (61.04) → 128 (59.34) → 64 (56.10); **it does NOT publish a 384-dim row**, so Vestige's 768→384 truncation has no sourced number (interpolation ≈61.5, arithmetic only). Truncation order matters: `layer_norm` → slice → L2-normalise.

**🟠 Backup: the online backup API can livelock in exactly Vestige's shape.** SQLite's own docs: writes *"by an external process or thread using a database connection other than pDb"* are much more expensive and *"If the backup process is restarted frequently enough it may never run to completion and the backupDb() function may never return."* **`VACUUM INTO` does not have this problem** (consistent snapshot at statement start) and additionally *"all deleted content is purged from the backup, leaving behind no forensic traces"* — a GDPR-relevant property. SQLite documents exactly three safe methods: `sqlite3_rsync` (new in 3.47.0, 2024-10-21), `VACUUM INTO`, and the backup API. ⚠️ **There is NO official Apple or SQLite documentation for Time Machine + WAL-mode SQLite** — only a 2018 sqlite-users thread with a reply from D. Richard Hipp. Mitigation: back up via `VACUUM INTO` to a single file and exclude the live WAL from Time Machine.
**Litestream correction: it IS maintained again** — site says *"v0.5.x — Latest — Actively maintained"*; Ben Johnson returned around Oct 2025. But its mechanism (*"starts a long-running read transaction to prevent any other process from checkpointing"*) is in **architectural tension with running other writers on the same file**, and neither project documents the combination. v0.5.0 **cannot restore 0.3.x backups**. ⚠️ SQLite has **no PITR and no WAL archiving**.

**🟠 Vector index deletion — now has peer-reviewed answers.** arXiv:2512.06200 (NeurIPS 2025 Workshop, 2025-12-05): **lazy/tombstone deletion degrades recall LINEARLY** (predictable, Δ=(R_S−R_0)/S); eager deletion converges to a stable floor θ; "Deletion Control" alternates lazy + periodic rebuild using only 10% of queries as calibration. → **Tombstones are functionally sufficient; periodic rebuild is the practical answer.** Also: **SQLite now ships an OFFICIAL vector extension ("Vec1", v0.7, page dated 2026-08-28)** — but it's **IVFADC+OPQ, not HNSW**, needs a training step, and its own roadmap says *"Testing is insufficient."* Not a drop-in replacement for the USearch sidecar. `sqlite-vss` is **deprecated**; `sqlite-vec` is still pre-v1.
**🚩 Vestige's row-count staleness check for the `.hnsw` sidecar is a weak signal** — a same-count delete+insert is invisible. Use a monotonic change counter or content hash. And ⚠️ **the `.hnsw` file gets none of the crash-atomicity that WAL gives the table** — no primary source addresses this.

**🟠 GDPR.** Art. 2(2)(c) household exemption is undercut by **Recital 18**, which ends: *"However, this Regulation applies to controllers or processors which provide the means for processing personal data for such personal or household activities."* → **local-first removes the residency and processor problems but NOT the vendor's Art. 25/32/transparency obligations.** **Art. 32(1)(c)–(d) literally require the ability to restore data and to regularly test that measures work** — i.e. **restore-verified backups are a compliance obligation**, and Vestige's `needsBackup` trigger being agent-dependent is a real gap. Embeddings as personal data: Morris et al., arXiv:2310.06816 (EMNLP 2023) recovers *"92% of 32-token text inputs exactly"* and full names from clinical notes — ⚠️ but that's 32-token inputs only, models unnamed; **do not generalize to long memories or to nomic-embed-text-v1.5**, and **no EDPB/DPA statement exists**.
**⚠️ MCP's security spec says essentially NOTHING about at-rest encryption, PII, retention, deletion, or server-side secrets** — I verified this by exhaustive grep of the 2026-07-28 page. **Vestige must not claim "MCP-compliant at-rest posture"; there is nothing to comply with.**

**🟠 Keychain trap:** if you store the encryption key with the default `WhenUnlocked` accessibility class, **the 6-hour consolidation loop will silently fail while the screen is locked** — i.e. exactly when it should run. Use `AfterFirstUnlock`.

**⚠️ Version discrepancy to resolve:** `Cargo.lock` has `fastembed 5.13.4` / `ort 2.0.0-rc.12`, while upstream now has `fastembed 7.0.1` (2026-09-16) / `ort =2.0.0-rc.13`. fastembed ships ~weekly with major bumps — **pin exactly**. Also note fastembed **downloads ONNX Runtime binaries and model weights at runtime** (`ort-download-binaries-native-tls`), which matters for offline use, build reproducibility, and supply-chain audit.

---

## 10. Delivery note

- Full Polish report: `docs/research/mcp-production-best-practices-2026-09-19.md` (sections 0–8, executive summary, 2 implementation checklists, 280+ sourced URLs).
- Two sibling reports from other agents already in the workspace and **not overlapping** with this one: `mcp-memory-poisoning-literature-2026-09-19.md`, `MCP_SECURITY_TOOLING_2026-09-19.md`.

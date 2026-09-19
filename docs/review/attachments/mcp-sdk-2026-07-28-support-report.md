# MCP SDK Support for Spec Revision 2026-07-28 — State as of 2026-09-19

**Scope:** official + major MCP SDKs; every claim sourced to a primary endpoint (registry API, GitHub release, CHANGELOG, README, official docs).
**Research date:** 2026-09-19. All registry data pulled directly from crates.io / npm / PyPI / NuGet / proxy.golang.org / repo1.maven.org APIs.

---

## 0. Baseline: the 2026-07-28 revision itself

- The revision exists and is the **current latest** MCP spec. `https://modelcontextprotocol.io/docs/sdk` 302-redirects to `https://modelcontextprotocol.io/docs/2026-07-28/sdk`, whose page chrome reads "Version 2026-07-28 (latest)".
- Official announcement: **"The 2026-07-28 Specification"**, MCP blog, published 2026-07-28 — https://blog.modelcontextprotocol.io/posts/2026-07-28/
  - Headline changes: stateless protocol core, removal of `initialize`/`initialized` handshake (SEP-2575), removal of `Mcp-Session-Id` (SEP-2567), Multi Round-Trip Requests (SEP-2322), standardized HTTP headers (SEP-2243), cacheable list results (SEP-2549), tasks moved to an extension (SEP-2663), Roots/Sampling/Logging deprecated (SEP-2577).
- **The blog's SDK section (the closest thing to a consolidated vendor statement):**
  > "All four Tier 1 SDKs speak 2026-07-28 as of today: TypeScript, Python, Go, C#. Beyond the Tier 1 set, the Rust SDK supports the new spec in beta."
  - Notably **Java is absent** from that list. The "beta" characterisation of Rust is **now outdated** (see §3).
- Spec changelog page: https://modelcontextprotocol.io/specification/2026-07-28/changelog — "changes made to the Model Context Protocol (MCP) specification since the previous revision, 2025-11-25".

### Summary table (as of 2026-09-19)

| SDK | Latest release | Date | Speaks 2026-07-28? | First version with it |
|---|---|---|---|---|
| TypeScript (v1 line) | `@modelcontextprotocol/sdk` **1.30.0** | 2026-07-27 | **No** | — (never; migrate to v2) |
| TypeScript (v2 line) | `@modelcontextprotocol/server` **2.0.0** | 2026-07-27 | **Yes** (opt-in) | 2.0.0-beta.1 (2026-06-30); wire code first in 2.0.0-alpha.4 (2026-06-30) |
| Python | `mcp` **2.2.0** | 2026-09-07 | **Yes** | **2.0.0** (2026-07-28) |
| Python (v1 line) | `mcp` **1.30.0** | 2026-09-07 | **No** | — |
| Rust | `rmcp` **3.4.0** | 2026-09-15 | **Yes** | **3.0.0** (2026-07-28); draft support in 3.0.0-beta.1 (2026-07-23) |
| Go | `go-sdk` **v1.8.0** | 2026-09-14 (release) / 2026-09-04 (tag) | **Yes** | **v1.7.0** (2026-07-28) |
| Java | `io.modelcontextprotocol.sdk:mcp` **2.0.1** | 2026-08-19 | **No** | — (none exists) |
| C# | `ModelContextProtocol` **2.2.0** | 2026-08-13 | **Yes** | **2.0.0** stable (2026-07-28); draft in 2.0.0-preview.1 (2026-06-26) |

---

## 1. TypeScript SDK — `modelcontextprotocol/typescript-sdk`

**There is a v2 line, and it is published under *different npm package names* — not `@modelcontextprotocol/sdk`.**

### (a) Current latest released versions

| Package | Latest | npm publish time (UTC) |
|---|---|---|
| `@modelcontextprotocol/sdk` (v1 line) | **1.30.0** | 2026-07-27T17:56:01.640Z |
| `@modelcontextprotocol/server` (v2) | **2.0.0** | 2026-07-27T23:55:22.239Z |
| `@modelcontextprotocol/client` (v2) | **2.0.0** | 2026-07-27T23:55:22.113Z |
| `@modelcontextprotocol/core` (v2) | **2.0.0** | 2026-07-27T23:55:21.808Z |

Source: `https://registry.npmjs.org/@modelcontextprotocol/sdk`, `.../server`, `.../client`, `.../core`.

- `dist-tags` for `@modelcontextprotocol/sdk` = `{"latest":"1.30.0"}` **only** — **there is no `next` tag** on the v1 package.
- `dist-tags` for `@modelcontextprotocol/server` = `{"latest":"2.0.0"}` only — v2 has graduated; no separate `next`/`beta` tag remains.
- v1.30.0 GitHub release published 2026-07-27T17:54:36Z: https://github.com/modelcontextprotocol/typescript-sdk/releases/tag/1.30.0
- v2 also ships middleware packages, all at 2.0.0 published 2026-07-27T23:55Z: `@modelcontextprotocol/node`, `/express`, `/fastify`, `/hono`, `/codemod`, `/server-legacy`.

### (b) Support for 2026-07-28

- **v2: YES, but opt-in.** `main` branch README: "**This is the `main` branch — v2 of the SDK** (`@modelcontextprotocol/server`, `@modelcontextprotocol/client`), implementing the [2026-07-28 MCP spec]… **v2 is the stable release line**, released alongside the 2026-07-28 spec. v1.x continues to receive bug fixes and security updates for at least 6 months after v2's release."
  https://github.com/modelcontextprotocol/typescript-sdk/blob/main/README.md
- **Critical nuance — opt-in, not default.** The v2 spec-support guide states: "Nothing in v2 puts a 2026-07-28 byte on the wire by default: a hand-constructed `Client` / `Server` / `McpServer` keeps speaking the 2025-era protocol it was written for. Serving or speaking 2026-07-28 is always an explicit opt-in" via `versionNegotiation` (`{mode:'auto'}`, `{pin:'2026-07-28'}`) or the modern HTTP entry point.
  https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28.html
- **v1: NO.** The v1.30.0 release notes contain only v1 maintenance work (Zod method literals, SSE keep-alive timers, Content-Type validation, a Hono dependency advisory) with **no mention of 2026-07-28**, and the v2 guide directs v1 users to migrate first: "If you are on `@modelcontextprotocol/sdk` (v1.x), start with `upgrade-to-v2.md` instead."

### (c) Exact version that first added 2026-07-28 support

- **First formally announced: `@modelcontextprotocol/server@2.0.0-beta.1`, published 2026-06-30T22:48:22.239Z.** Changelog entry (#2402): "**First beta release of SDK v2 with support for the MCP 2026-07-28 specification**".
  https://github.com/modelcontextprotocol/typescript-sdk/blob/main/packages/server/CHANGELOG.md
- **First artifact carrying 2026-07-28 wire code: `2.0.0-alpha.4`, published 2026-06-30T21:00:23.727Z** (same day, ~2h earlier). Its changelog entry (#2286) adds per-era wire codecs and `createMcpHandler(...)` "that serves the 2026-07-28 draft revision per request". This is a *draft*-era implementation, so beta.1 is the defensible "first 2026-07-28 support" answer.
- Final-revision alignment landed in **2.0.0** itself (#2513): "Align the 2026-07-28 wire with the final revision (spec PR #3002): `serverInfo` moves from the `DiscoverResult` body to the result `_meta`, and the per-request envelope's `clientInfo` demotes from required to SHOULD."

### (d) Status notes

- v2 is a **monorepo package split**: server / client / core + runtime middleware adapters. This is a breaking import-path migration from v1.
- PRs are rate-limited while v2 settles: "We're limiting pull requests to 1 per new contributor while v2 settles after the 2026-07-28 spec release."
- Documentation split: v1 at https://ts.sdk.modelcontextprotocol.io/ , v2 at https://ts.sdk.modelcontextprotocol.io/v2/ .

---

## 2. Python SDK — `modelcontextprotocol/python-sdk`

### (a) Current latest released version

- **PyPI `mcp` latest = 2.2.0**, uploaded **2026-09-07T16:06:19.711091Z** (`requires_python >=3.10`). Source: `https://pypi.org/pypi/mcp/json` and `https://pypi.org/pypi/mcp/2.2.0/json`.
- GitHub release **v2.2.0** published 2026-09-07T15:53:57Z: https://github.com/modelcontextprotocol/python-sdk/releases/tag/v2.2.0
- **Both lines are still shipping.** Recent PyPI uploads:
  - 2.x: 2.0.0 (2026-07-28), 2.1.0 (2026-08-24), 2.1.1 (2026-08-25), 2.0.1 (2026-08-26), **2.2.0 (2026-09-07)**
  - 1.x: 1.28.1 (2026-06-26), 1.29.0 (2026-07-28), 1.29.1 (2026-08-24), **1.30.0 (2026-09-07T14:34:14Z)**

### (b) Support for 2026-07-28

- **v2: YES, and as a first-class capability of the stable release.** v2.0.0 release notes: "This is v2.0.0, the stable v2 release of the MCP Python SDK. **It supports the 2026-07-28 revision of the Model Context Protocol and serves every earlier revision from the same server.** `pip install mcp` now installs 2.x."
  https://github.com/modelcontextprotocol/python-sdk/releases/tag/v2.0.0
- v2 README: "**This is v2 of the MCP Python SDK, the current stable release line.** It is a major rework of the SDK, both to support the [2026-07-28 MCP specification](https://modelcontextprotocol.io/specification/2026-07-28) (and every earlier revision)…"
  https://github.com/modelcontextprotocol/python-sdk/blob/main/README.md
- v2 docs: "v2 speaks the 2026-07-28 revision of MCP, which removes the connection handshake, the session, and every server-initiated request."
  https://py.sdk.modelcontextprotocol.io/whats-new/
- **v1 line: NO.** Verified directly in source on the `v1.x` branch, `src/mcp/types.py`:
  ```python
  LATEST_PROTOCOL_VERSION = "2025-11-25"
  ```
  and `src/mcp/shared/version.py`:
  ```python
  SUPPORTED_PROTOCOL_VERSIONS = ["2024-11-05", "2025-03-26", "2025-06-18", LATEST_PROTOCOL_VERSION]
  ```
  `2026-07-28` appears nowhere in the v1 supported-version list.
  https://github.com/modelcontextprotocol/python-sdk/blob/v1.x/src/mcp/types.py · https://github.com/modelcontextprotocol/python-sdk/blob/v1.x/src/mcp/shared/version.py

### (c) Exact version that first added 2026-07-28 support

- **`mcp` 2.0.0**, released **2026-07-28** (PyPI upload 2026-07-28T13:45:28.853348Z; GitHub release v2.0.0 published 2026-07-28T13:41:36Z/13:42:35Z) — i.e. the same day as the spec revision.
- Pre-release ladder (all PyPI): 2.0.0a1 (2026-06-11), 2.0.0a2 (2026-06-16), 2.0.0a3 (2026-06-26), 2.0.0b1 (2026-06-30), 2.0.0b2 (2026-07-14), 2.0.0rc1 (2026-07-27). These were draft-era. **The first *stable* release is 2.0.0.**

### (d) Status notes / conflicting sources

- **FLAG — documented v1 policy conflicts with actual v1 releases.** v2.0.0's notes said: "**v1.x is in maintenance mode and will only receive security fixes from now on.**" But v1.29.1 (2026-08-24) and v1.30.0 (2026-09-07) shipped afterwards with substantive bug fixes and **behaviour changes** (idle Streamable HTTP session expiry, OAuth issuer validation, redirect-origin restriction). The current main README softens this to "continues to receive critical bug fixes and security patches", and the v1.30.0 notes call themselves a "Maintenance release of the 1.x line". Treat "security fixes only" as **not accurate as of 2026-09-19**.
- Pin guidance from the SDK: use `mcp>=1.28,<2` to stay on v1.
- v2 API surface changed: `FastMCP` → `MCPServer`; a first-class `Client` replaces transport + `ClientSession` + `initialize()` layering.

---

## 3. Rust SDK — `modelcontextprotocol/rust-sdk` (`rmcp`)

### (a) Current latest released version

- **crates.io `rmcp` latest / max_stable_version = 3.4.0**, created **2026-09-15T15:44:08.726244Z**. Crate totals: 65 versions, 27,579,718 all-time downloads, `created_at` 2025-03-16T09:32:51Z, `updated_at` 2026-09-15T15:44:08Z.
  Source: `https://crates.io/api/v1/crates/rmcp` (and `https://crates.io/api/v1/crates/rmcp/versions`)
- GitHub release **rmcp-v3.4.0** published 2026-09-15T15:44:15Z: https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.4.0
- Companion crate `rmcp-macros` is versioned in lockstep (3.4.0, same timestamp).
- docs.rs: https://docs.rs/rmcp (latest = 3.4.0)

Recent 3.x cadence from crates.io: 3.0.0 (2026-07-28) · 3.0.1 (07-29) · 3.1.0 (07-31) · 3.1.1 (08-05) · 3.1.2 (08-07) · 3.1.3 (08-17) · 3.1.4 (08-20) · 3.2.0 (08-31) · 3.3.0 (09-10) · **3.4.0 (09-15)**.

### (b) Support for 2026-07-28

**YES.** Verbatim from the repo's root `README.md` (main branch):

> "This SDK implements the stable MCP **`2026-07-28`** specification while remaining fully compatible with the **`2025-11-25`** release and earlier versions. Features introduced in `2026-07-28` — server discovery & negotiation, transport-neutral subscriptions, long-running tasks, response caching, multi-round-trip requests, and standard HTTP routing headers — are documented below."

https://github.com/modelcontextprotocol/rust-sdk/blob/main/README.md

Concrete API evidence in the same README: `ProtocolVersion::V_2026_07_28`, `ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::V_2026_07_28] }`, and `ClientLifecycleMode::Auto { …, legacy_version: Some(ProtocolVersion::V_2025_11_25) }`. Troubleshooting note: `with_stateless_protocol_metadata_required(true)` rejects the compatibility path.

### (c) Exact version that first added 2026-07-28 support

- **First stable release: `rmcp` 3.0.0**, published **2026-07-28T22:52:36.694064Z** (crates.io). GitHub release `rmcp-v3.0.0` dated 2026-07-28. Changelog for 3.0.0 lists the fix "recognize 2026 MCP methods (#1076)".
  https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/CHANGELOG.md
- **First *any* release carrying the 2026-07-28 feature set: `3.0.0-beta.1`, published 2026-07-23T18:50:13.170872Z** — while the revision was still a draft. That changelog section adds, in one batch: "add server discovery and negotiation (SEP-2575)", "add SEP-2243 HTTP standard headers", "add MRTR model types / MRTR behavior support (SEP-2322)", "implement SEP-2549 cache hints", "add subscription listen streams (SEP-2575)", "add modern client lifecycle modes (SEP-2575)", plus the "Other" entry "**update for 2026-07-28 version (#1032)**" and a BREAKING rename clarifying that `legacy_session_mode` "only affects legacy protocol versions (`< 2026-07-28`); per SEP-2567 the `2026-07-28` draft version is always served statelessly".
- **The prior line did not have it:** `rmcp` 2.0.0 (2026-06-27 GitHub / 2026-06-29 crates.io) was "align model types with MCP 2025-11-25 spec (#927)". `rmcp` 2.2.0 (2026-07-08) still targeted 2025-11-25.

### (d) Official vs community status, and spec-compatibility table

- **OFFICIAL — confirmed from three independent primary sources:**
  1. crates.io `description`: "Rust SDK for Model Context Protocol"; `repository`: `https://github.com/modelcontextprotocol/rust-sdk/` — i.e. published under the `modelcontextprotocol` GitHub org.
  2. README, first line: "An **official** Rust Model Context Protocol SDK implementation with tokio async runtime."
  3. `https://modelcontextprotocol.io/docs/sdk` (→ `/docs/2026-07-28/sdk`) lists Rust as an **Official SDK, Tier 1**, alongside TypeScript, Python, C#, Go.
- **FLAG — no dedicated Rust "spec version compatibility" table exists.** The `rmcp` README has **prose only** (the paragraph quoted above) plus per-feature `**MCP Spec:**` links and a transport feature table — I found **no** version-vs-spec-revision matrix in the README or in `crates/rmcp/README.md`. The task's premise of a Rust "spec version compatibility table" is **not confirmed**. Tables of that exact kind *do* exist for **Go** (README) and **Java** (CHANGELOG) — see §4 and §5.
- **FLAG — the official blog is now stale on Rust.** The 2026-07-28 announcement said "the Rust SDK supports the new spec **in beta**". That matched `3.0.0-beta.x` on the day of publication, but as of 2026-09-19 `rmcp` 3.4.0 is a stable release whose README claims implementation of the *stable* 2026-07-28 spec. Sources conflict by date, not by substance.
- 3.x is a **breaking release**: the README carries a "**Migrating to 3.x?**" banner pointing at https://github.com/modelcontextprotocol/rust-sdk/discussions/969

---

## 4. Go SDK — `modelcontextprotocol/go-sdk`

### (a) Current module version

- **v1.8.0** is the latest module version.
  - `https://proxy.golang.org/github.com/modelcontextprotocol/go-sdk/@latest` → `{"Version":"v1.8.0","Time":"2026-09-04T08:08:52Z","Origin":{…"Ref":"refs/tags/v1.8.0"}}`
  - GitHub release **v1.8.0** published **2026-09-14T08:05:46Z**: https://github.com/modelcontextprotocol/go-sdk/releases/tag/v1.8.0
- **FLAG — date discrepancy to be aware of:** proxy.golang.org reports the tag/commit time as 2026-09-04, while GitHub reports the *release publication* as 2026-09-14. Both refer to v1.8.0; the ten-day gap is tag-vs-release-publication, not a version conflict.
- Full tag list (proxy.golang.org `@v/list`): … v1.6.1, v1.7.0-pre.1/2/3, **v1.7.0**, v1.8.0-pre.1/2, **v1.8.0**.

### (b) + (c) Support for 2026-07-28, and first version

**YES — first added in v1.7.0.** The repo README contains an explicit, official "Version Compatibility" table:

> | SDK Version | Latest MCP Spec | All Supported MCP Specs |
> |---|---|---|
> | **v1.7.0+** | **2026-07-28** | 2026-07-28, 2025-11-25\*, 2025-06-18, 2025-03-26, 2024-11-05 |
> | v1.4.0 - v1.6.1 | 2025-11-25\* | 2025-11-25\*, 2025-06-18, 2025-03-26, 2024-11-05 |
> | v1.2.0 - v1.3.1 | 2025-11-25\*\* | 2025-11-25\*\*, 2025-06-18, 2025-03-26, 2024-11-05 |
> | v1.0.0 - v1.1.0 | 2025-06-18 | 2025-06-18, 2025-03-26, 2024-11-05 |
>
> \* Client side OAuth has experimental support. \*\* Partial support for 2025-11-25 (client side OAuth and Sampling with tools not available).

https://github.com/modelcontextprotocol/go-sdk/blob/main/README.md

Corroborated by the v1.7.0 release notes: "**This release brings full support for protocol version 2026-07-28.** The wire protocol is largely rewritten: a stateless model with per-request `_meta`, a new `server/discover` RPC replacing the initialize handshake, multi-round-trip requests (MRTR) replacing server-initiated calls, a unified `subscriptions/listen` stream…"
https://github.com/modelcontextprotocol/go-sdk/releases/tag/v1.7.0

- **Version:** **v1.7.0** — GitHub release published 2026-07-28T13:09:53Z; proxy tag time 2026-07-27T15:20:53Z (tag ahead of publication by ~22h).
- Pre-releases that led up to it: v1.7.0-pre.1 (2026-06-24), -pre.2 (2026-07-09), -pre.3 (2026-07-17). v1.7.0-pre.3 was already used in production by GitHub ("serving more than half a million users", per the v1.7.0 notes).

### (d) Status notes

- **Opt-in for HTTP:** "The streamable HTTP transport accepts requests at protocol version 2026-07-28 only when `StreamableHTTPOptions.Stateless = true`. If you want to expose the new protocol over HTTP, set `Stateless = true`; if you want to keep stateful sessions, your clients will negotiate down to 2025-11-25."
- Backward compatibility preserved on every endpoint; the SDK "negotiates the highest mutually-supported version at connect time".
- Roots/sampling/logging deprecated as of 2026-07-28 by SEP-2577, retained for ≥12 months.
- `docs/README.md` still links the older spec revision ("These docs mirror the official MCP spec" → `/specification/2025-06-18`), i.e. the prose docs lag the README compatibility table. **FLAG: minor internal doc inconsistency.**

---

## 5. Java SDK — `modelcontextprotocol/java-sdk`

### (a) Current Maven version

- **`io.modelcontextprotocol.sdk:mcp` latest = `release` = 2.0.1**, `maven-metadata.xml` `lastUpdated` = **20260819143251**.
  Source: `https://repo1.maven.org/maven2/io/modelcontextprotocol/sdk/mcp/maven-metadata.xml`
- GitHub release **v2.0.1** published 2026-08-19T13:47:03Z: https://github.com/modelcontextprotocol/java-sdk/releases/tag/v2.0.1
- Sibling artifacts under the same groupId: `mcp-core`, `mcp-json`, `mcp-json-jackson2`, `mcp-json-jackson3`, `mcp-bom`, `mcp-spring-webflux`, `mcp-spring-webmvc`, `mcp-test`, `client-jdk-http-client`, `server-servlet`, `conformance-tests`.

### (b) Support for 2026-07-28

**NO — not supported as of 2026-09-19.** The repo `CHANGELOG.md` carries an explicit release-lines / spec-revision table:

> | Line | Latest | Spec revision | Status |
> |---|---|---|---|
> | 2.0.x | [2.0.1](…/v2.0.1) (2026-08-19) | **2025-11-25** | Active development |
> | 1.1.x | [1.1.4](…/v1.1.4) (2026-08-19) | 2025-06-18 | Security patches only |
> | 0.18.x | [0.18.4](…/v0.18.4) (2026-08-19) | 2025-06-18 | Security patches only |

https://github.com/modelcontextprotocol/java-sdk/blob/main/CHANGELOG.md

Corroborated by the 2.0.0 entry: "**First major release since 1.x, tracking the 2025-11-25 MCP specification.**" (v2.0.0 GitHub release 2026-06-11T15:43:52Z; Maven 2.0.0 present.) The README's own CLI example still passes `--spec-version 2025-11-25`.

Independent corroboration: the article list of SDKs shipping 2026-07-28 on release day names TypeScript, Python, Go, C# — **not Java** (https://blog.modelcontextprotocol.io/posts/2026-07-28/), and the official SDK page classifies Java as **Tier 2**, whose tier requirement is new-protocol-feature support "Within 6 months" rather than "before new spec version release" (https://modelcontextprotocol.io/community/sdk-tiers).

### (c) Exact version that first added 2026-07-28 support

- **None exists.** No Java SDK version supporting 2026-07-28 was found on Maven Central (complete version list ends at 2.0.1), in the GitHub releases list, or in the CHANGELOG. The latest 2.0.x line explicitly targets 2025-11-25.
- **FLAG:** no public Java roadmap/ETA for 2026-07-28 was located in the README or CHANGELOG within this research. Unverified.

### (d) Status notes

- 2.0.1 (2026-08-19) is a security/hardening release ("Bound STDIO and HTTP client/server reads to a configurable maximum size").
- Release lines 1.1.x and 0.18.x are on **2025-06-18** and receive security patches only.

---

## 6. C# SDK — `modelcontextprotocol/csharp-sdk`

### (a) Current NuGet version

- **`ModelContextProtocol` latest = 2.2.0**, published **2026-08-13T08:56:14.417Z** (NuGet registration); GitHub release v2.2.0 published 2026-08-13T08:45:54Z. 47 versions total on NuGet.
  Sources: `https://api.nuget.org/v3-flatcontainer/modelcontextprotocol/index.json`, `https://api.nuget.org/v3/registration5-gz-semver2/modelcontextprotocol/index.json`
- Related packages: `ModelContextProtocol.Core`, `ModelContextProtocol.AspNetCore`, `ModelContextProtocol.Extensions.Apps`, `ModelContextProtocol.Extensions.Tasks` (per README).
- Recent NuGet history: 1.4.1 (2026-07-09) → 2.0.0-preview.2 (07-09) → 2.0.0-preview.3 (07-15) → 2.0.0-rc.1 (07-25) → 2.0.0-rc.2 (07-28) → **2.0.0 (2026-07-28)** → 2.1.0 (08-05) → **2.2.0 (08-13)**.

### (b) Support for 2026-07-28

**YES.** The official .NET blog post — **"Announcing v2.0 of the official MCP C# SDK"**, by **Jeff Handley**, dated **July 28th, 2026** — states:

> "The Model Context Protocol (MCP) C# SDK has reached **v2.0**, implementing the **2026-07-28 revision of the MCP specification**. It's the largest revision of the protocol since it launched."

https://devblogs.microsoft.com/dotnet/announcing-v20-of-the-official-mcp-csharp-sdk/

Corroborated by the GitHub release notes for v2.0.0: "**Version 2.0.0 brings the C# SDK into stable alignment with the MCP 2026-07-28 specification.**"
https://github.com/modelcontextprotocol/csharp-sdk/releases/tag/v2.0.0

### (c) Exact version that first added 2026-07-28 support

- **First stable: `ModelContextProtocol` 2.0.0** — NuGet published 2026-07-28T21:38:31.737Z; GitHub release v2.0.0 published 2026-07-28T21:27:41Z.
- **First pre-release carrying it: `2.0.0-preview.1`** (GitHub release 2026-06-26T18:30:11Z; NuGet 2026-06-26T18:36:51.903Z), whose notes say: "This is the first preview of the C# MCP SDK 2.0.0, **designed for alignment with the 2026-07-28 MCP specification release**. The SDK implements the 2026-07-28 protocol version which fundamentally changes how clients and servers interact — removing the initialize handshake (SEP-2575), eliminating server-side session state (SEP-2567), introducing Multi Round-Trip Requests (SEP-2322)…"
  - Caveat: preview.1 was built against the **draft**; `2.0.0-preview.3` notes explicitly say "correctness fixes to the **draft** 2026-07-28 protocol". So **2.0.0 stable is the first release aligned with the *final* revision.**

### (d) Status notes

- v2.0 is **backward compatible**: "v2.0 is backward compatible. Upgrading the SDK doesn't force you off the clients and servers you already have, and your stable v1 code keeps compiling and running."
- **Stateless by default:** `HttpServerTransportOptions.Stateless` now defaults to `true`.
- Roots/Sampling/Logging APIs marked `[Obsolete]` (MCP9005/9006 diagnostics) per SEP-2577.
- Tasks moved to `ModelContextProtocol.Extensions.Tasks`, with no wire/API compatibility to the v1.4.x experimental implementation.

---

## 7. Consolidated official page listing SDKs × supported spec revisions

**No such consolidated page was found.** Detail:

- **`https://modelcontextprotocol.io/docs/sdk`** 302-redirects to **`https://modelcontextprotocol.io/docs/2026-07-28/sdk`** (title "SDKs"). It lists official SDKs **by tier only** — TypeScript, Python, C#, Go, Rust = **Tier 1**; Java, Ruby = **Tier 2**; Swift, PHP, Kotlin = **Tier 3** — and states that all SDKs support servers/clients, local+remote transports, and "Protocol compliance with type safety". **There is no column or row for supported spec revisions.** It also has no per-SDK version numbers.
- **`https://modelcontextprotocol.io/docs/2026-07-28/sdk`** was checked directly (last fetched 2026-09-19); the fetched text contains no "2025-11-25", "2026-07-28"-per-SDK mapping, or compatibility matrix. Tiers link to `/community/sdk-tiers`.
- **`https://modelcontextprotocol.io/community/sdk-tiers`** defines the tiering *requirements*, not a revision matrix. Relevant to spec-revision timing:
  - Tier 1: conformance "100% pass rate"; New Protocol Features — "**Before new spec version release**, timeline agreed per release based on feature complexity".
  - Tier 2: conformance "80% pass rate"; New Protocol Features — "**Within 6 months**".
  - Key dates: **2026-01-23** conformance tests available; **2026-02-23** official SDK tiering published.
  - Conformance scoping: "Tests for the specification version the SDK targets … Excluding legacy backward-compatibility tests (unless the SDK claims legacy support)".
  - https://modelcontextprotocol.io/community/sdk-tiers
- The **docs index** (`https://modelcontextprotocol.io/llms.txt`) confirms the only SDK page per spec version is `docs/<revision>/sdk.md` — there is no separate "spec version support matrix" page in the docs tree. (Checked via `https://modelcontextprotocol.io/llms.txt`.)
- **The nearest official consolidated statement** remains the blog's SDK paragraph: "All four Tier 1 SDKs speak 2026-07-28 as of today: TypeScript, Python, Go, C#. Beyond the Tier 1 set, the Rust SDK supports the new spec in beta." — https://blog.modelcontextprotocol.io/posts/2026-07-28/ (published 2026-07-28; superseded for Rust).
- **Per-SDK compatibility tables do exist, but inside individual repos**, not on the docs site:
  - **Go** README "Version Compatibility" table — https://github.com/modelcontextprotocol/go-sdk/blob/main/README.md
  - **Java** `CHANGELOG.md` "Release lines" table — https://github.com/modelcontextprotocol/java-sdk/blob/main/CHANGELOG.md
  - **TypeScript / Python / C#** have no such table; they express spec support via release notes plus per-language docs sites (`ts.sdk.modelcontextprotocol.io`, `py.sdk.modelcontextprotocol.io`, `csharp.sdk.modelcontextprotocol.io`).
  - **Rust** has no table (prose only).

---

## 8. Explicitly unverified / flagged

1. **Rust "spec version compatibility" table — NOT FOUND.** The task assumed one; the `rmcp` README has prose only. Nearest equivalents are the Go README table and the Java CHANGELOG table.
2. **Java 2026-07-28 roadmap — NOT FOUND.** No ETA, milestone, or roadmap entry for 2026-07-28 was located in the Java README, CHANGELOG, or release list. Whether/when Java will support it is **unknown** from primary sources checked.
3. **Rust first-support version is fence-posted.** `3.0.0-beta.1` (2026-07-23) shipped the feature set against the *draft*; `3.0.0` (2026-07-28) is the first *stable*. If "support for revision 2026-07-28" means the final revision, the answer is **3.0.0**; if it means the 2026-07-28 feature set, it is **3.0.0-beta.1**.
4. **TypeScript v2 first-support version is likewise fence-posted** between `2.0.0-alpha.4` (wire code) and `2.0.0-beta.1` (first announced beta) — both published 2026-06-30, ~2 hours apart.
5. **Go v1.8.0 date conflict:** proxy.golang.org tag time 2026-09-04T08:08:52Z vs GitHub release publication 2026-09-14T08:05:46Z. Same version; different event timestamps. I could not determine which the SDK maintainers consider canonical.
6. **C# pre-release spec status:** preview.1/2/3 targeted the *draft* 2026-07-28 (preview.3 notes say "draft 2026-07-28 protocol"). I did **not** individually verify whether rc.1/rc.2 already matched the final revision — `2.0.0` stable is the only version I verified as aligned with the final spec.
7. **Python v1 "security fixes only" claim is contradicted** by subsequently shipped v1 bug-fix releases with behaviour changes (see §2d). The v1.x branch source (`LATEST_PROTOCOL_VERSION = "2025-11-25"`) is the authoritative confirmation that v1 lacks 2026-07-28 — I read the branch head, which I assume corresponds to the current 1.30.0; I did not byte-verify the 1.30.0 sdist itself.
8. **GitHub REST API was IP-rate-limited** during part of this research (403 "API rate limit exceeded for 83.5.52.217"). Where the API failed I used `releases.atom` feeds, raw.githubusercontent.com, and rendered release pages instead; release bodies quoted above were extracted from those. No quoted figure depends on a search-engine snippet.
9. **`@modelcontextprotocol/sdk` v1 has no `next`/beta dist-tag** — I verified the complete `dist-tags` object is `{"latest":"1.30.0"}`. A "separate v2 package/next tag" exists only as the **separate package names** (`@modelcontextprotocol/server` etc.), not as a tag on the v1 package.

---

## Sources

All URLs verified reachable on **2026-09-19**.

### Registries (primary API endpoints)
- https://crates.io/api/v1/crates/rmcp — rmcp latest = 3.4.0, created 2026-09-15T15:44:08.726244Z (fetched 2026-09-19)
- https://crates.io/api/v1/crates/rmcp/versions — rmcp 3.x publish timestamps incl. 3.0.0 @ 2026-07-28T22:52:36.694064Z, 3.0.0-beta.1 @ 2026-07-23T18:50:13.170872Z (fetched 2026-09-19)
- https://docs.rs/rmcp — rmcp docs, latest 3.4.0 (fetched 2026-09-19)
- https://registry.npmjs.org/@modelcontextprotocol/sdk — latest 1.30.0 @ 2026-07-27T17:56:01.640Z, dist-tags `{latest:1.30.0}` (fetched 2026-09-19)
- https://registry.npmjs.org/@modelcontextprotocol/server — latest 2.0.0 @ 2026-07-27T23:55:22.239Z; betas from 2026-06-30 (fetched 2026-09-19)
- https://registry.npmjs.org/@modelcontextprotocol/client — latest 2.0.0 @ 2026-07-27T23:55:22.113Z (fetched 2026-09-19)
- https://registry.npmjs.org/@modelcontextprotocol/core — latest 2.0.0 @ 2026-07-27T23:55:21.808Z (fetched 2026-09-19)
- https://pypi.org/pypi/mcp/json — latest 2.2.0; full release timeline incl. 2.0.0 @ 2026-07-28T13:45:28.853348Z, 1.30.0 @ 2026-09-07T14:34:14.266679Z (fetched 2026-09-19)
- https://pypi.org/pypi/mcp/2.2.0/json — 2.2.0 uploaded 2026-09-07T16:06:19.711091Z (fetched 2026-09-19)
- https://api.nuget.org/v3-flatcontainer/modelcontextprotocol/index.json — 47 versions, last = 2.2.0 (fetched 2026-09-19)
- https://api.nuget.org/v3/registration5-gz-semver2/modelcontextprotocol/index.json — 2.0.0 @ 2026-07-28T21:38:31.737Z, 2.2.0 @ 2026-08-13T08:56:14.417Z (fetched 2026-09-19)
- https://proxy.golang.org/github.com/modelcontextprotocol/go-sdk/@latest — v1.8.0, Time 2026-09-04T08:08:52Z (fetched 2026-09-19)
- https://proxy.golang.org/github.com/modelcontextprotocol/go-sdk/@v/list — full tag list (fetched 2026-09-19)
- https://repo1.maven.org/maven2/io/modelcontextprotocol/sdk/mcp/maven-metadata.xml — latest/release 2.0.1, lastUpdated 20260819143251 (fetched 2026-09-19)

### Official MCP documentation & blog
- https://modelcontextprotocol.io/docs/sdk — redirects to /docs/2026-07-28/sdk; official SDK list + tiers, no spec-revision matrix (fetched 2026-09-19)
- https://modelcontextprotocol.io/community/sdk-tiers — tier requirements + key dates 2026-01-23 / 2026-02-23 (fetched 2026-09-19)
- https://modelcontextprotocol.io/specification/2026-07-28/changelog — "changes since 2025-11-25" (fetched 2026-09-19)
- https://modelcontextprotocol.io/llms.txt — docs index; confirms one `sdk` page per revision, no compatibility-matrix page (fetched 2026-09-19)
- https://blog.modelcontextprotocol.io/posts/2026-07-28/ — "The 2026-07-28 Specification", published 2026-07-28; SDK section (fetched 2026-09-19)

### SDK repositories — release notes, READMEs, CHANGELOGs
- https://github.com/modelcontextprotocol/typescript-sdk/releases/tag/1.30.0 — published 2026-07-27T17:54:36Z
- https://github.com/modelcontextprotocol/typescript-sdk/releases/tag/@modelcontextprotocol/server@2.0.0 — published 2026-07-27T23:55:41Z
- https://github.com/modelcontextprotocol/typescript-sdk/blob/main/README.md — v2 = main; "released alongside the 2026-07-28 spec"
- https://github.com/modelcontextprotocol/typescript-sdk/blob/main/packages/server/CHANGELOG.md — #2402 "First beta release of SDK v2 with support for the MCP 2026-07-28 specification"; #2286; #2513
- https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28.html — "2026-07-28 is always an explicit opt-in" (fetched 2026-09-19)
- https://github.com/modelcontextprotocol/python-sdk/releases/tag/v2.0.0 — published 2026-07-28T13:41:36Z; "supports the 2026-07-28 revision"
- https://github.com/modelcontextprotocol/python-sdk/releases/tag/v2.2.0 — published 2026-09-07T15:53:57Z
- https://github.com/modelcontextprotocol/python-sdk/releases/tag/v1.30.0 — published 2026-09-07T14:03:59Z
- https://github.com/modelcontextprotocol/python-sdk/blob/main/README.md — v2 is the current stable line
- https://github.com/modelcontextprotocol/python-sdk/blob/v1.x/src/mcp/types.py — `LATEST_PROTOCOL_VERSION = "2025-11-25"` (fetched 2026-09-19)
- https://github.com/modelcontextprotocol/python-sdk/blob/v1.x/src/mcp/shared/version.py — supported versions exclude 2026-07-28 (fetched 2026-09-19)
- https://py.sdk.modelcontextprotocol.io/whats-new/ — "v2 speaks the 2026-07-28 revision" (fetched 2026-09-19)
- https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.4.0 — published 2026-09-15T15:44:15Z
- https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.0.0 — dated 2026-07-28
- https://github.com/modelcontextprotocol/rust-sdk/releases.atom — rmcp release timeline (fetched 2026-09-19)
- https://github.com/modelcontextprotocol/rust-sdk/blob/main/README.md — "An official Rust Model Context Protocol SDK implementation"; "implements the stable MCP `2026-07-28` specification"
- https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/CHANGELOG.md — 3.0.0-beta.1 (2026-07-23) SEP batch + "update for 2026-07-28 version (#1032)"; 2.0.0 (2026-06-27) = 2025-11-25
- https://github.com/modelcontextprotocol/rust-sdk/discussions/969 — 3.x migration guide
- https://github.com/modelcontextprotocol/go-sdk/releases/tag/v1.7.0 — published 2026-07-28T13:09:53Z; "full support for protocol version 2026-07-28"
- https://github.com/modelcontextprotocol/go-sdk/releases/tag/v1.8.0 — published 2026-09-14T08:05:46Z
- https://github.com/modelcontextprotocol/go-sdk/blob/main/README.md — "Version Compatibility" table: v1.7.0+ → 2026-07-28
- https://github.com/modelcontextprotocol/java-sdk/releases/tag/v2.0.1 — published 2026-08-19T13:47:03Z
- https://github.com/modelcontextprotocol/java-sdk/blob/main/CHANGELOG.md — release-lines table: 2.0.x → 2025-11-25
- https://github.com/modelcontextprotocol/java-sdk/blob/main/README.md — `--spec-version 2025-11-25` example
- https://github.com/modelcontextprotocol/csharp-sdk/releases/tag/v2.0.0 — published 2026-07-28T21:27:41Z; "stable alignment with the MCP 2026-07-28 specification"
- https://github.com/modelcontextprotocol/csharp-sdk/releases/tag/v2.0.0-preview.1 — published 2026-06-26T18:30:11Z; "first preview … designed for alignment with the 2026-07-28 MCP specification release"
- https://github.com/modelcontextprotocol/csharp-sdk/releases/tag/v2.0.0-preview.3 — published 2026-07-15; "draft 2026-07-28 protocol"
- https://github.com/modelcontextprotocol/csharp-sdk/releases/tag/v2.2.0 — published 2026-08-13T08:45:54Z
- https://github.com/modelcontextprotocol/csharp-sdk/blob/main/README.md — official C# SDK; package list
- https://devblogs.microsoft.com/dotnet/announcing-v20-of-the-official-mcp-csharp-sdk/ — Jeff Handley, July 28th 2026; "reached v2.0, implementing the 2026-07-28 revision of the MCP specification"

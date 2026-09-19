# MCP Security Scanning / Red-Teaming / Defense Tooling — State as of 2026-09-19

All versions and dates below were verified against primary sources (GitHub releases via authenticated
`gh api`, PyPI JSON API, npm registry API, official docs). Anything unverifiable is marked
**NIE ZWERYFIKOWANO**.

> **HEADLINE FINDINGS**
> 1. `mcp-scan` no longer exists as a product — it is a **redirect stub** on PyPI. The project was
>    renamed **Snyk Agent Scan** and lives at **github.com/snyk/agent-scan**. The old
>    `invariantlabs-ai/mcp-scan` GitHub URL **301-redirects** to it. Invariant Labs was acquired by
>    Snyk (June 2025).
> 2. **Promptfoo is now part of OpenAI** — acquisition announced **2026-03-09**. Still MIT-licensed
>    and open source.
> 3. **Semgrep does have official MCP rules** — 6 MCP rules + 6 agent-skill rules, in
>    `semgrep-rules/ai/ai-best-practices/`, added March–May 2026.
> 4. **Docker MCP Gateway has 8 published security advisories**, 6 of them dated **2026-07-09**,
>    including 2 CRITICAL and 3 HIGH.
> 5. The current MCP spec revision is **2026-07-28** — a major statelessness rewrite. There *is* an
>    official security best-practices doc, versioned across spec revisions.
> 6. The official MCP Registry does **no** security scanning and explicitly disclaims moderation.

---

## 1. mcp-scan → Snyk Agent Scan (Invariant Labs / Snyk)

**Name:** mcp-scan (legacy) / **Snyk Agent Scan** (current)

**URLs**
- Legacy repo: https://github.com/invariantlabs-ai/mcp-scan → **HTTP 301 → https://github.com/snyk/agent-scan** (verified with `curl -sI`)
- Current repo: https://github.com/snyk/agent-scan
- Legacy PyPI: https://pypi.org/project/mcp-scan/ (redirect stub)
- Current PyPI: https://pypi.org/project/snyk-agent-scan/
- Releases: https://github.com/snyk/agent-scan/releases
- Risk reference: https://github.com/snyk/agent-scan/blob/main/docs/risks.md
- Issue codes (v0.5.x): https://github.com/snyk/agent-scan/blob/main/docs/issue-codes.md

**Current version + release date**
| Package | Version | Release date | Notes |
|---|---|---|---|
| `snyk-agent-scan` (PyPI) | **0.6.3** | **2026-09-10** | Current stable |
| `snyk-agent-scan` (GitHub `v0.6.3`) | v0.6.3 | 2026-09-10T08:23:04Z | Stable |
| `snyk-agent-scan` (GitHub snapshot) | v0.6.4-snapshot-6e2d290-1649 | **2026-09-19T05:58:32Z** | Prerelease snapshot, builds published same day |
| `mcp-scan` (PyPI, stub) | **0.4.3** | **2026-03-02** | 2.1 kB wheel; `requires_dist: ['snyk-agent-scan']`; summary says *"This package has been renamed to snyk-agent-scan. This is a redirect package that installs snyk-agent-scan and forwards the mcp-scan CLI to it."* |

**Maintainer / status:** ACTIVE and heavily maintained. Repo `pushed_at = 2026-09-19T05:58:32Z`.
Owner is now **Snyk** (org `snyk`, repo `agent-scan`), license **Apache-2.0**, 3,066 stars.
Maintainer login on releases: `lbeurerkellner` (Lukas Beurer-Kellner, Invariant co-founder) — the
original Invariant team continues to ship it. Repo created 2025-04-07 (continuity of the old repo,
not a fresh fork).

**What it detects**

It is now a *supply-chain scanner for agent components* (MCP servers, agent skills, and agent
harnesses), not just MCP servers. Two output generations coexist:

*v0.6+ (current) — scored risk indicators* (score 0–1000; Low 100 / Medium 300 / High 600 /
Critical 1000), using the `2026-07-10` analysis API. MCP server risks:
- `prompt_injection_tool_desc` — prompt injection hidden in tool descriptions
- `untrusted_content` — server reads attacker-submittable channels (email, issue trackers, tickets) → indirect prompt injection
- `private_data` — server can retrieve sensitive non-public data
- `destructive_capabilities` — state-changing/system-command tools

Skill risks: `prompt_injection_skill_instructions`, `suspicious_download_url`, `malicious_code`,
`insecure_credential_handling`, `secret_detection`, `direct_money_access`,
`third_party_content_exposure`, `unverifiable_dependencies`, `modifying_system_services`,
`missing_skill_md`.

*v0.5.x (planned deprecation) — issue codes*: 15+ risks across MCP servers and skills, incl.
Prompt Injection (E001), **Tool Poisoning (E001)**, **Tool Shadowing (E002)**, **Toxic Flows**;
skills: E004 prompt injection, E006 malware payloads, W011 untrusted content, W007 credential
handling, W008 hardcoded secrets.

**On your three specific questions:** tool poisoning ✅, cross-origin/cross-server escalation ✅
(as "Tool Shadowing" E002 + "Toxic Flow" analysis), prompt injection ✅ (both tool-desc and skill
instructions).

**2026 changes**
- Renamed from `mcp-scan` to **Agent Scan**; PyPI rename stub published **2026-03-02**.
- **v0.6.0 (2026-08-19)** — output model rewritten from issue codes to **scored risk indicators**
  (0–1000) tied to Snyk Evo; new `scan_path_responses` JSON; `--show-full-discovery` flag.
- **v0.4 (approx. Feb 2026)** — added **agent skill scanning**, shipped with a technical report on
  agent-skill ecosystem threats (`.github/reports/skills-report.pdf`).
- **v0.3.29** — `evo` command to push results to **Snyk Evo** (enterprise platform).
- 2026 release cadence (stable): 0.5.4→0.6.3 across Jan–Sep 2026; snapshots near-daily.
- Requires a **SNYK_TOKEN** for scanning — the analysis is server-side/Snyk-cloud-backed.
- Security warning in README: scanning MCP configs **executes the commands defined in them**; default
  requires per-server y/n consent; `--dangerously-run-mcp-servers` to skip; sandbox recommended.

**Snyk acquisition — VERIFIED.** The 301 redirect from `invariantlabs-ai/mcp-scan` to `snyk/agent-scan`
plus the PyPI rename stub are conclusive primary evidence that mcp-scan is now part of Snyk. The
acquisition itself was announced June 2025 ([ETH Zürich](https://inf.ethz.ch/news-and-events/spotlights/infk-news-channel/2025/06/eth-spin-off-aquired-by-snyk.html),
[SiliconANGLE 2025-06-24](https://siliconangle.com/2025/06/24/snyk-acquires-invariant-labs-expand-ai-agent-security-capabilities/)).
The `invariantlabs-ai` GitHub org still exists with other repos (`invariant`, `explorer`,
`invariant-gateway`, `mcp-injection-experiments`) but `mcp-scan` itself is gone.

**Verification status:** VERIFIED. Sources: PyPI JSON `https://pypi.org/pypi/mcp-scan/json` (version
0.4.3, license_expression Apache-2.0, requires_dist `snyk-agent-scan`), PyPI JSON
`https://pypi.org/pypi/snyk-agent-scan/json` (0.6.3, 2026-09-10), GitHub releases API, `curl -sI`
301 check, `docs/risks.md` on `main`.

---

## 2. Promptfoo — MCP red teaming + MCP Proxy

**Name:** Promptfoo (**now part of OpenAI** as of 2026-03-09)

**URLs**
- Repo: https://github.com/promptfoo/promptfoo
- Docs home: https://www.promptfoo.dev/
- **MCP security testing guide:** https://www.promptfoo.dev/docs/red-team/mcp-security-testing/
- **MCP plugin (attack vectors):** https://www.promptfoo.dev/docs/red-team/plugins/mcp/
- **MCP provider:** https://www.promptfoo.dev/docs/providers/mcp/
- Release notes: https://www.promptfoo.dev/docs/releases/
- GitHub releases: https://github.com/promptfoo/promptfoo/releases
- npm: https://www.npmjs.com/package/promptfoo
- MCP Proxy product page: https://www.promptfoo.dev/mcp/
- Acquisition announcement: https://www.promptfoo.dev/blog/promptfoo-joining-openai/

**Current version + release date**
- **0.123.1** — npm `latest`; published **2026-09-18T01:27:57Z** (npm) / GitHub release 2026-09-18T01:07:54Z
- 0.123.0 — 2026-09-10
- 0.122.2 — 2026-08-28 · 0.122.1 — 2026-08-26 · 0.122.0 — 2026-08-04
- 0.121.20 — 2026-07-31 · 0.121.19 — 2026-07-14 · 0.121.18 — 2026-07-08

**Maintainer / status:** ACTIVE. License **MIT** (repo LICENSE + npm `license: MIT`). 25,280 stars.
`pushed_at = 2026-09-19T02:39:38Z`. Acquired by **OpenAI**, announced **March 9, 2026** — the blog
states it "will remain open source" and continue to support diverse providers/models. Site footer
now reads "Promptfoo is now part of OpenAI".

**What it detects / does (MCP-specific)**

The **`mcp` red team plugin** generates attacks and grades responses for MCP-specific vulnerabilities.
Documented attack vectors:
- Function Discovery (exposing hidden tools)
- Parameter Injection
- Function Call Manipulation
- Excessive Function Call / recursion DoS
- System Information Leakage
- Function Output Manipulation
- **Tool Metadata Injection** — smuggling instructions via tool names/descriptions
- Unauthorized Tool Invocation / Privilege Escalation

The **MCP security testing guide** documents 3 threat-model scenarios and explicitly targets:
tool poisoning via hidden instructions in tool descriptions, sensitive data exfiltration via
side channels, **authentication hijacking and rug pulls**, **tool shadowing** and indirect prompt
injection, and **cross-server attacks**. Maps to the **OWASP Agentic AI Top 10** (T1–T15).
Recommended companion plugins: `pii`, `bfla`, `bola`, `sql-injection`.

`promptfoo redteam` CLI drives scans; `redteam generate` accepts `-d/--description`; strategies
include `basic`, `best-of-n`, `jailbreak`, plus multi-turn `hydra`, `custom-strategy`,
`mischievous-user`, `jailbreak:meta`. Outputs SARIF (since Oct 2025).

**MCP as a target/provider:** `providers: [{ id: mcp, config: { enabled: true, server: {...} } }]`
supports stdio (`command`/`args`/`env`) and remote (`url`/`headers`) servers, **multiple servers
simultaneously**, and auth types `bearer`, `basic`, `api_key`, plus **MCP OAuth with proactive token
refresh** (Dec 2025).

**2026 changes**
- **2026-03-09** — OpenAI acquisition announced.
- **January 2026 release highlights** — adaptive rate limiting, Transformers.js local inference,
  telecom red team plugins, video-generation providers, `promptfoo logs`, multi-input red team testing,
  strategy `numTests` config, early scan stop.
- **December 2025** — **OWASP Agentic AI Top 10 complete T1–T15 threat mapping**
  (https://www.promptfoo.dev/docs/red-team/owasp-agentic-ai/), OWASP API Security Top 10 example,
  MCP OAuth, multi-modal layer strategy, OpenTelemetry tracing.
- **November 2025** — `hydra` multi-turn strategy, code scanning, VS Code red team extension,
  OWASP Agentic Top 10 preset.
- **MCP Proxy** (enterprise, https://www.promptfoo.dev/mcp/) — runtime enforcement: whitelist approved
  MCP servers, per-app/per-user granular access control, real-time monitoring with PII/sensitive-data
  alerts, centralized audit logs and policy dashboard. First announced in the July 2025 release notes
  ("Enterprise-grade security for MCP servers with access control and traffic monitoring").
  No public version number or standalone repo — **NIE ZWERYFIKOWANO** for any MCP Proxy version.

**Verification status:** VERIFIED for version/license/date (npm registry JSON `promptfoo/latest` →
0.123.1, license MIT; GitHub releases API; LICENSE file). VERIFIED for MCP features via the three
official docs URLs. Acquisition date VERIFIED from the Promptfoo blog post (2026-03-09). The OpenAI
newsroom page returned HTTP 403 from this environment, so OpenAI's own copy of the announcement was
**NIE ZWERYFIKOWANO** directly.

---

## 3. Semgrep — official MCP rules

**Name:** Semgrep MCP rules (in the community `semgrep-rules` repository)

**URLs**
- Rules repo: https://github.com/semgrep/semgrep-rules
- **MCP rules directory:** https://github.com/semgrep/semgrep-rules/tree/develop/ai/ai-best-practices
- Registry: https://semgrep.dev/registry (https://semgrep.dev/r)
- Example rule permalink pattern: `https://semgrep.dev/r/generic.ai-best-practices.mcp-tool-poisoning`
- Product: https://semgrep.dev

**Current version + release date**

`semgrep-rules` is a continuously-updated rule repository (no semantic version/releases). The MCP
ruleset's introduction is datable by commit:

| Event | Date | Commit message |
|---|---|---|
| MCP rule family introduced | **2026-03-23T20:17:18Z** | `add ai rules from bestpractices` |
| Metadata pass | 2026-03-24T12:33:32Z | `Add metadata` |
| Rule fixes | 2026-03-27 (×5 commits) | `fix rules again` / `fix more rules` / `Fix languages mixed` |
| **Skill + MCP tool rules added** | **2026-05-10T06:51:52Z** | `feat: add skill manifest and MCP tool security rules` |

`semgrep-rules` repo: license **Semgrep Rules License v1.0** (SPDX `NOASSERTION`; see
https://semgrep.dev/legal/rules-license) — **not** an OSI open-source license. 1,254 stars,
`pushed_at = 2026-09-09T14:26:53Z`. Semgrep CLI itself (`semgrep/semgrep`) is **LGPL-2.1**, 16,697 stars.

**The 6 MCP rules** (`ai/ai-best-practices/<name>/<name>.yaml`, all tagged `technology: [mcp]`):

| Rule ID | Severity | CWE | Detects |
|---|---|---|---|
| `mcp-tool-poisoning-generic` | ERROR | CWE-77 | Hidden directives in Python tool docstrings: `<IMPORTANT>`, `~/.ssh`, `~/.cursor/mcp.json`, `/etc/shadow`, "read/cat/load .env", `do not mention`, `do not tell the user` |
| `mcp-ssrf-python` (taint mode) | ERROR | CWE-918 | MCP `@server.tool()` handler params flowing into `requests.get(...)` unvalidated |
| `mcp-command-injection-python` (taint mode) | ERROR | CWE-78 | Tool params flowing into `os.system(...)` |
| `mcp-unsanitized-return-python` (taint mode) | WARNING | CWE-116 | External HTTP response flowing into an MCP tool return → prompt-injection payload delivery |
| `mcp-typosquatted-tool-name-python` | WARNING | CWE-1357 | Tool names like `githbu_search` / `filesytem_read` — typosquats of well-known tools |
| `mcp-credential-in-response-python` | WARNING | CWE-522 | Tool returning dicts with `api_key`/`password`/`secret`/`token`/`access_token` |
| `mcp-hardcoded-config-secret-generic` | ERROR | CWE-798 | Plaintext keys in `*mcp*.json` / `claude_desktop_config.json` (`sk-`, `sk-ant-`, `sk-proj-`, `hf_`, `AIza`) |

Plus **agent-skill rules** added 2026-05-10: `skill-md-base64-payload`, `skill-md-prompt-injection`,
`skill-md-data-exfiltration`, `skill-md-sensitive-file-access`, and
`claude-settings-auto-enable-mcp` (WARNING — flags `enableAllProjectMcpServers: true`).

All MCP rules cite `https://modelcontextprotocol.io/specification/draft/basic/security_best_practices`
in their `metadata.references`.

**2026 changes:** the entire MCP ruleset is a 2026 addition (March–May 2026). Note the
`mcp-typosquatted-tool-name` rule also references the community
[Agent-Threat-Rule](https://github.com/Agent-Threat-Rule/agent-threat-rules) standard.

**Verification status:** VERIFIED. Method: `gh api search/code?q=repo:semgrep/semgrep-rules+mcp`
(21 matches, paths enumerated), raw `.yaml` files fetched from the `develop` branch, commit history
via `gh api repos/semgrep/semgrep-rules/commits?path=...`, license via raw `LICENSE`.
Note: the Semgrep **registry** search API (`https://semgrep.dev/api/registry/rules?q=mcp`) does not
surface these rules by keyword — they were verified from the GitHub source of truth instead.

---

## 4. Cisco AI Defense MCP Scanner

**Name:** Cisco AI Defense MCP Scanner (`cisco-ai-mcp-scanner`, module `mcpscanner`)

**URLs**
- Repo: https://github.com/cisco-ai-defense/mcp-scanner
- PyPI: https://pypi.org/project/cisco-ai-mcp-scanner/
- Releases: https://github.com/cisco-ai-defense/mcp-scanner/releases
- Product: https://www.cisco.com/site/us/en/products/security/ai-defense/index.html
- API docs: https://developer.cisco.com/docs/ai-defense/getting-started/#base-url
- Discord: https://discord.com/invite/nKWtDcXxtx

**Current version + release date**
- **4.8.4** — released **2026-08-28T17:43:44Z** (GitHub release) / PyPI upload 2026-08-28T17:44:15Z
- 4.8.3 — 2026-08-07 · 4.8.2 — 2026-07-30 · 4.8.1 — 2026-07-21 · 4.8.0 — 2026-07-14
- 4.7.7 — 2026-07-13 · 4.7.5 — 2026-06-24 · 4.7.4 — 2026-06-18 · 4.7.3 — 2026-06-05

Note: `pyproject.toml` on `main` says `version = "4.8.4"` — consistent with the latest release.

**Maintainer / status:** ACTIVE. Cisco AI Defense (`cisco-ai-defense` org), license **Apache-2.0**,
1,074 stars, repo created 2025-09-24, `pushed_at = 2026-09-19T00:02:05Z` (commits after the 4.8.4
release, e.g. `fix: use MCP-registered tool names across languages (#209)` on 2026-09-19,
`fix(codeql): safely scan fork pull requests (#260)` 2026-09-17 — so 4.8.5 is likely imminent).
Requires Python ≥3.11.4. Primary maintainers: `harishsg999`, `shrey-bagga`, `sisambam`, `mohitk1995`.

**What it detects / does**

**Three scanning engines**, usable together or independently:
1. **YARA rules** — pattern-based detection; custom YARA rules supported.
2. **LLM-based analysis** ("LLM-as-a-judge" renamed to "LLM-based analysis" in 4.8.4) — via LiteLLM;
   tested with OpenAI GPT-4o/GPT-4.1, default model `gpt-5.2`, AWS Bedrock Claude Sonnet 4.5,
   Azure OpenAI. Separate **Code Behavioral Analyzer** and **LLM meta-analyzer** for cross-analyzer
   finding enrichment.
3. **Cisco AI Defense inspect API** — API-based scanning via `MCP_SCANNER_API_KEY` +
   `MCP_SCANNER_ENDPOINT` (e.g. `https://us.api.inspect.aidefense.security.cisco.com/api/v1`).

Additional capabilities:
- **Behavioural code scanning** of MCP server source code for threats
- **Vulnerable package scanning** — Python deps via `pip-audit` (CVE/PYSEC/GHSA)
- **VirusTotal binary scanning** — SHA256 hash lookup on bundled binaries (images, PDFs, executables,
  archives); optional upload (`MCP_SCANNER_VIRUSTOTAL_UPLOAD_FILES`, default false)
- **PyPI package scanning in a Docker sandbox** with behavioral analysis (added 4.8.0)
- **Readiness scanning** — zero-dependency static analysis (timeouts, retries, error handling)
- **Static/offline scanning** of pre-generated JSON — for CI/CD and air-gapped environments
- Scans **tools, prompts, resources, and server instructions**
- **MCP server integration** — connect directly to stdio, SSE, or streamable HTTP servers; full
  **OAuth support** for SSE and streamable HTTP
- Runs as **CLI** (`mcp-scanner`) or **REST API server**
- Structured reporting; `--format summary`, JSON output; CI-friendly exit codes

**2026 changes**
- **4.8.4 (2026-08-28)** — transient-error retry in analyzer; "LLM-as-a-judge" → "LLM-based analysis"
- **4.8.3 (2026-08-07)** — **dynamic tool registration detection**
- **4.8.0 (2026-07-14)** — **Docker-sandboxed PyPI and npm package scanner with behavioral analysis**;
  meta-analyzer improvements
- **4.7.7 (2026-07-13)** — live MCP scans routed through a **hybrid proxy relay** (AIFW-28548)
- **4.7.5 (2026-06-24)** — behavioral logging; CodeQL open-redirect fix
- **4.7.4 (2026-06-18)** — YARA keyword false-positive fix; **LLM meta-analyzer** for cross-analyzer
  finding enrichment

**Verification status:** VERIFIED. Sources: GitHub releases API, repo metadata API, raw
`pyproject.toml`, raw `README.md`, PyPI JSON API. **License note:** the repo declares Apache-2.0
(README badge) and `gh api` reports `license.spdx_id = Apache-2.0`, but the PyPI JSON metadata has
**no** `license`/`license_expression`/License classifier — so the PyPI-recorded license is
**NIE ZWERYFIKOWANO** (repo license is authoritative and verified).

---

## 5. Docker MCP Gateway

**Name:** Docker MCP Gateway (part of the `docker mcp` CLI plugin / MCP Toolkit)

**URLs**
- Repo: https://github.com/docker/mcp-gateway
- Releases: https://github.com/docker/mcp-gateway/releases
- **Security model doc:** https://github.com/docker/mcp-gateway/blob/main/docs/security.md
- Catalog docs: https://docs.docker.com/ai/mcp-catalog-and-toolkit/catalog/
- Security model (docs site): https://docs.docker.com/ai/mcp-catalog-and-toolkit/security/
- Catalog registry: https://github.com/docker/mcp-registry
- Advisories: https://github.com/docker/mcp-gateway/security/advisories

**Current version + release date**
- **v0.43.3** — published **2026-07-16T18:17:41Z** (latest tag; tagged `prerelease=true` on GitHub)
- v0.43.1 — 2026-06-25 · v0.42.3 — 2026-06-12 · v0.42.2 — 2026-05-28 · v0.42.1 — 2026-05-05
- v0.42.0 — 2026-04-30 · v0.40.4 — 2026-04-09 · v0.41.0 — 2026-03-18 · v0.40.3 — 2026-03-20

**Note:** every GitHub release in this repo is flagged `prerelease: true`; there are **zero**
non-prerelease GitHub releases, and `GET /releases/latest` returns 404. Distribution is primarily
via Docker Desktop / Docker Hub, so **the "official stable version" is NIE ZWERYFIKOWANO** — v0.43.3
is the newest tagged release.

**Maintainer / status:** ACTIVE. Docker, license **MIT**, 1,574 stars, created 2025-04-22,
`pushed_at = 2026-09-16T18:47:09Z` (commits ~2 months after the last tag).

**Security features (from the official `docs/security.md`, which documents the trust model)**

The doc frames the gateway as "a security boundary between MCP clients and the MCP servers". Trust
assumptions: local OS user, Docker daemon, credential store, interceptors and local config are
**trusted**; catalogs, profiles, server entries, OCI image labels, registry entries, README URLs and
remote MCP URLs are **untrusted**; the code inside an enabled MCP server **may be malicious**.

Documented boundaries:
- **HTTP transports** — `sse`/`streaming` require a **Bearer token** (`MCP_GATEWAY_AUTH_TOKEN`, else
  generated); `--allow-unauthenticated` is an explicit opt-out. `Origin`-bearing browser requests
  only from localhost; `Origin`-less requests allowed for non-browser clients; `/health` unauthenticated.
- **Catalogs / local files / OCI metadata** — catalog paths must resolve under `~/.docker/mcp/catalogs/`;
  `file://` refs resolved relative to that dir **with symlink containment checks**; OCI image labels
  treated as import metadata only, must **not** inject runtime-shaping fields (commands, volumes,
  secrets, env, user, nested config).
- **Remote URLs** — require **public HTTPS** by default; rejects userinfo, unsafe hostnames, loopback,
  private, link-local, metadata-service and other non-public IP ranges; validates redirect
  destinations and re-validates at dial time. `DOCKER_MCP_ALLOW_INSECURE_REMOTE_URLS=1` is a dev opt-out.
- **Image verification / signing** — signature verification **enabled by default for Docker Hub
  `mcp/` namespace images**, which must be **referenced by digest** and are verified before pull/run.
  `--verify-signatures=false` is the explicit opt-out. **Third-party images outside the `mcp/`
  namespace are NOT verified** with Docker MCP signatures.
- **Container execution** — containers do not inherit the host environment; only configured env,
  server config values, and declared secrets are passed. Started with Docker isolation,
  `no-new-privileges`, and CPU/memory limits. Host bind mounts are validated; named/anonymous volumes
  allowed; host path binds **default to read-only**, must resolve under trusted roots or
  `MCP_GATEWAY_DOCKER_BIND_ALLOWED_PATHS`, and cannot target sensitive system/credential paths;
  writable host binds require an exact `MCP_GATEWAY_DOCKER_BIND_ALLOW_WRITABLE_PATHS` entry.
- **Network egress is NOT globally denied by default** — servers request `disableNetwork`/`allowHosts`,
  or the operator runs `--block-network`.
- **Secrets management** — Docker Desktop secrets engine (`docker mcp secret`), keeps secrets out of
  env vars; v0.43.3 refactored to the `docker/secrets-engine` SDK.

**CVEs / security advisories — 8 published, 6 dated 2026-07-09**

| GHSA | CVE | Severity | Published | Summary | Affected |
|---|---|---|---|---|---|
| GHSA-g879-4j4f-6vj7 | CVE-2026-76092 | **CRITICAL** | 2026-07-09 | Unauthenticated access to proxied tools in container mode — auth middleware never installed when `DOCKER_MCP_IN_CONTAINER`; binds `0.0.0.0` | >=0.25.0 |
| GHSA-mqq5-qh4g-2g8g | CVE-2026-76099 | **CRITICAL** | 2026-07-09 | Unvalidated config-driven bind mount via dynamic tools — `mcp-add`/`mcp-config-set` ungated; catalog `Spec.Volumes` expanded into `docker -v` with no host-path allowlist → mount Docker socket → **root RCE on host** | >=0.9.3 |
| GHSA-6m8f-w97w-99h7 | CVE-2026-76094 | HIGH | 2026-07-09 | MCP server image signature verification off by default; even when on, `mcp/` prefix filter lets non-`mcp/` images run unverified | >=0.4.1 |
| GHSA-r2xf-7jw5-pjg6 | CVE-2026-55887 | HIGH | 2026-06-16 | Argument injection via OCI image label YAML — `io.docker.server.metadata` YAML-unmarshalled into wide `catalog.Server` struct → arbitrary `docker run` args → host FS mount, UID 0, RCE | >=0.21.0, <0.42.2 |
| GHSA-46gc-mwh4-cc5r | CVE-2025-64443 | HIGH | 2025-12-03 | DNS rebinding in `sse`/`streaming` transport → browser-based exploitation; not affected in default `stdio` mode | <= v0.27.0 (**patched in v0.28.0**) |
| GHSA-625j-rw87-4ghr | CVE-2026-76093 | LOW | 2026-07-09 | SSRF via attacker-controlled remote server URL — no host/range allowlist; can reach cloud instance-metadata endpoint | >=0.13.0 |
| GHSA-6pq5-p7fc-7xhq | CVE-2026-76095 | MEDIUM | 2026-07-09 | Arbitrary host file read via `file://` server reference — no base-directory confinement | >= 0.35.0 |
| GHSA-m5m2-mrxf-7j7q | CVE-2026-76097 | MEDIUM | 2026-07-09 | Tool-name shadowing across aggregated servers — flat namespace, no collision check, last-writer-wins; tool-name-prefix off by default | >=0.25.0 |

Only **CVE-2025-64443** has a documented patched version (v0.28.0). The API reports
`first_patched_version: none` for the other seven — **the patched versions for the 2026 advisories
are NIE ZWERYFIKOWANO** from the advisory metadata. Notably, the `docs/security.md` boundaries
(public-HTTPS-only remote URLs, default-on signature verification for `mcp/` images, read-only
default host binds with allowlists, Bearer auth on HTTP transports) map almost exactly onto these
advisories — i.e. the security doc appears to have been written as the post-remediation contract
(PR #532 "Document gateway security boundaries" shipped in v0.43.3).

**Docker MCP Catalog security scanning claims.** From the official Docker docs FAQ:
- Submission flow: PR to https://github.com/docker/mcp-registry, reviewed and approved, then available
  within 24 hours in Docker Desktop MCP Toolkit, the Docker MCP Catalog, and the Docker Hub `mcp` namespace.
- Automated process **"includes: Pulling and building the code in an ephemeral build environment.
  Testing initialization and functionality. Verifying that tools can be successfully listed."**
- Docker **rejects** submissions that "fail automated testing and validation processes during pull
  request review"; human reviewers evaluate against specific requirements.
- **Docker's own accountability disclaimer:** "Does Docker take accountability for malicious MCP
  servers in the Toolkit? Docker's security measures currently represent a **best-effort approach**.
  While Docker implements automated testing, scanning, and met[adata review]…"
- `docs/security.md` explicitly states: "Docker's MCP Catalog build-time review, dependency scanning,
  malware scanning, and publishing process are separate from this repository."
- **The specific catalog scanning tooling/engine used is NIE ZWERYFIKOWANO** — Docker describes
  scanning existing but does not name the scanner or publish detection coverage.

**2026 changes**
- **v0.43.3 (2026-07-16)** — `docker/secrets-engine` SDK adoption, writable bind mounts in Dockerfile,
  **documented gateway security boundaries** (PR #532), remote-URL client for OAuth.
- **v0.42.0 (2026-04-30)** — npm/npx server support in catalog; OAuth token exchange/refresh fix;
  MCP working-sets/profiles enabled by default.
- Six advisories disclosed 2026-07-09.

**Verification status:** VERIFIED. Sources: GitHub releases API, repo metadata API, repo
`security-advisories` API (all 8 advisories with descriptions and vulnerability ranges), raw
`docs/security.md`, raw `README.md`, Docker docs catalog + FAQ pages.
**NIE ZWERYFIKOWANO:** existence of any non-prerelease/stable release marker; patched versions for
the 7 advisories without them.

---

## 6. Official MCP documentation + registry security

### 6a. Official "Security Best Practices" doc — YES, IT EXISTS

- **Current (2026-07-28 revision):** https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices
- **Previous (2025-11-25 revision):** https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices
- **Draft:** https://modelcontextprotocol.io/docs/draft/tutorials/security/security_best_practices
- Legacy path `https://modelcontextprotocol.io/specification/2025-06-18/basic/security_best_practices`
  now **301-redirects** to the `docs/<revision>/tutorials/security/` location.
- Also linked as `.../specification/2026-07-28/basic/security_best_practices` (same redirect target).

Covered attacks and mitigations (section list, current revision):
Confused Deputy Problem · Token Passthrough · **Server-Side Request Forgery (SSRF)** ·
**SSRF Against Authorization Servers** · **State Handle Hijacking** (renamed from "Session Hijacking"
in 2025-11-25 — tracks the 2026-07-28 removal of protocol-level sessions) · Local MCP Server
Compromise · OAuth Authorization URL Validation · stdio Transport Security in Proxy Scenarios ·
**Mix-Up Attacks** *(new)* · **Localhost Redirect URI Impersonation** *(new)* · Scope Minimization ·
Common Mistakes.

Notable requirement language: MCP proxy servers **MUST** implement per-client consent (registry of
approved `client_id`s per user, checked before initiating third-party authz, stored server-side).

### 6b. Current MCP specification revision

- **Latest: 2026-07-28** — https://modelcontextprotocol.io/specification/2026-07-28
  (the bare `https://modelcontextprotocol.io/specification/` URL resolves here)
- Previous: 2025-11-25 · Earlier: 2025-06-18
- Changelog: https://modelcontextprotocol.io/specification/2026-07-28/changelog
- Repo: https://github.com/modelcontextprotocol/modelcontextprotocol (9,254 stars, `pushed_at = 2026-09-18`)

**2026-07-28 security-relevant changes** (from the official Key Changes doc):
- **Removes protocol-level sessions and the `Mcp-Session-Id` header**; list endpoints no longer vary
  per-connection. Cross-call state must use explicit server-minted handles passed as ordinary tool
  arguments (SEP-2567). Removal of implicit session identity is a security-positive change.
- **Makes MCP stateless** — removes the `initialize`/`notifications/initialized` handshake; every
  request carries `protocolVersion` + `clientCapabilities` in `_meta`; servers SHOULD identify
  themselves in each result's `_meta` (SEP-2575).
- Adds **`server/discover`** (MUST implement) to advertise supported versions, capabilities, identity.
- Replaces HTTP GET + `resources/subscribe`/`unsubscribe` with **`subscriptions/listen`**.
- Removes `ping`, `logging/setLevel`; log level set per-request via `_meta`.
- Moves experimental tasks out of core into an official extension.
- Introduces the **Multi Round-Trip Requests (MRTR)** pattern.

### 6c. Official MCP Registry (registry.modelcontextprotocol.io)

- Repo: https://github.com/modelcontextprotocol/registry — 7,264 stars, license `NOASSERTION`
- **Latest release: v1.8.1 — 2026-08-06T23:35:18Z** (v1.8.0 2026-07-13, v1.7.9 2026-05-12)
- **Status: PREVIEW.** Moderation policy states: *"The MCP Registry is currently in preview. Breaking
  changes or data resets may occur before general availability."*

**Security scanning / verification: essentially NONE.**
- **Moderation policy:** https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/moderation-policy.mdx
  - TL;DR: *"The MCP Registry is quite permissive! We only remove illegal content, malware, spam,
    and completely broken servers."*
  - Explicit disclaimer: *"The MCP Registry **does not** make guarantees about moderation, and
    consumers should assume minimal-to-no moderation."* … *"We largely rely on upstream package
    registries (like NPM, PyPI, and Docker) or downstream subregistries (like the GitHub MCP
    Registry) to do more in-depth moderation."*
  - **"What We Don't Remove" explicitly includes "Servers with security vulnerabilities."**
  - Removal = set `status: "deleted"`, metadata remains in the API.
- **No automated security scanning.** There is a **proposed, in-progress**
  [Enhanced Server Validation design](https://github.com/modelcontextprotocol/registry/blob/main/docs/design/proposed-enhanced-validation.md)
  (three tiers: Schema → Semantic → **Linter** for "security concerns, style guidelines, naming
  conventions"), explicitly flagged: *"This work is in progress … and may change significantly or be
  abandoned."* Not shipped.
- **No signing / attestation / provenance / SLSA / sigstore.** The `server.json` spec's `_meta` field
  is generic reverse-DNS extension metadata; a SHA-256 hash appears only for package integrity
  (`_meta`-level). The only verification is **publisher namespace ownership**: **DNS verification**
  and **HTTP verification** to prove ownership of a domain/subdomain.
- **Verdict: the official MCP registry is NOT a security control.** Ecosystem security is delegated
  downstream — e.g. the GitHub MCP Registry and Docker's catalog.

**Verification status:** VERIFIED from modelcontextprotocol.io (spec index, 2026-07-28 changelog,
security best practices), the registry GitHub repo raw docs (moderation policy, proposed validation),
the server.json spec, and the registry releases API.

---

## NEW 2026 TOOLING

### Runtime enforcement / firewalls / policy engines

| Tool | Version + date | License | What it does | URL |
|---|---|---|---|---|
| **ShieldCortex** | **5.0.5** — 2026-09-13 (npm latest) | MIT | "Memory firewall" for agent memory + **runtime tool gates** on Claude Code, OpenClaw, Hermes. Directly relevant to agent-memory security. First npm release 2.0.0 on 2026-02-01. | https://www.npmjs.com/package/shieldcortex · https://github.com/Drakon-Systems-Ltd/ShieldCortex |
| **praxiom** | **0.2.0** — 2026-09-07 | MIT | MCP gateway fronting several MCP servers that **verifies every tool call against a formal policy**. | https://pypi.org/project/praxiom/ · https://github.com/dreamLogicc/praxiom |
| **mcp-zero-trust-layer** | **0.6.0** — 2026-09-17 | Apache-2.0 | Open-source, self-hosted **Zero Trust policy enforcement layer** for MCP servers. First release 0.1.0 on 2026-06-13. | https://pypi.org/project/mcp-zero-trust-layer/ · https://github.com/686f6c61/mcp-zero-trust-layer |
| **avakill** | **1.2.0** — 2026-03-08 | AGPL-3.0-only | Open-source **safety firewall for AI agents — intercepts tool calls, enforces policies, kills dangerous operations**. First release 2026-02-18. | https://pypi.org/project/avakill/ · https://github.com/log-bell/avakill |
| **agent-airlock** | **0.10.7** — 2026-09-19 | Apache-2.0 | Pydantic-based **contract layer / type-checker for AI agent tool calls** — deny-by-default, in-process, strict argument validation, ghost-argument stripping, self-healing retries for MCP servers. First release 2026-01-31; 130 releases. | https://pypi.org/project/agent-airlock/ · https://github.com/sattyamjjain/agent-airlock |
| **Microsoft Agent Governance Toolkit (AGT)** | **v4.1.0** — 2026-06-09 | MIT | Policy enforcement, zero-trust identity, execution sandboxing for autonomous agents; claims 10/10 OWASP Agentic Top 10 coverage. MCP-specific components: **`MCPGateway`, `MCPSecurityScanner`, `MCPResponseScanner`, `MCPMessageSigner`, `MCPSessionAuthenticator`, `CredentialRedactor`**, nonce replay protection, schema-drift and **rug-pull detection**, OSV/CVE feed. Repo created **2026-03-02**; 6,287 stars; very active. Publishes an [NSA MCP compliance mapping](https://github.com/microsoft/agent-governance-toolkit/blob/main/docs/compliance/nsa-mcp-alignment.md) (last reviewed 2026-09-18) self-assessing 8/11 NSA themes covered, 3 partial. | https://github.com/microsoft/agent-governance-toolkit · https://microsoft.github.io/agent-governance-toolkit/compliance/nsa-mcp-alignment/ |
| **Promptfoo MCP Proxy** | no public version | commercial | Runtime MCP proxy: server allowlisting, per-app/per-user access control, real-time PII monitoring, centralized audit. | https://www.promptfoo.dev/mcp/ |

### Detection-rule standards + scanners

| Tool | Version + date | License | What it does | URL |
|---|---|---|---|---|
| **Agent Threat Rules (ATR)** | **v4.0.0** — 2026-08-23 | MIT | Open **detection-rule standard for AI agent threats** — "like Sigma, but for AI agents". Executable rules across **10 categories** covering prompt injection, tool poisoning, **MCP attacks**, skill compromise. Repo created 2026-03-09, 394 stars, `pushed_at 2026-09-18`. Claims integration into Microsoft AGT, Cisco AI Defense, MISP, OWASP, FINOS and SigmaHQ. Ships a 2026 mega-scan paper ("96,096 Skills, 751 Malware: A Large-Scale Security Audit of the AI Agent Ecosystem") and an `MCP-Attack-Surface-2026` paper. | https://github.com/Agent-Threat-Rule/agent-threat-rules |
| **pyatr** | **0.3.0** — 2026-08-23 | MIT | Python engine for ATR; bundles the rule set. | https://pypi.org/project/pyatr/ |
| **mcphound** | **0.1.5** — 2026-08-31 | Apache-2.0 | Independent MCP server + agent-skill **security scanner and reputation layer**. Discovers MCP servers configured in Claude Code/Desktop, Cursor, Windsurf, Gemini CLI, OpenCode; checks hardcoded secrets, download-and-execute launch commands, over-broad permissions, pinned-version drift; **SARIF export** for GitHub code scanning; maps to OWASP LLM + Agentic Top 10. Roadmap: tool-description poisoning, typosquats, runtime rug-pulls, public reputation DB + GitHub Action. Explicitly positioned as complementary to Snyk Agent Scan. | https://pypi.org/project/mcphound/ · https://github.com/markdoyle4312-hash/mcphound |
| **shanefirek/mcp-security-scanner** | no releases; repo created 2026-05-29 | MIT | Static security scanner for MCP server configs and code, **mapped to the NSA's May 2026 MCP security guidance**, "35+ checks across 11 threat categories". ⚠️ 0 stars, single commit 2026-05-29 — **abandoned/low-maturity, treat with caution**. | https://github.com/shanefirek/mcp-security-scanner |

### Vendor / commercial MCP security in 2026

| Vendor | Product | Notes | URL |
|---|---|---|---|
| **Palo Alto Networks** | **Prisma AIRS MCP Server** | Documented product: "Prisma AIRS MCP Server for Centralized AI Agent Security"; docs cover configuring MCP server security using Prisma AIRS. Part of AI Runtime Security. | https://docs.paloaltonetworks.com/ai-runtime-security/activation-and-onboarding/prisma-airs-mcp-server-for-centralized-ai-agent-security/understanding-the-prisma-airs-mcp-server |
| **Wiz** | Wiz Research + MCP security guidance | Published **MCP Auto-Execution: From Git Clone to Cloud Compromise in Amazon Q VS Code Extension** (2026-06-26) disclosing **CVE-2026-12957 / CVE-2026-12958** in the Amazon Q VS Code extension (crafted `.amazonq/mcp.json` → arbitrary code execution + cloud credential theft; reported 2026-04-20, fix 2026-05-12, disclosure under AWS Security Bulletin 2026-047-AWS). Also maintains an MCP security academy page. | https://www.wiz.io/blog/amazon-q-vulnerability |
| **Snyk** | **Snyk Agent Scan** + **Snyk Evo** | See §1. Acquired Invariant Labs June 2025; mcp-scan renamed and folded into Snyk's platform with the `evo` push command. | https://github.com/snyk/agent-scan |
| **OpenAI** | **Promptfoo** | Acquired Promptfoo 2026-03-09 (see §2). | https://www.promptfoo.dev/blog/promptfoo-joining-openai/ |
| **Cisco** | **Cisco AI Defense** + open-source MCP Scanner | See §4. | https://github.com/cisco-ai-defense/mcp-scanner |
| **Microsoft** | **Agent Governance Toolkit** + Azure MCP security guidance | See table above. Also publishes OWASP MCP Top 10 guidance for Azure (`mcp-azure-security-guide`) and a June 30, 2026 security blog on agentic tool risk. | https://github.com/microsoft/agent-governance-toolkit |
| **Docker** | **MCP Gateway + MCP Catalog** | See §5. | https://github.com/docker/mcp-gateway |

**Trail of Bits MCP-specific security tooling: NIE ZWERYFIKOWANO.** A search of the Trail of Bits blog
index did not surface any MCP-specific scanner or red-teaming product with a version/release. They
publish AI/ML security research but I could not verify a named MCP tool.

### Standards / threat-intelligence sources published in 2026

| Source | Date | Notes | URL |
|---|---|---|---|
| **NSA — "Security Design Considerations for AI-Driven Automation Leveraging Model Context Protocol (MCP)"** | **May 2026** (per multiple secondary references; press release undated in retrievable text) | Official NSA CSI guidance on MCP security. **Caveat: nsa.gov returned HTTP 403 to this environment** for both the press release and the PDF, so the document's exact title, date and contents are **NIE ZWERYFIKOWANO** first-hand. Indirectly corroborated by Microsoft AGT's compliance mapping, which cites the press-release URL and `https://www.nsa.gov/Portals/75/documents/Cybersecurity/CSI_MCP_SECURITY.pdf`. | https://www.nsa.gov/Press-Room/Press-Releases-Statements/Press-Release-View/Article/4496698/nsa-releases-security-design-considerations-for-ai-driven-automation-leveraging/ |
| **OWASP MCP Top 10** | 2025 project, referenced throughout 2026 | Official OWASP project. **MCP01** Token Mismanagement & Secret Exposure · **MCP02** Privilege Escalation via Scope Creep · **MCP03** Tool Poisoning · **MCP04** Software Supply Chain Attacks & Dependency Tampering · **MCP05** Command Injection & Execution · **MCP06** Prompt Injection via Contextual Payloads · **MCP07** Insufficient Authentication & Authorization · **MCP08** Lack of Audit and Telemetry · **MCP09** Shadow MCP Servers · **MCP10** Context Injection & Over-Sharing. | https://owasp.org/projects/mcp-top-10 |
| **CSA research note — "MCP Attack Surface: Tool Poisoning and IDE Auto-Execution"** | **2026-07-01** | Cloud Security Alliance. Key data: **MCPTox benchmark average tool-poisoning attack success rate 36.5% across 20 LLMs, max 72.8%**; **TrustFall (Adversa AI, 2026-05-07)** — Claude Code, Cursor CLI, Gemini CLI, GitHub Copilot CLI all auto-execute project-defined MCP servers on folder-trust acceptance; **Miasma worm (June 2026, TeamPCP/UNC6780)** planted malicious MCP configs across **73 GitHub repositories including Microsoft's azure/durabletask**. Prior CVEs catalogued: CVE-2025-54135 (CurXecute, CVSS 8.6), CVE-2025-54136 (MCPoison), CVE-2026-12957/12958 (Amazon Q). | https://labs.cloudsecurityalliance.org/research/csa-research-note-mcp-tool-poisoning-auto-execution-20260701/ |
| **MCPTox benchmark paper** | arXiv:2508.14925, Aug 2025 | 353 adversarial tools from 45 live MCP servers × 20 models. | https://arxiv.org/abs/2508.14925 |

---

## NOT VERIFIED / NOT FOUND

Explicitly **NIE ZWERYFIKOWANO**:

1. **Stable (non-prerelease) version of Docker MCP Gateway.** Every GitHub release is flagged
   `prerelease=true` and `/releases/latest` 404s. v0.43.3 (2026-07-16) is the newest tag, but whether
   it is the "stable" release for Docker Desktop users could not be confirmed.
2. **Patched versions for 7 of the 8 Docker MCP Gateway advisories.** GitHub's advisory API returns
   `first_patched_version: none` for GHSA-625j-rw87-4ghr, GHSA-6pq5-p7fc-7xhq, GHSA-m5m2-mrxf-7j7q,
   GHSA-mqq5-qh4g-2g8g, GHSA-6m8f-w97w-99h7, GHSA-g879-4j4f-6vj7, GHSA-r2xf-7jw5-pjg6. Only
   CVE-2025-64443 has a documented patch (v0.28.0).
3. **Docker MCP Catalog scanning engine/detection coverage.** Docker confirms automated testing and
   scanning occur and calls the approach "best-effort", but does not name the scanner, rule set or
   coverage. No numbers verified.
4. **Promptfoo MCP Proxy version number or release date.** Commercial/enterprise; only a marketing
   page and a July 2025 release-notes mention exist. No versioned artifact found.
5. **NSA MCP guidance (May 2026) first-hand verification.** `nsa.gov` returned HTTP 403 for both the
   press release and `CSI_MCP_SECURITY.pdf`. Title/date/content are unverified from the primary source.
6. **An official, versioned MCP "security best practices" *version number*.** The doc exists and is
   versioned by spec revision (2025-06-18 → 2025-11-25 → 2026-07-28 → draft), but it carries no
   independent semantic version.
7. **Trail of Bits MCP-specific security product.** No named MCP scanner/red-team tool with a version
   could be verified.
8. **Any security scanning or signing/attestation/SLSA/sigstore in the official MCP Registry.** Searched
   the registry docs and repo — the only verification is DNS/HTTP namespace ownership. The enhanced
   validation design is explicitly *proposed and possibly abandoned*, not shipped. The registry
   explicitly will **not** remove servers with security vulnerabilities.
9. **`semgrep.dev` registry keyword search for "mcp".** The registry API query returned unrelated
   hardcoded-secret rules; the MCP rules were verified from the GitHub source instead. Whether these
   rules are browsable under a friendly registry URL/pack name is **NIE ZWERYFIKOWANO**.
10. **`hanyixxx/mcp-scan`, `piiiico/mcp-security-scanner` and similar GitHub namesakes.** These are
    unrelated third-party projects that collide with the `mcp-scan` name; the canonical project is
    Snyk Agent Scan. Not evaluated.
11. **Palo Alto Prisma AIRS MCP Server version/GA date.** Docs page verified to exist; no version
    number or GA date published on the page retrieved — **NIE ZWERYFIKOWANO**.
12. **OpenAI's own copy of the Promptfoo acquisition announcement** — `openai.com/index/openai-to-acquire-promptfoo/`
    returned HTTP 403. Date verified only from Promptfoo's blog (2026-03-09).

---

## Quick-reference summary table

| Tool | Current version | Release date | License | Maintainer status |
|---|---|---|---|---|
| mcp-scan (stub) | 0.4.3 | 2026-03-02 | Apache-2.0 | **Renamed** → snyk-agent-scan |
| Snyk Agent Scan | **0.6.3** (snapshot 0.6.4-…-1649, 2026-09-19) | **2026-09-10** | Apache-2.0 | Active (Snyk) |
| Promptfoo | **0.123.1** | **2026-09-18** | MIT | Active (OpenAI) |
| Semgrep MCP rules | n/a (rule repo) | rules added **2026-03-23**, skills **2026-05-10** | Semgrep Rules License v1.0 | Active |
| Cisco AI Defense MCP Scanner | **4.8.4** | **2026-08-28** | Apache-2.0 (repo) | Active |
| Docker MCP Gateway | **v0.43.3** | **2026-07-16** | MIT | Active; 8 advisories |
| Microsoft Agent Governance Toolkit | **v4.1.0** | **2026-06-09** | MIT | Active |
| Agent Threat Rules (ATR) | **v4.0.0** | **2026-08-23** | MIT | Active |
| mcphound | **0.1.5** | **2026-08-31** | Apache-2.0 | Early / v0.1 |
| mcp-zero-trust-layer | **0.6.0** | **2026-09-17** | Apache-2.0 | Early but active |
| agent-airlock | **0.10.7** | **2026-09-19** | Apache-2.0 | Active |
| praxiom | **0.2.0** | **2026-09-07** | MIT | Very early |
| ShieldCortex | **5.0.5** | **2026-09-13** | MIT | Active |
| avakill | **1.2.0** | **2026-03-08** | AGPL-3.0-only | Stale (6 months) |
| MCP spec | **2026-07-28** | 2026-07-28 | (NOASSERTION) | Active |
| MCP Registry | **v1.8.1** | **2026-08-06** | (NOASSERTION) | Preview; no security scanning |

### Verification methodology

- GitHub metadata/releases/advisories/commits/code-search: authenticated `gh api` (the unauthenticated
  REST API is rate-limited to 60 req/h from this host and was exhausted during research).
- PyPI: `https://pypi.org/pypi/<name>/json`.
- npm: `https://registry.npmjs.org/<name>` and `/<name>/latest`.
- Redirect verification: `curl -sI` on `github.com/invariantlabs-ai/mcp-scan` (HTTP 301).
- Docs: direct fetch of official pages; rule contents fetched as raw YAML from the `develop` branch.
- Report generated 2026-09-19.

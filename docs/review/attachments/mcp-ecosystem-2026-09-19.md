# MCP ecosystem — stan na 2026-09-19

Kontekst: bieżąca rewizja spec to **2026-07-28**. Niepewności oznaczone **⚠️**.

---

## 1. Conformance

**Istnieje, jest oficjalny.** Repo: [modelcontextprotocol/conformance](https://github.com/modelcontextprotocol/conformance). npm [`@modelcontextprotocol/conformance`](https://www.npmjs.com/package/@modelcontextprotocol/conformance): `latest` = **0.1.16** (2026-03-30), `alpha` = **0.2.0-alpha.11** (2026-08-07); pakiet modyfikowany **2026-09-17** ([registry](https://registry.npmjs.org/@modelcontextprotocol/conformance)).

Uruchamianie ([README](https://github.com/modelcontextprotocol/conformance/blob/HEAD/README.md)) — serwer HTTP musi **już działać**, harness go nie startuje:

```bash
npx @modelcontextprotocol/conformance server --url http://localhost:3000/mcp --suite active
npx @modelcontextprotocol/conformance server --url http://localhost:3000/mcp --scenario server-initialize
npx @modelcontextprotocol/conformance list --requirements 2026-07-28
```

- **Brak flagi `--server-command` i brak `conformance.yaml`.** Serwer testowany wyłącznie przez `--url` (Streamable HTTP); lifecycle zarządzasz sam.
- Plik konfiguracyjny istnieje tylko jako **baseline oczekiwanych porażek** — YAML, nie definicja testu:
  ```yaml
  server:
    - tools-call-with-progress
    - server-stateless:sep-2575-server-implements-discover  # pojedynczy check
  client:
    - sse-retry
  ```
- Kody wyjścia: fail+baseline → 0; fail bez wpisu → 1 (regresja); pass z wpisem → 1 (nieaktualny baseline).
- `--requirements <rewizja>` uruchamia dokładnie to, czego wymaga dana rewizja, zamrożone na moment wydania. `tier-check` ocenia SDK wobec [SEP-1730](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1730). GitHub Action: `modelcontextprotocol/conformance@v0.1.11`.

**Czy napędza serwer stdio w dowolnym języku? NIE bezpośrednio** — tryb serwerowy łączy się tylko po `--url`. stdio obsłużysz przez proxy HTTP. Tryb `client --command "<cmd>"` spawnuje *klienta* w dowolnym języku (URL jako ostatni arg + env `MCP_CONFORMANCE_SCENARIO`/`MCP_CONFORMANCE_CONTEXT`).

**Adopcja w SDK:**

- **Java SDK** — [`conformance-tests/VALIDATION_RESULTS.md`](https://github.com/modelcontextprotocol/java-sdk/blob/master/conformance-tests/VALIDATION_RESULTS.md), walidacja **2026-08-17** przeciw `0.2.0-alpha.11`, `--spec-version 2025-11-25`: serwer 73/73 checków (31 scenariuszy), SEP-1613 5/5, klient 3/4 (`sse-retry` w baseline), auth 193 checki / 14 scenariuszy / 0 błędów.
- [Kotlin SDK `conformance-test/`](https://github.com/modelcontextprotocol/kotlin-sdk/tree/main/conformance-test); [Go SDK `scripts/server-conformance.sh`](https://github.com/modelcontextprotocol/go-sdk/blob/main/scripts/server-conformance.sh). `known-sdks.ts` ma wbudowane wpisy dla typescript/go/csharp/rust/python-sdk.

**Alternatywy:**

| Narzędzie | Wersja | URL |
|---|---|---|
| `@modelcontextprotocol/inspector` | **2.7.0** | [npm](https://www.npmjs.com/package/@modelcontextprotocol/inspector) |
| `@mcpjam/inspector` | **3.8.1** | [npm](https://www.npmjs.com/package/@mcpjam/inspector) |
| `mcp-testing-kit` | **0.2.0** (2025-05-06) | [npm](https://www.npmjs.com/package/mcp-testing-kit) · [repo](https://github.com/thoughtspot/mcp-testing-kit) |

Inspector = debug interaktywny, nie suite zgodności; `mcp-testing-kit` = biblioteka testowa, peer dep `@modelcontextprotocol/sdk ^1.11.0` (tylko v1). Innych pakietów npm z „conformance" w nazwie nie znalazłem.

---

## 2. MCP Registry

**Istnieje, ale w PREVIEW — nie GA.** „Breaking changes or data resets may occur before general availability." ([faq](https://modelcontextprotocol.io/registry/faq), [quickstart](https://modelcontextprotocol.io/registry/quickstart)). API: `https://registry.modelcontextprotocol.io` ([OpenAPI](https://registry.modelcontextprotocol.io/openapi.yaml)).

**Publikacja** — CLI `mcp-publisher` ([quickstart](https://modelcontextprotocol.io/registry/quickstart), [authentication](https://modelcontextprotocol.io/registry/authentication)): opublikuj artefakt w rejestrze pakietów + dodaj metadane weryfikacyjne → `mcp-publisher init` (generuje `server.json`) → `mcp-publisher login github` → `mcp-publisher publish`. Autoryzacja: GitHub OAuth **albo weryfikacja DNS** (rekord TXT). Namespace musi pasować do zweryfikowanej tożsamości (`io.github.<user>/...`).

**`server.json`** ([generic-server-json.md](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md), schema `.../schemas/2025-12-11/server.schema.json`):

- wymagane: `$schema`, `name` (reverse-DNS, dokładnie jeden `/`), `version` (semver; zakresy odrzucane), `description`
- opcjonalne: `title`, `websiteUrl`, `repository` (`url`/`source`/`subfolder`/`id`), `icons[]`
- `packages[]` (serwery lokalne): `registryType` (`npm`/`pypi`/`oci`/`nuget`/`cargo`/`mcpb`), `registryBaseUrl`, `identifier`, `version`, `runtimeHint` (`npx`/`uvx`/`dnx`), `transport.type` (`stdio` | `streamable-http`), `packageArguments[]`, `runtimeArguments[]`, `environmentVariables[]` (`name`, `isRequired`, `isSecret`, `default`, `choices`), `fileSha256` (MCPB)
- `remotes[]` (serwery zdalne): `type` (`streamable-http`/`sse`), `url` (templating `{tenant_id}`), `headers[]`, `variables{}`
- `_meta` wyłącznie pod `io.modelcontextprotocol.registry/publisher-provided`, limit **4 KB**

**Serwery stdio-only: TAK, wspierane** — `transport.type: "stdio"`, podstawowy udokumentowany przypadek. Rejestr hostuje **tylko metadane**, nie artefakty. Ograniczenia: brak unpublish ([issue #104](https://github.com/modelcontextprotocol/registry/issues/104)), metadane wersji niezmienne, dodatkowe restrykcje w [official-registry-requirements](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/official-registry-requirements.md).

---

## 3. MCP Server Card / `.well-known`

**Status: EXPERIMENTAL — nie zaakceptowana funkcja core, nie oficjalna extension.**

- Repo przemianowane z `experimental-ext-server-card` na [modelcontextprotocol/ext-server-card](https://github.com/modelcontextprotocol/ext-server-card), ale README nadal: *„Status: Experimental. This work is for prototyping and feedback only, and is not an accepted or official MCP extension."*
- [SEP-2127](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2127) (utworzony **2026-01-21**, autorzy dsp-ant, Nick Cooper, Tadas Antanavicius): status **Draft**, Standards Track. Wymaga reference implementation przed Final — *„To be added"*.
- Wcześniejsza próba wejścia do core ([PR #2652](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2652)) **porzucona**.
- Istnieje **graduation plan**: po akceptacji `schema.ts` przenosi się do `schema/draft/schema.ts` głównej spec, repo zostaje zarchiwizowane.

**Ścieżki i media type — ZMIANA WOBEC SEP:**

| | SEP-2127 (draft) | ext-server-card (aktualne) |
|---|---|---|
| Karta | `/.well-known/mcp-server-card` (+ `/{server-name}`) | `GET <streamable-http-url>/server-card`; każdy „unreserved URI" OK |
| Content-Type | `application/json` | **`application/mcp-server-card+json`** |
| Katalog domenowy | — | `/.well-known/ai-catalog.json`, `application/ai-catalog+json` |

Aktualne wytyczne **odradzają** `.well-known` dla samej karty („site-wide vs application-level metadata"), zachowując je dla katalogu AI i OAuth ([docs/discovery.md](https://github.com/modelcontextprotocol/ext-server-card/blob/main/docs/discovery.md)). **Realna rozbieżność — ścieżka niestabilna.**

**Schemat karty** ([SEP-2127](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/aa59517442d323a33ed915fc408f1584c4a23dfa/seps/2127-mcp-server-cards.md)): wymagane `$schema` (`https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json`), `name`, `version`, `description`; opcjonalne `title`, `websiteUrl`, `repository`, `icons[]` (`src`/`sizes`/`mimeType`/`theme`), `remotes[]` (`type`, `url`, `supportedProtocolVersions[]`, `headers[]`, `variables{}`), `_meta`. **Karta celowo nie zawiera** tools/resources/prompts. CORS `*`, `Cache-Control: public, max-age=3600`, `ETag` + `If-None-Match`.

**Relacja do `server/discover`: komplementarna, nie redundantna.**

- `server/discover` jest **core i obowiązkowe**: *„Servers **MUST** implement it."* Zwraca `supportedVersions`, `capabilities`, `_meta['io.modelcontextprotocol/serverInfo']`, `instructions`, `ttlMs`/`cacheScope` ([discover.mdx](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/draft/server/discover.mdx)).
- Karta odpowiada *gdzie się połączyć* (przed połączeniem, cross-domain); `server/discover` — *jak* (po połączeniu).
- Dokumentacja nakazuje: *„Clients MUST NOT treat Server Card contents as authoritative for security or access-control decisions"* i *„SHOULD verify a Server Card's claims against the live connection, preferring the runtime values."*

---

## 4. `x-mcp-header` — realna adopcja

**Tak, `rmcp` realnie emituje i waliduje.**

- [rust-sdk README](https://github.com/modelcontextprotocol/rust-sdk/blob/main/README.md), sekcja *Standard HTTP Headers*: *„`rmcp` emits and validates these automatically once a connection negotiates `2026-07-28` or newer — no call-site changes are required."* Obsługuje `Mcp-Method`, `Mcp-Name`, `Mcp-Param-*` + Base64 `=?base64?...?=`. README linkuje migration guide do **3.x** ([discussion 969](https://github.com/modelcontextprotocol/rust-sdk/discussions/969)).
- Publiczny moduł [`transport/common/http_header.rs`](https://docs.rs/rmcp/latest/src/rmcp/transport/common/http_header.rs.html).
- ⚠️ `rmcp` zawęża adnotacje do *top-level* właściwości, a specyfikacja dopuszcza zagnieżdżone obiekty (byle łańcuch szedł tylko przez `properties`).

**Reguła „MUST reject":**

- **Spec (normatywnie):** *„Clients using the Streamable HTTP transport **MUST** reject tool definitions where any `x-mcp-header` value violates these constraints. [...] the client **MUST** exclude the invalid tool from the result of `tools/list`. Clients **SHOULD** log a warning [...]. Clients using other transports (e.g., stdio) **MAY** ignore `x-mcp-header` entirely."* ([streamable-http.mdx](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/draft/basic/transports/streamable-http.mdx)). Scenariusz conformance: `server-stateless:sep-2575-server-implements-discover`.
- **`rmcp`:** implementuje (walidacja + odrzucanie).
- **TS SDK v2:** podział na `@modelcontextprotocol/core` / `/client` / `/server` w wersji **2.0.0** (widać w zależnościach inspectora 2.7.0). **⚠️** brak dowodu na emisję `Mcp-Param-*` ani egzekwowanie odrzucania.
- **Python SDK v2:** obecna stabilna linia (`pip install mcp` = 2.x; v1.x na [gałęzi v1.x](https://github.com/modelcontextprotocol/python-sdk/tree/v1.x)). Commity dot. SEP-2243 ([10dc173](https://github.com/modelcontextprotocol/python-sdk/commit/10dc17363762b5438368f2d8969d8cf3046d7673)) → prace trwają. **⚠️** brak potwierdzenia pełnej emisji.
- **Java SDK:** v2 ma nagłówki, **⚠️** niepewne odrzucanie. **Go SDK:** **⚠️** niezweryfikowane.

**Wniosek:** `x-mcp-header` jest normatywne w 2026-07-28 (SEP-2243), ale jedyna udokumentowana implementacja kliencka to `rmcp`. Wąskie gardło adopcji.

---

## 5. Metryki / observability (OpenTelemetry)

**Semconv MCP są w statusie Development (nie stable) i zostały PRZENIESIONE.**

- [opentelemetry.io/docs/specs/semconv/registry/attributes/mcp/](https://opentelemetry.io/docs/specs/semconv/registry/attributes/mcp/) — wszystkie atrybuty oznaczone `Deprecated`, „Moved to the OpenTelemetry GenAI semantic conventions repository".
- Aktywne źródło: [open-telemetry/semantic-conventions-genai](https://github.com/open-telemetry/semantic-conventions-genai), plik [`docs/gen-ai/mcp.md`](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/mcp.md). Status: **[Development]**; wersja semconv na stronie OTel: **1.44.0**.

**Atrybuty `mcp.*`** (wszystkie Development): `mcp.method.name` (Required), `mcp.resource.uri` (Conditionally Required), `mcp.protocol.version` (Recommended), `mcp.session.id` (Recommended). Powiązane: `gen_ai.tool.name`, `gen_ai.prompt.name`, `gen_ai.operation.name = execute_tool`, `jsonrpc.request.id`, `jsonrpc.protocol.version`, `rpc.response.status_code`, `network.transport` (`pipe` = stdio, `tcp`/`quic` = HTTP), opt-in `gen_ai.tool.call.arguments` / `.result` / `gen_ai.prompt.variable.<name>` (ostrzeżenie o danych wrażliwych).

**Metryki** (Histogram, `s`, Development): `mcp.client.operation.duration`, `mcp.server.operation.duration`, `mcp.client.session.duration`, `mcp.server.session.duration`. Buckety: `[0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 30, 60, 120, 300]`. Nazwa spana: `{mcp.method.name} {target}` (np. `tools/call get-weather`), kind `CLIENT`/`SERVER`.

**Propagacja trace w `_meta`:** [SEP-414](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/seps/414-request-meta.md) — **Status: Final** (2025-04-25, Adrian Cole). Dokumentuje wyjątek od DNS-prefiksowania: `traceparent`, `tracestate`, `baggage` **bez prefiksu**, w formatach W3C. Semconv: *„Instrumentations SHOULD propagate context [...] by injecting it into the MCP request `params._meta`"*. **Implementacje** (sekcja Reference Implementation SEP-414): C# SDK (`ModelContextProtocol.Core/Diagnostics.cs`), Python SDK ([PR #1693](https://github.com/modelcontextprotocol/python-sdk/pull/1693)), OpenInference (Python + TypeScript), Envoy AI Gateway, Logfire, ToolHive. SEP dokumentuje **istniejącą praktykę**, nie nowy wymóg.

---

## 6. Liczba narzędzi / context bloat

Wszystko poniżej to oficjalna dokumentacja Anthropic. **⚠️ Nie znalazłem** równoważnych, liczbowych wytycznych w oficjalnej dokumentacji OpenAI ani Google.

**a) „Introducing advanced tool use on the Claude Developer Platform"** — [URL](https://www.anthropic.com/engineering/advanced-tool-use), **2025-11-24**.

- 5 serwerów: GitHub 35 narzędzi (~26K tokenów), Slack 11 (~21K), Sentry 5 (~3K), Grafana 5 (~3K), Splunk 2 (~2K) = **58 narzędzi ≈ 55K tokenów** przed startem rozmowy. Jira sama ~17K. *„At Anthropic, we've seen tool definitions consume 134K tokens before optimization."*
- Tradycyjnie: ~72K tokenów na 50+ narzędzi, ~77K zużycia kontekstu przed pracą.
- Z Tool Search Tool: ~500 tokenów + 3–5 narzędzi (~3K) = **~8.7K, ~85% redukcji, 95% kontekstu zachowane**.
- Trafność (MCP evals): Opus 4 **49% → 74%**; Opus 4.5 **79.5% → 88.1%**.
- *„The most common failures are wrong tool selection and incorrect parameters"* — zwłaszcza przy podobnych nazwach.
- **Kiedy używać:** definicje >10K tokenów, problemy z trafnością, 10+ narzędzi. **Kiedy nie:** <10 narzędzi, wszystkie często używane, zwięzłe definicje.
- PTC: **43 588 → 27 297 tokenów** (‑37%); knowledge retrieval 25.6% → 28.5%; GIA 46.5% → 51.2%.
- Tool Use Examples: trafność złożonych parametrów **72% → 90%**.
- Beta header `advanced-tool-use-2025-11-20`; typy `tool_search_tool_regex_20251119`, `tool_search_tool_bm25_20251119`, `code_execution_20250825`.

**b) „Tool search tool"** — [URL](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool).

- **Najważniejsza liczba:** *„Claude's ability to pick the right tool degrades once you exceed **30–50 available tools**."*
- Setup multiserver ~**55k tokenów**; tool search redukuje o **>85%**, ładując **3–5 narzędzi** na żądanie.
- Mechanizm: `defer_loading: true` + `mcp_toolset` z `default_config.defer_loading`. Zalecenie: *„Keep your three to five most-used tools always loaded, defer the rest."*
- **Nie psuje prompt caching** (deferred tools poza initial prompt). Warianty: `tool_search_tool_regex_20251119`, `tool_search_tool_bm25_20251119`, `tool_search`.

**c) „Code execution with MCP: Building more efficient agents"** — [URL](https://www.anthropic.com/engineering/code-execution-with-mcp), **2025-11-04**.

- *„In cases where agents are connected to thousands of tools, they'll need to process hundreds of thousands of tokens before reading a request."*
- 2-godzinny transkrypt = ~**50 000 dodatkowych tokenów** przepływających przez kontekst (dwukrotnie).
- Serwery MCP jako drzewo plików z kodem: **150 000 → 2 000 tokenów (‑98.7%)**.
- Wzorzec `search_tools` z parametrem szczegółowości (name only / name+description / pełna definicja).

---

## Luki

- **⚠️** Brak oficjalnych wytycznych OpenAI/Google z konkretnymi liczbami.
- **⚠️** Egzekwowanie odrzucania `x-mcp-header`: potwierdzone tylko dla `rmcp`; TS v2 / Python v2 / Go / Java — brak dowodu.
- **⚠️** Sprzeczna ścieżka i media type Server Card między draftem SEP a repo. Nie stabilizować się na żadnej.
- **⚠️** Conformance nie testuje stdio bezpośrednio — wymagany proxy HTTP.
- **⚠️** Dokładna wersja `rmcp` na crates.io niezweryfikowana (README wskazuje 3.x).

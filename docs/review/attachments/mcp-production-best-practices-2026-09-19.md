# MCP w produkcji — stan na 2026-09-19

> Raport dla serwera local-first z dwoma transportami (stdio + streamable HTTP) i ~28 narzędziami.
> Wszystkie twierdzenia mają URL źródła i datę. Pozycje niezweryfikowane oznaczono **⚠️ NIEPOTWIERDZONE**.

**Najważniejszy wniosek w jednym zdaniu:** bieżąca rewizja specyfikacji to **`2026-07-28`** i jest to zmiana **łamiąca kompatybilność** — MCP jest od niej protokołem **bezstanowym**: nie ma `Mcp-Session-Id`, nie ma handshake'u `initialize`, jest nowe wymagane RPC `server/discover`. Serwer wspierający wyłącznie `2025-11-25` i starsze jest dziś serwerem *legacy*.

---

## Streszczenie zarządcze — 12 rzeczy, które trzeba wiedzieć

| # | Fakt | Konsekwencja dla Vestige |
|---|---|---|
| 1 | Rewizja **`2026-07-28`** jest **bezstanowa**: brak `Mcp-Session-Id`, brak `initialize` | To jest zmiana **łamiąca**. Obecny transport = era *legacy* |
| 2 | **`server/discover`** jest **MUST** dla serwerów | Trzeba dodać nowe RPC |
| 3 | Stałe **`MCP-Protocol-Version`, `Mcp-Method`, `Mcp-Name`**; niezgodność → **400 + `-32020`** | Walidacja nagłówków ↔ ciało to nowy obowiązek |
| 4 | Wyniki muszą mieć **`resultType`**; listy muszą mieć **`ttlMs` + `cacheScope`** | Deterministyczne sortowanie katalogu = darmowy zysk na prompt cache |
| 5 | **SSE zostaje**, ale bez `Last-Event-ID`; GET usunięty → **`subscriptions/listen`** | Trzeba przepisać dostarczanie notyfikacji |
| 6 | **Serwer dual-era MAY obsługiwać obie ery równolegle** | Migracja **nie musi** być flag-day |
| 7 | **DCR zdeprecjonowany → CIMD** (`draft-ietf-oauth-client-id-metadata-document-00`) | Dotyczy tylko trybu HTTP poza loopback |
| 8 | **stdio: „SHOULD NOT follow this specification"** — auth z env | Zgodne ze spec jest **brak OAuth** dla stdio |
| 9 | **Logging/LVL zdeprecjonowane; `logging/setLevel` USUNIĘTE**; logi per-request przez `_meta` | Loguj na `stderr` + OTel |
| 10 | **OTel: 4 metryki `mcp.*`**, brak metryki „tool duration", brak liczników, brak tokenów | Filtr `mcp.method.name="tools/call"` + `gen_ai.tool.name` |
| 11 | **Oficjalny conformance suite istnieje**: 37 scenariuszy serwera dla `2026-07-28` | `npx @modelcontextprotocol/conformance server --url ... --requirements 2026-07-28` |
| 12 | **Brak mechanizmu on-demand tool discovery w spec** — tylko „progressive discovery" w roadmapie | Redukcja kontekstu: scope'owanie, tool search po stronie hosta, code execution |

### Bilans „~28 narzędzi" — czy to problem?

**Tak, jesteś na progu.** Anthropic podaje, że **trafność wyboru narzędzia spada powyżej 30–50 dostępnych narzędzi** ([Tool search tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool)). Przy ~28 narzędziach:
- Sam serwer jest w normie, ale **host zwykle łączy kilka serwerów** — łatwo przekroczysz 50.
- Koszt definicji: **~17 000 – 53 000 tokenów** (600–1 900 tokenów/narzędzie × 28).
- Anthropic zaleca tool search, gdy definicje przekraczają **10 000 tokenów** — czyli **już go przekraczasz**.

**Darmowe optymalizacje do zrobienia natychmiast:**
1. **Deterministyczne sortowanie `tools/list`** (wymóg SHOULD spec) — poprawia prompt cache.
2. **`ttlMs` wysokie** (np. 300000) + `cacheScope: "public"` jeśli katalog nie zależy od uprawnień.
3. **Skrócenie `description`** — to one dominują koszt (Slack: ~1 900 tokenów/narzędzie).

---

## 0. Mapa rewizji protokołu i status governance

| Rewizja | Data | Status |
|---|---|---|
| `2024-11-05` | 2024-11-05 | najstarsza wspierana; transport HTTP+SSE **Deprecated** |
| `2025-03-26` | 2025-03-26 | Streamable HTTP wprowadzony; HTTP+SSE zdeprecjonowany |
| `2025-06-18` | 2025-06-18 | OAuth 2.1 jako resource server; `MCP-Protocol-Version` header |
| `2025-11-25` | 2025-11-25 | ostatnia rewizja „legacy" (handshake `initialize`) |
| **`2026-07-28`** | **2026-07-28** | **bieżąca; protokół bezstanowy — BREAKING** |

Źródła: [lista specyfikacji](https://modelcontextprotocol.io/specification/2026-07-28/index.md), [Key Changes 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog.md), [blog „The 2026-07-28 Specification"](https://blog.modelcontextprotocol.io/posts/2026-07-28/) (2026-07-28).

**Governance:** MCP zostało przekazane przez Anthropic, Block i OpenAI do **Agentic AI Foundation (AAIF)**, projektu Linux Foundation, w **grudniu 2025**. Źródło: [draft-abbott-mcp-ax-00, §1.4](https://datatracker.ietf.org/doc/html/draft-abbott-mcp-ax-00) (2026-05-04; ⚠️ to źródło *trzeciej strony* — sam draft nie ma statusu IETF). Potwierdzenie pośrednie: blog AAIF [„MCP in production"](https://aaif.io/blog/mcp-in-production-what-changes-after-the-demo-works) (2026-07-31) publikowany jest na aaif.io.

**Polityka deprecjacji:** [SEP-2596](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2596) wprowadza formalny cykl życia Active → Deprecated → Removed z **minimalnym 12-miesięcznym oknem deprecjacji**. Rejestr: [Deprecated Features](https://modelcontextprotocol.io/specification/2026-07-28/deprecated.md). Status quo na 2026-09-19: **żadna funkcja nie została jeszcze usunięta** („No features have been removed under this policy yet").

---

## 1. Streamable HTTP w `2026-07-28`

Źródło podstawowe: [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http.md).

### 1.1 Czy `Mcp-Session-Id` zniknął? — TAK

> „Remove protocol-level sessions and the `Mcp-Session-Id` header from the Streamable HTTP transport."
> — [changelog, Major change #1](https://modelcontextprotocol.io/specification/2026-07-28/changelog.md)

Usunięty został również handshake `initialize`/`notifications/initialized`:

> „Make MCP stateless: remove the `initialize`/`notifications/initialized` handshake. Every request now carries its protocol version and client capabilities in `_meta`."
> — [changelog, Major change #2](https://modelcontextprotocol.io/specification/2026-07-28/changelog.md)

Konsekwencje operacyjne (cytaty normatywne):

- Serwery **MUST NOT** polegać na wcześniejszych żądaniach na tym samym połączeniu. Każde żądanie dostarcza metadane w `_meta`. — [.../basic/index.md, sekcja „Statelessness"](https://modelcontextprotocol.io/specification/2026-07-28/basic/index.md)
- Serwery **SHOULD** być przygotowane na obsługę żądań powiązanych z wieloma zadaniami/wątkami/rozmowami jednocześnie.
- Klienci **SHOULD NOT** używać pojedynczego zadania/wątku jako granicy życia procesu stdio.
- Stan międzybajtowy **MUST** być przekazywany jawnym identyfikatorem w każdym żądaniu.
- Nota: „an open connection, such as a STDIO process, is not a conversation or session".

### 1.2 Czy SSE nadal jest wymagane? — TAK, ale w innym kształcie

Serwer **MUST** odpowiedzieć na żądanie albo `Content-Type: application/json` (pojedynczy obiekt), albo `Content-Type: text/event-stream`; klient **MUST** obsługiwać oba. Strumień SSE jest **zakresowany do pojedynczego żądania** i niesie najpierw `notifications/progress` / `notifications/message`, a na końcu finalną odpowiedź.

**Co zostało usunięte z SSE:**
- Samodzielny strumień SSE przez HTTP GET — usunięty (zastąpiony przez `subscriptions/listen`).
- Wznawianie strumienia przez `Last-Event-ID` i event ID — usunięte. „Resumable SSE streams via `Last-Event-ID` are not supported." Przerwany strumień = utracone żądanie; klient **MUST** wysłać je ponownie z nowym ID.
- Serwer **MUST NOT** wysyłać samodzielnych żądań JSON-RPC na strumieniu SSE. Interakcje serwer→klient (sampling, elicitation, roots) są teraz osadzone jako `InputRequiredResult` (MRTR).

**Co jest zalecane operacyjnie:**
- Serwery **SHOULD** wysyłać nagłówek `X-Accel-Buffering: no` przy inicjacji SSE — wyłącza buforowanie odpowiedzi w nginx i zapobiega akumulacji zdarzeń.
- Dla długo żyjących strumieni (szczególnie `subscriptions/listen`) zaleca się okresowe wysyłanie **linii komentarza SSE** (linia zaczynająca się od `:`, np. `:\r\n`) jako keep-alive — chroni przed zamknięciem połączenia przez pośredników i idle-timeouty klienta.

### 1.3 Wymagane nagłówki HTTP (nowe, twarde wymagania)

| Nagłówek | Źródło wartości | Wymagany dla |
|---|---|---|
| `MCP-Protocol-Version` | `_meta["io.modelcontextprotocol/protocolVersion"]` | **wszystkie** POST |
| `Mcp-Method` | `method` | **wszystkie** żądania |
| `Mcp-Name` | `params.name` lub `params.uri` | `tools/call`, `resources/read`, `prompts/get` |

**⚠️ Ważne rozróżnienie MUST vs SHOULD w `_meta`** (częsty błąd implementacyjny, bo SEP-2575 był bardziej rygorystyczny niż finalna specyfikacja):

| Pole `_meta` | Wymagane? | Uwaga |
|---|---|---|
| `io.modelcontextprotocol/protocolVersion` | **MUST** (Tak) | brak → `-32602`, na HTTP `400` |
| `io.modelcontextprotocol/clientCapabilities` | **MUST** (Tak) | brak → `-32602`, na HTTP `400` |
| `io.modelcontextprotocol/clientInfo` | **SHOULD** (Nie) | „Clients SHOULD include … unless specifically configured not to do so" — [PR #3002](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/3002) osłabił to z MUST |
| `io.modelcontextprotocol/serverInfo` (w wyniku) | **SHOULD** (Nie) | serwery SHOULD dołączać do każdego wyniku; top-level `DiscoverResult.serverInfo` zostało **usunięte** na rzecz `_meta` |

Źródło zmiany: [SEP-2575 → „Changes since SEP became Final"](https://modelcontextprotocol.io/seps/2575-stateless-mcp.md) (PR [#3002](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/3002) i [#2953](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2953)).

- Niezgodność nagłówka z ciałem → **HTTP 400** + JSON-RPC error **`-32020` `HeaderMismatch`**.
- Serwer nie zna/nie wspiera wersji → **HTTP 400** + `UnsupportedProtocolVersionError` (`-32022`) z listą `supported`.
- Serwer nie zna metody → **HTTP 404** + JSON-RPC `-32601` (`Method not found`).
- Serwer wspierający klientów < `2025-06-18` **MAY** traktować brak nagłówka jako wersję `2025-03-26`; serwer, który takich klientów nie wspiera, **MUST** odrzucić żądanie bez nagłówka.

**Niestandardowe nagłówki z parametrów narzędzia (`x-mcp-header`)** — [SEP-2243](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2243):
- Serwer **MAY** oznaczyć parametr narzędzia adnotacją `"x-mcp-header": "Region"` w `inputSchema`; klient **MUST** odzwierciedlić wartość w nagłówku `Mcp-Param-Region`.
- Dozwolone tylko dla typów prymitywnych (`string`, `integer`, `boolean`); typ `number` **nie jest dozwolony**; `integer` musi mieścić się w zakresie bezpiecznym JS (−2^53+1 … 2^53−1).
- Tylko właściwości *statycznie osiągalne* ze korzenia schematu wyłącznie przez łańcuch kluczy `properties` — **nie** przez `items`, `oneOf`/`anyOf`/`allOf`/`not`, `if`/`then`/`else` ani `$ref`.
- Wartości nie-ASCII / ze spacjami / ze znakami kontrolnymi kodowane jako `=?base64?{...}?=` (markery **case-sensitive**, małymi literami).
- **Ostrzeżenie ze specyfikacji:** „Server developers **SHOULD NOT** mark sensitive parameters (passwords, API keys, tokens, PII) with `x-mcp-header`, as header values are visible to network intermediaries."
- Wartość biznesowa dla operatora: gateway, rate limiter i WAF mogą routować i limitować po nagłówkach **bez parsowania ciała JSON**. Spec zaleca pośrednikom: jeśli `MCP-Protocol-Version` wskazuje starszą wersję lub go brak — **odrzuć** żądanie, zamiast ufać niezweryfikowanym wartościom nagłówków.

### 1.4 Kompatybilność wsteczna z `2024-11-05` … `2025-11-25`

Model „er" zdefiniowany w [Versioning and Compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning.md):

- **Modern** = `2026-07-28` i późniejsze (metadane per-żądanie).
- **Legacy** = `2025-11-25` i wcześniejsze (handshake `initialize`).
- **Dual-era** = implementacja obsługująca oba.

**Kluczowy cytat dla serwera takiego jak Vestige:**

> „A dual-era server **MAY** serve both eras concurrently on the same endpoint or process."
> — [versioning.md, „Backward Compatibility with Initialization-Based Versions"](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning.md)

Selekcja ery po stronie serwera dual-era:
- Żądanie z nowoczesnym `_meta` → obsłuż bezstanowo wg `2026-07-28`.
- Żądanie `initialize` → semantyka legacy, ograniczona do procesu (stdio) lub sesji (HTTP) wg wynegocjowanej wersji.

**Macierz kompatybilności (skrót):**

| Klient | Serwer | Wynik |
|---|---|---|
| Modern | Modern | ✅ działa |
| Modern | Legacy | ❌ zawodzi (serwer może milczeć, zwrócić błąd implementacyjny, albo — groźniej — zinterpretować metodę dwuznaczną wg semantyki legacy) |
| Legacy | Modern | ❌ zawodzi; stdio: `initialize` odrzucone; HTTP: brak wymaganych nagłówków → 400 |
| Legacy | Dual-era | ✅ działa |
| Modern | Dual-era | ✅ działa |

**Co ma zrobić serwer wspierający wyłącznie `2026-07-28`, gdy dostanie ruch od starszego klienta** (cytaty ze specyfikacji):
- HTTP GET lub DELETE na endpoint MCP → **`405 Method Not Allowed`**.
- Nagłówek `Mcp-Session-Id` na żądaniu → **zignoruj**; nie twórz i nie odbijaj ID sesji.
- Nagłówek `Last-Event-ID` → **zignoruj**; strumienie nie są wznawialne.

**Serwer modern-only, który dostanie `initialize`**, **SHOULD** wymienić wspierane wersje w błędzie — na każdej trasie — bo klienci legacy nie mają mechanizmu „fall-forward" i ten komunikat może być jedyną diagnostyką, jaką pokażą użytkownikowi.

**Detekcja ery po stronie klienta:**
- **stdio:** klient **SHOULD** wysłać `server/discover` przed jakimkolwiek innym żądaniem. Trzy wyniki: `DiscoverResult` → modern; rozpoznany nowoczesny błąd JSON-RPC (np. `UnsupportedProtocolVersionError`) → modern, **nie** cofaj się do `initialize`; **cokolwiek innego albo timeout** → legacy, cofnij się do `initialize`. Fallback **MUST NOT** być uzależniony od jednego konkretnego kodu błędu — serwery legacy odpowiadają na nieznane żądania przed `initialize` błędami zależnymi od implementacji (często `-32601` lub `-32602**) albo nie odpowiadają wcale.
- **Streamable HTTP:** klient próbuje żądania modern; przy `400 Bad Request` **SHOULD** sprawdzić ciało odpowiedzi, zanim się cofnie — bo modern serwery też używają 400 (dla `UnsupportedProtocolVersionError`, `MissingRequiredClientCapabilityError` i błędów walidacji nagłówków). Rozpoznany nowoczesny błąd → modern. Puste ciało / nierozpoznany błąd → fallback do `initialize`, ewentualnie dalej do zdeprecjonowanego HTTP+SSE.
- Ustalenie ery jest własnością **serwera**, nie pojedynczego żądania. Klienci **SHOULD** cache'ować wynik na czas życia procesu (stdio) lub origin (HTTP) i **MAY** utrwalać go między restartami.

**Utrzymanie kompatybilności z HTTP+SSE (`2024-11-05`):** serwer **SHOULD** hostować **oba** stare endpointy (SSE + POST) obok nowego „MCP endpoint". Można też połączyć stary POST z nowym endpointem, ale spec ostrzega, że „this may introduce unneeded complexity".

### 1.5 Serverless, stateless, skalowanie poziome, sticky sessions, load balancery

Specyfikacja nie zawiera osobnej sekcji „deployment", ale wymagania czynią deployment trywialnym, i to jest explicytnym celem wydania:

> „Every request is self-describing, with an optional discovery call for clients that want capabilities up front, so any request can land on any instance behind a plain round-robin load balancer."
> — [blog, 2026-07-28](https://blog.modelcontextprotocol.io/posts/2026-07-28/)

> „With the session gone, each request carries everything a server needs to answer it, so any instance can. A deployment that needed sticky sessions and a shared store can run behind a plain round-robin load balancer."
> — [AAIF, „MCP in production"](https://aaif.io/blog/mcp-in-production-what-changes-after-the-demo-works) (2026-07-31)

Praktyczne implikacje (z tych samych źródeł):
1. **Sticky sessions / session affinity: usunąć** z load balancera. Nie ma już czego „przyklejać".
2. **Wspólny store sesji: zbędny** na poziomie protokołu.
3. **Routing po nagłówkach** `Mcp-Method` / `Mcp-Name` — gateway nie musi parsować ciała.
4. **Cache:** `ttlMs` + `cacheScope` (`"public"` | `"private"`) na `tools/list`, `resources/list`, `resources/read` itd. pozwalają cache'ować katalog narzędzi i utrzymać stabilny prompt cache. `cacheScope: "public"` na `tools/list` jest bezpieczne tylko, jeśli lista narzędzi nie zależy od uprawnień; jeśli filtrujesz narzędzia po scope, użyj `"private"`.
5. **Stan aplikacyjny** przenosi się na jawne uchwyty („handles") mintowane przez narzędzie i przekazywane jako zwykły argument. Rekomendacje ze specyfikacji ([Tools → Stateful Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)):
   - **Autoryzacja:** dla serwerów z auth uchwyt jest *nazwą, nie capability* — waliduj uprawnienia do uchwytu przy **każdym** wywołaniu. Dla serwerów bez auth uchwyt jest bearer-tokenem: generuj z wystarczającą entropią (np. UUIDv4) i nadaj ograniczony czas życia.
   - **Nieprzezroczystość:** nie koduj struktury wewnętrznej w uchwycie.
   - **Czas życia:** polityka retencji **musi** być opisana w `description` narzędzia tworzącego (np. „baskets expire after 24 hours of inactivity"), żeby model ją widział.
   - **Błędy wygaśnięcia:** wywołanie z wygasłym/nieznanym uchwytem **powinno** zwrócić *tool execution error* z tą informacją, by model mógł się odtworzyć.
6. **Wymóg bezpieczeństwa transportu** (spec, „Security & Endpoint"):
   - Serwer **MUST** walidować nagłówek `Origin` przy każdym połączeniu (obrona przed DNS rebinding). Jeśli `Origin` jest obecny i nieprawidłowy → **HTTP 403 Forbidden**.
   - Uruchamiany lokalnie serwer **SHOULD** bindować się wyłącznie do `127.0.0.1`, nie `0.0.0.0`.
   - Serwer **SHOULD** implementować uwierzytelnianie dla wszystkich połączeń.
7. **`X-Accel-Buffering: no`** — konieczne, jeśli SSE idzie przez nginx/reverse proxy.

**Serverless / edge:** wydanie jest explicytnie targetowane pod platformy serverless. Cytaty partnerów z [bloga wydania](https://blog.modelcontextprotocol.io/posts/2026-07-28/): AWS („deploy MCP servers on standard, scalable infrastructure without managing sessions or persistent connections", Amazon Bedrock AgentCore), Cloudflare („run MCP servers directly in Workers, call tools without transport-session overhead"), Netlify, Microsoft Foundry. ⚠️ To materiały marketingowe partnerów — traktuj jako sygnał wsparcia, nie jako benchmark.

**Rozszerzenie na przyszłość:** [roadmap, aktualizacja 2026-08-22](https://modelcontextprotocol.io/development/roadmap.md) zapowiada „**HTTP over stdio**": Streamable HTTP jako jedyne binding, mówione po stdin/stdout przez HTTP/2 — czyli jeden model transportu zamiast dwóch. To jeszcze nie jest w specyfikacji.

### 1.6 Koszt migracji — dane ilościowe z SEP-2567

[SEP-2567](https://modelcontextprotocol.io/seps/2567-sessionless-mcp.md) (Final, Standards Track, utworzony 2026-03-11) zawiera **automatyczne badanie losowej próby 1000 repozytoriów open-source serwerów MCP**, klasyfikowanych przez analizę LLM per-repo. To najlepsza publiczna estymata kosztu migracji:

| Kategoria | Udział | Migracja |
|---|---:|---|
| Brak odniesienia do session ID na poziomie aplikacji | **90,0%** | brak |
| `Map<sessionId, Transport>` (boilerplate TS SDK) | **3,5%** | usuwane przez sessionless transport w SDK |
| Tylko setup transportu (`sessionIdGenerator`, nigdy nie czytany) | **2,8%** | usunięcie jednej opcji konstruktora |
| **Stan aplikacji kluczowany po sesji** | **2,5%** | migracja na explicit handles lub principala auth |
| **Proxy / gateway ze sticky routing** | **0,7%** | wymaga zaprojektowanego zamiennika |
| **Binding artefaktów auth do session ID** (JWT claims, PKCE verifier) | **0,5%** | nonce generowany przez serwer albo subject tokenu |

**Wniosek dla Vestige:** jeśli stan (baza pamięci) jest w SQLite, a nie w mapie kluczowanej po `Mcp-Session-Id`, należysz do 90% bezkosztowych. Jeśli jakakolwiek logika (np. „bieżąca sesja konwersacji", `session_context`) opiera się na tożsamości połączenia — to jest ta 2,5% i wymaga jawnego uchwytu.

**Uzasadnienie usunięcia sesji (z SEP-2567) — bardzo istotne dla projektowania:**
> „[S]essions have not converged on a consistent meaning across clients: some scope them per tool call, some per application launch, some per page load, and almost none resume them. A server author cannot predict what scope or lifetime a session will have when their server is connected to an arbitrary client, which has made the session unreliable as a container for application state."

Konkretne dowody przywołane w SEP: ChatGPT tworzył **świeżą sesję dla każdego pojedynczego wywołania narzędzia**, Claude.ai robił to samo do niedawna ([microsoft/playwright-mcp#1045](https://github.com/microsoft/playwright-mcp/issues/1045), 2025-09); większość klientów desktop/IDE tworzy jedną sesję na start aplikacji; klienci webowi — jedną na załadowanie strony. Referencyjny TypeScript SDK **nie ma publicznego API do rekonstrukcji sesji na innym węźle**, więc wdrożenia wielowęzłowe nie mogą honorować wznowienia ([typescript-sdk#1658](https://github.com/modelcontextprotocol/typescript-sdk/issues/1658), 2026-03).

**Ogólne zalecenie dla serwerów stdio utrzymujących stan w pamięci procesu** (cyt. z SEP-2567): „such servers **SHOULD NOT** rely on process-lifetime state and **SHOULD** migrate to explicit handles. Process lifetime has the same undefined-scope problem this SEP removes for HTTP."

**Kandydaci na uchwyty w Vestige:** ⚠️ to moja propozycja, nie cytat. Kandydaci: identyfikator „sesji konwersacji" (`session_id` już istnieje w `smart_ingest` — dziś jako opcjonalne pole provenance, nie jako stan serwera), identyfikator zadania `dream`/`reflect`, identyfikator eksportu/backupu, identyfikator transakcji wsadowej `smart_ingest` batch. Każdy z nich powinien mieć: nieprzezroczystą postać, właściciela, czas życia **opisany w `description` narzędzia tworzącego**, sprawdzenie uprawnień przy każdym wywołaniu i czytelny błąd wygaśnięcia.

**Wytyczne dla uchwytów, których jeszcze nie ma w spec, a są w SEP-2567:**
- **Nieprzezroczystość:** uchwyt kodujący strukturę wewnętrzną (`cart_user42_2026-03-11`) zaprasza do parsowania i zgadywania.
- **Posiadanie ≠ autoryzacja** (gdy istnieje auth): waliduj `(handle, auth_context)` przy **każdym** wywołaniu — uchwyty trafią do logów czatu, schowka i promptów subagentów.
- **Bez auth: uchwyt jest tokenem bearer.** SEP-2567 podaje konkret: **co najmniej 128 bitów kryptograficznie bezpiecznej entropii** (np. UUIDv4 albo 22+ znaki URL-safe base64), nigdy nie wyprowadzany z przewidywalnych danych, z ograniczonym czasem życia. (Uwaga: specyfikacja w [Tools → Stateful Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md) mówi tylko „sufficient entropy (e.g., a UUIDv4)" — liczbę 128 bitów podaje SEP-2567, nie spec.)
- **Trwałość opisana w `description` narzędzia `create_*`** — polityka tylko w dokumentacji serwera **nie jest widoczna dla modelu**.
- **Błędy wygaśnięcia muszą być konkretne:** „basket `bsk_a1b2c3` has expired", nie „invalid argument".
- **Tworzenie przyjmuje parametry:** `create_context(cluster="staging")` zamiast `create_context()` + `set_cluster(...)` — jedno round-trip zamiast dwóch i brak stanu częściowo skonfigurowanego.
- **Odpowiedzialność klienta:** klient musi zadbać, by string uchwytu przetrwał **kompakcję kontekstu**. Jeśli podsumowanie konwersacji wyrzuci uchwyt, stan staje się osierocony.

**Konsekwencja, którą łatwo przeoczyć:** SEP-2567 zakazuje wzorca „wywołaj `connect_database()`, a `query` i `list_tables` pojawią się w kolejnym `tools/list`". **Serwery nie mogą już mutować wyników listy jako efektu ubocznego innych żądań.** Zamiast tego wystawia się `query` i `list_tables` bezwarunkowo, a one przyjmują argument `connection_id`. Dla Vestige: **nie dodawaj narzędzi dynamicznie po `session_context`** — to jest teraz nielegalne w `2026-07-28`.

**Server Card jako alternatywa dla `server/discover`:** [SEP-2127](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2127) proponuje dokument `.well-known/mcp.json` dla discovery po HTTP. SEP-2575 mówi, że **oba mechanizmy są celowo zachowane**: Server Card jest dobry dla HTTP (bez auth, cache'owalny, indeksowalny), `server/discover` daje jednolite RPC na HTTP i stdio. Jest też [Server Card Working Group](https://modelcontextprotocol.io/community/working-groups/server-card.md). ⚠️ Status SEP-2127 niezweryfikowany.

### 1.7 Anulowanie żądań — różni się między transportami

Źródło: [Cancellation](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation.md), [Streamable HTTP → Cancellation](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http.md).

| Transport | Sygnał anulowania |
|---|---|
| **Streamable HTTP** | **Zamknięcie strumienia SSE odpowiedzi.** Serwer **MUST** traktować rozłączenie klienta jako anulowanie żądania. `notifications/cancelled` **nie jest wymagane ani oczekiwane**. |
| **stdio** | Brak strumienia per-żądanie. Klient **MUST** wysłać `notifications/cancelled` z `requestId`. |

Serwer **MUST NOT** wysyłać `notifications/cancelled` w żadnym innym celu niż zerwanie subskrypcji `subscriptions/listen`.

**Timeouty** (spec): implementacje **SHOULD** ustawiać timeouty dla wszystkich wysyłanych żądań; po przekroczeniu — anuluj i przestań czekać. SDK **SHOULD** pozwalać konfigurować timeout per-żądanie. Implementacje **MAY** resetować zegar timeoutu po otrzymaniu `notifications/progress` (dowód, że praca trwa), ale **SHOULD** zawsze egzekwować maksymalny timeout niezależnie od postępu.

---

## 2. OAuth 2.1 dla MCP w `2026-07-28`

Źródła podstawowe: [Authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/index.md), [Client Registration](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration.md), [Authorization Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md), [Security Best Practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md).

### 2.1 Wymagane standardy (stan bieżący)

Spec wymienia jako podstawę:
- **OAuth 2.1 IETF DRAFT** — `draft-ietf-oauth-v2-1-13`
- **RFC 6750** — OAuth 2.0 Bearer Token Usage
- **RFC 8414** — Authorization Server Metadata
- **RFC 7591** — Dynamic Client Registration ⚠️ **DEPRECATED w MCP**
- **RFC 8707** — Resource Indicators
- **RFC 9728** — Protected Resource Metadata
- **RFC 9207** — Authorization Server Issuer Identification
- **draft-ietf-oauth-client-id-metadata-document-00** — Client ID Metadata Documents (**CIMD**)
- **OpenID Connect Discovery 1.0** + **OIDC Dynamic Client Registration 1.0**
- **RFC 7636 (PKCE)** — ⚠️ **nie jest wymieniony wprost w sekcji „Standards Compliance" `2026-07-28`**. Flow na diagramie zawiera krok „Generate PKCE parameters", a `iss`/PKCE są opisane w [Security Best Practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md). Traktuj PKCE jako wymagane przez OAuth 2.1, nie przez osobną normę MCP. **RFC 8693 (token exchange)** — ⚠️ **nie jest wymagany przez spec `2026-07-28`**; pojawia się wyłącznie w [roadmapie](https://modelcontextprotocol.io/development/roadmap.md) (2026-08-22) jako plan na przyszłość („Agent identity and delegation… RFC 8693 token exchange, coordinated with the IETF OAuth and WIMSE working groups") oraz w rozszerzeniu Enterprise-Managed Authorization.

### 2.2 Wymagania normatywne (MUST/SHOULD) — dokładne brzmienie

1. **Authorization servers MUST** implementować OAuth 2.1 z odpowiednimi środkami bezpieczeństwa dla klientów confidential i public.
2. **Authorization servers i MCP clients SHOULD** wspierać **CIMD**.
3. **Authorization servers i MCP clients MAY** wspierać **RFC 7591 DCR** — „Dynamic Client Registration is deprecated and retained for backwards compatibility".
4. **MCP servers MUST** implementować **RFC 9728** (Protected Resource Metadata). **MCP clients MUST** używać RFC 9728 do discovery AS.
5. **MCP authorization servers MUST** udostępnić co najmniej jedno z: RFC 8414 **lub** OIDC Discovery 1.0. **MCP clients MUST** wspierać **oba** mechanizmy.
6. Klient **MUST** uzyskać `client_id` przez jeden z trzech mechanizmów: CIMD, pre-registration, albo DCR — w tej kolejności priorytetu.
7. **MCP clients MUST** implementować **RFC 8707 Resource Indicators** — `resource` **MUST** być w żądaniu autoryzacyjnym **i** w żądaniu tokenu, i **MUST** identyfikować serwer MCP. Klienci **MUST** wysyłać ten parametr niezależnie od tego, czy AS go wspiera.
8. **MCP servers MUST** walidować, że access token został wydany **konkretnie dla nich** (audience wg RFC 8707). **MCP servers MUST NOT** akceptować ani przekazywać dalej żadnych innych tokenów. Nieprawidłowe/wygasłe tokeny → **HTTP 401**.
9. **MCP clients MUST NOT** wysyłać tokenów innych niż wydane przez AS tego serwera MCP.
10. `iss` (RFC 9207): AS **SHOULD** dołączać `iss` w odpowiedziach autoryzacyjnych (także błędnych); AS, które to robią, **MUST** ustawić `authorization_response_iss_parameter_supported: true`. Klient **MUST** walidować obecny `iss` przez **proste porównanie stringów** (RFC 3986 §6.2.1) — **MUST NOT** normalizować wielkości liter schematu/hosta, domyślnych portów, końcowych slashy ani percent-encodingu. **Zapowiedź:** „A future revision of this specification is expected to upgrade authorization server inclusion of `iss` from **SHOULD** to **MUST**."
11. **Client credentials są związane z AS, który je wydał** ([SEP-2352](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2352)): klienci **MUST** kluczować utrwalone poświadczenia po `issuer`, **MUST NOT** używać ich z innym AS i **MUST** re-rejestrować się przy zmianie AS. Wyjątek: client ID z CIMD są **przenośne** między AS (bo to self-hosted URL) — re-rejestracja niepotrzebna.
12. **DCR: klienci MUST** podać `application_type` ([SEP-837](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/837)). Aplikacje natywne (desktop, mobile, CLI, lokalnie hostowane web appy na `localhost`) **SHOULD** użyć `"native"`; aplikacje webowe na nie-lokalnym hoście — `"web"`. Pominięcie domyślnie daje `"web"` w OIDC, co **koliduje z natywnymi redirect URI** — to jest właśnie źródło błędów `redirect_uri` w CLI.
13. **Refresh tokeny:** serwery (jako Protected Resource) **SHOULD NOT** dołączać `offline_access` do `WWW-Authenticate` scope ani do `scopes_supported` w PRM — refresh tokeny nie są wymogiem zasobu.

### 2.3 Co się zmieniło / co jest przestarzałe

| Element | Zmiana w `2026-07-28` | SEP/PR |
|---|---|---|
| **DCR (RFC 7591)** | **DEPRECATED** → CIMD. Działa dalej dla kompatybilności; „will be removed in a future version" | [PR #2858](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2858) |
| **`iss` (RFC 9207)** | Dodane jako SHOULD dla AS / MUST walidacji dla klientów | [SEP-2468](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2468) |
| **`application_type` w DCR** | Nowy obowiązek klienta | [SEP-837](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/837) |
| **Binding poświadczeń do issuera** | Nowe MUST | [SEP-2352](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2352) |
| **Resource not found** | Kod błędu `-32002` → `-32602` | [SEP-2164](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2164) |

Rejestr deprecjacji: „Dynamic Client Registration … Deprecated in `2026-07-28` … Earliest removal: First revision released on or after 2027-07-28" — [Deprecated Features](https://modelcontextprotocol.io/specification/2026-07-28/deprecated.md).

### 2.4 Client ID Metadata Documents (CIMD) — status

- **Numer draftu: `draft-ietf-oauth-client-id-metadata-document-00`** (Internet-Draft, nie RFC). URL: https://datatracker.ietf.org/doc/html/draft-ietf-oauth-client-id-metadata-document-00
- MCP: **SHOULD** wspierać (zarówno klienci, jak i AS). Rekomendowany mechanizm dla scenariusza „brak wcześniejszej relacji klient–serwer" (najczęstszy w MCP).
- Advertise: AS ustawia `client_id_metadata_document_supported: true` w metadanych RFC 8414.
- Wymagania: `client_id` **MUST** być URL-em `https` z komponentem ścieżki (np. `https://example.com/client.json`); dokument **MUST** zawierać `client_id`, `client_name`, `redirect_uris`; `client_id` w dokumencie **MUST** dokładnie odpowiadać URL-owi.
- AS: **MUST** walidować zgodność `client_id` z URL, **MUST** walidować `redirect_uris`, **SHOULD** cache'ować z poszanowaniem nagłówków HTTP.
- ⚠️ **Uwaga bezpieczeństwa:** CIMD otwiera AS na **SSRF** — AS pobiera URL podany przez nieznanego klienta. Patrz [Security Best Practices → SSRF Against Authorization Servers](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md).
- ⚠️ **Localhost Redirect URI Impersonation:** gdy klient używa CIMD, dokument dowodzi kontroli nad domeną, ale **nie** dowodzi, który lokalny proces nasłuchuje na `localhost`. Atakujący może podać `client_id` legalnego klienta i własny port `localhost`. Mitygacje po stronie AS: dodatkowe ostrzeżenia dla `localhost`-only redirect URI i wyraźne pokazywanie hostname'a redirect URI. Patrz [Localhost Redirect URI Risks](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations).

### 2.5 Serwery **stdio / lokalne** — czy auth jest potrzebne?

**Odpowiedź ze specyfikacji jest jednoznaczna: NIE, i wręcz przeciwnie — nie należy go implementować.**

> „Authorization is **OPTIONAL** for MCP implementations. When supported: Implementations using an HTTP-based transport **SHOULD** conform to this specification. Implementations using an **STDIO transport SHOULD NOT follow this specification, and instead retrieve credentials from the environment**."
> — [Authorization → Protocol Requirements](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/index.md)

Potwierdzenie w [basic/index.md → Auth](https://modelcontextprotocol.io/specification/2026-07-28/basic/index.md): „implementations using STDIO transport **SHOULD NOT** follow this specification, and instead retrieve credentials from the environment. Additionally, clients and servers **MAY** negotiate their own custom authentication and authorization strategies."

**Rekomendowana postawa dla serwera local-first (Vestige):**

1. **stdio: zero OAuth.** Poświadczenia (jeśli w ogóle) z env / pliku konfiguracyjnego / macOS Keychain. To jest zgodne ze spec, nie jest obejściem.
2. **HTTP: minimalna poprawna postawa, jeśli bindujesz się do localhost.** Spec dla transportu mówi: serwer **MUST** walidować `Origin` (403 przy nieprawidłowym), **SHOULD** bindować tylko do `127.0.0.1`, **SHOULD** implementować uwierzytelnianie. Praktycznie: walidacja `Origin` + bind na loopback załatwia DNS rebinding; token bearer z pliku z uprawnieniami `0600` załatwia „proper authentication".
3. **Spec wprost wskazuje alternatywy dla lokalnego HTTP** ([Security Best Practices → Local MCP Server Compromise](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md)): „Use the `stdio` transport to limit access to just the MCP client" oraz „Restrict access if using an HTTP transport, such as: Require an authorization token; **Use unix domain sockets or other IPC mechanisms with restricted access**".
4. **Jeśli wystawiasz HTTP poza loopback, pełny OAuth 2.1 staje się obowiązkowy** — bo wtedy jesteś resource serverem z realnym confused-deputy i token-passthrough ryzykiem. Spec: „MCP servers **MUST NOT** accept any tokens that were not explicitly issued for the MCP server."
5. **Resource indicators dla serwera lokalnego:** kanoniczny URI **MUST** mieć schemat; przykłady poprawnych to `https://mcp.example.com/...`. `mcp.example.com` (bez schematu) i URI z fragmentem są **nieprawidłowe**. Dla `localhost` spec nie podaje wprost przykładu kanonicznego URI dla `http://localhost:PORT/mcp` — ⚠️ **NIEPOTWIERDZONE**: spec nie rozstrzyga, czy `http://127.0.0.1:3927/mcp` jest poprawnym `resource`. Wskazówka praktyczna: OAuth 2.1 dopuszcza `http://` dla **loopback redirect URI** (nie dla dowolnych URL-i), a [Security Best Practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md) mówi: „Reject `http://` URLs except for loopback addresses (`localhost`, `127.0.0.1`, `::1`) during development". Traktuj `http://127.0.0.1:PORT/mcp` jako `resource` tylko w trybie dev; w produkcji wymagaj `https://`.
6. **Localhost redirect URI** jest wprost uznany za prawidłowy w przykładzie dokumentu CIMD: `"redirect_uris": ["http://127.0.0.1:3000/callback", "http://localhost:3000/callback"]` — [Client Registration → Example Metadata Document](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration.md). To jest *redirect URI klienta*, nie `resource` serwera — nie mylić tych dwóch.

### 2.6 Rozszerzenia autoryzacyjne (poza rdzeniem)

- **Enterprise-Managed Authorization (EMA)** — scentralizowana kontrola dostępu przez IdP.
- **OAuth Client Credentials** — M2M ([SEP-1046](https://modelcontextprotocol.io/seps/1046-support-oauth-client-credentials-flow-in-authoriza.md)).
- Repozytorium: [github.com/modelcontextprotocol/ext-auth](https://github.com/modelcontextprotocol/ext-auth). Lista: [Authorization Extensions](https://modelcontextprotocol.io/extensions/auth/overview.md).
- Roadmap 2026-08-22: **DPoP** (Demonstrating Proof of Possession) do sfinalizowania przez Agent Identity WG; **Workload Identity Federation** ([SEP-1933](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1933)); **ID-JAG**; **RFC 8693 token exchange**.

---

## 3. Rate limiting, limity zasobów, progress, timeouty, rozmiary, Tasks

### 3.1 Co mówi specyfikacja — i czego NIE mówi

**Specyfikacja MCP nie definiuje żadnego konkretnego mechanizmu ani liczby rate limitingu.** Jedyne wymagania normatywne:

> „Servers **MUST**: Validate all tool inputs; Implement proper access controls; **Rate limit tool invocations**; Sanitize tool outputs."
> — [Tools → Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)

Dodatkowo:
- **Logging (zdeprecjonowane):** serwery **SHOULD** rate-limitować wiadomości logów — [Logging → Implementation Considerations](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/logging.md).
- **Progress:** „Both parties **SHOULD** implement rate limiting to prevent flooding" — [Progress](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress.md).
- **JSON Schema:** implementacje **SHOULD** nakładać rozsądne granice na złożoność schematów — maksymalna głębokość, limit liczby subschematów, budżet czasu na walidację — bo „a malicious schema [could act] as a Denial-of-Service vector against the validator". **MUST NOT** automatycznie dereferencjonować `$ref` do URI sieciowych. — [basic/index.md → `$ref` Resolution i Composition-Keyword Resource Use](https://modelcontextprotocol.io/specification/2026-07-28/basic/index.md)
- **Klienci:** „Clients **SHOULD** implement timeouts for tool calls" i „Log tool usage for audit purposes" — [Tools → Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md).
- **Pagination:** „**Page size** is determined by the server, and clients **MUST NOT** assume a fixed page size". Nie ma zaleconej liczby elementów na stronę. Nieprawidłowy kursor → `-32602`. — [Pagination](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/pagination.md)
- **Rozmiary żądań/odpowiedzi:** ⚠️ **spec nie definiuje żadnego limitu rozmiaru wiadomości JSON-RPC** — ani w bajtach, ani w tokenach. To luka, którą każdy operator musi zamknąć sam.

### 3.2 Gdzie przykładać limity — model z praktyki

Kluczowa obserwacja operacyjna: **w MCP nie ma pojęcia „sesji" w transporcie, więc nie ma naturalnego klucza do limitowania per-sesja.** `Mcp-Session-Id` zniknął ([changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog.md)), a `notifications/initialized` już nie istnieje. Dostępne klucze to:

| Poziom | Klucz | Realizacja |
|---|---|---|
| **Per-IP / per-origin** | adres klienta | przed serwerem (reverse proxy / WAF) |
| **Per-token / per-tenant** | `sub` / `aud` z zwalidowanego access tokenu | gateway z walidacją JWT |
| **Per-metoda** | nagłówek `Mcp-Method` (bez parsowania ciała!) | gateway / rate limiter |
| **Per-narzędzie** | nagłówek `Mcp-Name` (bez parsowania ciała!) | gateway z deskryptorami CEL na `body.params.name` |
| **Per-handle** | jawny uchwyt stanu przekazany jako argument | logika serwera |
| **Globalny sufit** | cały gateway | twardy backstop |

Nowe nagłówki `Mcp-Method` i `Mcp-Name` z [SEP-2243](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2243) istnieją **dokładnie po to**, żeby gateway mógł limitować bez parsowania JSON-a. To najważniejsza zmiana operacyjna w `2026-07-28` dla rate limitingu.

### 3.3 Konkretne liczby z realnych wdrożeń

**agentgateway (Solo.io), wersja docs `2026.7.1` LTS i `2026.9.0`** — [Rate limiting for MCP](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/rate-limit.md). To najlepiej udokumentowany publiczny przykład liczb.

Kluczowa pułapka, którą dokumentują wprost: **limit „requests per second" ≠ „tool calls per second".**

> „A `requests: 5` per-second limit doesn't allow 5 tool calls per second. Instead, the limit allows roughly 1 tool call session per second (5 requests ÷ ~5 per session). Size your limits accordingly: **think in sessions, not raw HTTP requests**."

| Akcja klienta | POST-y do `/mcp` (wersja `2025-11-25` i starsze) |
|---|---|
| Połączenie z serwerem | `initialize` → 1 POST |
| Lista narzędzi | `tools/list` → 1 POST |
| Jedno wywołanie narzędzia | `tools/call` → 1 POST |
| **Razem na sesję** | **~3–5 POST-ów** |

⚠️ **Uwaga:** ta tabela pochodzi z dokumentacji, która opisuje model *z handshake'em*. W `2026-07-28` handshake zniknął, więc liczba POST-ów na „sesję" spada o `initialize` + `notifications/initialized`. Przy `npx @modelcontextprotocol/inspector --cli` dokumentacja podaje **3 POST-y na sekwencję** (`initialize` → `tools/list` → `tools/call`) i **15 ÷ 3 = 5 pełnych sekwencji** przy limicie `5/s` z `burst: 10`. **Traktuj te liczby jako górną granicę dla ery legacy; dla `2026-07-28` licz ~2 POST-y** (⚠️ mój wniosek, nie cytat — nie znalazłem opublikowanego przelicznika dla `2026-07-28`).

**Konkretne wartości konfiguracji z dokumentacji agentgateway:**

- **Limit lokalny (per-replika, in-process):** `requests: 5, unit: Seconds, burst: 10` na `HTTPRoute` MCP → łącznie 15 żądań przed 429. Uzasadnienie bur stu: „during session initialization, an agent typically fires `initialize` → `tools/list` → several `tools/call` requests back-to-back. Without burst capacity, the MCP server would hit the limit before doing any real work."
- **Globalny sufit na Gateway:** `requests: 10000, unit: Minutes, burst: 5` jako backstop dla całego ruchu (HTTP + MCP + LLM).
- **Limity per-narzędzie (globalne, przez Envoy Rate Limit Service + Redis):**
  - `trigger-long-running-operation`: **3 wywołania/minutę**
  - `sampleLLMCall`: **3 wywołania/minutę**
  - wszystkie pozostałe `tools/call`: **10 wywołań/minutę**
  - Każde narzędzie ma **własny licznik** w Redisie; wyczerpanie budżetu jednego nie wpływa na inne.
- **Nagłówki odpowiedzi:** `x-ratelimit-limit`, `x-ratelimit-remaining`, `x-ratelimit-reset`, oraz `x-envoy-ratelimited` (tylko przy globalnym rate limitingu z Envoy RL service — **nie** przy lokalnym).

**Warstwy:** polityka gateway-level działa jako twardy backstop dla całego ruchu; polityka route-level ma pierwszeństwo dla ruchu pasującego do route'a. Stosuj obie.

**Pułapka kosztowa (realny incydent):** ⚠️ źródło to blog Qiita (japoński), nie dokumentacja vendorа — traktuj jako anegdotę. Opisuje wyczerpanie darmowego limitu Cloudflare Workers (100 000 żądań) w 7 godzin, bo `GET /mcp` zwracał 200 i klienci/probingu pollingowali go w pętli. Wniosek uogólnialny i wart uwagi: **`GET /mcp` w erze `2025-11-25` był kosztownym, długo żyjącym połączeniem — w `2026-07-28` GET jest usunięty i serwer modern-only SHOULD zwracać `405`**, co samo w sobie eliminuje tę klasę marnotrawstwa. Źródło: [qiita.com/viva_tweet_x/items/3ff94aa7a63392e56d20](https://qiita.com/viva_tweet_x/items/3ff94aa7a63392e56d20).

**Pułapka paginacji:** realny bug w LiteLLM MCP Gateway — `mcp-rest/tools/list` **po cichu ucinał wynik na 100 narzędziach** i nie podążał za `nextCursor`. Issue: [BerriAI/litellm#32229](https://github.com/BerriAI/litellm/issues/32229). Wniosek dla serwera z ~28 narzędziami: jeśli nie paginujesz, nie musisz walidować `nextCursor`; jeśli paginujesz, upewnij się, że domyślna strona jest **mniejsza** niż typowy limit klienta — w przeciwnym razie trafisz na klientów, którzy cicho obcinają.

### 3.4 Rozmiary żądań i odpowiedzi

- **Spec: brak limitu.** ⚠️ To największa luka operacyjna. Nie znalazłem żadnej normatywnej ani zalecanej wartości w `2026-07-28`.
- **Praktyka (agregat commitów open-source, nie autorytet):** typowe implementacje MCP wprowadzają własne limity ciała żądania i zwracają **HTTP 413** przy przekroczeniu. Przykłady: commit [„fix(mcp): deliver the 413 for oversized request bodies"](https://github.com/silverstein/minutes/commit/ebab303c5a70b41f98ee821fbe710fb246e9d3ab), commit [„fix(http): bound body and request lifetime"](https://github.com/aiconnai/engram/commit/1958ed55db890d9e679a666426be0ba7c57b12ac), commit [„cap JSON-RPC request bodies at the shared entry point"](https://github.com/koala73/worldmonitor/commit/efc5be788cad95e9eef06fb840f11dda9f5d421a). Konkretny przykład wartości **4 MB**: PR [sourcemeta/one#1039 „Increase request maximum size to 4 MB"](https://github.com/sourcemeta/one/pull/1039). ⚠️ To nie jest standard MCP — to zbieżna praktyka. **Rekomendacja: przyjmij jawny limit (np. 1–4 MB) na endpoint HTTP i osobny, większy limit na pojedynczą linię JSON w stdio, plus `Content-Length` check przed parsowaniem.**
- **stdio a duże odpowiedzi:** liniowy framing stdio (jedna linia JSON = jedna wiadomość, **MUST NOT** zawierać osadzonych newline'ów — [stdio](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio.md)) oznacza, że domyślne bufory `readline` w klientach mogą być ograniczone (np. historyczny limit 64 KB w Node — PR [TokenRhythm/opensquilla#953](https://github.com/TokenRhythm/opensquilla/pull/953) opisuje dokładnie ten problem). **Wniosek dla serwera pamięci:** duże wyniki (np. `export`, `memory_graph`, `find_duplicates`) powinny być stronicowane albo zwracane jako `resource_link`, a nie jako jedna ogromna linia JSON.

### 3.5 Progress notifications

Źródło: [Progress](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress.md).

- Klient, który chce progresu, dołącza `progressToken` w `_meta`. Token **MUST** być stringiem albo integerem i **MUST** być unikalny wśród wszystkich aktywnych żądań.
- Serwer **MAY** wysłać `notifications/progress` z `progressToken`, `progress`, opcjonalnym `total` i opcjonalnym `message`.
- `progress` **MUST** rosnąć z każdą notyfikacją, nawet gdy `total` nie jest znany. `progress` i `total` **MAY** być float.
- `message` **SHOULD** nieść czytelną dla człowieka informację o postępie.
- Serwer **MAY** w ogóle nie wysyłać progresu, wysyłać go z dowolną częstotliwością, albo pominąć `total`.
- **Progress MUST stop po zakończeniu** operacji.
- **Rate limiting progresu: SHOULD** po obu stronach (spec).
- **W `2026-07-28` `notifications/progress` jest request-scoped** — płynie wyłącznie na strumieniu odpowiedzi żądania, którego dotyczy. **NIE** jest dostarczany na strumieniu `subscriptions/listen`.

### 3.6 Timeouts

- **Spec:** implementacje **SHOULD** ustanawiać timeouty dla wszystkich wysyłanych żądań. SDK i middleware **SHOULD** pozwalać konfigurować timeouty per-żądanie. Implementacje **MAY** resetować zegar przy `notifications/progress`, ale **SHOULD** zawsze egzekwować **maksymalny** timeout niezależnie od progresu — żeby ograniczyć wpływ źle zachowującego się klienta lub serwera. — [Cancellation → Timeouts](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation.md)
- **Serwer SHOULD** implementować timeouty wywołań narzędzi (strona klienta) — [Tools → Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md).
- ⚠️ **Spec nie podaje żadnej konkretnej wartości timeoutu.** Praktyczna wskazówka z uzasadnienia Tasks: „Many clients and transport intermediaries impose timeouts that make [blocking] impractical beyond a few seconds" — [Tasks extension](https://modelcontextprotocol.io/extensions/tasks/overview.md). Czyli: **kilka sekund to praktyczny sufit dla synchronicznego `tools/call` za pośrednikami HTTP.**

### 3.7 Długo działające narzędzia vs Tasks

**Tasks to teraz oficjalne rozszerzenie, nie rdzeń.** Przeniesione z eksperymentalnego rdzenia do `io.modelcontextprotocol/tasks` ([SEP-2663](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2663)). Repozytorium: [github.com/modelcontextprotocol/ext-tasks](https://github.com/modelcontextprotocol/ext-tasks). Dokumentacja: [Tasks](https://modelcontextprotocol.io/extensions/tasks/overview.md).

Zmiany względem eksperymentalnej wersji:
- `tasks/result` (blokujące) → **`tasks/get` (polling)**.
- Nowe **`tasks/update`** dla wejścia klient→serwer.
- **`tasks/list` usunięte** — „it cannot be scoped safely without sessions".
- Serwer może zwracać uchwyty zadań **bez per-request opt-in**; wystarczy jednorazowy opt-in przez capability.
- `notifications/tasks` jako push, opt-in przez `subscriptions/listen`.

**Kiedy używać Tasks** (cytat z dokumentacji): długo działające operacje (CI, batch, trening modeli — „minutes or hours"), workflowy z człowiekiem w pętli (`input_required`), systemy zewnętrznych jobów (jeśli API już używa job ID — zwróć task przy tworzeniu joba), niestabilne połączenia (task ID przeżywa rozłączenie), batch processing z sensownym częściowym progresem.

**Statusy:** `working` | `input_required` | `completed` | `failed` | `cancelled`. Trzy ostatnie są terminalne.

**Anulowanie jest kooperatywne** — serwer potwierdza intencję, ale nie jest zobowiązany zatrzymać pracę.

**Rekomendacja dla serwera pamięci z ~28 narzędziami:** operacje takie jak `dream` (konsolidacja), `reflect --depth deep`, `backfill` embeddingów, `find_duplicates` na dużej bazie, `gc` czy `export` całej bazy **kwalifikują się do Tasks**. `session_context` i `search` — nie; muszą być synchroniczne.

⚠️ **Zastrzeżenie:** wsparcie klienckie dla Tasks jest ograniczone. Dokumentacja mówi wprost: „Host support varies by client" i odsyła do [client matrix](https://modelcontextprotocol.io/extensions/client-matrix). **Serwer MUST NOT zwracać taska klientowi, który nie zadeklarował rozszerzenia** — „Never return a task to a client that did not declare support." Praktyczny wniosek: musisz umieć obsłużyć **oba** warianty tego samego narzędzia (synchroniczny i task) zależnie od per-request `clientCapabilities`.

---

## 4. Sandboxing i izolacja

### 4.1 Co mówi oficjalne MCP — sandboxing to SHOULD, nie MUST

**Tylko *zgoda użytkownika* jest MUST. Sandboxing jest SHOULD.**

Z [Security Best Practices → Local MCP Server Compromise](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md):

> „If an MCP client supports one-click local MCP server configuration, it **MUST** implement proper consent mechanisms prior to executing commands."

**Pre-Configuration Consent (MUST dla klienta):**
- Pokazać **dokładne polecenie**, które zostanie wykonane, **bez ucinania** (łącznie z argumentami i parametrami).
- Jasno oznaczyć to jako potencjalnie niebezpieczną operację wykonującą kod w systemie użytkownika.
- Wymagać **jawnej** zgody użytkownika przed kontynuacją.
- Pozwolić użytkownikowi anulować konfigurację.

**Sandboxing (SHOULD dla klienta)** — cytaty dosłowne:
- „Execute MCP server commands in a **sandboxed environment with minimal default privileges**"
- „Launch MCP servers with restricted access to the file system, network, and other system resources"
- „Use **platform-appropriate sandboxing technologies** (containers, chroot, application sandboxes, etc.)"
- „**Keep sandboxing solutions up-to-date** to account for emerging vulnerabilities"
- „Provide mechanisms for users to explicitly grant additional privileges"
- Ostrzegać, że „MCP servers run with the same privileges as the client"

**Co MAY zrobić serwer, który ma być uruchamiany lokalnie** (SHOULD):
- „Use the `stdio` transport to limit access to just the MCP client"
- „Restrict access if using an HTTP transport, such as: Require an authorization token; **Use unix domain sockets or other Inter Process Communication (IPC) mechanisms with restricted access**"

**[SEP-1024](https://modelcontextprotocol.io/seps/1024-mcp-client-security-requirements-for-local-server-.md)** — **Final**, utworzony **2025-07-22**, autor Den Delimarsky. ⚠️ **Ważne zastrzeżenie: SEP-1024 wymaga WYŁĄCZNIE zgody (consent).** Sandboxing pojawia się w nim jedynie jako **nie-normatywne** środki ograniczania ryzyka rezydualnego. Pole „Reference Implementation" = `N/A`.

**Zmiana nazewnictwa, którą trzeba znać:** **„Session Hijacking" zostało wycofane.** `2026-07-28` usunął sesje protokołu i `Mcp-Session-Id`; zastąpiło je **„State Handle Hijacking"**:
> „MCP servers **MUST NOT** treat possession of a state handle as authentication."
> Mitygacja: „bind handles server-side to the authenticated user, for example by keying stored state as `<user_id>:<handle>` where the user ID is derived from the verified token rather than supplied by the client."
Strona specyfikacji explicytnie odsyła do wersji `2025-11-25` po wytyczne o starym session hijackingu.

**⚠️ WERYFIKOWANY NEGATYW — bardzo istotny:** **żadna z dwóch stron bezpieczeństwa `2026-07-28` nie zawiera sekcji o tool poisoning ani prompt injection.** Najbliższy normatywny haczyk to jedno zdanie w specyfikacji narzędzi:
> „For trust & safety and security, clients **MUST** consider tool annotations to be untrusted unless they come from trusted servers."
> — [Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)

Prace nad tym są odłożone w **Security Interest Group** ([charter](https://modelcontextprotocol.io/community/interest-groups/security.md), 2026-06-13); otwarte tematy obejmują SEP-2809 (ATSA) i „Capability declarations: hints or contracts".

### 4.2 Docker MCP Gateway — co realnie robi

**Wersja: `docker/mcp-gateway` v0.43.3, wydana 2026-07-16.** ~1 574★, licencja MIT. Wymaga **Docker Desktop 4.59+**. Repo: https://github.com/docker/mcp-gateway

**⚠️ v0.43.1 (2026-06-25) to duże łamiące wydanie bezpieczeństwa z explicytną sekcją „Action required":**
- HTTP / SSE / streaming **wymagają teraz Bearer auth domyślnie** (`MCP_GATEWAY_AUTH_TOKEN`; opt-out przez `--allow-unauthenticated`).
- Katalogi i referencje `file://` **muszą** znajdować się pod `~/.docker/mcp/catalogs`.
- Bind-mounty hosta **muszą** być read-only i pod zaufanymi korzeniami.
- Obrazy MCP (`mcp/`) **muszą** być przypięte digestem.
- Kolizje nazw narzędzi/promptów/zasobów są **odrzucane**.
- Logi wywołań narzędzi **nie zapisują już surowych wartości argumentów**.

**Publikowany threat model:** https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md — i ⚠️ **explicytnie wyłącza prompt injection i tool-description poisoning ze swojego zakresu**.

**Sekrety — najważniejszy fakt, który bywa przekłamywany:**

> **Kontener DOCELOWO WIDZI sekret w postaci plaintext, jako zmienną środowiskową.**

Specyfikacja wpisu serwera: `secrets: [{name: github.personal_access_token, env: GITHUB_PERSONAL_ACCESS_TOKEN}]`. Gateway przekazuje Docker Desktopowi URI `se://`, Docker Desktop rozwiązuje go w czasie działania kontenera przez named pipes (nota w v0.40.3), ale **rozwiązana wartość jest wstrzykiwana do środowiska kontenera serwera**. Sekrety są przechowywane w VM Docker Desktop od wersji Desktop **4.43.0**. Mitygacje: scope per-serwer, `--block-secrets` (domyślnie `true` — skanuje argumenty narzędzi i odpowiedzi tekstowe), `--log-calls` (domyślnie `true`, tylko metadane).

> **Kontrast: Docker Sandboxes robi ODWROTNIE** — proxy po stronie hosta wstrzykuje poświadczenia **do nagłówków HTTP**; „raw credential values never enter the VM". To realna różnica architektoniczna — **nie mieszaj tych dwóch mechanizmów.**

**⚠️ Konflikt źródłowy w tym samym repo:** `security.md` mówi, że sekrety są scope'owane per-serwer; `profiles.md` mówi „Secrets are scoped across all servers rather than for each profile". Oba dokumenty żyją w repo dzisiaj.

**Egress sieciowy NIE jest domyślnie blokowany** (cytat dosłowny z threat modelu). Dźwignie:
- per-serwer `disableNetwork: true`
- per-serwer `allowHosts: [host:port]`
- globalnie `--block-network`
- zdalne URL-e MCP dostają domyślnie ścisłą walidację publicznego HTTPS

**Izolacja = zwykły Docker/runc, NIE gVisor.**
> ⚠️ **ZWERYFIKOWANY NEGATYW:** w kompletnym, generowanym CLI reference **nie istnieje flaga `--runtime`**. Dokumentacja mówi: „Containers are started with Docker isolation, `no-new-privileges`, and configured CPU and memory limits". Limity: `--cpus` domyślnie **1**, `--memory` domyślnie **2 GB**. **Słowa gVisor / runsc / Kata / microVM nie pojawiają się nigdzie w dokumentacji repo.** Twierdzenie „Docker MCP Gateway używa gVisor" jest **niepoparte dokumentacją Dockerа** — traktuj jako rozpowszechniony mit.

**CVE-2026-55887 / GHSA-r2xf-7jw5-pjg6** — argument injection przez etykietę obrazu OCI (YAML); dotyczy `>= v0.21.0 < v0.42.2`, opublikowane **2026-06-25**, naprawione w **v0.42.2**. Dodatkowo **GO-2025-4179** (DNS rebinding w sse/streaming).

**Docker Sandboxes** (uruchomione ok. 2026-04, blog **2026-04-16**) używa **celowo zbudowanego VMM** na Apple Hypervisor.framework / Windows Hypervisor Platform / Linux KVM. Docker **explicytnie odrzucił** Firecracker (brak macOS/Windows) i izolaty WASM/V8.
> ⚠️ **Krytyczne zastrzeżenie: lokalne serwery MCP po stdio uruchamiają się na HOŚCIE, nie w VM sandboxa — „Treat local MCP servers as trusted host integrations."** Czyli Docker Sandboxes **nie chroni** przed złośliwym lokalnym serwerem MCP po stdio.

### 4.3 gVisor

**Wersja: `20260914.0`, wydana 2026-09-16** (tygodniowy kadencja).

Architektura: **Sentry** (jądro userspace w Go — brak przekazywania wywołań systemowych do hosta) + **Gofer** (mediacja systemu plików przez 9P) + `runsc` jako runtime OCI. Platformy: KVM (najlepszy na bare metal), **systrap** (domyślny od połowy 2023; używa `SECCOMP_RET_TRAP` → `SIGSYS`), ptrace (już niewspierany, przewidziany do usunięcia).

> ⚠️ **ZWERYFIKOWANY NEGATYW: nie znalazłem żadnych opublikowanych wytycznych ani integracji gVisor specyficznych dla MCP.**
> ⚠️ **Ani Docker Desktop, ani Docker Sandboxes nie używają gVisor domyślnie.** Sandboxes używa własnego VMM. **Każde twierdzenie, że „Docker MCP Gateway używa gVisor", jest niepoparte dokumentacją Dockerа.**

**Praktyczny wniosek:** gVisor jest realną opcją dla self-hosted serwera MCP wystawionego przez HTTP (uruchom `runsc` jako runtime kontenera), ale **nie ma żadnego MCP-specificznego wsparcia ani przepisu** — musisz zbudować to sam.

### 4.4 WASM / WASI

**Stan techniczny — dojrzały:**

| Komponent | Wersja | Data |
|---|---|---|
| **WASI** | **0.3.0** | **2026-06-11** (0.3.1: 2026-08-11) |
| **Wasmtime** | **48.0.2** | **2026-09-10** |
| **Extism** | **1.30.0** | **2026-06-04** |
| **wasmCloud** | **2.9.0** | **2026-09-08** |
| **Spin** (Fermyon) | **4.1.0** | **2026-08-26** |

**Zmiany w WASI 0.3:** `wasi:io` **usunięte**; **`wasi:sockets` skonsolidowane z 7 interfejsów do 2**; **zasób `network` usunięty — „network access is now granted at the world level"**. Wasmtime 46+ włącza WASI 0.3 + `component-model-async` domyślnie; Wasmtime 41/42 wymagają `-Sp3 -W component-model-async=y`.

**⚠️ Zmiana domyślnego zachowania istotna dla bezpieczeństwa:** **Wasmtime 48.0.0** — „The `wasmtime-wasi` crate's default configuration now **denies creation of TCP/UDP sockets by default**."

**Dwie świeże podatności związane z sandboxem, naprawione 2026-08-20** (w 47.0.4 i pokrewnych):
- **GHSA-vqjp-4c8c-hfgg** — ucieczka z sandboxa systemu plików przez końcowe slashe.
- **GHSA-x84v-gj2h-g759** — kontrolowana przez gościa alokacja sterty hosta przez strumienie WASIp3.

**Model capability jest konkretny:** `wasmtime --dir . foo.wasm` tworzy preopen; dokumentacja podkreśla, że **pomyłka w kolejności flag powoduje ciche przekazanie `--dir .` do gościa**. Brak ambient authority jest właściwością konstrukcyjną, nie konfiguracyjną.

> ⚠️ **ZWERYFIKOWANY NEGATYW: oficjalny MCP Registry NIE MA typu pakietu WASM/WASI.** Obsługiwane wartości `registryType` to dokładnie: **`npm`, `pypi`, `nuget`, `cargo`, `oci`, `mcpb`** — [MCP Registry Supported Package Types](https://modelcontextprotocol.io/registry/package-types.md). **Nie istnieje też żaden SEP związany z WASM.**

**Prace WASM nad MCP są wyłącznie badawcze:** **MCP-SandboxScan / SandScope** ([arXiv:2601.01241](https://arxiv.org/abs/2601.01241), v1 2026-01-03, v2 2026-06-22) uruchamia narzędzia MCP pod WASI jako **harness audytujący** — korpus 100 repozytoriów, **1 127 narzędzi w 71 repo**, **886 z deklarowanymi capability wrażliwymi na bezpieczeństwo**.

**Wniosek dla Vestige:** WASM/WASI jest technicznie gotowy i daje najmocniejszą izolację capability, ale **nie ma żadnej ścieżki publikacji w oficjalnym rejestrze MCP** i nie ma wsparcia w SDK. To opcja badawcza / dla bardzo wrażliwych narzędzi, nie domyślna ścieżka produkcyjna.

### 4.5 macOS — Seatbelt, `sandbox-exec` i Claude Code

**Status `sandbox-exec`:** udokumentowany jako **DEPRECATED** w **man page z 2017-03-09** („Developers who wish to sandbox an app should instead adopt the App Sandbox feature"). ⚠️ **Ale NIE został usunięty** — Anthropic nadal na nim opiera sandboxing w **v0.0.77 (2026-09-18)**.
> ⚠️ **NIEPOTWIERDZONE:** nie znalazłem żadnego oświadczenia Apple usuwającego lub wyłączającego `sandbox-exec` w macOS 15 ani 26. Nie powtarzaj powszechnego twierdzenia, że „Apple to usunął". Również NIEPOTWIERDZONE: atrybuty deprecacji `sandbox_init(3)`, entitlement Endpoint Security, kanoniczne ścieżki profili SBPL.

**Mechanizm sandboxingu Claude Code** (najlepszy publiczny wzorzec dla lokalnego serwera MCP):

| Platforma | Mechanizm |
|---|---|
| **macOS** | **Seatbelt** (`sandbox-exec`) |
| **Linux / WSL2** | **bubblewrap + socat + opcjonalny filtr seccomp** (`@anthropic-ai/sandbox-runtime`); blokuje `socket(AF_UNIX)` przez `prctl` oraz `io_uring_*` |
| **Windows natywnie** | ❌ niewspierane |

**Klucze konfiguracji (z bramkami wersji v2.1.186 → v2.1.271):**
- `sandbox.enabled`
- `sandbox.filesystem.{allowWrite, denyWrite, denyRead, allowRead, disabled}`
- `sandbox.credentials.{files, envVars}` z trybami `deny` / `mask` (+ `extract`, `decode: "jwt"`, `maskClaims`, `awsPairs`, `sigv4`)
- `network.{allowedDomains, deniedDomains, strictAllowlist, allowManagedDomainsOnly, tlsTerminate}`
- `excludedCommands`, `allowUnsandboxedCommands`, `failIfUnavailable`
- `enableWeakerNestedSandbox`, `enableWeakerNetworkIsolation`, `allowAppleEvents`

**Udokumentowane problemy na macOS (bardzo praktyczne):**
- **Apple Events zablokowane** (błąd **-600**).
- **CLI napisane w Go oblewają weryfikację TLS pod Seatbelt** — dotyczy `gh`, `gcloud`, `terraform`.
- `docker` — niekompatybilny w sandboxie.
- `jest` — wymaga `--no-watchman`.

**Zmierzona korzyść:** „sandboxing safely reduces permission prompts by **84%**".
> ⚠️ **To liczba dotyczaca REDUKCJI PROMPTÓW, a NIE odporności na injection.** Nie cytuj jej jako dowodu, że sandboxing zmniejsza skuteczność tool poisoning.

**`srt` ma explicytny przepis na MCP:**
```json
{"command": "srt", "args": ["npx", "-y", "@modelcontextprotocol/server-filesystem"]}
```
plus `~/.srt-settings.json`.

> ⚠️ **NIEPOTWIERDZONE: nie znalazłem żadnej dokumentacji sandboxingu procesów MCP w Claude Desktop.**

### 4.6 Linux — Landlock, bubblewrap, seccomp, `no-new-privileges`

**[Landlock](https://docs.kernel.org/security/landlock.html)** (dokumentacja jądra z **sierpnia 2026**) — mapa ABI:

| ABI | Funkcja |
|---|---|
| v4 | ograniczanie sieci TCP |
| v5 | `IOCTL_DEV` |
| v6 | `SCOPE_*` |
| v9 | `FS_RESOLVE_UNIX` |
| v10 | ograniczanie sieci UDP |
| v11 | `LANDLOCK_RESTRICT_SELF_NO_NEW_PRIVS` |

⚠️ **Krytyczne zastrzeżenie z dokumentacji jądra (cytat dosłowny):** gdy `no_new_privs` nie jest ustawione, binaria setuid/setgid/z file capabilities „would then run with elevated privileges while being restricted by a Landlock domain they may not expect, making them potential **confused deputies**."

**Udokumentowane luki Landlock — czego NIE da się ograniczyć:** `chdir`, `stat`, `flock`, `chmod`, `chown`, `setxattr`, `utime`, `fcntl`, `access`.

**bubblewrap 0.12.0 (2026-08-26)** — ⚠️ **USUNIĘTO wsparcie setuid w całości**; łamiące dla wszystkich na starej ścieżce deploymentu setuid. Naprawiono też **GHSA-pxhw-h44j-8pfx** (setup sandboxa podążał za symlinkami rodzica poza sandbox; naprawione przez `openat2` + `RESOLVE_IN_ROOT`). Wcześniej v0.11.2 (2026-04-23) naprawiło **CVE-2026-41163**.

**Domyślny profil seccomp Dockerа** wyłącza **~44 z ponad 300 wywołań systemowych**, `defaultAction: SCMP_ACT_ERRNO`, z per-syscall `SCMP_ACT_ALLOW`. Istotne pozycje: `socket` zablokowany dla **`AF_ALG`** (CVE-2026-31431) i **`AF_VSOCK`**; `io_uring_setup/enter/register` zablokowane ([moby#46762](https://github.com/moby/moby/issues/46762)).

**`no_new_privs`:** od Linuksa 3.5, `prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)`; jest dziedziczny i **nie da się go wyłączyć**.
⚠️ **Ograniczenie (cytat dosłowny):** „does not prevent privilege changes that do not involve `execve()`".

**⚠️ BARDZO WAŻNE — SDK MCP NIE ROBIĄ SANDBOXINGU (zweryfikowane ze źródeł):**

| SDK | Co robi | Czego NIE robi |
|---|---|---|
| **TypeScript** | `cross-spawn` z `shell: false`; env = `getDefaultEnvironment()` (allowlist POSIX: `HOME`, `LOGNAME`, `PATH`, `SHELL`, `TERM`, `USER`) scalone z env serwera; pomija wartości `()`; bufor max **10 MB** | ❌ **żadnego confinementu** |
| **Python v2** | identyczna allowlist env; `start_new_session=True`; SIGTERM → 2 s → SIGKILL na drzewie procesów; Job Objects na Windows | ❌ **żadnego confinementu** |

**Oba implementują higienę env i teardown; żaden nie implementuje izolacji.** Czyli **sandboxing lokalnego serwera MCP jest w całości odpowiedzialnością klienta/hosta lub operatora** — nie ma go w SDK.

### 4.7 Tool poisoning — liczby i uczciwa ocena luki

**MCPTox** ([arXiv:2508.14925](https://arxiv.org/abs/2508.14925), sierpień 2025), w omówieniu CSA (**2026-07-01**):
- **45 żywych serwerów MCP**, **353 narzędzia adwersarialne**, **20 LLM-ów**.
- **Średni ASR (attack success rate): 36,5%**
- **Maksimum: 72,8% (o1-mini)**
- Claude 3.7 Sonnet: **~34%**
- ⚠️ **Bardziej zdolne modele były BARDZIEJ podatne.**
- ⚠️ **Zastrzeżenie metodologiczne:** te liczby docierają przez omówienie CSA, **nie przez treść artykułu**. To najważniejszy pozostały do weryfikacji element.

**Automatyczne wykonanie (auto-execution) — CVE:**
- **CVE-2025-54135** „CurXecute", CVSS **8.6**, Cursor < 1.3
- **CVE-2025-54136** „MCPoison"
- **CVE-2026-12957 / CVE-2026-12958** — Amazon Q, **2026-06-26**
- Adversa AI **„TrustFall"** (2026-05-07) — obejmuje Claude Code / Cursor CLI / Gemini CLI / Copilot CLI
- **Robak „Miasma"**, czerwiec 2026 — **73 repozytoria GitHub**, w tym Microsoft `azure/durabletask`

> ### ⚠️ NAJUCZCIWSZE I NAJWAŻNIEJSZE USTALENIE W CAŁEJ SEKCJI 4
>
> **Sandboxing jest powszechnie zalecany, ale NIE ISTNIEJE żadne opublikowane, kontrolowane pomiaru jego wpływu na skuteczność tool poisoning (ASR).**
>
> Cytowana liczba **84%** to **redukcja promptów o uprawnienia**, nie odporność na injection. **Każdy, kto twierdzi, że sandboxing obniża liczbę 36,5%, miesza dwa różne pomiary.**
>
> Traktuj sandboxing jako **obronę w głąb opartą na rozsądku i ograniczaniu blast radius**, a nie jako środek o zmierzonym efekcie na prompt injection.

### 4.8 Zalecana postawa dla Vestige (local-first, dwie trasy)

⚠️ **To moja synteza na podstawie powyższych źródeł, nie cytat.**

| Warstwa | Działanie | Uzasadnienie źródłowe |
|---|---|---|
| **1. Minimalizacja uprawnień procesu** | Uruchamiaj na koncie użytkownika, bez `sudo`, bez setuid. Nigdy nie instaluj binarki setuid. | Landlock: setuid + brak `no_new_privs` = confused deputy |
| **2. macOS** | Dostarczaj profil Seatbelt (SBPL) dla trybu „restricted"; świadomie zaakceptuj, że `sandbox-exec` jest deprecated od 2017, ale działa. Ostrzeż użytkownika o blokadzie Apple Events (-600) i o problemach Go CLI z TLS. | Claude Code v0.0.77 (2026-09-18) nadal na tym jedzie |
| **3. Linux** | Dostarczaj profil `bubblewrap` (bez setuid — 0.12.0 to usunęło) + `landlock` dla ograniczenia FS i sieci. **Ustaw `no_new_privs`.** | bwrap 0.12.0 (2026-08-26); Landlock ABI v11 |
| **4. Kontener** | Obraz OCI z `--security-opt no-new-privileges`, `--read-only`, limitami `--cpus` / `--memory`, sieć wyłączona domyślnie. **Nie licz na gVisor, chyba że sam go skonfigurujesz.** | Docker MCP Gateway: `--cpus` 1, `--memory` 2 GB, `no-new-privileges` |
| **5. Sieć** | Domyślnie **brak egressu**. Allowlist per-host, jeśli embeddings/reranker muszą coś pobierać. | Docker threat model: egress niezablokowany domyślnie; Wasmtime 48: sockets zablokowane domyślnie |
| **6. Sekrety** | Trzymaj w macOS Keychain (albo env z pliku `0600`). **Nie wysyłaj ich do modelu.** Świadomość: Docker MCP Gateway **wstrzykuje plaintext do env kontenera** — jeśli tego nie chcesz, użyj modelu Docker Sandboxes (nagłówki HTTP, wartość nie wchodzi do VM) albo własnego proxy. | docker/mcp-gateway `security.md`; Docker Sandboxes |
| **7. Transport** | **Domyślnie stdio.** HTTP tylko gdy naprawdę potrzebny, i wtedy **tylko unix domain socket albo loopback z walidacją `Origin` + token bearer.** | Security Best Practices: „Use the `stdio` transport to limit access to just the MCP client"; „Use unix domain sockets…" |
| **8. Świadomość luki** | Nie ogłaszaj, że sandboxing chroni przed prompt injection. Nie ma na to pomiaru. | Brak opublikowanego kontrolowanego badania |

---

## 5. Projektowanie narzędzi przy dużej liczbie narzędzi (i ~28 to już „dużo")

### 5.1 Ile kosztuje definicja narzędzia — konkretne liczby

To najlepiej udokumentowany ilościowo obszar całego MCP. Wszystkie liczby pochodzą z pierwotnych źródeł Anthropic i GitHub.

**Anthropic, [„Introducing advanced tool use"](https://www.anthropic.com/engineering/advanced-tool-use) (2025-11-24):**

| Zestaw | Liczba narzędzi | Koszt w tokenach |
|---|---|---|
| GitHub | 35 | **~26 000** |
| Slack | 11 | **~21 000** |
| Sentry | 5 | **~3 000** |
| Grafana | 5 | **~3 000** |
| Splunk | 2 | **~2 000** |
| **Razem (5 serwerów)** | **58** | **~55 000** |
| Jira (osobno) | — | **~17 000** |
| Wewnętrznie w Anthropic (przed optymalizacją) | — | **134 000** |

> „That's 58 tools consuming approximately 55K tokens before the conversation even starts. Add more servers like Jira (which alone uses ~17K tokens) and you're quickly approaching 100K+ token overhead."

**Uwaga o rozkładzie:** koszt **nie** rozkłada się równomiernie. Slack: 11 narzędzi ≈ 21K tokenów ≈ **~1 900 tokenów/narzędzie**. GitHub: 35 narzędzi ≈ 26K ≈ **~740 tokenów/narzędzie**. Sentry/Grafana: ~600 tokenów/narzędzie. **Rozrzut ~600–1 900 tokenów na narzędzie** zależy od bogactwa `inputSchema` i długości `description`.

**Przeliczenie dla Vestige (~28 narzędzi):** przy 600–1 900 tokenów/narzędzie daje to **~17 000 – 53 000 tokenów** samych definicji. To dokładnie ten przedział, w którym Anthropic zaleca włączenie tool search („**Use it when:** Tool definitions consuming **>10K tokens**").

**Anthropic, [Tool search tool (docs)](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool) (2026):**
- Typowy multiserverowy setup (GitHub, Slack, Sentry, Grafana, Splunk) = **~55k tokenów** w definicjach przed rozpoczęciem pracy.
- „Tool search typically reduces this by **over 85 percent**, loading only the **3–5 tools** Claude needs for a given request."
- **Kluczowa liczba jakościowa:** „Claude's ability to pick the right tool **degrades once you exceed 30–50 available tools**." — Vestige z 28 narzędziami jest **na granicy** tego progu, a po dodaniu drugiego serwera MCP w tym samym hoście przekroczy go.
- Anthropic rekomenduje: „Keep your **three to five most-used tools always loaded**, defer the rest."

**Anthropic, [„Code execution with MCP"](https://www.anthropic.com/engineering/code-execution-with-mcp) (2025-11-04):**
- Redukcja z **150 000 tokenów do 2 000 tokenów** — oszczędność **98,7%** — w scenariuszu Google Drive → Salesforce.
- Kontekst: 2-godzinny transkrypt spotkania to **~50 000 tokenów** przepływających przez kontekst **dwukrotnie** (raz jako wynik `getDocument`, raz jako argument `updateRecord`).
- Podejście: narzędzia prezentowane jako **drzewo plików** (`./servers/google-drive/getDocument.ts`), agent eksploruje filesystem i ładuje tylko potrzebne definicje.

**Anthropic, [advanced tool use](https://www.anthropic.com/engineering/advanced-tool-use) — Programmatic Tool Calling:**
- Zużycie tokenów spadło z **43 588 → 27 297** (**−37%**) na złożonych zadaniach badawczych.
- Trafność wewnętrznego retrievalu: **25,6% → 28,5%**; benchmark GIA: **46,5% → 51,2%**.
- Dane wejściowe: **200 KB → 1 KB** wyniku (2000+ pozycji wydatków → 2–3 rekordy).
- Tool Search Tool — trafność na ewaluacjach MCP: **Opus 4: 49% → 74%**; **Opus 4.5: 79,5% → 88,1%**.
- Tool Use Examples — trafność obsługi złożonych parametrów: **72% → 90%**.

**GitHub, [„Improving token efficiency in GitHub Agentic Workflows"](https://github.blog/ai-and-ml/github-copilot/improving-token-efficiency-in-github-agentic-workflows/):**
- GitHub MCP server z **40 narzędziami** dodawał **~10–15 KB schematu na turę**.
- Usunięcie nieużywanych narzędzi ucięło kontekst per-wywołanie o **8–12 KB**, oszczędzając tysiące tokenów na przebieg **bez zmiany zachowania**.
- Skrajny przypadek: w workflow, który tylko skanował lokalne zmiany plików, narzędzie `search_repositories` zostało wywołane **342 razy w jednym przebiegu** = **58% wszystkich wywołań narzędzi**, mimo że nie miało żadnej roli w zadaniu. Zostało usunięte przez optymalizator.
- ⚠️ Cytuję tę liczbę z [AAIF, „MCP in production"](https://aaif.io/blog/mcp-in-production-what-changes-after-the-demo-works) (2026-07-31), które podaje je jako „the clearest public numbers"; sam artykuł GitHub bloga nie oddał mi treści przy pobraniu (truncated), więc ⚠️ **liczby 10–15 KB / 8–12 KB / 342× / 58% pochodzą z wtórnego omówienia AAIF, nie z bezpośredniej weryfikacji w źródle GitHub.**

### 5.2 Co mówi specyfikacja `2026-07-28` o dużej liczbie narzędzi

1. **Deterministyczna kolejność `tools/list` jest obowiązkowa (SHOULD)** — i ma konkretny cel:
   > „Servers **SHOULD** return tools in a deterministic order … Deterministic ordering enables clients to reliably cache the tool list and **improves LLM prompt cache hit rates** when tools are included in model context."
   > — [changelog, Minor change #3](https://modelcontextprotocol.io/specification/2026-07-28/changelog.md) i [Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)

   **To jest darmowa optymalizacja:** posortuj katalog narzędzi stabilnie (np. po nazwie) i **nigdy nie zmieniaj kolejności** między żądaniami, gdy zbiór się nie zmienił.

2. **`tools/list` może zależeć od autoryzacji:**
   > „The set … **MUST NOT** vary per-connection or as a side effect of other requests on the connection. The set **MAY** vary by the authorization presented on the request — for example, returning only the tools the caller's granted scopes permit — since credentials are per-request input, not connection state."
   > — [Tools → Capabilities](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)
   
   **Uwaga:** to nie to samo co „filtrowanie per sesja" — filtrowanie per sesja jest teraz **zabronione**. Filtrowanie po scope z tokenu jest dozwolone i jest jedynym sankcjonowanym mechanizmem redukcji katalogu na poziomie protokołu. ⚠️ **Ale jeśli filtrujesz katalog po uprawnieniach, musisz ustawić `cacheScope: "private"`** — inaczej `"public"` pozwoli współdzielić cache między różnymi kontekstami autoryzacji. Spec: „different access tokens can leverage the same cache" — [Caching → Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/caching.md).

3. **Paginated `tools/list`** — serwer decyduje o rozmiarze strony ([Pagination](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/pagination.md)). Dla 28 narzędzi paginacja jest zbędna; dla setek — konieczna.

4. **`notifications/tools/list_changed`** istnieje nadal, ale **tylko przez `subscriptions/listen`** z filtrem `toolsListChanged: true`. Serwer **MUST NOT** wysyłać typów notyfikacji, o które klient nie poprosił. — [Subscriptions](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions.md)

5. **`ttlMs` + `cacheScope` obowiązkowe na `tools/list`** — pozwalają klientowi cache'ować katalog i nie odpytować przy każdej turze. [Caching](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/caching.md): „`ttlMs` is a hint … analogous to HTTP `Cache-Control: max-age`"; brak `ttlMs` → klient **SHOULD** przyjąć `0` (natychmiast przestarzałe). **Dla stabilnego katalogu 28 narzędzi ustaw wysokie `ttlMs`** (np. 300000 = 5 min, jak w przykładach spec) i `cacheScope: "public"` jeśli katalog nie zależy od uprawnień.

6. **Nazwy narzędzi** ([SEP-986](https://modelcontextprotocol.io/seps/986-specify-format-for-tool-names.md), [Tools → Tool Names](https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md)):
   - długość **SHOULD** 1–128 znaków,
   - **SHOULD** być case-sensitive,
   - dozwolone znaki **SHOULD**: `A-Z a-z 0-9 _ - .`
   - **SHOULD NOT** zawierać spacji, przecinków ani innych znaków specjalnych,
   - **SHOULD** być unikalne w obrębie serwera.
   - **Kolizje przy agregacji:** „Tool name uniqueness is scoped to a single server. Clients or proxies that aggregate tools from multiple servers **MAY** encounter naming collisions (for example, two servers each exposing a `search` tool) and **SHOULD** implement a disambiguation strategy such as **prefixing tool names with a server identifier**." ⚠️ „The server `name` (from `serverInfo`) is **not guaranteed to be unique** across servers and **SHOULD NOT** be relied upon for disambiguation."
   - **Kropka jest dozwolona** w nazwach (`admin.tools.list` to przykład ze spec) — co czyni ją naturalnym separatorem przestrzeni nazw. Vestige używa płaskich nazw (`smart_ingest`, `session_context`, …) — to jest zgodne, ale przy agregacji wielu serwerów host doda prefiks.

### 5.3 On-demand tool discovery — co jest w specyfikacji, a co nie

| Mechanizm | Status na 2026-09-19 |
|---|---|
| `tools/list` z filtrowaniem po autoryzacji | ✅ **w spec `2026-07-28`** (may vary by authorization) |
| `notifications/tools/list_changed` + `subscriptions/listen` | ✅ **w spec `2026-07-28`** |
| `ttlMs` / `cacheScope` na `tools/list` | ✅ **w spec `2026-07-28`** |
| Paginacja `tools/list` | ✅ **w spec** |
| **Dynamiczne wyszukiwanie narzędzi (`tools/search`, deferred loading)** | ❌ **NIE MA w spec `2026-07-28`** |
| **„Progressive discovery"** | 🟡 **zapowiedziane w roadmapie**, nie zaimplementowane |

**Dowody na brak mechanizmu w spec:**
- **[SEP-1821: Dynamic Tool Discovery](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1821)** — issue w repo specyfikacji. ⚠️ Nie udało mi się odczytać statusu (GitHub zwrócił tylko szkielet strony przy pobraniu). Jest to **issue/SEP w toku**, nie przyjęta funkcja.
- **[SEP-1300: Tool Filtering with Groups and Tags](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1300)** — propozycja filtrowania po grupach/tagach. Również **issue**, nie funkcja.
- **Roadmap (2026-08-22)** wymienia to wprost jako pracę **przyszłą**:
  > „**Progressive discovery**: Core Primitives WG. Clients learn a server's tools and resources as they need them instead of ingesting the full catalog up front, with a defined interaction with the caching work…"
  > — [Roadmap → 4. Improved Primitives](https://modelcontextprotocol.io/development/roadmap.md)
- Istnieje też **Interest Group „Primitive Grouping"** — [charter](https://modelcontextprotocol.io/community/interest-groups/primitive-grouping.md).
- Roadmap zapowiada również **„Tool result shape"** — redesign `tools/call`, bo obecnie pozwala zwracać jednocześnie `content` i `structuredContent`, „which has confused server and client authors alike and produced diverging implementations".

**Wniosek dla Vestige:** **nie ma dziś standardowego mechanizmu on-demand discovery w MCP.** Redukcję kontekstu osiąga się dziś na trzy sposoby, wszystkie po stronie klienta/hosta:
1. **Filtrowanie po scope** (protokół, sankcjonowane) — np. osobne scopes dla `search`/`memory` (read) vs `dream`/`gc`/`backup` (admin).
2. **Tool search po stronie hosta** (Anthropic: `defer_loading: true` + `tool_search_tool_regex_20251119` / `tool_search_tool_bm25_20251119`; beta header `advanced-tool-use-2025-11-20`).
3. **Code execution z MCP** (Anthropic: narzędzia jako drzewo plików; Cloudflare: „Code Mode", https://blog.cloudflare.com/code-mode/).

⚠️ **Uwaga o własnej implementacji `search_tools`:** Anthropic opisuje to jako wzorzec („Alternatively, a `search_tools` tool can be added to the server to find relevant definitions… Including a **detail level parameter** … such as name only, name and description, or the full definition with schemas also helps the agent conserve context"). **To jest legalne i wdrażalne w serwerze MCP dzisiaj** — jako zwykłe narzędzie, którego wynikiem jest lista definicji. Ale to nie jest mechanizm protokołu i wymaga, żeby host/model faktycznie wywołał to narzędzie zamiast polegać na `tools/list`.

### 5.4 Agregacja przez gateway

- **MCP-AX** (`draft-abbott-mcp-ax-00`, 2026-05-04, autor indywidualny Ira Abbott, SoftOboros) — [IETF Datatracker](https://datatracker.ietf.org/doc/draft-abbott-mcp-ax/). ⚠️ **To NIE jest standard** — „This I-D is **not endorsed by the IETF** and has **no formal standing**". Dokument sam przyznaje: „MCP-AX treats MCP as a stable external specification". Wartościowe jako ** źródło pomysłów**, nie jako norma.
  - Przestrzeń nazw: `<root-segment>.<aggregator-segment>.<local-name>`, segmenty `[a-z0-9_-]{1,63}`, pełna nazwa ≤ **255 znaków**.
  - **Rate limiting w agregatorze:** domyślnie **100 notyfikacji/s per subserwer**; przy przekroczeniu bufor do **1000**, potem porzucanie najstarszych, licznik `notifications_dropped` i syntetyczna notyfikacja `notification_overflow`. To jedyna znaleziona przeze mnie **konkretna liczba dla rate limitingu notyfikacji** — ale z draftu bez statusu.
  - Bezpieczeństwo: uwierzytelnianie **per hop**, poświadczenia **MUST NOT** być przekazywane między hopami, tokeny scope'owane per hop wg RFC 8707.
  - Bramki nieodwracalności: narzędzia `reversible: false` + `mutable: true` **MUST** być oflagowane; agregator w trybie „gated" przechwytuje `tools/call` i wymaga `mcpax/confirm` z **kryptograficznym dowodem** od podmiotu zewnętrznego; niepotwierdzone wygasają po **300 s** (domyślnie). Uzasadnienie: „Without this requirement, an autonomous model client can trivially self-confirm."
  - Monotoniczność bramek bezpieczeństwa: raz nałożona bramka **nie może** być zdjęta ani zdegradowana przez żaden hop wyżej. „Any aggregator MAY escalate an ungated request to gated; none MAY de-escalate."
- **agentgateway (Solo.io)** — komercyjne/OSS gateway z natywnym wsparciem MCP: tryby [static MCP](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/static-mcp/), [dynamic MCP](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/dynamic-mcp/), [tool modes](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/tool-mode/), [virtual MCP](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/virtual/), [guardrails](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/guardrails/), [control access to tools](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/tool-access/), [MCP spec compatibility matrix](https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/spec-compatibility/).
- **Docker MCP Gateway** — patrz sekcja 4.

---

## 6. Obserwowalność

### 6.1 Gdzie żyją konwencje semantyczne — uwaga, przeniesione

**Najważniejszy fakt strukturalny:** konwencje semantyczne OTel dla **GenAI** i dla **MCP** zostały **przeniesione** z repozytorium `open-telemetry/semantic-conventions` do nowego repo **`open-telemetry/semantic-conventions-genai`**, począwszy od semconv **v1.42.0 (2026-06-16)**. Większość zaindeksowanych stron — w tym `opentelemetry.io` — wciąż wskazuje starą lokalizację.

| Zasób | URL | Status |
|---|---|---|
| **Kanoniczne źródło MCP semconv** | https://raw.githubusercontent.com/open-telemetry/semantic-conventions-genai/main/docs/gen-ai/mcp.md | **Development** (eksperymentalne) |
| Stary rejestr atrybutów MCP | https://opentelemetry.io/docs/specs/semconv/registry/attributes/mcp/ | oznaczone **Deprecated / „Moved"** |
| Strona GenAI | https://opentelemetry.io/docs/specs/semconv/gen-ai/ | renderuje się jako **„Moved"** |
| Nowe repo | https://github.com/open-telemetry/semantic-conventions-genai | utworzone 2026-05-05 |

⚠️ **Konflikt do odnotowania:** README nowego repo ma `## Schema URL` → **`TODO`**, a CHANGELOG zawiera tylko puste `## Unreleased`. **Nie istnieje dziś żadna wersja semconv, którą można przypiąć dla `mcp.*`.** To realny problem dla CI i dla stabilności dashboardów.

**Historia wersji semconv** (daty zweryfikowane przez GitHub releases atom): v1.39.0 = **2026-01-14** (tu po raz pierwszy weszły konwencje MCP; CHANGELOG: „Add MCP semantic conventions", issues #2043/#2083), v1.40.0 = 2026-02-19, v1.41.0 = 2026-04-28, v1.42.0 = 2026-06-16 (przeniesienie), v1.43.0 = 2026-07-03, v1.44.0 = 2026-08-04 (bieżąca).

**Status `gen_ai.*`: `Development`** — nie stable, nie RC. Ze referencjonowanych atrybutów rdzeniowych stable są tylko `error.type`, `server.*`, `network.*`, `client.*`; `rpc.response.status_code` = **Release Candidate**.

**Rekomendacja ze semconv:** przy instrumentowaniu MCP **RECOMMENDED** jest stosowanie konwencji MCP, a **nie** konwencji RPC — bo „MCP spans and metrics provide domain-specific context and record details that are not covered by the RPC conventions such as message exchanges within streaming calls". Konwencje HTTP też nie wystarczają, bo „multiple MCP requests could be sent over a single HTTP request".

### 6.2 Cztery oficjalne metryki MCP

Wszystkie to **Histogram**, jednostka **`s`**, status **Development**, poziom **Recommended**.

| Metryka | Opis |
|---|---|
| `mcp.client.operation.duration` | Czas trwania żądania/notyfikacji MCP obserwowany po stronie **nadawcy**, od wysłania do otrzymania odpowiedzi/ack |
| `mcp.server.operation.duration` | To samo po stronie serwera |
| `mcp.client.session.duration` | Czas trwania sesji MCP po stronie klienta |
| `mcp.server.session.duration` | Czas trwania sesji MCP po stronie serwera |

**Zalecane kubełki (`ExplicitBucketBoundaries`)** — identyczne dla wszystkich czterech:
```
[0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 30, 60, 120, 300]
```

**Atrybuty na `mcp.{client,server}.operation.duration`:**

| Atrybut | Poziom | Wartości / uwagi |
|---|---|---|
| `mcp.method.name` | **Required** | np. `tools/call`, `tools/list`, `resources/read`, `prompts/get`, `initialize` |
| `error.type` | Cond. Required (jeśli operacja się nie powiodła) | Przy błędzie JSON-RPC — stringowa reprezentacja kodu. **Gdy `CallToolResult.isError == true`, `error.type` SHOULD być ustawione na `tool_error`** |
| `gen_ai.prompt.name` | Cond. Required | gdy operacja dotyczy promptu |
| `gen_ai.tool.name` | Cond. Required | gdy operacja dotyczy narzędzia |
| `rpc.response.status_code` | Cond. Required | kod błędu JSON-RPC |
| `gen_ai.operation.name` | Recommended | **`execute_tool`** przy wywołaniu narzędzia; **SHOULD NOT** być ustawiane w innych przypadkach |
| `mcp.protocol.version` | Recommended | np. `2026-07-28` |
| `jsonrpc.protocol.version` | Recommended (gdy ≠ `2.0`) | |
| `network.transport` | Recommended | **`pipe`** dla stdio; **`tcp`** lub **`quic`** dla HTTP |
| `network.protocol.name` / `.version` | Recommended | `http` / `2` |
| `server.address` / `server.port` | Recommended | za pośrednikami — adres serwera docelowego |
| `gen_ai.tool.call.arguments` | **Opt-In** | ⚠️ „may contain sensitive information" |
| `gen_ai.tool.call.result` | **Opt-In** | ⚠️ „may contain sensitive information" |
| `gen_ai.prompt.variable.<name>` | **Opt-In** | ⚠️ „may contain sensitive information" |

**Ważne negatywy (nie wymyślaj tych metryk):**
- **NIE istnieje metryka o nazwie „tool call duration".** Wyprowadza się ją filtrując `mcp.method.name="tools/call"`.
- **NIE ma liczników `mcp.*` typu request-count ani error-rate.** Liczność bierzesz z `_count` histogramu, a błędy z `error.type`.
- **Zużycie tokenów NIE jest w `mcp.*`** — jest w `gen_ai.usage.*` (`input_tokens`, `output_tokens`, per-modality, `reasoning.output_tokens`) na spanach inferencji GenAI, czyli **po stronie klienta/agenta, nie serwera MCP**.

**Mapowanie transportu (tabela ze semconv):**

| Transport MCP | `network.transport` | `network.protocol.*` | `mcp.protocol.version` |
|---|---|---|---|
| stdio | `pipe` | — | any |
| Streamable HTTP | `tcp` (lub `quic`) | `http` / `2` | `2025-06-18`+ |
| HTTP z SSE (legacy) | `tcp` (lub `quic`) | `http` / `1.1` (lub `2`) | `2024-11-05` lub starsze |
| Custom: websockets | `tcp` | `websocket` | any |

⚠️ **`mcp.session.id` istnieje w semconv, ale w `2026-07-28` nie ma sesji.** Semconv nadal definiuje `mcp.session.id` jako Recommended „When the MCP request or notification is part of a session" i linkuje do `2025-06-18`. To **niespójność między semconv a specyfikacją `2026-07-28`** — semconv nie został jeszcze zaktualizowany do modelu bezstanowego. Przy stdio możesz nadal używać `mcp.session.id` jako korelatora procesu; przy HTTP `2026-07-28` **nie ma czego wpisać**. ⚠️ Nie znalazłem opublikowanej decyzji OTel w tej sprawie.

### 6.3 Spany

**Konwencja nazewnictwa spanów:**
- `{mcp.method.name} {target}`, gdzie `target` **SHOULD** odpowiadać `gen_ai.tool.name` lub `gen_ai.prompt.name`, gdy dotyczy.
- Bez targetu o niskiej kardynalności → samo `{mcp.method.name}`.
- **Instrumentacja MOŻE pozwolić użytkownikowi włączyć `mcp.resource.uri` jako `target`, ale SHOULD NOT domyślnie** — „to avoid high cardinality span names". **To konkretna pułapka: URI zasobów w nazwach spanów rozwala kardynalność.**
- `mcp.client` span: kind **`CLIENT`**; `mcp.server` span: kind **`SERVER`**.
- **Kompatybilność z GenAI:** „MCP tool call execution spans are compatible with GenAI `execute_tool` spans. If the MCP instrumentation can reliably detect that outer GenAI instrumentation is already tracing the tool execution, it **SHOULD NOT create a separate span**. Instead, it SHOULD add MCP-specific attributes to the existing tool execution span." — czyli **nie duplikuj spanów**, jeśli host już je tworzy.
- Jeśli span status = `ERROR`, opis statusu **SHOULD** odpowiadać `JSONRPCError.message`.

### 6.4 Propagacja kontekstu (SEP-414)

[SEP-414](https://modelcontextprotocol.io/seps/414-request-meta.md) — **Final**, Standards Track, utworzony **2025-04-25**, autor Adrian Cole (@codefromthecrypt), sponsor Marcelo Trylesinski (@Kludex).

Co ustala:
1. Gdy kontekst trace jest propagowany przez `_meta`, klucze `traceparent`, `tracestate`, `baggage` używają formatów W3C Trace Context i W3C Baggage.
2. Przykład nie-normatywny.
3. Nota wyjaśniająca, że to **jawny wyjątek od reguły prefiksowania DNS** kluczy `_meta`.

> „[D]iffering interpretations could materialize, such as namespacing traceparent like `io.modelcontextprotocol.traceparent`, which will break traces and log correlation."

**Reguły instrumentacji (ze semconv, nie z SEP-a — SEP sam nie ustanawia MUST/MUST NOT):**
- Instrumentacje **SHOULD** propagować kontekst przez skonfigurowane propagatory OTel, **wstrzykując go do `params._meta`** przy tworzeniu żądania/notyfikacji.
- Odbiorca wyodrębnia kontekst z `params._meta` i używa go jako **remote parent**.
- „Although the MCP convention expects keys in `params._meta` to be DNS-prefixed, **the context propagation keys SHOULD be written unprefixed**."
- Instrumentacja serwera MCP **SHOULD** domyślnie używać kontekstu z `params._meta` jako rodzica dla spanu serwera i **SHOULD** linkować bieżący kontekst ambient, jeśli istnieje.

**Ważna nota o niezależności kontekstów:** „MCP and underlying transport (such as HTTP) contexts are **independent**. One MCP request can be served by multiple HTTP requests (for example, because of retries) and one streamable HTTP request can serve more than one MCP request/notification. The MCP client span becomes a parent of the MCP server span regardless of transport used; span links allow recording the transport context (if present)."

SEP-414 trafił do changelogu `2026-07-28` jako **Minor change #2**. Powiązane: SEP-1788 (klucze zastrzeżone w `_meta`), SEP-2028 (forwardowanie `_meta` do nagłówków HTTP). Równoważna zmiana w ACP: [agentclientprotocol#297](https://github.com/agentclientprotocol/agent-client-protocol/pull/297).

### 6.5 Deprecjacja Logging — dwie rzeczy, które łatwo pomylić

To jest **najczęściej mylony punkt** w całym `2026-07-28`:

| Co | Status | Szczegół |
|---|---|---|
| **`logging/setLevel` RPC** | **USUNIĘTE (removed)** | [SEP-2575](https://modelcontextprotocol.io/seps/2575-stateless-mcp.md): „`logging/setLevel`: Removed. The log level is now specified per-request via the `io.modelcontextprotocol/logLevel` `_meta` field. **There is no replacement RPC.**" |
| **Funkcja Logging** (`notifications/message` + capability `logging`) | **ZDEPRECJONOWANA, ale obecna** | [SEP-2577](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2577); usunięcie nie wcześniej niż w pierwszej rewizji wydanej **2027-07-28** lub później |

**Nowe reguły logowania (normatywne, [Logging](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/logging.md)):**
- Serwer **MUST NOT** emitować `notifications/message` dla żądania, które **nie** zawiera `io.modelcontextprotocol/logLevel` w `_meta`. Opt-in jest **per-żądanie**, nie per-sesja.
- `notifications/message` jest **request-scoped**: serwer **MUST NOT** dostarczać go na strumieniu `subscriptions/listen` ani żadnym innym niż strumień odpowiedzi na to konkretne żądanie.
- Nieznany poziom logu → `-32602` (Invalid params); błędy wewnętrzne → `-32603`.
- Poziomy: syslog severity wg RFC 5424 — `debug`, `info`, `notice`, `warning`, `error`, `critical`, `alert`, `emergency`.
- Serwery **SHOULD** rate-limitować wiadomości logów.
- Log messages **MUST NOT** zawierać: poświadczeń/sekretów, PII, ani szczegółów wewnętrznych systemu mogących pomóc w ataku.

**Ścieżka migracji (cytat z rejestru deprecjacji):**
> „Log to `stderr` for stdio transports; use [OpenTelemetry](https://opentelemetry.io/) for observability."
> — [Deprecated Features](https://modelcontextprotocol.io/specification/2026-07-28/deprecated.md)

**⚠️ Konflikt źródłowy:** proza w SEP-2577 mówi „expected June 2026" i sugeruje kroczące okno roczne per wersja; przyjęta polityka SEP-2596 to **jedno okno 12 miesięcy** liczone **od wydania rewizji, która oznaczyła funkcję jako Deprecated, nie od finalizacji SEP-a**. Polityka jest źródłem autorytatywnym. Polityka dopuszcza też przyspieszone usunięcie: **floor 90 dni** + zgoda Core Maintainerów + opublikowany security advisory.

### 6.6 Co logować na `stderr` (stdio) — i czego NIE ma

Z [Debugging](https://modelcontextprotocol.io/docs/2026-07-28/tools/debugging.md) i [stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio.md):
- Serwer **MAY** pisać UTF-8 do `stderr` w dowolnym celu (info, debug, error).
- Serwer **MUST NOT** pisać niczego do `stdout`, co nie jest poprawną wiadomością MCP.
- Klient **MAY** przechwytywać/przekazywać/ignorować `stderr` i **SHOULD NOT** zakładać, że `stderr` oznacza błąd.
- Hosty zwykle przechwytują `stderr` automatycznie.
- **Dla Streamable HTTP `stderr` NIE jest przechwytywany** → potrzebna agregacja po stronie serwera albo OTel.
- Oficjalna lista „ważnych zdarzeń do logowania": kroki startowe, dostęp do zasobów, wykonanie narzędzia, warunki błędów, metryki wydajności.

⚠️ **NIEPOTWIERDZONE:** **nie istnieje oficjalny schemat logów strukturalnych** (kanoniczne nazwy pól JSON) dla `stderr`. Wytyczne są wyłącznie prozą. Jeśli chcesz log strukturalny, wybierz konwencję sam (np. JSON z `ts`, `level`, `tool`, `duration_ms`, `error.type`) i udokumentuj ją.

### 6.7 Wsparcie OTel w SDK MCP — stan faktyczny

**Kluczowa odpowiedź: projekt MCP NIE publikuje żadnego pakietu instrumentacji.** Wsparcie jest albo natywne w SDK, albo第三方.

| SDK | Wersja | Wsparcie OTel | Szczegóły |
|---|---|---|---|
| **Python `mcp`** | **2.2.0** (PyPI, 2026-09-07) | ✅ **natywne, domyślnie WŁĄCZONE** | [Dokumentacja](https://py.sdk.modelcontextprotocol.io/run/opentelemetry/): „Every server you create emits an OpenTelemetry span for every message it handles… It is there the moment you call `MCPServer(...)`." Span serwera: `tools/call search_books`. Atrybuty: `mcp.method.name`, `mcp.protocol.version`, `jsonrpc.request.id`; dla narzędzi dodatkowo `gen_ai.operation.name="execute_tool"` i `gen_ai.tool.name`; dla `prompts/get` — `gen_ai.prompt.name`. Zależy tylko od `opentelemetry-api` → **bez SDK/exportera jest no-op**. Propagacja SEP-414 automatyczna. ⚠️ Wyłączenie wymaga usunięcia `mcp.server._otel.OpenTelemetryMiddleware` — **import z podkreśleniem = API prowizoryczne**, dokumentacja mówi wprost, że zmieni się. |
| **C# SDK** | — | ✅ **natywne** | [Diagnostics.cs](https://raw.githubusercontent.com/modelcontextprotocol/csharp-sdk/main/src/ModelContextProtocol.Core/Diagnostics.cs): `ActivitySource` **i** `Meter` o nazwie **`"Experimental.ModelContextProtocol"`**; histogramy w jednostce `"s"` z tymi samymi 14 kubełkami; ekstrakcja/wstrzykiwanie przez `DistributedContextPropagator`; pomija `notifications/message`; **ponownie używa zewnętrznego activity `execute_tool` zamiast zagnieżdżać**. |
| **TypeScript `@modelcontextprotocol/sdk`** | **1.30.0** (npm, 2026-07-27) | ❌ **brak oficjalnego OTel** | Zweryfikowany negatyw: grep `opentelemetry|otel|telemetr|tracing|instrument` w README (179 linii) = 0 trafień. ⚠️ Nie przeszukano wnętrza pakietu — oznaczone jako niepełne. |
| **Go / Java / Rust SDK** | — | ⚠️ **NIEZWERYFIKOWANE** | Nie znaleziono oficjalnej dokumentacji OTel. |

**⚠️ Uwaga o Rust — istotna dla Vestige:** Rust SDK wspiera `2026-07-28` w becie ([blog wydania](https://blog.modelcontextprotocol.io/posts/2026-07-28/)), ale **nie znalazłem żadnej oficjalnej dokumentacji OTel dla Rust SDK**. Jeśli używasz `rmcp`, instrumentację OTel prawdopodobnie musisz napisać sam przez crate `tracing-opentelemetry` + własne metryki `opentelemetry` — i samodzielnie zaimplementować wstrzykiwanie/ekstrakcję `traceparent` z `params._meta`.

**Instrumentacje сторонніе:**
- **`opentelemetry-instrumentation-mcp` 0.62.3 (2026-08-10)** — należy do **traceloop/openllmetry**, **NIE** do orgu open-telemetry. Użycie: `McpInstrumentor().instrument()`.
  > ⚠️ **ODWRÓCENIE DOMYŚLNEJ POLITYKI PRYWATNOŚCI:** README tego pakietu mówi: „By default, this instrumentation logs prompts, completions, and embeddings to span attributes" — wyłączenie przez `TRACELOOP_TRACE_CONTENT=false`. To jest **odwrotność** domyślnego zachowania semconv (patrz 6.8). **Jeśli go użyjesz, natychmiast ustaw `TRACELOOP_TRACE_CONTENT=false`** — inaczej treści pamięci użytkownika trafią do spanów.
- npm `@theharithsa/opentelemetry-instrumentation-mcp` 1.0.4 (2025-09-26, ~5★) — ⚠️ bardzo mała adopcja.
- npm `mcp-opentelemetry` 0.2.0 (2026-09-06, ~0★) — celuje w TS SDK v2 / `2026-07-28`. ⚠️ świeże, niezweryfikowane.

### 6.8 Przechwytywanie treści — domyślnie WYŁĄCZONE

Semconv, dosłownie:
> „OpenTelemetry instrumentations **SHOULD NOT capture them by default**, but SHOULD provide an option for users to opt in."

**Atrybuty Opt-In:** `gen_ai.system_instructions`, `gen_ai.input.messages`, `gen_ai.output.messages`, `gen_ai.tool.definitions`, `gen_ai.prompt.variable`, `gen_ai.tool.call.arguments`, `gen_ai.tool.call.result`, `gen_ai.memory.query.text`, `gen_ai.memory.records`.

**Trzy sankcjonowane wzorce:**
1. **[Domyślny]** Nie rejestruj.
2. Rejestruj na spanach.
3. **Przechowuj zewnętrznie i rejestruj referencje (zalecane dla produkcji).**

Hooks do zewnętrznego przechowywania **SHOULD** uruchamiać się **niezależnie od decyzji o samplowaniu i niezależnie od flag opt-in**, i **SHOULD** być wywoływane **przed serializacją JSON**.

**⚠️ Uwaga o precyzji:** `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT` pojawia się w semconv jako *„for example"* — **semconv NIE standaryzuje nazwy flagi**.

⚠️ **Rozbieżność vendorów:** Sentry używa `recordInputs`/`recordOutputs`, które domyślnie podążają za `dataCollection.genAI.*`, a w JS SDK 10.x **fallbackują do `sendDefaultPii`**. To inna semantyka niż semconv. Sekcja `### Streaming chunks` w semconv to dosłownie `TODO`.

### 6.9 Instrumentacja vendorów — co realnie istnieje

**Sentry** — najlepsze źródło pierwotne dla instrumentacji MCP: [docs.sentry.io/product/mcp-servers/getting-started](https://docs.sentry.io/product/mcp-servers/getting-started.md) + spec w `develop-docs/sdk/expected-features/mcp-instrumentation/tracing.mdx`.
- Wersje: Node SDK **9.46.0+**, JS SDK **10.70.0+** (dla MCP server 2.x), **10.33.0+** (dla `recordInputs`/`recordOutputs`), `@sentry/cloudflare` **10.49.0+**, Python `sentry-sdk` **2.43.0+**.
- API: `Sentry.wrapMcpServerWithSentry(...)` / `MCPIntegration()`.
- Span ops: `mcp.server`, `mcp.notification.client_to_server|server_to_client`. Nazwy: `tools/call {tool}`, `prompts/get {prompt}`, `resources/read {uri}`, `initialize`.
- Atrybuty: `mcp.method.name`, `network.transport`, **`mcp.transport`** (specyficzny dla Sentry), `mcp.request.id`, `mcp.session.id`, `mcp.tool.result.is_error`, `mcp.tool.result.content_count`, `mcp.tool.result.content`, `mcp.request.argument.<key>`, `mcp.resource.name`.
- ⚠️ **Sentry nie publikuje żadnych nazw metryk** (tylko trace'y), a kilka atrybutów **odbiega od semconv OTel** (`mcp.transport`, `mcp.request.id`, `mcp.tool.result.*` nie istnieją w oficjalnych konwencjach).

**Grafana + OpenLIT** — [blog Grafana](https://grafana.com/blog/ai-observability-MCP-servers/), opublikowany **2026-03-20**. Konkretna nazwa metryki: **`tool_invocation_duration_ms`** (specyficzna dla OpenLIT, **nie** semconv). Dashboard „AI Observability → MCP Observability": wydajność narzędzi, zdrowie protokołu, zużycie zasobów, śledzenie błędów. Instalacja: `pip install openlit mcp` + `openlit.init()`. ⚠️ Fragmenty kodu w blogu **nie zgadzają się** z realnym API MCP Python SDK — cytuj nazwę metryki, nie kod.

**Datadog** — ma przewodnik monitorowania **klienta** MCP ([docs.datadoghq.com/llm_observability/guide/monitor_mcp_client](https://docs.datadoghq.com/llm_observability/guide/monitor_mcp_client/)) i blog, ale ⚠️ **bez widocznej daty publikacji** i **bez możliwych do wyodrębnienia konkretnych nazw metryk/spanów/atrybutów `mcp.*`**. Datadog ma też własny „Datadog MCP Server" (odwrotny kierunek) — **nie mylić**.

**Eksplicytne negatywy (nie zmyślaj):** dokumentacja New Relic agentic-ai/mcp to referencja narzędzi **ich własnego** serwera MCP, nie monitoringu serwerów MCP. Dokumentacja Honeycomb `/integrations/mcp/*` dotyczy **Honeycomb MCP**. **Dynatrace i Elastic/EDOT: nic oficjalnego nie zweryfikowano.** Wersje i atrybuty OpenInference — niezweryfikowane.

### 6.10 Rekomendowany zestaw metryk dla serwera pamięci

⚠️ **To moja synteza, nie cytat** — łączy oficjalne metryki semconv z potrzebami operacyjnymi serwera z ~28 narzędziami.

| Metryka | Typ | Źródło | Po co |
|---|---|---|---|
| `mcp.server.operation.duration` (filtr `mcp.method.name="tools/call"`, atrybut `gen_ai.tool.name`) | Histogram, `s` | **oficjalna** | latency per narzędzie; `_count` = liczba wywołań |
| `mcp.server.operation.duration` (filtr `mcp.method.name="tools/list"`) | Histogram | **oficjalna** | koszt katalogu 28 narzędzi |
| `error.type` na powyższych | atrybut | **oficjalny** | `tool_error` gdy `isError: true`; kod JSON-RPC przy błędzie protokołu |
| `mcp.server.session.duration` | Histogram | **oficjalna** | ⚠️ w `2026-07-28` HTTP nie ma sesji — użyteczne tylko dla stdio (czas życia procesu) |
| Własna: czas trwania operacji specyficznych dla pamięci (`dream`, `reflect`, `gc`, `backup`, `find_duplicates`, `session_context`) | Histogram | **własna** | te nie są „tool calls" w sensie semantycznym — to zadania wsadowe |
| Własna: rozmiar bazy (liczba węzłów, rozmiar pliku SQLite, rozmiar sidecara HNSW) | Gauge | **własna** | wzrost bez ograniczeń to główne ryzyko serwera pamięci |
| Własna: trafienia/chybienia cache'u embeddingów i rerankera | Counter | **własna** | najdroższe obliczeniowo ścieżki |
| Własna: czas trwania i liczba elementów konsolidacji w tle | Histogram + Counter | **własna** | wykrywanie zapętleń i regresji |
| Własna: `mcp.request.body.size` / `mcp.response.body.size` | Histogram, `By` | **własna** | spec nie definiuje limitów rozmiaru — musisz je mierzyć |

**Zasada nadrzędna:** `mcp.method.name` i `gen_ai.tool.name` to **Required/Conditionally Required** — bez nich metryki są bezużyteczne, bo nie odróżnisz `search` od `dream`.

---

## 7. Testowanie i zgodność (conformance)

### 7.1 Oficjalny zestaw conformance — istnieje i jest utrzymywany

**Repozytorium: https://github.com/modelcontextprotocol/conformance** (utworzone 2025-07-10, ostatni push 2026-09-14, ~127★).

⚠️ **KLUCZOWA PUŁAPKA: dwie linie wersji, a README dokumentuje funkcje, które istnieją tylko na linii alpha.**

| Linia | Wersja | Data publikacji |
|---|---|---|
| npm `latest` | **0.1.16** | **2026-03-30** (GitHub release v0.1.16: 2026-03-27) |
| npm `alpha` | **0.2.0-alpha.11** | **2026-08-07** |

Funkcje `--requirements`, `tier-check` i per-check baseline istnieją **tylko na alpha**. **Przypnij wersję explicytnie** — inaczej README nie zgadza się z tym, co masz zainstalowane.

**Uruchamianie:**
```bash
# Wszystkie scenariusze serwera (domyślnie)
npx @modelcontextprotocol/conformance server --url http://localhost:3000/mcp

# Pojedynczy scenariusz
npx @modelcontextprotocol/conformance server --url http://localhost:3000/mcp --scenario server-initialize

# Dokładnie to, czego wymaga dana rewizja (tylko alpha)
npx @modelcontextprotocol/conformance server --url http://localhost:3000/mcp --requirements 2026-07-28

# Co ta rewizja faktycznie wymaga (lista)
npx @modelcontextprotocol/conformance list --requirements 2026-07-28

# Test klienta
npx @modelcontextprotocol/conformance client --command "tsx examples/clients/typescript/everything-client.ts" --suite auth
```

**Artefakty:** `results/<scenario>-<timestamp>/checks.json` (dla klienta dodatkowo `stdout.txt`, `stderr.txt`).

**Kontrakt kodów wyjścia z `--expected-failures` (YAML):**

| Wynik scenariusza | W baseline? | Wynik |
|---|---|---|
| Fail | Tak | exit 0 — oczekiwana porażka |
| Fail | Nie | **exit 1 — nieoczekiwana regresja** |
| Pass | Tak | **exit 1 — nieaktualny baseline** |
| Pass | Nie | exit 0 — normalne przejście |

**Baseline per-check:** `<scenario>:<check-id>` (np. `server-stateless:sep-2575-server-implements-discover`). ⚠️ **Uwaga na spację:** `- scenario:check-id` to string; `- scenario: check-id` to YAML-owy mapping i zostanie odrzucony. Nie można wpisać jednocześnie całego scenariusza i pojedynczego checka — to sprzeczne i jest odrzucane.

**`wire-schema-valid` / `wire-schema-harness-error`:** każdy scenariusz dodatkowo waliduje każdą wiadomość JSON-RPC na drucie względem JSON Schema specyfikacji dla wynegocjowanej wersji. `wire-schema-valid` oblewa, gdy **implementacja pod testem** wysłała niepoprawną wiadomość; `wire-schema-harness-error` oblewa, gdy **sam harness** wysłał niepoprawną — to bug w suite, nie w twojej implementacji.

**GitHub Action:** `modelcontextprotocol/conformance@v0.1.11` (inputs: `mode`, `url`, `command`, `expected-failures`, `suite`, `scenario`, `timeout` = 30000 ms, `verbose`, `node-version` = 20).

**⚠️ KLUCZOWE DLA VESTIGE — testowanie dual-era wymaga DWÓCH przebiegów.** Scenariusz należący do obu rewizji **musi być uruchomiony raz pod każdą**, bo „the dated revisions through `2025-11-25` use the stateful initialize handshake and `2026-07-28` is stateless with per-request `_meta`, and a scenario emits different checks under each. A scenario belonging to both revisions must therefore be run twice, once under each set. **Passing it on one wire says nothing about the other.**"

### 7.2 Ile scenariuszy wymaga zgodność — dokładne liczby

Wyliczone z plików `requirements/<rewizja>.yaml` (zamrożonych w momencie wydania rewizji):

| Rewizja | Scenariusze serwera | Scenariusze klienta | `not_scored` |
|---|---:|---:|---:|
| **`2026-07-28`** | **37** | **32** | 20 |
| `2025-11-25` | 30 | 18 | 10 |

Źródła: [requirements/2026-07-28.yaml](https://raw.githubusercontent.com/modelcontextprotocol/conformance/main/requirements/2026-07-28.yaml), [requirements/2025-11-25.yaml](https://raw.githubusercontent.com/modelcontextprotocol/conformance/main/requirements/2025-11-25.yaml).

**Pełna lista 37 wymaganych scenariuszy serwera dla `2026-07-28`** (to jest twoja lista zadań dla Vestige):

```
server-stateless
completion-complete
tools-list
tools-call-simple-text
tools-call-image
tools-call-audio
tools-call-embedded-resource
tools-call-mixed-content
tools-call-error
tools-call-with-progress
server-sse-multiple-streams
resources-list
resources-read-text
resources-read-binary
resources-templates-read
sep-2164-resource-not-found
prompts-list
prompts-get-simple
prompts-get-with-args
prompts-get-embedded-resource
prompts-get-with-image
dns-rebinding-protection
caching
input-required-result-basic-elicitation
input-required-result-basic-sampling
input-required-result-basic-list-roots
input-required-result-request-state
input-required-result-multiple-input-requests
input-required-result-multi-round
input-required-result-missing-input-response
input-required-result-non-tool-request
input-required-result-result-type
input-required-result-unsupported-methods
input-required-result-tampered-state
input-required-result-capability-check
input-required-result-ignore-extra-params
input-required-result-validate-input
```

**Scenariusze klienta (32)** — istotne dla Vestige, jeśli kiedykolwiek będzie miał tryb klienta: `tools_call`, `request-metadata`, 20× `auth/*` (w tym `auth/basic-cimd`, `auth/iss-*`, `auth/scope-step-up`, `auth/authorization-server-migration`, `auth/pre-registration`), `sep-2322-client-request-state`, `http-standard-headers`, `http-custom-headers`, `http-invalid-tool-headers`, `json-schema-ref-no-deref`.

**`not_scored` (20) — uruchamiane, raportowane, ale nie liczą się do zdawalności:**

| Powód | Znaczenie | Przykłady |
|---|---|---|
| `extension` | Opcjonalne z definicji (SEP-1730: „Experimental features and protocol extensions … are not required for any tier") | 11× `tasks-*`, `auth/client-credentials-*`, `auth/dpop`, `auth/dpop-nonce`, `auth/wif-jwt-bearer`, `auth/enterprise-managed-authorization` |
| `added-after-release` | Scenariusz nie istniał, gdy rewizja była wydana | `json-schema-2020-12-preservation` |
| `pending` | Własny fixture referencyjny suite'a nie umie go jeszcze przejść — ale twoja implementacja może | `json-schema-2020-12`, **`http-header-validation`**, **`http-custom-header-server-validation`** |

⚠️ **WAŻNE:** `http-header-validation` i `http-custom-header-server-validation` (oba z [SEP-2243](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2243), czyli walidacja `Mcp-Method`/`Mcp-Name`/`Mcp-Param-*`) są oznaczone jako **`pending`** — **nie są punktowane**, bo fixture referencyjny sobie nie radzi. To znaczy, że **oficjalny suite nie wymusi na tobie poprawnej walidacji nagłówków**, mimo że jest to wymóg MUST w specyfikacji. **Zaimplementuj to i przetestuj własnymi testami.**

### 7.3 SEP-2484 — wymóg testów zgodności dla SEP-ów

[SEP-2484](https://modelcontextprotocol.io/seps/2484-conformance-tests-required-for-final-seps.md) — **Final**, typ **Process** (nie Standards Track), utworzony **2026-03-27**, autor Paul Carleton, bez sponsora.

Bramkuje przejście `Accepted → Final` dla SEP-ów Standards Track, które mają **obserwowalne zachowanie protokołu**:
- Wymaga scenariusza oznaczonego numerem SEP-a.
- Wymaga pliku traceability **`sep-NNNN.yaml`** mapującego **każde** MUST / MUST NOT / SHOULD / SHOULD NOT na check albo udokumentowane wykluczenie.
- Checki na poziomie SHOULD raportowane są jako **ostrzeżenia, nie porażki**; MAY nie wymaga wiersza.
- **Nie działa wstecz.**
- **Zastępuje SEP-1627.**
- Tekst specyfikacji jest autorytatywny nad testami.
- Etykieta `disputed` zamraża test na potrzeby tieringu.
- Scenariusze pisane w **TypeScript**.
- Wymienione prerekwizyty: schemat traceability, scaffolder `new-scenario --sep <number>`, `MAINTAINERS.md`.

**Konsekwencja dla Vestige:** jeśli katalog narzędzi ewoluuje, a chcesz śledzić zgodność, wzorzec `sep-NNNN.yaml` + traceability jest dobrym wzorcem wewnętrznym nawet bez formalnego SEP-a.

### 7.4 Minimalny poziom zgodności wg tieringu SDK (SEP-1730)

[SDK Tiering System](https://modelcontextprotocol.io/community/sdk-tiers.md):

| Wymóg | Tier 1 | Tier 2 | Tier 3 |
|---|---|---|---|
| **Testy conformance** | **100%** | **80%** | brak minimum |
| Nowe funkcje protokołu | przed wydaniem nowej wersji spec | w 6 miesięcy | brak zobowiązania |
| Triage issue | 2 dni robocze | miesiąc | brak |
| Rozwiązanie krytycznego buga | 7 dni | 2 tygodnie | brak |
| Stabilne wydanie | wymagane | co najmniej jedno | niewymagane |

**Kluczowe daty:** **2026-01-23** — dostępne testy zgodności; **2026-02-23** — opublikowany tiering oficjalnych SDK.

**Degradacja tieru:** Tier 1 → 2 gdy **jakikolwiek** test conformance oblewa; Tier 2 → 3 gdy oblewa **>20%**; w obu przypadkach po **4 tygodniach** ciągłego obl ewania na najnowszym stabilnym wydaniu. Dodatkowo degradacja gdy issue pozostają nierozwiązane przez 2 miesiące.

**Definicja P0:** podatności bezpieczeństwa z **CVSS ≥ 7.0** albo awarie rdzeniowej funkcjonalności uniemożliwiające podstawowe operacje MCP (nawiązanie połączenia, wymiana wiadomości, użycie prymitywów tools/resources/prompts).

**Wyliczanie wyniku:** tylko **„applicable required tests"** — testy dla wersji specyfikacji, którą SDK targetuje; **z wyłączeniem** testów `pending`/`skipped`, testów funkcji eksperymentalnych, **testów kompatybilności wstecznej legacy (chyba że SDK deklaruje wsparcie legacy)** oraz testów oznaczonych `disputed`.

⚠️ **To jest kontrintuicyjne i ważne:** **testy legacy NIE liczą się do wyniku, chyba że SDK zadeklaruje wsparcie legacy.** Dla Vestige oznacza to, że deklarowanie wsparcia dla `2024-11-05`…`2025-11-25` **dobrowolnie zwiększa** zakres testów, które musisz przejść. Rozważ, czy warto deklarować wsparcie legacy explicytnie, czy po prostu je utrzymywać bez deklaracji w tieringu.

### 7.5 MCP Inspector

npm `@modelcontextprotocol/inspector`:

| Tag | Wersja | Data |
|---|---|---|
| `latest` | **2.7.0** | **2026-09-16** |
| `next` | **2.0.0-rc.3** | **2026-07-28** |
| `v1-latest` | **1.0.2** | **2026-08-24** |

~10 913★. **Wymaga Node 22.19.0+.** Jeden binarny, trzy klienty: `--web` (domyślny), `--cli`, `--tui`. Launcher parsuje flagi trybu tylko na początku argv; `mcp-inspector --cli --help` przekazuje `--help` do CLI.

**Uruchamianie:**
```bash
# stdio
npx @modelcontextprotocol/inspector node path/to/server/index.js

# Streamable HTTP
npx @modelcontextprotocol/inspector --cli https://api.example.com/mcp --transport http --method tools/list

# Wywołanie narzędzia
npx @modelcontextprotocol/inspector --cli http://localhost:3000/mcp --transport http \
  --method tools/call --tool-name search --tool-arg query=otters
```

**CLI:** jedno `--method` na przebieg. Metody: `initialize`, `tools/list`, `tools/call` (z `--tool-name`), `resources/list|read|templates/list`, `prompts/list|get`, `logging/setLevel` (**tylko legacy**), `servers/list|show` (bez łączenia się).
- `--tool-arg key=value` — wartość **koercjonowana jako JSON**
- `--tool-args-json` — wartość **dosłowna**; **wzajemnie wykluczające się** z `--tool-arg`

**Format wyjścia:** `--format json` = pojedynczy obiekt JSON na stdout, bez banerów; `tools/list --app-info` zawsze NDJSON.

**Kody wyjścia — to jest kontrakt do CI:**

| Kod | Znaczenie |
|---|---|
| **0** | OK |
| **1** | błąd użycia (usage) |
| **2** | nie znaleziono MCP App |
| **3** | wymagane uwierzytelnienie |
| **4** | serwer nieosiągalny |
| **5** | błąd narzędzia / narzędzie nie znalezione |

Przy niezerowym wyjściu CLI pisze **jedną linię JSON do `stderr`** — parsowalną przez:
```bash
... 2>&1 | tail -1 | jq .error
```
Przykład: `{"error":{"code":"auth_required",...}}`.

**⚠️ W CI używaj `--stored-auth-only`** — to flaga, której chce CI; bez niej przebiegi bez TTY kończą się szybkim błędem zamiast zawieszenia. Roots konfiguruje się **tylko przez plik konfiguracyjny**. Proxy: honorowane `HTTPS_PROXY` / `NO_PROXY`.

### 7.6 mcpjam — narzędzie сторонне, NIE oficjalne

**https://github.com/MCPJam/inspector** — **~2 222★**, utworzone 2025-05-23, ostatni push **2026-09-19**, licencja **Apache-2.0**. npm **`@mcpjam/inspector` 3.8.1, opublikowany 2026-09-18**. Bardzo szybki kadencja wydań (3.7.2 → 2026-09-17; 3.8.0 i 3.8.1 → 2026-09-18).

`npx @mcpjam/inspector@latest`

Sam się opisuje jako: „testing & evaluations platform for MCP server developers … across **16 client configurations and 170+ models**". Funkcje: Playground (MCP Apps / OpenAI Apps SDK + emulator widgetów), **OAuth Debugger z checkami zgodności dla `03-26`, `06-18`, `11-25` i `2026-07-28` + DCR/CIMD**, Server Debugging, Skills, Workspaces, Evals, CLI, SDK, CI/CD. Dokumentacja: docs.mcpjam.com.

⚠️ **Jego „conformance" jest WŁASNY, oddzielny od oficjalnego suite'a.** Traktuj jako uzupełnienie (szczególnie debugger OAuth, którego oficjalny suite nie ma w formie interaktywnej), nie jako zamiennik.

### 7.7 Inne narzędzia — z uczciwą oceną stanu

| Narzędzie | Status | Uwaga |
|---|---|---|
| **Inspector V2 WG** | ✅ **oficjalna** grupa robocza | [charter](https://modelcontextprotocol.io/community/working-groups/inspector-v2.md), przyjęty **2026-04-11**; prowadzą Cliff Hall / Ola Hungerford / Bob Dickinson; zakres: Inspector Core + Web/CLI/TUI + aparat testowy + **deprecacja `main` do gałęzi `v1.x`** + adopcja TS SDK V2 (obecnie **Blocked** na TS SDK WG). ⚠️ Kryteria sukcesu mówią „End of Q1/Q2/Q3" **bez roku** — niezweryfikowane. Dowód, że ląduje: npm `next` = 2.0.0-rc.3 (2026-07-28), `v1-latest` = 1.0.2 (2026-08-24). |
| **`mcptools`** (f/mcptools) | ⚠️ **przestarzałe** | 1 625★, MIT, **ostatni push 2025-12-18 (~9 miesięcy)**. README zwraca 404 na `main`. Wersja niezweryfikowana. **Nie polecam.** |
| **`mcp-scan`** | ⚠️ **zmiana nazwy, pułapka** | PyPI `mcp-scan` 0.4.3 (2026-03-02) to teraz **przekierowanie do `snyk-agent-scan`** (bieżący **0.6.3**). ⚠️ npm `mcp-scan` 2.0.13 (2026-09-09) to **niepowiązany projekt** autorstwa Abanoub-Rodolf. Repo `invariantlabs-ai` niezweryfikowane. |
| **`mcp-eval`** | ⚠️ przestarzałe + kolizja nazw | `lastmile-ai/mcp-eval` 37★, Apache-2.0, **ostatni push 2025-11-19**. PyPI `mcpevals`. OTel jako single source of truth; style `@task`/pytest/dataset; asercje `Expect.tools.*`, `Expect.performance.response_time_under`, `Expect.judge.llm`, `Expect.path.efficiency`; recipes GH Actions + GitLab CI. ⚠️ Nazwa dzielona z `alpic-ai/mcp-eval` i `smart-mcp-proxy/mcp-eval`. |
| **Docker `mcp-gateway`** | ✅ aktywne, ale **to NIE conformance** | 1 574★, MIT, push 2026-09-16. Wymaga **Docker Desktop 4.59+**. `docker mcp tools ls|inspect|call`, `docker mcp gateway run --transport streaming`, profile/katalogi przez OCI. **Docker nie publikuje zestawu conformance dla MCP** — to fixtures i packaging. |
| **NIE ZNALEZIONO / nie polecam:** `mcp-safety`, `pytest-mcp`, `mcp-unit`, `mcp-test-harness`, `mcpcontract` | ❌ | Przeszukane i nie znalezione. `mcp-client-cli` istnieje w metadanych npm, ale **nie eksponuje żadnych dist-tags ani wersji** — nie polecaj. |

### 7.8 Ery protokołu w Inspectorze — bardzo istotne dla serwera dual-era

Źródło: [Protocol eras](https://modelcontextprotocol.io/docs/2026-07-28/tools/inspector/protocol-eras.md).

**Era to ustawienie PER-SERWER, ortogonalne do transportu.** Trzy tryby:

| Tryb | Zachowanie |
|---|---|
| **`legacy`** | **DOMYŚLNY.** Zwykły `initialize`, **żadnego probingu**. |
| `auto` | Sonduje `server/discover`, cofa się przy każdym nie-nowoczesnym wyniku |
| `modern` | Przypina dokładnie `2026-07-28`; **brak fallbacku, oblewa głośno** |

**Uzasadnienie domyślnego `legacy` (cytat):**
> „A debugging tool must not auto-probe. A `server/discover` probe stalls against silent legacy stdio servers, and it pollutes the recorded transcript you came here to read."

Konfiguracja: Server Settings (web) / pole `protocolEra` (catalog albo config file) / ten sam plik (CLI + TUI).

**Konsekwencje dla testowania Vestige jako serwera dual-era:**
1. **Era ≠ transport** — CI potrzebuje `protocolEra` jako osobnej osi testowej. Nie wystarczy testować „stdio" i „HTTP".
2. **Modern-only serwer testowany z domyślnymi ustawieniami dostanie zwykłe `initialize`** i **musi odpowiedzieć `-32022` + `data.supported`, a nie zawiesić się.**
3. **Cisza jest poprawna** — nowoczesny serwer bez `logLevel` w żądaniu, który nie emituje logów, **jest zgodny**. Test „asercja, że logi się pojawiają" to pułapka fałszywej porażki.
4. **Bezstanowość psuje stateful fixtures** — nowoczesna konfiguracja subskrypcji celowo pomija `update_resource`, bo mutacja trafiłaby w jednorazową instancję.
5. **Reprodukowalne konfiguracje są w repo** — po `npm install && npm run build && cd clients/web && npm run test-servers:build` dostępne są m.in. `logging-legacy-http.json`, `logging-modern-http.json`, `subscriptions-*-http.json`, `tasks-*-http.json`, `mrtr-showcase-http.json`, `xmcpheader-modern-http.json`, `modern-network-http.json`.
6. **⚠️ ZNANA LUKA: mirrorowanie `Mcp-Param-*` jest pomijane przez SDK w przeglądarce.** Testy zgodności dla `x-mcp-header` **musisz** prowadzić z CLI albo TUI, **nie** z klienta webowego. To dokładnie wyjaśnia, dlaczego scenariusze `http-custom-header-server-validation` są `pending`.
7. `mrtr_loop` nigdy się nie kończy — gotowy fixture do testowania `MRTR_MAX_ROUNDS`.
8. Typowane błędy nowoczesne: `-32020` HeaderMismatch / `-32021` MissingRequiredClientCapability / `-32022` UnsupportedProtocolVersion / `-32601`. `-32602` rozróżnia „Unknown Tool" od „Invalid Parameters". Legacy `collect_elicitation` (żądania serwer→klient) **oblewa** na `2026-07-28` — zamiennikiem jest MRTR.

### 7.9 Luki w oficjalnym pokryciu testowym — powiedz to wprost

| Obszar | Stan |
|---|---|
| Testy jednostkowe handlerów narzędzi w izolacji | ❌ **Brak oficjalnych wytycznych MCP.** Oficjalne stanowisko jest integracyjne/wire-first — SEP-2484 definiuje conformance jako **obserwowalne zachowanie na drucie**. Najbliższe narzędzie w stylu jednostkowym to сторонніе `mcp-eval`. |
| **Testowanie obciążeniowe** | ❌ **Nie znaleziono ŻADNYCH wytycznych ani harnessu — nigdzie.** Traktuj load testing MCP jako niepokryte. |
| Walidacja nagłówków HTTP (`Mcp-Method`/`Mcp-Name`/`Mcp-Param-*`) | 🟡 Scenariusze istnieją, ale są `pending` → **niepunktowane**. Musisz je przetestować sam. |
| JSON Schema 2020-12 | 🟡 `json-schema-2020-12` = `pending`; `json-schema-2020-12-preservation` = `added-after-release`. |
| Tasks (`io.modelcontextprotocol/tasks`) | 🟡 11 scenariuszy `tasks-*` = `extension` → **niepunktowane** i `pending` przeciwko fixture referencyjnemu. |

**Rekomendacja praktyczna dla Vestige:** zbuduj własny harness CI, który:
1. używa **oficjalnego suite'a** (`--requirements 2026-07-28` i osobno `--requirements 2025-11-25`) jako bramki regresji,
2. używa **konformance'u Inspectora CLI** w trybach `legacy`, `auto`, `modern` jako drugiej osi,
3. **dopisuje własne testy** dla tego, czego suite nie punktuje: walidacji nagłówków, rozmiarów ciała, rate limitingu, anulowania (dla HTTP — zamknięcie strumienia SSE; dla stdio — `notifications/cancelled`), timeoutów i testów obciążeniowych.

---

## 8. Pamięć jako długo żyjący serwer MCP — backup, szyfrowanie, współbieżność, budżet tokenów

### 8.1 SQLite WAL — twarde fakty ze specyfikacji SQLite

Źródło podstawowe: [SQLite — Write-Ahead Logging](https://sqlite.org/wal.html), strona ostatnio aktualizowana **2026-08-25**.

**Zalety WAL (cytowane):**
1. „significantly faster in most scenarios";
2. „more concurrency as readers do not block writers and a writer does not block readers";
3. „Disk I/O operations tends to be more sequential";
4. „uses many fewer fsync() operations and is thus less vulnerable to problems on systems where the fsync() system call is broken".

**Wady WAL — istotne operacyjnie:**
1. **„All processes using a database must be on the same host computer; WAL does not work over a network filesystem."** Powód: WAL wymaga współdzielonej pamięci na wal-index. **Nie kładź bazy na NFS/SMB/iCloud Drive/Dropbox.**
2. Transakcje obejmujące wiele `ATTACH`-owanych baz są atomowe per baza, ale **nie jako zbiór**.
3. **Nie można zmienić `page_size` po wejściu w tryb WAL** — ani na pustej bazie, ani przez `VACUUM`, ani przez restore z backupu. Trzeba najpierw wrócić do trybu rollback journal.
4. Może być „very slightly slower (perhaps **1% or 2%** slower)" w aplikacjach głównie czytających, które rzadko piszą.
5. Dodatkowe pliki `-wal` i `-shm` (co czyni SQLite mniej atrakcyjnym jako *format pliku aplikacji*).
6. Dodatkowa operacja checkpointu — domyślnie automatyczna, ale trzeba o niej myśleć.

**Współbieżność — dokładne reguły:**
- Czytelnicy nie blokują pisarzy, pisarze nie blokują czytelników. **Ale: „since there is only one WAL file, there can only be one writer at a time."**
- Każdy czytelnik ma własny **end mark**; transakcja czytająca widzi spójny punkt w czasie.
- **Checkpoint MUSI się zatrzymać**, gdy dojdzie do strony poza end markiem jakiegokolwiek aktywnego czytelnika. „Thus a long-running read transaction can prevent a checkpointer from making progress."
- Gdy pisarz stwierdzi, że cały WAL jest już w bazie i żaden czytelnik nie korzysta z WAL-a, **przewija WAL na początek**. To mechanizm zapobiegający nieograniczonemu wzrostowi.

**Checkpointing — konkretne liczby:**
- Domyślnie checkpoint następuje, gdy WAL osiągnie **1000 stron** (≈ **4 MB**, przy domyślnej stronie 4096 B) — potwierdzone także w sekcji „Avoiding Excessively Large WAL Files": „until the WAL file accumulates about 1000 pages (and is thus **about 4MB in size**)".
- Dodatkowo checkpoint przy zamknięciu ostatniego połączenia.
- Typy checkpointów: **PASSIVE** (domyślny — robi tyle, ile może bez przeszkadzania innym), **FULL**, **RESTART**. Tylko `sqlite3_wal_checkpoint_v2()` może robić FULL/RESTART.
- **Checkpoint NIE obcina pliku WAL**, chyba że ustawiono `journal_size_limit` — zamiast tego zaczyna nadpisywać od początku. To ważne: `-wal` może **zostać na dysku** mimo zakończonego checkpointu.

**⚠️ Trzy udokumentowane przyczyny nieograniczonego wzrostu WAL:**
1. **Wyłączenie automatycznego checkpointu.**
2. **Checkpoint starvation** — cytat: „if a database has many concurrent overlapping readers and there is always at least one active reader, then **no checkpoints will be able to complete and hence the WAL file will grow without bound**." Lekarstwo: zapewnić „reader gaps" albo użyć `SQLITE_CHECKPOINT_RESTART` / `SQLITE_CHECKPOINT_TRUNCATE` (kosztem blokowania czytelników).
3. **Bardzo duże transakcje zapisu** — WAL nie może zostać zresetowany w trakcie transakcji.

**Ustawienia:**
- `PRAGMA journal_mode=WAL;` — **jest trwałe** (w przeciwieństwie do innych trybów). Ustawienie na jednym połączeniu ustawia je na wszystkich połączeniach do tego pliku.
- `PRAGMA synchronous=FULL` — pisarze synchronizują WAL przy każdym commicie.
- `PRAGMA synchronous=NORMAL` — **pomijają sync przy commicie**; wtedy „the checkpoint is the only operation to issue an I/O barrier or sync operation". Zysk: brak blokowania na fsync w wątku głównym. **Koszt: „transactions are no longer durable and might rollback following a power failure or hard reset."** Dla serwera pamięci to jest realny trade-off do świadomej decyzji.
- `PRAGMA journal_size_limit` — jedyny sposób, by checkpoint faktycznie obciął plik WAL.
- `PRAGMA wal_autocheckpoint` / `sqlite3_wal_autocheckpoint()` — zmiana progu.

**Kiedy zapytanie w trybie WAL zwróci `SQLITE_BUSY`** (specyfikacja explicytnie ostrzega: „applications should be prepared for that happenstance"):
1. Inne połączenie trzyma bazę w **exclusive locking mode**.
2. **Gdy ostatnie połączenie się zamyka** — przez krótki czas bierze exclusive lock, sprzątając WAL i pliki shared-memory.
3. **Gdy ostatnie połączenie się zawiesiło** — pierwsze nowe połączenie przeprowadza recovery pod exclusive lockiem. Trzecie połączenie dostanie `SQLITE_BUSY`.
   → **Praktyczny wniosek: ustaw `PRAGMA busy_timeout` na sensowną wartość (np. 5000 ms) i obsługuj `SQLITE_BUSY` jako normalny, retriable błąd.** ⚠️ Specyfikacja SQLite **nie podaje zalecanej wartości `busy_timeout`** — to wybór implementacyjny.

**⚠️⚠️ KRYTYCZNE: „WAL-reset bug" — podatność wykryta i naprawiona w 2026**

To jest najważniejsze odkrycie tej sekcji i jest **bezpośrednio istotne dla Vestige**, bo architektura Vestige („SQLite WAL mode, `Mutex<Connection>` reader/writer split") opisuje dokładnie warunki brzegowe tego buga.

Cytaty ze [sqlite.org/wal.html §11](https://sqlite.org/wal.html):

| Fakt | Wartość |
|---|---|
| Wykryty | **2026-03-03** (przez dewelopera SQLite, Dana) |
| Obecny w wersjach | **3.7.0 (2010-07-21) – 3.51.2 (2026-01-09)** |
| Naprawiony w | **3.51.3 (2026-03-13)** |
| Backporty | **3.44.6** i **3.50.7** |
| Skutek | **Korupcja bazy danych** |
| Warunek | „only affects databases in WAL mode when there are **two or more database connections open on the same file, in separate threads or processes**, and when those two connections attempt to **write or checkpoint at the same instant**" |
| Charakterystyka | „a data race with **tight timing constraints** and that requires an unusual usage pattern. It is unlikely to occur in common use. The developers were **unable to reproduce the bug organically**" |

**Mechanizm (6 kroków, streszczenie):** checkpoint #1 kończy się w całości → startuje checkpoint #2 → inne połączenie commituje transakcję, która **resetuje WAL** i pisze nową treść na początek → z powodu wyścigu checkpoint #2 nie zauważa resetu i zostawia **błędne pole w nagłówku WAL-index**, twierdzące że część WAL jest już checkpointowana → kolejne transakcje powiększają WAL → checkpoint #3 **pomija część transakcji**, która nigdy nie trafia do pliku bazy → **korupcja**.

**Aktualizacja z 2026-08-24 (cytowana na tej samej stronie):** „Phil Eaton has devised a **reproducer that does not involve using the special testing code hack**" — link: [Another look at SQLite's WAL-Reset bug](https://theconsensus.dev/p/2026/08/23/another-look-at-sqlite-wal-reset.html). To podnosi wagę problemu: bug **nie wymaga już patologicznego timingu** do wywołania.

**Ocena ryzyka przez samych deweloperów SQLite (cytat):** „Based on available telemetry, the occurrence rate of this problem in the wild appears to be **less than or equal to the expected occurrence rate of SSD malfunctions and/or cosmic-ray hits**."

**✅ STATUS VESTIGE — SPRAWDZONE I DOBRE:**
```
Cargo.toml:        rusqlite = { version = "0.39", features = ["chrono", "serde_json"] }
                   crates/vestige-mcp/Cargo.toml: rusqlite features = ["bundled"]
Cargo.lock:        rusqlite 0.39.0, libsqlite3-sys 0.37.0
libsqlite3-sys-0.37.0/sqlite3/sqlite3.h:149:
                   #define SQLITE_VERSION "3.51.3"
```
**Vestige buduje z wersją 3.51.3 — czyli z wersją, w której bug jest naprawiony.** ✅

**⚠️ ALE: to nie jest gwarantowane na stałe.** Ryzyko: jeśli kiedykolwiek zbudujesz **bez** feature'a `bundled` w którymś z crate'ów, `libsqlite3-sys` zlinkuje się z systemowym SQLite. macOS Tahoe/Sequoia dostarcza SQLite **starszy niż 3.51.3**, czyli **podatny**. Rekomendacje:
1. **Utrzymuj `bundled` wszędzie** i dodaj test CI, który asertuje `SQLITE_VERSION` z `rusqlite::version()`.
2. **Dodaj `PRAGMA busy_timeout`** — zmniejsza szansę na równoczesny checkpoint+writes.
3. **Unikaj dwóch procesów piszących** do tego samego pliku (patrz 8.2).
4. Rozważ **`PRAGMA wal_autocheckpoint`** i okresowy `wal_checkpoint(TRUNCATE)` w oknie bez czytelników, żeby kontrolować checkpointy, zamiast polegać na PASSIVE w losowych momentach.

Źródła: [sqlite.org/wal.html](https://sqlite.org/wal.html) (akt. 2026-08-25); [theconsensus.dev — Another look at SQLite's WAL-Reset bug](https://theconsensus.dev/p/2026/08/23/another-look-at-sqlite-wal-reset.html) (2026-08-23).

### 8.2 Wielu klientów MCP na jednym pliku SQLite — co się realnie psuje

To najważniejsze pytanie operacyjne dla serwera local-first z dwoma transportami.

**Scenariusz A: dwa klienty MCP po stdio → dwa osobne procesy `vestige-mcp` → dwa `Connection` do tego samego pliku.**
- Na poziomie SQLite to jest **wspierane i bezpieczne w normalnej pracy** (WAL jest do tego zaprojektowany; wiele procesów na tym samym hoście to podstawowy przypadek użycia).
- ⚠️ **ALE** to jest dokładnie konfiguracja, w której występuje WAL-reset bug („two or more database connections open on the same file, **in separate threads or processes**"). Naprawione w 3.51.3 ✅, ale nie ma miejsca na starszy SQLite.
- ⚠️ **Drugi realny problem: `busy_timeout` i `SQLITE_BUSY`.** Dwa niezależne procesy nie dzielą ze sobą `busy_timeout` — każdy ustawia własny. Bez `busy_timeout` zobaczysz natychmiastowe `SQLITE_BUSY` przy kolizji zapisu.
- ⚠️ **Trzeci problem: zewnętrzny sidecar HNSW.** Vestige trzyma indeks wektorowy USearch w pliku `vestige.hnsw` + meta JSON, ładowanym przy starcie z „row-count-validated fast path" i przebudową z SQLite przy niezgodności. **Dwa procesy piszące do tego samego sidecara = nadpisanie się nawzajem.** Meta JSON z licznikiem wierszy to dobra heurystyka wykrywania rozjazdu, ale **nie jest to blokada** — dwa procesy mogą oba uznać sidecar za aktualny i oba go nadpisać. ⚠️ To jest ryzyko, którego SQLite nie rozwiąże za ciebie, bo sidecar jest poza transakcją.
- ⚠️ **Czwarty problem: pętle w tle.** Vestige ma `ConsolidationScheduler` co 6 h (`VESTIGE_CONSOLIDATION_INTERVAL_HOURS`) i inline consolidation po wywołaniach narzędzi. Przy dwóch procesach **obie pętle działają równolegle** → podwójna konsolidacja, podwójne zużycie CPU/ONNX, potencjalne konflikty zapisu i wyścigi w logice kognitywnej (FSRS-6, reconsolidation windows). To jest realny bug logiczny, nie tylko wydajnościowy.

**Scenariusz B: stdio + streamable HTTP jednocześnie.** Ten sam problem co A, plus HTTP może mieć wielu klientów → potencjalnie N pisarzy.

**Scenariusz C: serwer HTTP z wieloma wątkami w jednym procesie.** To jest **najbezpieczniejszy** wariant: jeden proces = jedna kolejka zapisów = `Mutex<Connection>` faktycznie serializuje. Nadal obowiązuje reguła „one writer at a time", ale jest egzekwowana przez proces, a nie przez SQLite.

**Rekomendowana architektura — wzorzec jednego pisarza (single-writer daemon):**
1. **Jeden proces właścicielem pliku bazy.** Uruchamiaj `vestige-mcp` w trybie HTTP na loopback lub unix domain sockecie i pozwól wielu klientom MCP podłączyć się do **niego**, zamiast uruchamiać osobny proces per klient.
2. **Osobny tryb `--read-only` dla procesów stdio**, jeśli muszą istnieć. SQLite wspiera to od **3.22.0 (2018-01-22)**: baza WAL może być czytana bez prawa zapisu, jeśli spełniony jest któryś z warunków — `-shm` i `-wal` istnieją i są czytelne, LUB jest prawo zapisu do katalogu, LUB połączenie używa parametru `immutable`. ([sqlite.org/wal.html §5](https://sqlite.org/wal.html)) ⚠️ **Ale `immutable` jest niebezpieczne dla żywej bazy** — mówi SQLite, że plik się nie zmieni; użyj go tylko dla snapshotu.
3. **Plik blokady (lockfile) na czas życia procesu** — np. `flock` na `vestige.lock` albo plik PID, żeby drugi proces piszący odmówił startu z czytelnym komunikatem zamiast cicho korumpować sidecar.
4. **`PRAGMA busy_timeout = <ms>`** w każdym połączeniu.
5. **Ustaw `PRAGMA synchronous` świadomie** — `NORMAL` jest rozsądnym domyślnym dla lokalnej pamięci (zyszek: brak blokowania na fsync; koszt: możliwy rollback ostatnich transakcji po utracie zasilania). Jeśli chcesz trwałości — `FULL`.
6. **Sidecar HNSW: nigdy nie pisz z dwóch procesów.** Albo trzymaj go tylko w procesie-właścicielu, albo traktuj jako **cache odtwarzalny** (co Vestige już robi — rebuild z SQLite) i pozwól procesom read-only go ignorować.

⚠️ **Uspokajająca informacja:** [SEP-2567](https://modelcontextprotocol.io/seps/2567-sessionless-mcp.md) pokazuje, że problem „wielu procesów" jest w MCP **znany i adresowany** — proces stdio nie jest już sesją ani konwersacją, a klienci **SHOULD NOT** używać pojedynczego zadania jako granicy życia procesu stdio. To znaczy, że architektura „jeden długo żyjący serwer, wielu klientów" jest zgodna z kierunkiem protokołu.

### 8.2a Indeks wektorowy: krajobraz zmienił się materialnie w 2026

**⚠️ SQLite ma teraz OFICJALNE rozszerzenie wektorowe: „Vec1"** — [sqlite.org/vec1](https://sqlite.org/vec1), strona wygenerowana **2026-08-28**.
- Przenośny C, SIMD AVX2/NEON, **IVFADC + OPQ** (nie HNSW!), bieżące wydanie **wersja 0.7**.
- Roadmap (cytat): „**No further features are required before a 1.0 release. But: Testing is insufficient.**" oraz „Add an option for a modern graph-based index as an alternative to IVFADC. HNSW? DiskANN?"
- Deweloper SQLite Dan Kennedy, **2026-03-30**: „There is still no vec1 release, but we are getting closer… Some paths that use SIMD on x86 are not yet using SIMD on ARM (or WASM), and **testing is woefully inadequate**." W tym samym wątku: 36 ostrzeżeń MSVC, pozostawiony `printf`, zepsuty build WASM.
- **Wniosek dla Vestige:** Vec1 to **IVFADC, nie HNSW**, i **wymaga kroku treningowego** — **nie jest drop-in replacement dla sidecara USearch**. Warto obserwować, ale nie migrować teraz.

| Biblioteka | Status na 2026-09-19 |
|---|---|
| **Vec1** (oficjalne SQLite) | v0.7, „testing is woefully inadequate", brak 1.0 |
| **`sqlite-vec`** | **pre-v1**; README ostrzega „expect breaking changes". crates.io max `0.1.10-alpha.4` (2026-05-18), **max stabilne `0.1.9` (2026-03-31)**. MIT/Apache-2.0 |
| **`sqlite-vss`** | ⚠️ **ZDEPRECJONOWANE** przez własny README: „not in active development… effort is now going towards sqlite-vec" |
| **USearch** | 2.26.2 (2026-08-31), Apache-2.0. ⚠️ Twierdzenie „10× szybszy niż FAISS" to **blog vendora**, nie niezależny pomiar. **Vestige już go używa — to dobry wybór** |
| **`instant-distance`** | ⚠️ ostatnia publikacja **2023-06-26** — efektywnie porzucone |

**⚠️ Usuwanie z indeksu ANN — istnieje recenzowana praca i daje konkretną odpowiedź.**
Yamashita / Amagata / Matsui, [**arXiv:2512.06200**](https://arxiv.org/abs/2512.06200), zgłoszony **2025-12-05**, **NeurIPS 2025 Workshop on ML for Systems**.

| Strategia usuwania | Dokładność | QPS usuwania | Pamięć |
|---|---|---|---|
| Reconstruction (przebudowa) | **najwyższa** | najniższy | — |
| Eager | średnia | średni | **najlepsza** |
| **Lazy (tombstone)** | **najniższa** | **najwyższy** | — |

Kluczowe ustalenia:
- **Degradacja dokładności przy lazy deletion jest LINIOWA** i przewidywalna: `Δ = (R_S − R_0)/S`.
- **Degradacja przy eager deletion zbiega do stabilnego poziomu θ** — nie rośnie nieograniczenie.
- **„Deletion Control"** przeplata lazy + okresową przebudowę, zużywając tylko **10% zapytań** jako kalibrację.
- ⚠️ Własne ograniczenie pracy: „**The performance impact of concurrent deletion and querying is not discussed.**"

→ ✅ **ODPOWIEDŹ: tombstone'y są funkcjonalnie wystarczające; okresowa przebudowa to praktyczne rozwiązanie.** To potwierdza, że strategia Vestige (rebuild z SQLite) jest właściwa — **ale musi być uruchamiana okresowo, nie tylko przy niezgodności licznika wierszy.**

> 🚩 **SŁABOŚĆ OBECNEGO ZABEZPIECZENIA VESTIGE:** walidacja sidecara HNSW przez **licznik wierszy** to **słaby sygnał starzenia** — **usunięcie + wstawienie w tej samej transakcji daje identyczny licznik i jest niewidoczne.** Rekomendacja: **monotoniczny licznik zmian (change counter) albo hash treści** w meta JSON zamiast (lub obok) liczby wierszy.
>
> ⚠️ **Dodatkowo: SQLite WAL daje tabeli atomowość przy awarii, ale plik sidecara `.hnsw` NIE DOSTAJE ŻADNEJ.** ⚠️ **Żadne źródło pierwotne nie adresuje tego problemu** — to jest nieudokumentowana szczelina, którą trzeba zamknąć samodzielnie (np. zapis sidecara przez plik tymczasowy + atomowy `rename`).

### 8.2b FTS5 w produkcji

Vestige używa FTS5 jako jednego z trzech filarów wyszukiwania hybrydowego (BM25 + semantic + RRF), więc to jest istotne operacyjnie.

- **`optimize` a `merge`:** `optimize` „can take a long time to run". **Rekomendacja: użyj przyrostowego `merge`** — `INSERT INTO ft(ft, rank) VALUES('merge', 500)`. Wykrywanie no-op przez różnicę `sqlite3_total_changes()` < 2. ⚠️ **„only the first call should specify a negative parameter".** To idealnie pasuje do **pętli konsolidacji w tle** w Vestige.
- **Domyślne parametry:** `automerge` **4** (maks. 16), `crisismerge` **16**, `usermerge` **4**, `pgsz` **4050**.
- **Tabele external-content:** **„It is the responsibility of the user to ensure… kept consistent"** — udokumentowane tryby **cichej niespójności**. Odzyskiwanie: `INSERT INTO ft(ft) VALUES('rebuild')`. ⚠️ **Brak obsługi konfliktu REPLACE** (jest ABORT). Kontrola spójności: `VALUES('integrity-check', 1)` dla external-content.
  → **Dla Vestige: skoro masz `memory(action="delete")`, `gc` i `split_memories(dry_run: false)`, musisz mieć ścieżkę `rebuild` FTS5 wywoływaną, gdy `integrity-check` zawiedzie.** To realny scenariusz, nie teoretyczny.
- **⚠️ Narzut rozmiaru indeksu:** **jedyne znalezione źródło to pomiar z forum społeczności (2024-05-20)** — 2,8 GB treści → **9,9 GB** dla FTS5 przechowującego treść (**~3,5×**), **5,4 GB** dla contentless/external-content (**~1,9×**). **SQLite nie publikuje oficjalnej liczby**, a oszczędność opcji `detail` jest niekwantyfikowana.
- **⚠️ Pułapka contentless:** tabele contentless **psują `snippet` i `highlight`** (zwracają NULL) oraz **psują usuwanie bez oryginalnej treści**. Jeśli Vestige pokazuje snippety w wynikach `search`, **nie możesz użyć tabeli contentless.**

### 8.3 Backup i restore

**Fundamentalna zasada z [sqlite.org/wal.html §4](https://sqlite.org/wal.html):**
> „The WAL file is part of the persistent state of the database and should be kept with the database if the database is copied or moved. **If a database file is separated from its WAL file, then transactions that were previously committed to the database might be lost, or the database file might become corrupted.**"

> „**The only safe way to remove a WAL file** is to open the database file using one of the `sqlite3_open()` interfaces then immediately close the database using `sqlite3_close()`."

**⚠️ Wniosek: kopiowanie samego pliku `.db` (albo `.db` + `.db-wal` bez transakcyjnej spójności) jest NIEBEZPIECZNE.** To najczęstszy błąd w narzędziach robiących „backup" przez `cp`/`rsync`.

**Trzy bezpieczne metody (w kolejności preferencji dla Vestige):**

| Metoda | Zastosowanie | Uwagi |
|---|---|---|
| **`VACUUM INTO 'plik.db'`** | **Zalecana dla Vestige.** Tworzy kompletny, spójny, **skompaktowany** plik bez WAL-a. Spójny snapshot w momencie rozpoczęcia instrukcji | Plik docelowy **nie może istnieć** (albo musi być pusty); przerwanie zostawia uszkodzony wynik; przy `synchronous` NORMAL/FULL fsync przy zakończeniu; **„all deleted content is purged from the backup, leaving behind no forensic traces"** |
| **Online Backup API** (`sqlite3_backup_*`, w `rusqlite`: `Connection::backup`) | Backup przyrostowy / do już otwartego pliku | ⚠️ **PATRZ OSTRZEŻENIE PONIŻEJ — może się zagłodzić przy wielu pisarzach** |
| **`sqlite3_rsync`** | Nowe w **SQLite 3.47.0 (2024-10-21)**; przez SSH | Trzecia z oficjalnych bezpiecznych metod |
| **`PRAGMA wal_checkpoint(TRUNCATE)` + kopia** | Gdy chcesz fizycznej kopii pliku | Wymaga **okna bez czytelników** — inaczej checkpoint nie dobiegnie końca i kopia będzie niespójna |

**Oficjalne stanowisko SQLite:** istnieją **dokładnie trzy** bezpieczne metody — `sqlite3_rsync`, `VACUUM INTO` i backup API. Kopiowanie pliku jest bezpieczne **„as long as there are no transactions in progress"**. Uzasadnienie, dlaczego kopie „na żywo" są niebezpieczne: **„The backup copy then might contain some old and some new content, and thus be corrupt."** ([sqlite.org/howtocorrupt.html §1.2](https://sqlite.org/howtocorrupt.html))

> 🚩 **KRYTYCZNE OSTRZEŻENIE DOTYCZĄCE BACKUP API — dokładnie w scenariuszu Vestige.**
> Z [sqlite.org/backup.html](https://sqlite.org/backup.html): przy użyciu online backup API
> > „writes to a file-based source database **by an external process or thread using a database connection other than pDb** are significantly more expensive… **If the backup process is restarted frequently enough it may never run to completion and the backupDb() function may never return.**"
>
> → **Backup API może się ZAGŁODZIĆ (livelock) przy wielu klientach MCP piszących do tej samej bazy.** To jest bezpośrednia konsekwencja architektury „Claude Desktop + Cursor + dashboard na jednym pliku".
> → ✅ **`VACUUM INTO` NIE ma tego problemu** — robi spójny snapshot w momencie rozpoczęcia instrukcji. **To jest rozstrzygający argument za `VACUUM INTO` jako domyślną metodą backupu w Vestige.**

**Rekomendacja praktyczna dla Vestige** (⚠️ moja synteza):
1. Backup przez **`VACUUM INTO`** do pliku tymczasowego (ścieżka, która **nie istnieje**).
2. **Weryfikacja**: `PRAGMA integrity_check` na pliku wynikowym + porównanie liczników wierszy (`system_status` już to raportuje).
3. **Restore-verified:** backup, którego nie odtworzono, nie jest backupem. ⚠️ To nie jest tylko dobra praktyka — **RODO art. 32(1)(d) wymaga „a process for regularly testing, assessing and evaluating the effectiveness of technical and organisational measures"**, a art. 32(1)(c) — **„the ability to restore the availability and access to personal data in a timely manner"**. **Testowanie restore jest obowiązkiem compliance, nie tylko higieną.**
4. Kompresja + (opcjonalnie) szyfrowanie archiwum.
5. Rotacja: trzymaj N kopii, bo backup to nie archiwizacja.
6. **Backup nie zawiera sidecara HNSW** — i dobrze. Sidecar jest odtwarzalny z SQLite. Dokumentuj to, żeby użytkownik nie „odtwarzał" starego sidecara (patrz 8.2a: licznik wierszy nie wykryje rozjazdu).
7. **Przetestuj restore w CI.** Vestige ma binarkę `vestige-restore` — użyj jej na artefakcie z backupu, żeby wykryć regresje formatu.
8. **`VACUUM INTO` czyści usuniętą treść** („no forensic traces") — to jest **właściwość istotna dla RODO**, bo zwykła kopia pliku **zachowuje usunięte wiersze w wolnych stronach**.

**⚠️ Time Machine + SQLite w trybie WAL — prawdziwa, nieudokumentowana szczelina.**
> ⚠️ **Nie istnieje ŻADNA oficjalna dokumentacja Apple ani SQLite** dotycząca backupu Time Machine dla bazy SQLite w trybie WAL. Jedyne znalezione źródło to **wątek na liście dyskusyjnej sqlite-users z 2018**, w którym odpowiadał **D. Richard Hipp**, i wyłoniła się teoria, że Time Machine „captures the sqlite DB files a few milliseconds apart 'sometimes' so that a journal file… is in the TimeMachine backup captured still in its uncommitted state". Towarzyszą temu raporty o odtworzonej bazie **bez pliku `-wal`**, a więc nieotwieralnej.
> **Mitygacja:** backupuj przez `VACUUM INTO` do **pojedynczego pliku** (bez `-wal`/`-shm`, z wyczyszczoną usuniętą treścią) i **wyklucz żywy WAL z Time Machine**.
> ⚠️ `fdesetup status`, `tmutil addexclusion`, `NSURLIsExcludedFromBackupKey` — **niezweryfikowane względem dokumentacji Apple w tej sesji.**

**Litestream — ⚠️ KOREKTA WCZEŚNIEJSZEJ UWAGI: jest znowu utrzymywany.**
- Nagłówek strony: **„v0.5.x — Latest — Actively maintained with new features and bug fixes"**.
- Historia: Fly.io „paused development on Litestream for almost two years in favor of LiteFS"; Ben Johnson wrócił do projektu ok. **października 2025** i wydał **v0.5.0** ([mtlynch.io, 2025-10-14, akt. 2025-10-17](https://mtlynch.io/litestream/)).
- ⚠️ **v0.5.0 NIE potrafi odtworzyć backupów z 0.3.x** (zmiana formatu); `replicas:` (tablica) → `replica:` (słownik).
- **Mechanizm** ([litestream.io/how-it-works](https://litestream.io/how-it-works)): **„takes over the checkpointing process. It starts a long-running read transaction to prevent any other process from checkpointing."**
  → ⚠️ **To jest w napięciu architektonicznym z uruchamianiem innych pisarzy na tym samym pliku** (długo żyjąca transakcja czytająca = **checkpoint starvation**, patrz 8.1), a **żaden z projektów nie dokumentuje tego połączenia**.
- Kompaktowanie: **L0 ciągle, L1 co 30 s, L2 co 5 min, L3 co 1 h, snapshot co 24 h**; `max-sync-wal-bytes` **64 MiB**.
- ⚠️ **Litestream nie publikuje żadnej liczbowej gwarancji RPO/RTO** — tylko mechanizm.
- **Rekomendacja dla Vestige:** `VACUUM INTO` w cronie jest prostszy i nie wprowadza konfliktu z wieloma pisarzami. **Litestream rozważ tylko jeśli przejdziesz na wzorzec jednego pisarza** (8.2), i wtedy z świadomością przejęcia kontroli nad checkpointami.

**PITR:** ⚠️ **SQLite sam w sobie NIE MA point-in-time recovery ani archiwizacji WAL.** Istnieją tylko: łańcuch TXID Litestream, D1 Time Travel (⚠️ **30 dni Paid / 7 dni Free; 10 restore'ów na 10 min**) albo własne snapshoty + changelog. Vestige ma `memory_changelog` — to jest dobry fundament pod własny PITR, ale **nie jest to PITR bazy**.

**Luka w standardzie MCP:** ⚠️ specyfikacja MCP `2026-07-28` **nie mówi nic** o backupie, trwałości ani odtwarzaniu danych serwera. To w całości odpowiedzialność implementacji. Nie ma wzorca `resources`-owego na backup.

### 8.4 Szyfrowanie at-rest

**Opcje i licencje:**

| Opcja | Licencja | Komercyjne? | Uwagi |
|---|---|---|---|
| **SQLCipher Community Edition** | **BSD-style** | ✅ **TAK** | ⚠️ **Wymaga atrybucji**: „requires attribution and reproduction of license grants **in the application interface and/or materials**. This can be in an about or licensing screen in the application, in the product documentation on a website linked from the application, **but must be a user accessible location**." |
| SQLCipher Commercial Edition | Komercyjna | ✅ (płatna) | „enhanced performance, feature extensions, and private support" |
| SQLCipher Trial | Trial | ❌ | „prohibits redistribution or use in production environments" |
| SQLite SEE | Komercyjna | ✅ (płatna) | Niezbadane w tej sesji |
| FileVault (macOS) | Systemowa | ✅ | Cały dysk; nie chroni przed innym procesem tego samego użytkownika |
| LUKS / fscrypt (Linux) | Systemowa | ✅ | Jak wyżej |

Źródło: [SQLCipher License Information](https://www.zetetic.net/sqlcipher/license/) (© 2026 Zetetic LLC).

**✅ Dobre wieści dla Vestige:** projekt ma już feature `encryption = ["rusqlite/bundled-sqlcipher"]` w `crates/vestige-core/Cargo.toml`. To jest właściwa droga:
- `rusqlite` z feature `bundled-sqlcipher` daje SQLCipher CE.
- Licencja BSD-style jest kompatybilna z użyciem komercyjnym.
- ⚠️ **Trzeba dodać ekran/sekcję atrybucji** — to jest wymóg licencji, nie dobra praktyka. Bez tego naruszasz warunki.
- ⚠️ **`bundled-sqlcipher` nadal wymaga `bundled`** — i to jest dodatkowy argument za utrzymaniem `bundled` (patrz 8.1: chroni też przed WAL-reset bugiem).

**Czego szyfrowanie at-rest NIE załatwia:**
- **Nie chroni, gdy serwer już działa** — proces ma klucz w pamięci i odszyfrowane strony. At-rest chroni przed kradzieżą dysku / kopii backupu, nie przed lokalnym atakującym z tymi samymi uprawnieniami.
- **Nie chroni modelu embeddingów.** Wektory są danymi wyprowadzonymi z treści; ⚠️ **nie znalazłem żadnego opublikowanego badania o tym, czy z wektorów embeddingów da się odtworzyć oryginalny tekst** (inwersja embeddingów). Traktuj wektory jako **równie wrażliwe jak treść** — jeśli szyfrujesz `.db`, szyfruj też pliki embeddingów/sidecar.
- **Nie rozwiązuje „prawa do usunięcia"** — patrz 8.5.

**Gdzie trzymać klucz (macOS):** macOS Keychain przez crate `security-framework` albo `keyring`. ⚠️ **NIEPOTWIERDZONE:** nie zbadałem w tej sesji, jak `rusqlite` przyjmuje klucz (`PRAGMA key`) ani czy istnieje bezpieczniejszy wzorzec niż przekazanie go jako string. **Sprawdź dokumentację `rusqlite::Connection::pragma_update` i SQLCipher docs przed wdrożeniem.**

### 8.5 Data residency i „prawo do usunięcia" w indeksie wektorowym

**Kontekst regulacyjny (⚠️ to nie jest porada prawna — to odczyt źródeł pierwotnych):**

- **RODO art. 2(2)(c)** wyłącza przetwarzanie „by a natural person in the course of a **purely personal or household activity**".
  > 🚩 **ALE Recital 18 kończy się zdaniem:** „**However, this Regulation applies to controllers or processors which provide the means for processing personal data for such personal or household activities.**"
  > → **Local-first usuwa problem rezydencji danych i problem procesora, ale NIE usuwa obowiązków vendora z art. 25 / 32 ani obowiązków przejrzystości.**
- **RODO art. 3(1)** stosuje się „**regardless of whether the processing takes place in the Union or not**" — vendor z siedzibą w UE **nie ucieka** od RODO przez uczynienie przetwarzania lokalnym.
- **CJEU *Ryneš*, C-212/13 (2014-12-11)** zawęża wyłączenie „gospodarstwa domowego" (kamera domowa obejmująca przestrzeń publiczną ≠ działalność domowa).
- 🚩 **Specyficzne ryzyko dla Vestige:** pipeline ekstrakcji encji **celowo buduje system ewidencji osób trzecich** (`entity:john-smith`, typ węzła `person`, `node_type: person`). To jest decyzja projektowa **na słabym końcu wyłączenia „gospodarstwa domowego"** — nie jest to „prywatna notatka", to zorganizowany rejestr.
- **art. 5** — sześć zasad + rozliczalność. **art. 17** — wyczerpujący katalog przesłanek; wyjątek 17(3) obejmuje cele archiwalne/naukowe z art. 89(1) „in so far as the right… is likely to render impossible or seriously impair the achievement of the objectives" — **to jedyny możliwy haczyk dla retencji w indeksie i jest wąski; nie opieraj się na nim.**
- **art. 32** — „state of the art", „pseudonymisation and encryption of personal data", oraz krytycznie:
  - **32(1)(c)** „the ability to **restore the availability and access** to personal data in a timely manner";
  - **32(1)(d)** „a process for **regularly testing, assessing and evaluating the effectiveness** of technical and organisational measures".
  > 🚩 **To jest dosłownie obowiązek weryfikowania, że restore działa.** A w Vestige trigger `needsBackup` **zależy od agenta** — użytkownik, który nigdy nie otworzy agenta honorującego `automationTriggers`, **nigdy nie dostanie backupu**. **To jest luka compliance, nie tylko UX.**
- **art. 25(2)** — domyślnie tylko dane niezbędne; „applies to the amount of personal data collected, the extent of their processing, **the period of their storage** and their accessibility" — **w napięciu z bezterminową retencją**; mitygacją jest `gc(min_retention)` i aktywne zapominanie (FSRS-6 retention decay).

**⚠️ Czy embeddingi są danymi osobowymi? Najmocniejszy dostępny dowód: TAK, mogą być.**
Morris / Kuleshov / Shmatikov / Rush, [**arXiv:2310.06816**](https://arxiv.org/abs/2310.06816), **2023-10-10**, **EMNLP 2023**:
> „a multi-step method… is able to recover **92% of 32-token text inputs exactly**" oraz „can recover important personal information (**full names**) from a dataset of clinical notes."

> 🚩 **Zastrzeżenia, które MUSZĘ zgłosić:** 92% dotyczy **wyłącznie wejść 32-tokenowych**, a abstrakt **nie nazywa dwóch testowanych modeli embeddingowych**. **NIE generalizuj tego na długie wspomnienia ani na `nomic-embed-text-v1.5`.** ⚠️ **Nie znaleziono żadnego stanowiska EDPB/DPA klasyfikującego embeddingi jako dane osobowe.**
> **Obronna pozycja inżynierska:** traktuj embeddingi jako **pseudonimizowane dane osobowe** wg testu z Recitalu 26 („means reasonably likely to be used").

**⚠️ Kluczowy problem techniczny: usunięcie wiersza z SQLite NIE usuwa wektora z indeksu HNSW.**

✅ **Dobra wiadomość: istnieje recenzowana praca, która to rozstrzyga** (patrz 8.2a): [arXiv:2512.06200](https://arxiv.org/abs/2512.06200), NeurIPS 2025 Workshop. **Lazy (tombstone) deletion działa, ale degraduje recall LINIOWO; okresowa przebudowa to praktyczna odpowiedź.**
> ⚠️ **Żadne źródło nie twierdzi, że przebudowa jest prawnie wymagana.** ⚠️ **Żadna znaleziona praca nie adresuje, czy usunięty embedding jest ODWTRACALNY z resztkowej struktury grafu HNSW** — to jest otwarte pytanie.
> **Odczyt compliance:** **tombstone z zachowanym wektorem to przypadek ryzykowny** (wektor nadal tam jest, a powyżej wykazano, że wektor może być danymi osobowymi). **Vector-purge + tombstone + okresowa przebudowa to wariant obronny.**

**Praktyczna konsekwencja dla Vestige:** `memory(action="delete")` usuwa wiersz z SQLite, ale **wektor może nadal istnieć w sidecarze HNSW**. Bez przebudowy „usunięta" treść nadal wpływa na wyniki wyszukiwania (jako sąsiad w grafie) albo przynajmniej pozostaje na dysku.

**Rekomendacja (3 elementy):**
1. Przy `delete` **fizycznie usuwaj wektor** (nie tylko oznaczaj) **oraz** zostawiaj tombstone ID, i **filtruj wyniki ANN po ID** przed zwróceniem.
2. Udostępnij operację **`compact`/`rebuild index`**, która faktycznie odtwarza graf z SQLite — i **wywołuj ją okresowo** (nie tylko przy niezgodności licznika wierszy). Vestige już ma rebuild — **zrób z niego operację zgodności z RODO z jawnym wyzwalaczem.**
3. W dokumentacji **nie twierdź, że `delete` usuwa dane w pełni**, dopóki rebuild się nie wykona. ⚠️ Vestige ma moduł `storage/sqlite/gdpr.rs` — **audytuj, który z dwóch wariantów realizuje.**

**Szyfrowanie a usunięcie:** przy SQLCipher usunięcie wiersza nie nadpisuje stron — dane pozostają w wolnych stronach pliku do czasu `VACUUM`. **Po żądaniu usunięcia wykonaj `VACUUM`**, jeśli zależy ci na fizycznym usunięciu. ✅ `VACUUM INTO` przy backupie **czyści usuniętą treść automatycznie** („no forensic traces") — to jest właściwość, którą warto udokumentować.

**⚠️ MCP a prywatność — specyfikacja milczy, i to trzeba powiedzieć wprost.**
Zweryfikowałem [Security Best Practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md) (protokół `2026-07-28`, HTTP 200): dokument jest **w całości o autoryzacji** — „complementing the MCP Authorization specification… primary audience includes developers implementing MCP authorization flows". Wyczerpujące przeszukanie pod `storage|persist|secret|credential|PII|personal data|at rest|encrypt|privacy|GDPR|minimi[sz]` daje **tylko**: „encrypted cookie" (stan CSRF/sesji), „sensitive locations (home directory, SSH keys, system directories)" i „log leakage, memory scraping, or local interception".
> **Nie ma ŻADNYCH normatywnych wytycznych MCP o szyfrowaniu at-rest, PII, retencji, usuwaniu ani sekretach po stronie serwera.**
> → 🚩 **Vestige NIE POWINIEN twierdzić, że jego postawa dot. danych at-rest jest „zgodna ze specyfikacją MCP" — nie ma z czym być zgodnym.** Można twierdzić zgodność z RODO (jeśli faktycznie jest), ale nie z MCP.

**Normatywne zdania, które MCP faktycznie ma i które mają zastosowanie:**
1. „MCP servers that implement authorization **MUST** verify all inbound requests. **MCP servers MUST NOT treat possession of a state handle as authentication.**"
2. „MCP servers **SHOULD** use secure, non-deterministic handles… Avoid predictable or sequential identifiers."
3. „MCP servers **SHOULD** bind handles server-side to the authenticated user… keying stored state as `<user_id>:<handle>`."
4. „MCP servers **MUST NOT** accept any tokens that were not explicitly issued for the MCP server." (token passthrough „explicitly forbidden")
5. **Wytyczne dla serwerów lokalnych (najistotniejsze dla Vestige):** „**Use the `stdio` transport to limit access to just the MCP client**; Restrict access if using an HTTP transport, such as: Require an authorization token; **Use unix domain sockets or other Interprocess Communication (IPC) mechanisms with restricted access.**"
6. Strona klienta: „**Warn that MCP servers run with the same privileges as the client**"; „Execute MCP server commands in a sandboxed environment"; „Launch MCP servers with restricted access to the file system, network, and other system resources."
7. Nota: spec `2026-07-28` jest explicytnie **bezstanowa** — **daemon musi sam kluczować swój stan** (nie ma sesji protokołu, na której mógłby się oprzeć).

**Konflikt z MCP (powtórzenie, bo istotne):** ⚠️ specyfikacja MCP nie definiuje **żadnego** mechanizmu eksportu ani usunięcia danych użytkownika z serwera. Vestige ma własne narzędzia `export` i `memory(action="delete")` — to jest dobra praktyka, ale **poza standardem**, więc nie da się jej „certyfikować" jako MCP-conformant.

### 8.6 ⚠️⚠️ LICENCJE MODELI — ZIDENTYFIKOWANA MINA

To jest najpoważniejsze znalezisko tej sekcji po WAL-reset bugu.

| Model | Licencja | Komercyjne użycie? |
|---|---|---|
| **nomic-embed-text-v1.5** | **Apache-2.0** | ✅ **TAK, bez ograniczeń** |
| **Jina Reranker v2 Base Multilingual (278M)** | **CC-BY-NC-4.0** | ❌ **NIE — NON-COMMERCIAL** |

**Dowód dla Jina — cytat z model card** ([huggingface.co/jinaai/jina-reranker-v2-base-multilingual](https://huggingface.co/jinaai/jina-reranker-v2-base-multilingual), pole front-matter: `license: cc-by-nc-4.0`):

> **„This model repository is licenced for research and evaluation purposes under CC-BY-NC-4.0. For commercial usage, please refer to Jina AI's APIs, AWS Sagemaker or Azure Marketplace offerings. Please contact us for any further clarifications."**

**Konsekwencje, które trzeba wypowiedzieć wprost:**
1. **Vestige (AGENTS.md) deklaruje użycie „Jina Reranker v2 Base Multilingual (278M params) cross-encoder" jako lokalnego rerankera.** Przy licencji **CC-BY-NC-4.0** to jest **naruszenie licencji w każdym użyciu komercyjnym**.
2. **Dystrybucja binarki, która pobiera te wagi, też jest problematyczna** — CC-BY-NC-4.0 zabrania użycia „primarily intended for or directed towards commercial advantage or monetary compensation". Nawet jeśli Vestige sam nie jest komercyjny, **użytkownicy komercyjni** byliby w naruszeniu.
3. **Nie ma tu „furtki" Apache-2.0.** CC-BY-NC-4.0 to licencja Creative Commons, nie software'owa; „NC" jest twarde.
4. **Opcje wyjścia:**
   - (a) **Zamienić model** na permissywnie licencjonowany reranker. ⚠️ **Nie zweryfikowałem w tej sesji konkretnych alternatyw** — sprawdź np. `bge-reranker-v2-m3` (jest w tabeli porównawczej Jina, ale **jego licencji nie sprawdziłem**), albo modele Apache-2.0/MIT.
   - (b) **Uczynić reranker opcjonalnym**, domyślnie wyłączonym, z jawnym ostrzeżeniem licencyjnym przy włączaniu.
   - (c) **Użyć API Jiny** dla użytkowników komercyjnych — ale to łamie local-first i przenosi dane poza urządzenie (patrz 8.5).
   - (d) Uzyskać komercyjną licencję od Jina AI.
5. **Rekomendacja:** (b) natychmiast jako mitygacja, (a) jako docelowe rozwiązanie.

**nomic-embed-text-v1.5 — dobre wieści i konkretne liczby** ([model card](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5), `license: apache-2.0`):
- **Apache-2.0** — pełna swoboda komercyjna, także redystrybucja wag i binarki.
- **Matryoshka Representation Learning** — tabela z model card:

| Wariant | SeqLen | Wymiar | MTEB |
|---|---|---|---|
| nomic-embed-text-v1 | 8192 | 768 | **62,39** |
| nomic-embed-text-v1.5 | 8192 | 768 | 62,28 |
| nomic-embed-text-v1.5 | 8192 | **512** | **61,96** |
| nomic-embed-text-v1.5 | 8192 | **256** | **61,04** |
| nomic-embed-text-v1.5 | 8192 | 128 | 59,34 |
| nomic-embed-text-v1.5 | 8192 | 64 | 56,10 |

  → **Kluczowa obserwacja:** zejście z 768 do **256** wymiarów kosztuje **1,24 punktu MTEB (62,28 → 61,04)** przy **3× mniejszym** wektorze. ⚠️ **Vestige używa 384 wymiarów, a model card NIE podaje wartości MTEB dla 384** — trzeba ją interpolować (między 61,96 dla 512 a 61,04 dla 256), co daje ~61,5. **To interpolacja, nie pomiar.**
- **Wymagane prefiksy zadań** — cytat: „the text prompt *must* include a *task instruction prefix*". Prefiksy: `search_document:`, `search_query:`, `clustering:`, `classification:`. **Vestige ma `VESTIGE_NOMIC_PREFIXES` domyślnie `off`** — ⚠️ **to znaczy, że domyślnie model jest używany niezgodnie z instrukcją producenta, co może obniżać jakość retrievalu.** Model card mówi „must", nie „should". Rozważ zmianę domyślnej wartości na `on` (wymaga `regenerate_embeddings`).
- **Kontekst 8192** — natywnie; powyżej 2048 trzeba ustawić `rope_parameters` z `rope_theta: 1000.0`, `rope_type: "dynamic"`, `factor: 2.0`.
- **Multimodalność:** `nomic-embed-vision-v1.5` jest wyrównany do tej samej przestrzeni embeddingów.

**📌 Konsekwencja licencyjna dla Vestige:** skoro **embedder jest Apache-2.0 a reranker CC-BY-NC-4.0**, to **Vestige jako całość ma problem licencyjny wyłącznie z powodu rerankera.** Usunięcie lub opcjonalizacja rerankera **rozwiązuje cały problem** — embedder jest czysty.

⚠️ **NIEPOTWIERDZONE:** nie zweryfikowałem licencji **ONNX Runtime** w źródle pierwotnym. Raport subagenta podaje, że **ONNX Runtime jest MIT, © Microsoft — bezpieczny komercyjnie**, a `ort` jest **MIT OR Apache-2.0**, `usearch` **Apache-2.0**. ✅ Przyjmij to jako roboczo poprawne, ale **potwierdź przed publikacją binarki**.

**⚠️ Pamiętaj: „fastembed wspiera model X" NIE JEST oświadczeniem licencyjnym o X.** Licencja Apache-2.0 fastembed obejmuje **kod, nie wagi**. To samo dotyczy „pobierania wag w czasie działania" — **CC-BY-NC ogranicza UŻYCIE, nie tylko dystrybucję**, więc pobieranie na żądanie **nie jest** obejściem licencji.

**nomic-embed-text-v2-moe — istnieje, nowszy, też Apache-2.0, ALE ⚠️ uwaga:**
- **„Maximum Sequence Length: 512 tokens"** — **16× krótszy kontekst niż v1.5 (8192)**. Dla długich wpisów pamięci to prawdopodobnie **dyskwalifikujące**.
- MoE: 475M parametrów łącznie / 305M aktywnych; BEIR 52,86 / MIRACL 65,80.
- ⚠️ **fastembed wspiera go TYLKO za feature'em `nomic-v2-moe` na backendzie Candle, nie ONNX.** Vestige ma ten feature zdefiniowany (`nomic-v2 = ["embeddings", "fastembed/nomic-v2-moe"]`) — **jeśli go włączysz, wychodzisz ze ścieżki ONNX.**

**⚠️ Wydajność na Apple Silicon — znalezisko, które warto potwierdzić pomiarem.**
Feature'y akceleracji w fastembed to `directml`, `cuda`, `cudnn`, `mkl`, `metal`, `accelerate` — **ale `metal` i `accelerate` są oba zależne od backendów Candle (`qwen3`, `nomic-v2-moe`), a `CoreML` NIE MA w ogóle.**
→ **Ścieżka ONNX dla `nomic-embed-text-v1.5` w Vestige działa na CPU execution provider ONNX Runtime — bez Metal, bez CoreML, bez ANE.** `ort` sam ma feature `coreml`, ale **fastembed go nie eksponuje**. Włączenie CoreML wymaga patchowania fastembed albo użycia `ort` bezpośrednio.
> ⚠️ **Nie istnieje żaden opublikowany benchmark lokalnego ONNX dla tych modeli** — ani zużycia pamięci, ani latencji zimnego startu, ani porównania CPU vs CoreML. Strona wydajności ONNX Runtime to **spis treści bez liczb**. `ort` README mówi „super quick" bez żadnych wartości.
> **⚠️ Szacunek arytmetyczny (wyraźnie oznaczony jako OBLICZENIE, nie pomiar):** nomic v1.5 ≈ **547 MB fp32** / ≈137 MB int8; **Jina Reranker v2 ≈ 1,11 GB fp32** / ≈278 MB int8. **Reranker ~1 GB ładowany w tym samym procesie co serwer MCP to poważny problem dla „local-first desktop app".** **Zmierz lokalnie i opublikuj liczbę** — to jest brakujące dane, którego nikt nie dostarczył.

**⚠️ ROZBIEŻNOŚĆ WERSJI DO WYJAŚNIENIA:** raport subagenta podaje **`fastembed 7.0.1` (2026-09-16)** i **`ort` przypięty na `=2.0.0-rc.13`**, podczas gdy `Cargo.lock` w tym repo ma **`fastembed 5.13.4`** i **`ort 2.0.0-rc.12`**. ⚠️ **Sprawdź `cargo update --dry-run -p fastembed`** — albo lockfile jest nieaktualny względem intencji, albo subagent raportował najnowszą dostępną wersję, nie używaną.
> ⚠️ **Kadencja wydań fastembed jest tygodniowa, z podbiciami major:** 6.0.1 (2026-08-23), 6.0.2, 6.0.3, 6.1.0 (2026-09-12), 7.0.0 i 7.0.1 (2026-09-16). **Przypnij dokładną wersję** — inaczej `cargo update` może cicho zmienić zachowanie.
> ✅ `fastembed` **nie vendoruje ONNX Runtime**: `default = ["ort-download-binaries-native-tls", …]` — **pobiera prebuilt binaria ORT i wagi modeli w czasie działania** z HuggingFace/pyke.io. Ma to konsekwencje dla środowisk offline, reproducibility buildów i dla audytu łańcucha dostaw.

**📌 Wymogi compliance licencyjnego (do wykonania przed publikacją):** dla Apache-2.0 — tekst licencji + plik `NOTICE` + „state significant changes" + klauzula patentowa; dla MIT — notice; dla BSD-3 SQLCipher — **dodatkowo widoczna dla użytkownika atrybucja Zetetic** (patrz 8.4) i zakaz endorsements. Dla CC-BY-NC-4.0 (Jina) — **brak ścieżki komercyjnej bez umowy z Jina AI.**

### 8.6a Zarządzanie kluczami — pułapka z pętlą w tle

⚠️ **To jest subtelny, ale realny błąd, który objawia się jako „konsolidacja czasem nie działa".**

Jeśli klucz szyfrujący trzymasz w **macOS Keychain**, domyślna klasa dostępności `WhenUnlocked` powoduje, że **odczyt klucza zawodzi, gdy ekran jest zablokowany**.
→ 🚩 **Vestige ma pętlę konsolidacji co 6 godzin (`VESTIGE_CONSOLIDATION_INTERVAL_HOURS`) działającą w tle.** Przy domyślnej klasie dostępności **ta pętla będzie cicho zawodzić, gdy użytkownik ma zablokowany ekran** — czyli dokładnie wtedy, gdy komputer jest bezczynny i konsolidacja powinna się wykonywać. **Objaw będzie mylący: `needsDream` będzie rósł mimo działania serwera.**
**Rekomendacja:** użyj klasy **`kSecAttrAccessibleAfterFirstUnlock`** (albo `…AfterFirstUnlockThisDeviceOnly`) zamiast domyślnej `WhenUnlocked`. ⚠️ **Niezweryfikowane względem dokumentacji Apple w tej sesji** — potwierdź przed wdrożeniem.

### 8.7 Budżet tokenów przy wstrzykiwaniu kontekstu

**Opublikowane dane ilościowe — i WAŻNA KOREKTA powszechnie cytowanego twierdzenia**

Z [arXiv:2504.19413](https://arxiv.org/abs/2504.19413) („Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory", zgłoszony **2025-04-28**, wersja v1 bez rewizji). **Tabela 2 to prawdziwe dane:**

| System | Tokeny | p95 latency | LLM-as-a-Judge |
|---|---:|---:|---:|
| **full-context** | **26 031** | 17,117 s | **72,90%** ⬅️ **NAJWYŻSZY** |
| Mem0 | **1 764** | 1,440 s | 66,88% |
| Mem0^g (graf) | 3 616 | — | 68,44% |
| Zep | 3 911 | — | 65,99% |
| OpenAI | 4 437 | — | 52,90% |
| A-Mem | 2 520 | — | 48,38% |
| LangMem | 127 | p50 17,99 s / p95 59,82 s | — |

> 🚩 **KOREKTA — to jest najczęściej przekłamywana liczba w całej literaturze o pamięci agentów.**
> **Baseline „full-context" OSIĄGNĄŁ NAJWYŻSZY wynik (72,90% vs 66,88% Mem0).** Sam artykuł to przyznaje:
> > „a full-context method that ingests a chunk of roughly 26 000 tokens **still achieves the highest J score (approximately 73%)**."
>
> **Zwycięstwo Mem0 to KOSZT I LATENCJA, nie dokładność.** Cytowanie Mem0 jako „dokładniejszego niż full-context" jest błędnym odczytaniem.
>
> 🚩 Artykuł jest **wewnętrznie niespójny co do latencji**: abstrakt mówi 91%, §4.3 mówi 92%.
> 🚩 Nagłówek „26% ponad OpenAI" jest przeciwko baseline'owi, który sami autorzy uznają za niemierzony rzetelnie: „the OpenAI implementation does not perform memory search… it requires pre-extraction of relevant context, which is not reflected in the reported metrics."
> 🚩 Redukcja tokenów (moje przeliczenie z Tabeli 2): 1764/26031 = **93,2%** dla Mem0; 3616/26031 = **86,1%** dla Mem0^g. Abstraktowe „>90%" jest uczciwe dla bazowego Mem0, ale **zawyżone dla Mem0^g**.

**🚩 KONFLIKT — README mem0 (kwiecień 2026) podaje zupełnie inne liczby:** LoCoMo 71,4 → **92,5**, LongMemEval 67,8 → **94,4**, BEAM(1M) 64,1, przy **7,0K tokenów / 0,88 s p50** (vs 1 764 tokenów w artykule). README **explicytnie ujawnia**: „Scores reflect **Mem0's managed platform, which includes proprietary optimizations not available in the open-source SDK**", a metodologia to „Single-pass retrieval… at a **top_200** retrieval budget".
→ **Cytuj 7,0K / 92,5 dla obecnego produktu; NIE mieszaj liczb z artykułu i z README.** To dwa różne systemy i dwie różne metodologie.

**✅ Zep — najczystsze liczby redukcji tokenów i, inaczej niż Mem0, wygrywa też dokładnością.**
Rasmussen et al., [arXiv:2501.13956](https://arxiv.org/abs/2501.13956), zgłoszony **2025-01-20**. **LongMemEval, Tabela 2:**

| Konfiguracja | Dokładność | Czas | Tokeny kontekstu |
|---|---:|---:|---:|
| full-context / gpt-4o-mini | 55,4% | 31,3 s | **115k** |
| **Zep / gpt-4o-mini** | **63,8%** | **3,20 s** | **1,6k** |
| full-context / gpt-4o | 60,2% | 28,9 s | 115k |
| **Zep / gpt-4o** | **71,2%** | **2,58 s** | **1,6k** |

→ **98,6% mniej tokenów kontekstu (moje przeliczenie), +15,2% / +18,5% dokładności, ~90% redukcji latencji.**
🚩 **Ale:** to także **samoocena vendora**, a **artykuł Mem0 pokazuje Zep PRZEGRYWAJĄCY** (65,99 vs 66,88 / 68,44) na LOCOMO. Różne benchmarki, różne modele, **brak niezależnej replikacji**. Artykuł Zep sam zastrzega, że wyniki DMR są nieistotne: „each conversation contains only 60 messages, easily fit within current LLM context windows."
✅ **Liczba 1,6k tokenów to najlepsza dostępna kotwica dla budżetu wstrzykiwania pamięci i potwierdza domyślne 2 000 tokenów w Vestige jako dobrze uzasadnione.**

**✅ „Lost in the Middle" — najbardziej użyteczna liczba dla projektanta RAG/pamięci.**
Liu et al., [arXiv:2307.03172](https://arxiv.org/abs/2307.03172), v1 **2023-07-06**, v3 2023-11-20, **TACL 2023**.
- Zamknięta księga vs oracle: GPT-3.5-Turbo **56,1% / 88,3%**; Claude-1.3 48,3% / 76,1%.
- Cytat: „GPT-3.5-Turbo's multi-document QA performance **can drop by more than 20%** — in the worst case, performance in 20- and 30-document settings is **lower than performance without any input documents** (closed-book 56,1%)." Krzywa **U-kształtna** (primacy + recency).
- **🎯 NASYCENIE RETRIEVALU — kluczowa liczba:** „model performance **saturates long before retriever recall saturates**… using **50 documents instead of 20** retrieved documents only **marginally improves performance (∼1,5% for GPT-3.5-Turbo and ∼1% for claude-1.3)**."
  → **Przejście z top-20 na top-50 daje ~1–1,5 punktu za 2,5× kontekstu. Próg trafności + twarde top-k w zakresie 10–20 to domyślna wartość uzasadniona dowodami.**
- ⚠️ Testowana mitygacja („query bracketing") daje „near-perfect performance on the synthetic key-value task, but **minimally changes trends in multi-document QA**" — **nie jest ogólnym rozwiązaniem**, wbrew temu, jak często się ją cytuje.
- ⚠️ **Zastrzeżenie:** to praca z 2023 o modelach z 2023 (GPT-3.5, Claude-1.3, MPT-30B). **Ekstrapolacja „środek kontekstu ginie w Claude Sonnet 5 / Opus 5" NIE jest przez nią poparta.**

**✅ Mem0 — najlepsza pojedyncza liczba dla rozmiaru jednostki retrievalu.**
Z Tabeli 2 artykułu, sweep rozmiaru chunka przy k=2, wynik J:
`128 → 59,56` · **`256 → 60,97` (szczyt)** · `512 → 58,19` · `1024 → 50,68` · `2048 → 48,57` · `4096 → 51,79` · `8192 → 60,53`.
Przy k=1 degraduje monotonicznie od 256 (50,15) do 4096 (36,84).
→ **To ilościowy dowód, że jednostki retrievalu rzędu ~256 tokenów biją chunki 1024–4096 tokenów.** To jest **empiryczne potwierdzenie reguły „atomic memory" w Vestige** (jedna myśl na rekord) — i warto to tak zacytować, bo dziś uzasadnienie w AGENTS.md jest jakościowe („degrade search recall by 40-60%", ⚠️ **tej liczby nie zweryfikowałem w źródłach pierwotnych**).

**Anthropic o inżynierii kontekstu** (opublikowane **2025-09-29**, [effective-context-engineering-for-ai-agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)):
- Termin Anthropic to **„context rot"**: „as the number of tokens in the context window increases, the model's ability to accurately recall information from that context decreases… **this characteristic emerges across all models**."
- Zasada nadrzędna: **„finding the smallest possible set of high-signal tokens that maximize the likelihood of some desired outcome."**
- Mechanizm: LLM-y mają **„attention budget"**; „**Every new token introduced depletes this budget by some amount**"; relacje parami n².
- Kształt degradacji: „a **performance gradient rather than a hard cliff**".
- ⚠️ **Anthropic NIE publikuje żadnej liczbowej wartości budżetu tokenów — nie przypisuj im żadnej.**
- ✅ **Anthropic explicytnie endorsuje kształt architektury Vestige:** „the most effective agents might employ a hybrid strategy, **retrieving some data up front for speed, and pursuing further autonomous exploration at its discretion**… CLAUDE.md files are naively dropped into context up front, while primitives like glob and grep allow it to navigate its environment and retrieve files just-in-time."
  → **Mała wstrzyknięta porcja na starcie + tanie, sterowane przez agenta drążenie na żądanie** (czyli `expandable` IDs + `get_batch` w Vestige), **a nie duży zrzut na początku.**
- ⚠️ „Context poisoning / distraction / confusion" **NIE jest taksonomią Anthropic** — to blog Drew Breuniga. Anthropic nazywa tylko „context rot" i „context pollution".

### 8.7a ⚠️⚠️ Prompt caching — strukturalny problem w obecnym kształcie wstrzykiwania

To jest **najwyższej wartości znalezisko inżynierskie tej sekcji**, bo wskazuje na błąd projektowy, nie na brakującą optymalizację.

**Mechanizm cache'owania (cytat dosłowny z [docs.anthropic.com/en/docs/build-with-claude/prompt-caching](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching); ⚠️ dokumentacja **nie ma możliwej do wyodrębnienia daty publikacji**):**
> „Prompt caching references the **entire prompt — tools, system, and messages (in that order)** — up to and including the block designated with `cache_control`."
> „**Because the hash is cumulative**, covering everything up to and including the breakpoint, **changing any block at or before the breakpoint produces a different hash on the next request**."
> Z checklisty troubleshootingu: „Confirm your breakpoint is on a block that stays identical across requests… **if that block changes (timestamps, per-request context, the incoming message), the prefix hash never matches.** The lookback does not find stable content behind the breakpoint."

**🎯 Konsekwencja dla Vestige — dwa problemy, oba poważne:**

1. **🚩 Obecny projekt prawdopodobnie CAŁKOWICIE psuje cache.** `session_context` wstrzykuje **świeżo wyszukaną, zależną od zapytania pamięć** na początku kontekstu (system prompt / wczesne bloki). Ponieważ hash jest **kumulatywny od początku**, prefiks zmienia się **przy każdej turze** → **cache nigdy nie trafia**. Płacisz 1,25× za zapis, którego nikt nie odczyta (a właściwie: płacisz pełną cenę wejścia za cały prefiks przy każdym żądaniu).

2. **🚩 Nawet gdyby cache działał, obecny budżet może być poniżej progu.**

**Minimalna długość promptu podlegająca cache'owaniu — zależy od modelu (dane exact):**

| Minimum | Modele |
|---:|---|
| **512** tokenów | Claude Fable 5.1, Mythos 5.1, Opus 5, Fable 5, Mythos 5 |
| **1 024** | Opus 4.8, Sonnet 5, Sonnet 4.6, Sonnet 4.5, Opus 4.1, Opus 4, Sonnet 4 |
| **2 048** | Mythos Preview, Opus 4.7 |
| **4 096** | Opus 4.6, Opus 4.5 |

⚠️ **Przesłanka z zadania badawczego („1024 lub 2048") jest nieaktualna** — zakres to dziś **512 / 1024 / 2048 / 4096**.

→ `session_context` z `token_budget: 2000` jest **powyżej progu 1 024** dla klasy Sonnet, ale **wyraźnie poniżej progu 4 096** dla Opus 4.5/4.6. **Presety budżetu powinny być świadome modelu.** Anthropic: „If your prompt falls just short of the minimum… **expanding the cached content to reach the threshold is often worthwhile**."

**Pozostałe dane liczbowe o cache (exact):**
- **TTL:** domyślnie **5 minut**, odświeżane bezpłatnie przy każdym użyciu; **1 godzina** przez `"ttl": "1h"`. ⚠️ Subtelność: czas życia liczy się **„from the start of the request that writes or reads the cache entry, not from the end of its response"** — jeśli odpowiedź streamuje się 4 minuty, kolejne żądanie musi wystartować ~1 minutę po jej zakończeniu.
- **Mnożniki kosztu:** zapis 5-min = **1,25×**; zapis 1-godzinny = **2×**; odczyt = **0,1×** (0,025× dla Fable 5.1 / Mythos 5.1). Stackują się z rabatami Batch.
- **Breakpointy: maksymalnie 4** (automatyczny breakpoint zużywa jeden z 4 slotów).
- **Okno lookback: 20 bloków** — „If a growing conversation pushes your breakpoint **20 or more blocks** past the last write, the lookback window misses it. **Add a second breakpoint closer to that position.**"
- **Współbieżność:** „a cache entry **only becomes available after the first response begins**."

**🎯 Cache-compatible kształt kontekstu dla Vestige (⚠️ moja rekomendacja, nie cytat):**
```
[tools]                        ← 28 definicji; MUSZĄ być deterministyczne
[system prompt statyczny]      ← bez timestampów, bez danych per-request
[blok stabilnej pamięci]       ← profil/preferencje; CACHE BREAKPOINT #1, TTL 1h
─────── powyżej breakpointu = cache'owane ───────
[świeżo wyszukany kontekst]    ← sesyjny, zależny od zapytania; NIGDY nie cache'owany
[wiadomość użytkownika]
```
- **Pamięć dwupoziomowa mapuje się naturalnie na dwa breakpointy:** blok długiego TTL (profil, preferencje, decyzje architektoniczne) + wyniki wyszukiwania **po** ostatnim breakpoincie.
- **🚩 KRYTYCZNE: kolejność i treść `tools` są częścią hasha.** Vestige eksponuje 28 narzędzi. **Jeśli `catalog.rs::build_tools_list` emituje niedeterministyczną kolejność (iteracja po `HashMap` w Rust to realne zagrożenie), KAŻDE żądanie chybia cache.** To jest dokładnie ten sam problem, który specyfikacja MCP adresuje wymogiem deterministycznej kolejności `tools/list` — „improves LLM prompt cache hit rates". **Zweryfikuj `build_tools_list` pod kątem stabilności kolejności — to jest darmowy zysk o dużym efekcie.**
- **`session_context` z `include_status`/`include_intentions`/`include_predictions` wstrzykuje dane zmienne w czasie** (liczniki, triggery `needsDream`, `needsBackup`) — **jeśli trafiają przed breakpoint, cache nigdy nie zadziała.** Rozważ rozdzielenie: część stabilna przed breakpointem, część dynamiczna po.

**⚠️ NIEPOTWIERDZONE w tej sekcji:** nie zbadałem samodzielnie dokumentacji prompt caching (polegam na researchu subagenta, który cytuje ją dosłownie); dokumentacja nie ma daty publikacji, więc **nie mogę podać daty źródła** — to jest słabość tego konkretnego źródła. Nie znalazłem też opublikowanych heurystyk „keep injected memory under N tokens" w źródłach pierwotnych — **branża nie opublikowała tu konsensusu**; najlepsze dostępne kotwice to Zep 1,6k, Mem0 1 764 / 7,0K, nasycenie 20→50 z „Lost in the Middle" i szczyt 256-tokenowy Mem0.

**⚠️ Luki badawcze w tym obszarze (nie zmyślaj):**
- **Próg trafności vs top-k oraz MMR/diversity — nie znaleziono NICZEGO ilościowego.** Praca Carbonell & Goldstein o MMR (SIGIR 1998) nie została pobrana. **Żadne źródło nie popiera konkretnej wartości progu** (np. cosine 0,7).
- MemGPT ([arXiv:2310.08560](https://arxiv.org/abs/2310.08560), v1 2023-10-12, v2 2024-02-12) w abstrakcie **nie publikuje żadnych liczb budżetu tokenów** ani progów pressure. ⚠️ **Nie cytuj „progu MemGPT", nie czytając treści.**

**Praktyczne heurystyki budżetu (⚠️ moja synteza, oparta na powyższych źródłach i na tym, co już robi Vestige):**

| Zasada | Uzasadnienie |
|---|---|
| **Trzymaj wstrzykiwany kontekst w setkach–kilku tysiącach tokenów, nie dziesiątkach tysięcy** | Mem0: >90% oszczędności vs full-context; Anthropic: tool search oszczędza >85% i ładuje 3–5 narzędzi |
| **Używaj progu trafności, nie tylko top-k** | Przy stałym top-k przy nieistotnym zapytaniu wstrzykujesz szum, który wypiera inne informacje z kontekstu |
| **Jawny budżet tokenów zamiast stałego limitu liczby rekordów** | Vestige już to robi (`token_budget`); to jest właściwy wzorzec, bo długość rekordów jest bardzo zmienna |
| **Ustaw domyślny budżet, ale pozwól go nadpisać** | `session_context` domyślnie `2000`, `search` `3000` — rozsądne. „Deep context" `3000–5000` dla trudnych zapytań |
| **Sortuj wstrzykiwany kontekst deterministycznie** | ⚠️ Uzasadnienie z MCP: deterministyczna kolejność `tools/list` „improves LLM prompt cache hit rates" — ta sama logika dotyczy wstrzykiwanego kontekstu. Ten sam prefiks = ten sam cache |
| **Nagłówek „co to jest" na początku, szczegóły dalej (progressive disclosure)** | Anthropic code execution: ładuj definicje na żądanie; Vestige używa już `expandable` IDs — to jest ten wzorzec |
| **Nie wstrzykuj pełnych treści, gdy wystarczy streszczenie** | `detail_level: brief \| summary \| full` — `full` tylko do debugowania |

**⚠️ NIEPOTWIERDZONE:** nie zweryfikowałem w tej sesji **progów prompt caching Anthropic** (minimalna liczba tokenów podlegająca cache'owaniu, liczba cache breakpoints). Podane w zadaniu wartości (1024/2048) **nie zostały przeze mnie potwierdzone** — sprawdź [platform.claude.com/docs/en/build-with-claude/prompt-caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching) przed optymalizacją pod cache. Nie znalazłem też opublikowanych heurystyk „keep injected memory under N tokens" w źródłach pierwotnych — branża nie opublikowała tu konsensusu.

### 8.8 Podsumowanie operacyjne sekcji 8 — lista kontrolna

| # | Działanie | Priorytet |
|---|---|---|
| 1 | **Zweryfikować `SQLITE_VERSION` w CI** — musi być ≥ **3.51.3** (WAL-reset bug) | 🔴 **KRYTYCZNY** |
| 2 | **Rozwiązać licencję Jina Reranker v2** — CC-BY-NC-4.0 blokuje użycie komercyjne. Uczynić opcjonalnym natychmiast; docelowo zamienić model | 🔴 **KRYTYCZNY** |
| 3 | **Wprowadzić wzorzec jednego pisarza** — jeden proces-właściciel bazy, resztę read-only albo przez HTTP do właściciela | 🔴 **KRYTYCZNY** |
| 4 | **`PRAGMA busy_timeout`** w każdym połączeniu | 🟠 Wysoki |
| 5 | **Guard na sidecar HNSW** — nigdy nie pisać z dwóch procesów; plik blokady | 🟠 Wysoki |
| 6 | **Backup przez `VACUUM INTO`** (+ `integrity_check`), nigdy `cp` pliku `.db`. Przetestować restore w CI | 🟠 Wysoki |
| 7 | **Rebuild indeksu jako operacja RODO** — `delete` + tombstone + compact, żeby wektory faktycznie znikały | 🟠 Wysoki |
| 8 | **Ekran atrybucji SQLCipher CE** (wymóg licencji) jeśli włączasz `encryption` | 🟡 Średni |
| 9 | **`VESTIGE_NOMIC_PREFIXES=on`** — model card mówi „must include a task instruction prefix" | 🟡 Średni |
| 10 | **`VACUUM` po usunięciach** przy SQLCipher (fizyczne nadpisanie wolnych stron) | 🟡 Średni |
| 11 | **Monitorować rozmiar `-wal`** i logować ostrzeżenie > 100 MB (checkpoint starvation przy długich czytelnikach) | 🟡 Średni |
| 12 | **Zweryfikować licencje ONNX Runtime i usearch** przed publikacją binarki | 🟡 Średni |
| 13 | **Wyłączyć równoległe pętle konsolidacji** przy wielu procesach | 🟡 Średni |
| 14 | **`PRAGMA synchronous`** — udokumentować świadomy wybór (`NORMAL` = brak trwałości po utracie zasilania) | 🟢 Niski |
| 15 | **Nie kłaść bazy na NFS/SMB/chmurze** (WAL nie działa przez sieć) — udokumentować i wykrywać | 🟢 Niski |

---

## Appendix A. Lista kontrolna wdrożenia dla Vestige

⚠️ **To moja synteza na podstawie raportu — nie cytat ze specyfikacji.** Uporządkowana od najwyższego priorytetu.

### A.1 Zgodność protokołu (blokujące dla `2026-07-28`)

- [ ] Zaimplementować **`server/discover`** (MUST) — `supportedVersions`, `capabilities`, `io.modelcontextprotocol/serverInfo` w `_meta`, opcjonalne `instructions`, `ttlMs`, `cacheScope`.
- [ ] Dodać **`resultType: "complete"`** do **wszystkich** wyników.
- [ ] Odczytywać **`_meta["io.modelcontextprotocol/protocolVersion"]`** i **`clientCapabilities`** na każdym żądaniu; brak → `-32602` (HTTP: 400).
- [ ] Walidować nagłówki **`MCP-Protocol-Version`** (musi zgadzać się z `_meta`) i **`Mcp-Method`**; dla `tools/call`/`resources/read`/`prompts/get` także **`Mcp-Name`**. Niezgodność → **400 + `-32020`**.
- [ ] Dodać **`ttlMs`** i **`cacheScope`** do `tools/list` (i `resources/list`, `resources/read`, `prompts/list`, `server/discover`, jeśli używane).
- [ ] **Deterministyczna kolejność** `tools/list` (sortowanie po nazwie) — stała między żądaniami.
- [ ] Zaimplementować **`subscriptions/listen`** z filtrem `toolsListChanged`; **usunąć** GET i `resources/subscribe`.
- [ ] **Usunąć resumability**: ignorować `Last-Event-ID`; ignorować `Mcp-Session-Id`; GET/DELETE → **405**.
- [ ] Zamienić wszystkie samodzielne żądania serwer→klient na **`InputRequiredResult`** (MRTR) z `inputRequests` + `requestState`; wymagać **innego** `id` przy ponowieniu.
- [ ] **Nie emitować** `notifications/message` bez `_meta["io.modelcontextprotocol/logLevel"]`.
- [ ] Usunąć `ping`, `logging/setLevel`, `notifications/roots/list_changed`.
- [ ] Zmienić `-32002` → **`-32602`** (resource not found); akceptować `-32002` od starszych serwerów.
- [ ] Anulowanie: HTTP = zamknięcie strumienia SSE; stdio = `notifications/cancelled`.
- [ ] Rozważyć **tryb dual-era** (obsługa `initialize` + `2026-07-28` na tym samym endpoincie) zamiast flag-day.
- [ ] Rozważyć `x-mcp-header` dla parametrów routingu (np. `tenant`, `codebase`) — **ale nigdy dla sekretów ani PII**.

### A.2 Bezpieczeństwo

- [ ] **stdio:** bez OAuth; poświadczenia z env / Keychain. Zgodne ze spec.
- [ ] **HTTP:** walidacja **`Origin`** (403 przy nieprawidłowym), bind tylko na `127.0.0.1` albo **unix domain socket**, token bearer z pliku `0600`.
- [ ] **Nigdy** nie przekazywać tokenów dalej (token passthrough) — MUST NOT.
- [ ] Uchwyty stanu: nieprzezroczyste, ≤128 bitów entropii (jeśli bez auth), czas życia **opisany w `description`**, walidacja przy każdym wywołaniu, czytelny błąd wygaśnięcia.
- [ ] **Nie dodawać narzędzi dynamicznie** po wywołaniu innego narzędzia (zakazane od `2026-07-28`).
- [ ] Sandboxing: profil Seatbelt (macOS) / bubblewrap + Landlock (Linux); `no-new-privileges`; egress domyślnie zamknięty.
- [ ] Jeśli wystawiasz HTTP poza loopback: pełne **OAuth 2.1 + RFC 9728 + RFC 8707**; CIMD zamiast DCR.

### A.3 Obserwowalność

- [ ] Metryka **`mcp.server.operation.duration`** z kubełkami `[0.01…300]`, atrybuty `mcp.method.name` (Required) i `gen_ai.tool.name`, `error.type`.
- [ ] `error.type = "tool_error"` gdy `CallToolResult.isError == true`.
- [ ] Przy `2026-07-28` HTTP **nie ma sesji** → **nie ustawiaj** `mcp.session.id` (semconv jeszcze tego nie odzwierciedla).
- [ ] **Nie umieszczaj `mcp.resource.uri` w nazwach spanów** domyślnie (kardynalność).
- [ ] Wstrzykiwanie/ekstrakcja **`traceparent`** w `params._meta` **bez prefiksu**.
- [ ] **Domyślnie NIE przechwytuj treści** (`gen_ai.tool.call.arguments` / `.result` tylko opt-in).
- [ ] ⚠️ Jeśli użyjesz `opentelemetry-instrumentation-mcp` — **natychmiast** `TRACELOOP_TRACE_CONTENT=false` (odwrócony default!).
- [ ] Logi na `stderr` (stdio) ze strukturalnym JSON; dla HTTP — OTel, bo `stderr` nie jest przechwytywany.

### A.4 Testowanie

- [ ] Wpiąć oficjalny suite do CI **dla obu rewizji osobno**:
      `--requirements 2026-07-28` oraz `--requirements 2025-11-25` (dual-era wymaga dwóch przebiegów!).
- [ ] Przypiąć wersję `@modelcontextprotocol/conformance` (alpha dla `--requirements`/`tier-check`).
- [ ] Inspector CLI w trzech trybach ery: `legacy`, `auto`, `modern`; używać `--stored-auth-only` w CI.
- [ ] **Dopisać własne testy** dla: `x-mcp-header` (scenariusze oficjalne są `pending` — niepunktowane), limitów rozmiaru ciała, rate limitingu, anulowania, timeoutów, obciążenia (oficjalnie **niepokryte**).
- [ ] Nie testować mirrorowania `Mcp-Param-*` z klienta webowego Inspectora (znana luka — użyj CLI/TUI).

### A.5 Zgodność rate limitingu i limitów

- [ ] Zdefiniować **jawne limity rozmiaru** na: ciało żądania HTTP (np. 1–4 MB → **413**), pojedynczą linię JSON w stdio, głębokość i liczbę subschematów JSON Schema, czas walidacji schematu.
- [ ] Limitować po **nagłówkach** `Mcp-Method` / `Mcp-Name` (nie trzeba parsować ciała) — per-metoda, per-narzędzie, per-token/tenant.
- [ ] **Liczyć w sesjach, nie w żądaniach** — jedno wywołanie narzędzia to w erze legacy ~3–5 POST-ów.
- [ ] Rozważyć Tasks dla `dream`, `reflect --depth deep`, `find_duplicates`, `gc`, `backfill`, `export`.
- [ ] Timeout serwerowy zawsze egzekwowany, nawet przy `notifications/progress`.
- [ ] Rate-limitować `notifications/progress` i `notifications/message`.

### A.6 Zgodność, prawo i licencje

- [ ] **🔴 ROZWIĄzać licencję Jina Reranker v2 (CC-BY-NC-4.0)** przed jakimkolwiek wydaniem komercyjnym. Uczynić opcjonalnym natychmiast; docelowo zamienić model.
- [ ] **🔴 `VESTIGE_NOMIC_PREFIXES=on`** — model card mówi „must include a task instruction prefix". Wymaga `regenerate_embeddings` (mieszanie wektorów z prefiksem i bez **po cichu degraduje recall**).
- [ ] **🔴 Przerobić kolejność wstrzykiwania kontekstu pod prompt caching** — stabilny blok przed breakpointem, zmienny kontekst po nim. Zweryfikować, że `catalog.rs::build_tools_list` emituje **deterministyczną kolejność**.
- [ ] **🔴 Hard-cap retrievalu na 10–20 elementów z progiem trafności** (nasycenie 20→50 z „Lost in the Middle"); jednostki ~256 tokenów (szczyt Mem0).
- [ ] **🟠 Restore-verified backup** — RODO art. 32(1)(c)–(d) to **obowiązek**, nie higiena. Backup nie może zależeć od agenta.
- [ ] **🟠 Sprawdzić klasę dostępności klucza Keychain** — `AfterFirstUnlock`, nie domyślne `WhenUnlocked`, inaczej pętla 6 h cicho zawodzi przy zablokowanym ekranie.
- [ ] **🟠 Nie twierdzić zgodności z MCP dla postawy at-rest** — spec MCP milczy o szyfrowaniu, PII, retencji i usuwaniu. Można twierdzić RODO, nie MCP.
- [ ] **🟡 Ekran atrybucji SQLCipher CE** (wymóg licencji) jeśli włączasz `encryption`.
- [ ] **🟡 `VACUUM` po usunięciach** przy SQLCipher (fizyczne nadpisanie wolnych stron).
- [ ] **🟡 `rebuild` indeksu jako operacja RODO** — tombstone + vector-purge + okresowa przebudowa, nie tombstone z zachowanym wektorem.
- [ ] **🟡 Audyt `storage/sqlite/gdpr.rs`** — czy realizuje wariant obronny, czy ryzykowny.

### A.7 Operacyjne SQLite — lista kontrolna

- [ ] **Monitorować rozmiar `-wal`** i logować ostrzeżenie > 100 MB (**checkpoint starvation** przy długich czytelnikach).
- [ ] **`PRAGMA journal_size_limit`** — domyślnie **-1 (bez limitu)**; ustaw explicytnie.
- [ ] **`BEGIN IMMEDIATE` dla wszystkich zapisów** — udokumentowane lekarstwo na `SQLITE_BUSY` w środku transakcji.
- [ ] **Obsłużyć `SQLITE_BUSY_SNAPSHOT` (517)** — przy przejściu read→write. ⚠️ **NIE da się naprawić retryem na tym samym snapshocie** — trzeba rollback + restart.
- [ ] **`VACUUM INTO` jako domyślny backup** — nie backup API (livelock przy wielu pisarzach), nie `cp`.
- [ ] **Wykluczyć żywy WAL z Time Machine** — ⚠️ brak jakiejkolwiek oficjalnej dokumentacji Apple/SQLite; tylko wątek z 2018.
- [ ] **Zamienić walidację sidecara HNSW z licznika wierszy na monotoniczny licznik zmian / hash treści** — licznik nie wykryje delete+insert.
- [ ] **Atomowy zapis sidecara** (plik tymczasowy + `rename`) — `.hnsw` nie ma atomowości WAL.
- [ ] **FTS5: `merge` zamiast `optimize`** w pętli w tle + ścieżka `rebuild` gdy `integrity-check` zawiedzie.
- [ ] **Nie używać tabel contentless FTS5** jeśli `search` zwraca snippety (psują `snippet`/`highlight`).
- [ ] **Odrzucać / wykrywać systemy plików sieciowych** — WAL nie działa przez NFS/SMB/chmurę.
- [ ] **`PRAGMA synchronous`** — udokumentować świadomy wybór (`NORMAL` = brak trwałości po utracie zasilania, ale zawsze spójność).
- [ ] **Wyłączyć równoległe pętle konsolidacji** przy wielu procesach (podwójna konsolidacja + wyścigi w logice kognitywnej).
- [ ] **Dodać test CI asertujący `SQLITE_VERSION` ≥ 3.51.3** i utrzymać feature `bundled` we wszystkich crate'ach.

---

## Appendix B. Główne źródła (full URL + data)

### Specyfikacja MCP — rewizja bieżąca `2026-07-28`

| Zasób | URL | Data |
|---|---|---|
| Key Changes (changelog) | https://modelcontextprotocol.io/specification/2026-07-28/changelog.md | 2026-07-28 |
| Deprecated Features (rejestr) | https://modelcontextprotocol.io/specification/2026-07-28/deprecated.md | 2026-07-28 |
| Versioning and Compatibility | https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning.md | 2026-07-28 |
| Base protocol / `_meta` / JSON Schema / błędy | https://modelcontextprotocol.io/specification/2026-07-28/basic/index.md | 2026-07-28 |
| Streamable HTTP | https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http.md | 2026-07-28 |
| stdio | https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio.md | 2026-07-28 |
| Cancellation | https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation.md | 2026-07-28 |
| Progress | https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress.md | 2026-07-28 |
| Subscriptions | https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions.md | 2026-07-28 |
| Caching | https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/caching.md | 2026-07-28 |
| Pagination | https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/pagination.md | 2026-07-28 |
| Logging (zdeprecjonowane) | https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/logging.md | 2026-07-28 |
| Tools | https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md | 2026-07-28 |
| Discovery (`server/discover`) | https://modelcontextprotocol.io/specification/2026-07-28/server/discover.md | 2026-07-28 |
| Authorization | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/index.md | 2026-07-28 |
| Client Registration (CIMD) | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration.md | 2026-07-28 |
| Authorization Security Considerations | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations.md | 2026-07-28 |
| Security Best Practices | https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md | 2026-07-28 |
| Debugging | https://modelcontextprotocol.io/docs/2026-07-28/tools/debugging.md | 2026-07-28 |
| Inspector — Protocol eras | https://modelcontextprotocol.io/docs/2026-07-28/tools/inspector/protocol-eras.md | 2026-07-28 |
| SDK Tiering System | https://modelcontextprotocol.io/community/sdk-tiers.md | 2026 |
| Feature Lifecycle and Deprecation Policy | https://modelcontextprotocol.io/community/feature-lifecycle.md | 2026 |
| Roadmap | https://modelcontextprotocol.io/development/roadmap.md | **2026-08-22** |
| Extensions — Tasks | https://modelcontextprotocol.io/extensions/tasks/overview.md | 2026 |
| Extensions — Authorization | https://modelcontextprotocol.io/extensions/auth/overview.md | 2026 |
| Registry — Supported Package Types | https://modelcontextprotocol.io/registry/package-types.md | 2026 |

### SEP-y

| SEP | URL | Status / data utworzenia |
|---|---|---|
| SEP-414 OTel Trace Context | https://modelcontextprotocol.io/seps/414-request-meta.md | Final, 2025-04-25 |
| SEP-986 Format nazw narzędzi | https://modelcontextprotocol.io/seps/986-specify-format-for-tool-names.md | — |
| SEP-991 CIMD | https://modelcontextprotocol.io/seps/991-enable-url-based-client-registration-using-oauth-c.md | — |
| SEP-1024 Wymogi bezpieczeństwa klienta dla lokalnych serwerów | https://modelcontextprotocol.io/seps/1024-mcp-client-security-requirements-for-local-server-.md | **Final, 2025-07-22** |
| SEP-1046 OAuth client credentials | https://modelcontextprotocol.io/seps/1046-support-oauth-client-credentials-flow-in-authoriza.md | — |
| SEP-2243 Standaryzacja nagłówków HTTP | https://modelcontextprotocol.io/seps/2243-http-standardization.md | — |
| SEP-2468 `iss` w odpowiedziach auth | https://modelcontextprotocol.io/seps/2468-recommend-issuer-claim-for-auth.md | — |
| SEP-2484 Testy zgodności wymagane dla SEP-ów | https://modelcontextprotocol.io/seps/2484-conformance-tests-required-for-final-seps.md | Final (Process), **2026-03-27** |
| SEP-2549 TTL dla wyników list | https://modelcontextprotocol.io/seps/2549-TTL-for-list-results.md | — |
| SEP-2567 Sessionless MCP (explicit state handles) | https://modelcontextprotocol.io/seps/2567-sessionless-mcp.md | Final, **2026-03-11** |
| SEP-2575 Make MCP Stateless | https://modelcontextprotocol.io/seps/2575-stateless-mcp.md | Final, 2025-06-18 |
| SEP-2577 Deprecate Roots/Sampling/Logging | https://modelcontextprotocol.io/seps/2577-deprecate-roots-sampling-and-logging.md | Final, **2026-04-14** |
| SEP-2596 Feature Lifecycle | https://modelcontextprotocol.io/seps/2596-spec-feature-lifecycle-and-deprecation.md | — |
| SEP-2663 Tasks Extension | https://modelcontextprotocol.io/seps/2663-tasks-extension.md | — |
| SEP-1821 Dynamic Tool Discovery (issue) | https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1821 | **nie przyjęty** |
| SEP-1300 Tool Filtering with Groups and Tags (issue) | https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1300 | **nie przyjęty** |

### Blogi i ogłoszenia oficjalne

| Zasób | URL | Data |
|---|---|---|
| The 2026-07-28 Specification (blog MCP) | https://blog.modelcontextprotocol.io/posts/2026-07-28/ | **2026-07-28** |
| AAIF — „MCP in production: what changes after the demo works" | https://aaif.io/blog/mcp-in-production-what-changes-after-the-demo-works | **2026-07-31** |

### OTel / semconv

| Zasób | URL | Data |
|---|---|---|
| MCP semconv (kanoniczne, nowe repo) | https://raw.githubusercontent.com/open-telemetry/semantic-conventions-genai/main/docs/gen-ai/mcp.md | Development |
| Nowe repo semconv-genai | https://github.com/open-telemetry/semantic-conventions-genai | utw. 2026-05-05 |
| Stary rejestr atrybutów MCP (Deprecated/Moved) | https://opentelemetry.io/docs/specs/semconv/registry/attributes/mcp/ | — |
| Go `mcpconv` (semconv v1.41.0) | https://pkg.go.dev/go.opentelemetry.io/otel/semconv/v1.41.0/mcpconv | — |
| Python SDK — OpenTelemetry | https://py.sdk.modelcontextprotocol.io/run/opentelemetry/ | 2026 |
| C# SDK Diagnostics.cs | https://raw.githubusercontent.com/modelcontextprotocol/csharp-sdk/main/src/ModelContextProtocol.Core/Diagnostics.cs | 2026 |
| Sentry — MCP servers | https://docs.sentry.io/product/mcp-servers/getting-started.md | 2026 |
| Grafana — AI observability for MCP | https://grafana.com/blog/ai-observability-MCP-servers/ | **2026-03-20** |
| Datadog — monitor MCP client | https://docs.datadoghq.com/llm_observability/guide/monitor_mcp_client/ | ⚠️ brak daty |

### Conformance / testowanie

| Zasób | URL | Data |
|---|---|---|
| Oficjalny suite conformance | https://github.com/modelcontextprotocol/conformance | push 2026-09-14 |
| Requirements `2026-07-28` (zamrożone) | https://raw.githubusercontent.com/modelcontextprotocol/conformance/main/requirements/2026-07-28.yaml | 2026-07-28 |
| Requirements `2025-11-25` | https://raw.githubusercontent.com/modelcontextprotocol/conformance/main/requirements/2025-11-25.yaml | 2025-11-25 |
| Inspector V2 WG charter | https://modelcontextprotocol.io/community/working-groups/inspector-v2.md | przyjęty **2026-04-11** |
| mcpjam (сторонній) | https://github.com/MCPJam/inspector | npm 3.8.1, 2026-09-18 |

### Sandboxing / izolacja

| Zasób | URL | Wersja / data |
|---|---|---|
| Docker MCP Gateway | https://github.com/docker/mcp-gateway | **v0.43.3, 2026-07-16** |
| Docker MCP Gateway — threat model | https://raw.githubusercontent.com/docker/mcp-gateway/main/docs/security.md | 2026 |
| GHSA-r2xf-7jw5-pjg6 (CVE-2026-55887) | https://github.com/advisories/GHSA-r2xf-7jw5-pjg6 | **2026-06-25**, fix v0.42.2 |
| Landlock (dokumentacja jądra) | https://docs.kernel.org/security/landlock.html | **sierpień 2026** |
| bubblewrap 0.12.0 | https://github.com/containers/bubblewrap/releases | **2026-08-26** |
| GHSA-pxhw-h44j-8pfx (bubblewrap) | https://github.com/advisories/GHSA-pxhw-h44j-8pfx | 2026 |
| Wasmtime | https://github.com/bytecodealliance/wasmtime/releases | **48.0.2, 2026-09-10** |
| GHSA-vqjp-4c8c-hfgg (Wasmtime FS escape) | https://github.com/advisories/GHSA-vqjp-4c8c-hfgg | fix 2026-08-20 |
| GHSA-x84v-gj2h-g759 (Wasmtime WASIp3) | https://github.com/advisories/GHSA-x84v-gj2h-g759 | fix 2026-08-20 |
| gVisor | https://github.com/google/gvisor/releases | **20260914.0, 2026-09-16** |
| Extism | https://github.com/extism/extism/releases | **1.30.0, 2026-06-04** |
| MCP-SandboxScan (arXiv) | https://arxiv.org/abs/2601.01241 | v1 2026-01-03, v2 2026-06-22 |

### SQLite / pamięć / licencje / kontekst (sekcja 8)

| Zasób | URL | Data / wersja |
|---|---|---|
| SQLite — Write-Ahead Logging (w tym WAL-reset bug) | https://sqlite.org/wal.html | akt. **2026-08-25** |
| SQLite — How To Corrupt An SQLite Database File | https://sqlite.org/howtocorrupt.html | 2026 |
| SQLite — Online Backup API (ostrzeżenie o livelocku) | https://sqlite.org/backup.html | 2026 |
| SQLite — VACUUM (w tym `VACUUM INTO`) | https://sqlite.org/lang_vacuum.html | 2026 |
| SQLite — PRAGMA (synchronous, journal_size_limit, wal_autocheckpoint) | https://sqlite.org/pragma.html | 2026 |
| SQLite — Vec1 (oficjalne rozszerzenie wektorowe) | https://sqlite.org/vec1 | strona **2026-08-28**, v0.7 |
| „Another look at SQLite's WAL-Reset bug" (Phil Eaton) | https://theconsensus.dev/p/2026/08/23/another-look-at-sqlite-wal-reset.html | **2026-08-23** |
| arXiv:2512.06200 — ANN deletion strategies (NeurIPS 2025 Workshop) | https://arxiv.org/abs/2512.06200 | **2025-12-05** |
| arXiv:2310.06816 — Text Embeddings Reveal (Almost) As Much As Text (EMNLP 2023) | https://arxiv.org/abs/2310.06816 | **2023-10-10** |
| SQLCipher License Information | https://www.zetetic.net/sqlcipher/license/ | © 2026 Zetetic |
| SQLCipher Community Edition | https://www.zetetic.net/sqlcipher/community/ | 2026 |
| Litestream (status „actively maintained") | https://litestream.io/ | 2026 |
| Litestream — How it works (przejęcie checkpointów) | https://litestream.io/how-it-works/ | 2026 |
| mtlynch.io — Litestream maintenance history | https://mtlynch.io/litestream/ | **2025-10-14**, akt. 2025-10-17 |
| nomic-embed-text-v1.5 (Apache-2.0, tabela Matryoshka) | https://huggingface.co/nomic-ai/nomic-embed-text-v1.5 | utw. 2024-02-10, mod. 2026-04-07 |
| jina-reranker-v2-base-multilingual (**CC-BY-NC-4.0**) | https://huggingface.co/jinaai/jina-reranker-v2-base-multilingual | mod. 2025-10-21 |
| arXiv:2504.19413 — Mem0 (Tabela 2, sweep rozmiaru chunka) | https://arxiv.org/abs/2504.19413 | **2025-04-28** |
| arXiv:2501.13956 — Zep (LongMemEval, 1,6k tokenów) | https://arxiv.org/abs/2501.13956 | **2025-01-20** |
| arXiv:2307.03172 — Lost in the Middle (TACL 2023) | https://arxiv.org/abs/2307.03172 | v1 2023-07-06, v3 2023-11-20 |
| arXiv:2310.08560 — MemGPT | https://arxiv.org/abs/2310.08560 | v1 2023-10-12, v2 2024-02-12 |
| Anthropic — Effective context engineering for AI agents | https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | **2025-09-29** |
| Anthropic — Prompt caching (progi 512/1024/2048/4096) | https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching | ⚠️ **brak daty** |
| fastembed-rs | https://github.com/Anush008/fastembed-rs | lockfile 5.13.4; upstream 7.0.1 (2026-09-16) |

### Rate limiting / skala

| Zasób | URL | Data |
|---|---|---|
| agentgateway — Rate limiting for MCP | https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/rate-limit.md | 2026 |
| agentgateway — MCP spec compatibility | https://agentgateway.dev/docs/kubernetes/latest/documentation/mcp/spec-compatibility/ | 2026 |
| MCP-AX (draft indywidualny, bez statusu IETF) | https://datatracker.ietf.org/doc/draft-abbott-mcp-ax/ | **2026-05-04** |
| LiteLLM MCP Gateway — bug paginacji 100 narzędzi | https://github.com/BerriAI/litellm/issues/32229 | 2026 |

### Koszt kontekstu / projektowanie narzędzi

| Zasób | URL | Data |
|---|---|---|
| Anthropic — Introducing advanced tool use | https://www.anthropic.com/engineering/advanced-tool-use | **2025-11-24** |
| Anthropic — Code execution with MCP | https://www.anthropic.com/engineering/code-execution-with-mcp | **2025-11-04** |
| Anthropic — Tool search tool (docs) | https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool | 2026 |
| GitHub — Improving token efficiency in Agentic Workflows | https://github.blog/ai-and-ml/github-copilot/improving-token-efficiency-in-github-agentic-workflows/ | ⚠️ treść nieodczytana bezpośrednio; liczby przez AAIF |
| Cloudflare — Code Mode | https://blog.cloudflare.com/code-mode/ | — |



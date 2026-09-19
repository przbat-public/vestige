# MCP: rewizja specyfikacji 2026-07-28 i poprzedzająca 2025-11-25 — raport faktograficzny

**Data badania:** 2026-09-19
**Metoda:** źródła pierwotne — strony `modelcontextprotocol.io` (wersje `.md`), pliki `schema.ts` / `schema.json` obu rewizji, pliki SEP w repo `modelcontextprotocol/modelcontextprotocol`, blog `blog.modelcontextprotocol.io`, repozytoria `ext-apps` / `ext-tasks` / `ext-server-card`, oraz rejestry pakietów (crates.io, npm, PyPI, NuGet, proxy.golang.org, repo1.maven.org).
**Konwencja:** wszystkie twierdzenia mają URL. Pozycje niezweryfikowane lub sprzeczne oznaczono **⚠️ FLAG**.

---

## 1. Lista rewizji, kanoniczny changelog, najnowsza rewizja

**Najnowsza opublikowana rewizja na 2026-09-19: `2026-07-28` (status „current"). Nie istnieje nic nowszego.**

| Dowód | Źródło |
|---|---|
| „The **current** protocol version is **2026-07-28**" | https://modelcontextprotocol.io/docs/2026-07-28/learn/versioning |
| Selektor wersji w UI docs: „Version 2026-07-28 (latest)" | https://modelcontextprotocol.io/docs/2026-07-28/sdk |
| Changelog draftu jest **pusty**: „Changes since the most recent release will accumulate here." — brak nowej rewizji w przygotowaniu jako opublikowany dokument | https://modelcontextprotocol.io/specification/draft/changelog |
| Roadmap (2026-08-22) mówi o „the next specification release and beyond" **bez daty** | https://blog.modelcontextprotocol.io/posts/mcp-roadmap/ , https://modelcontextprotocol.io/development/roadmap |

**Kanoniczny URL changelogu („Key Changes"):**
`https://modelcontextprotocol.io/specification/2026-07-28/changelog`
(wersja `.md`: `https://modelcontextprotocol.io/specification/2026-07-28/changelog.md`)

Powiązane:
- Pełny diff Git: `https://github.com/modelcontextprotocol/specification/compare/2025-11-25...2026-07-28` (linkowany z changelogu)
- Źródło prawdy schema: `https://github.com/modelcontextprotocol/specification/blob/main/schema/2026-07-28/schema.ts`
- Rejestr funkcji wycofanych: `https://modelcontextprotocol.io/specification/2026-07-28/deprecated`

**Stany rewizji** (Draft / Current / Final) i **stany funkcji** (Active / Deprecated / Removed) — https://modelcontextprotocol.io/docs/2026-07-28/learn/versioning oraz polityka cyklu życia: https://modelcontextprotocol.io/community/feature-lifecycle (SEP-2596).

**Znane rewizje w kolejności:** `2024-11-05` → `2025-03-26` → `2025-06-18` → `2025-11-25` → `2026-07-28`.
Changelog 2025-11-25 (dla porównania): https://modelcontextprotocol.io/specification/2025-11-25/changelog

**⚠️ FLAG:** Nie znalazłem żadnej strony z formalną, wyliczoną „listą rewizji" poza stroną Versioning i archiwum URL-i `/specification/<data>/`. Nie ma osobnego, kanonicznego indeksu rewizji.

---

## 2. Protokół bezstanowy (stateless / sessionless)

**SEP-y źródłowe:**
| SEP | Tytuł | Status | Utworzony | URL |
|---|---|---|---|---|
| **SEP-2575** | *Make MCP Stateless* | Final, Standards Track | 2025-06-18 | https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2575 |
| **SEP-2567** | *Sessionless MCP via Explicit State Handles* | Final, Standards Track | 2026-03-11 | https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567 |

### Co dokładnie usunięto / zmieniono
1. **Usunięto handshake `initialize` / `notifications/initialized`.** „Make MCP stateless: remove the `initialize`/`notifications/initialized` handshake" (changelog, poz. główna 2).
2. **Usunięto nagłówek `Mcp-Session-Id`** i protokolarny koncept sesji. „Remove protocol-level sessions and the `Mcp-Session-Id` header from the Streamable HTTP transport" (changelog, poz. główna 1, SEP-2567).
3. **Usunięto endpoint HTTP GET** oraz **wznawianie strumieni SSE** (`Last-Event-ID`, event ID). „Remove SSE stream resumability and message redelivery… A broken response stream loses the in-flight request; clients **MUST** re-issue it as a new request with a new request ID" (changelog, poz. główna 9).
4. **Listy nie zależą już od połączenia:** „List endpoints (`tools/list`, `resources/list`, `prompts/list`) no longer vary per-connection."
5. **Stan międzywywołaniowy = jawne uchwyty (handles)** przekazywane jako zwykłe argumenty narzędzia. „Servers that need cross-call state use explicit, server-minted handles passed as ordinary tool arguments." Nie ma na to żadnego konstruktu protokolarnego — to wzorzec projektowy narzędzi (SEP-2567).
6. **Zdefiniowano bezstanowość normatywnie:** „Servers **MUST NOT** rely on prior requests over the same connection to establish context (e.g., capabilities, protocol version, client identity)." + „an open connection, such as a STDIO process, is not a conversation or session" — https://modelcontextprotocol.io/specification/2026-07-28/basic/index#statelessness

### Negocjacja wersji (bez handshake'u)
Każde żądanie deklaruje wersję samo. „There is no negotiation handshake. Every request carries its protocol version, and the server accepts or rejects each request independently" — https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning

- Brak wsparcia → `UnsupportedProtocolVersionError`, kod **`-32022`**, `data.supported` (lista) + `data.requested`. Klient **SHOULD** wybrać wspólną wersję i ponowić.
- Terminologia interoperacyjności: **Modern** (= `2026-07-28` i późniejsze, metadane per-request), **Legacy** (= `2025-11-25` i wcześniejsze, handshake), **Dual-era** (obsługa obu).
- Macierz zgodności klient×serwer (6 kombinacji) i mechanika detekcji ery: STDIO → sonda `server/discover` z fallbackiem na `initialize`; Streamable HTTP → nowoczesne żądanie, inspekcja ciała `400 Bad Request` przed fallbackiem. Dual-era serwer może obsługiwać obie ery równocześnie na tym samym endpointcie.

### Pola per-request w `_meta`
Tabela z https://modelcontextprotocol.io/specification/2026-07-28/basic/index#meta:

| Klucz | Typ | Wymagany | Opis |
|---|---|---|---|
| `io.modelcontextprotocol/protocolVersion` | `string` | **Tak** | np. `"2026-07-28"` |
| `io.modelcontextprotocol/clientInfo` | `Implementation` | Nie (SHOULD) | nazwa + wersja klienta |
| `io.modelcontextprotocol/clientCapabilities` | `ClientCapabilities` | **Tak** | zdolności klienta dla tego żądania |
| `io.modelcontextprotocol/logLevel` | `LoggingLevel` | Nie | minimalny poziom logów dla żądania |

- Brak pola wymaganego → `-32602` (Invalid params), a na HTTP status **`400 Bad Request`**.
- Serwer **MUST NOT** polegać na niezadeklarowanych zdolnościach → `MissingRequiredClientCapabilityError` **`-32021`** z `data.requiredCapabilities`, HTTP `400`.
- **Odpowiedzi:** serwer **SHOULD** dołączać `io.modelcontextprotocol/serverInfo` w `_meta` każdego wyniku.
- `clientInfo`/`serverInfo` są **self-reported i nieuwierzytelnione** — „Implementations **SHOULD NOT** use them to change the behavior of the client or server, and **SHOULD NOT** rely on them for security decisions."

### Nagłówki HTTP (SEP-2243)
- **`MCP-Protocol-Version`** — wymagany na każdym POST; **MUST** zgadzać się z `_meta.io.modelcontextprotocol/protocolVersion` (niezgodność → `400` + błąd `HeaderMismatch`).
- **`Mcp-Method`** — wymagany dla wszystkich żądań.
- **`Mcp-Name`** — wymagany dla `tools/call`, `resources/read`, `prompts/get` (źródło: `params.name` lub `params.uri`).
- Niestandardowe nagłówki z parametrów narzędzia: adnotacja **`x-mcp-header`** w `inputSchema` → nagłówek **`Mcp-Param-{Name}`**. Tylko typy prymitywne (integer/string/boolean; `number` **zabroniony**), tylko ścieżki statycznie osiągalne wyłącznie przez `properties`.
- Kodowanie: gdy wartość nie jest bezpieczna jako ASCII → sentinel Base64 `=?base64?{...}?=` (wielkość liter znacząca).
- Niezgodność nagłówków → **`-32020` `HeaderMismatch`**, HTTP `400`.

### Renumeracja kodów błędów
Nowa polityka alokacji: `-32000`–`-32019` = legacy (zastane, nie wolno przydzielać nowych), `-32020`–`-32099` = zarezerwowane dla MCP.
- `HeaderMismatch` `-32001` → **`-32020`**
- `MissingRequiredClientCapability` `-32003` → **`-32021`**
- `UnsupportedProtocolVersion` `-32004` → **`-32022`**
- `-32002` (resource not found) → **`-32602`**; `-32042` (URL elicitation required) usunięty.
→ https://modelcontextprotocol.io/specification/2026-07-28/basic/index#error-codes

**Nowa mandatoryjna RPC: `server/discover`** — patrz sekcja 10.

---

## 3. MRTR (Multi Round-Trip Requests)

**SEP-2322** — *Multi Round-Trip Requests*, Final, Standards Track, utworzony 2026-02-03, autorzy Mark D. Roth, Caitie McCaffrey, Gabriel Zimmerman; sponsor Caitie McCaffrey — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2322
Dokumentacja: https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr

**To jest zmiana łamiąca:** „Servers **MUST** send server-to-client requests (such as `roots/list`, `sampling/createMessage`, or `elicitation/create`) using the MRTR pattern. **The previous pattern of server-initiated requests is no longer supported. This is a breaking change.**"

### Dokładny kształt wyniku
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "resultType": "input_required",
    "inputRequests": {
      "<klucz>": { "method": "elicitation/create", "params": { ... } },
      "<klucz>": { "method": "sampling/createMessage", "params": { ... } }
    },
    "requestState": "AEAD-protected blob"
  }
}
```

| Pole | Typ | Semantyka |
|---|---|---|
| `resultType` | `"input_required"` | Dyskryminator wyniku pośredniego. **Wymagany** na każdym wyniku (SEC-2322): `"complete"` = zwykły wynik. Klient **MUST** traktować brak pola (serwery starsze) jako `"complete"`. Rozszerzenia **MAY** dodawać własne wartości `resultType`. |
| `inputRequests` | `InputRequests` | Mapa: klucze = identyfikatory nadane przez serwer (unikalne w obrębie żądania), wartości = `ElicitRequest` \| `CreateMessageRequest` \| `ListRootsRequest`. Opcjonalne. |
| `requestState` | `string` (opaque) | Opcjonalne. „Clients **MUST NOT** inspect, parse, modify, or make any assumptions about its contents." Klient **MUST** odesłać dokładnie tę samą wartość przy ponowieniu; jeśli serwer jej nie przysłał — **MUST NOT** dołączać. |

**Odpowiedź klienta:** `inputResponses` (mapa klucz→wynik: `ElicitResult` / `CreateMessageResult` / `ListRootsResult`) plus `requestState`, dołączone **do ponowienia oryginalnego żądania**.

### Reguły normatywne (wybór)
- Serwer **MUST** umieścić co najmniej jedno z `inputRequests` / `requestState`.
- Serwer **MUST NOT** wysyłać `inputRequests` na zdolność, której klient nie zadeklarował.
- Serwer **MUST NOT** zakładać, że klient odpowie lub ponowi; **MAY** zwracać `InputRequiredResult` wielokrotnie.
- Klient **MUST** skonstruować żądane dane przed ponowieniem; **MUST** użyć **innego `id` JSON-RPC** dla ponowienia („they are independent requests").
- Brakujące dane → serwer **SHOULD** zwrócić kolejny `InputRequiredResult`, a nie błąd.
- Nieznane klucze w `inputResponses` → serwer **SHOULD** ignorować.

**Obsługiwane żądania** (tylko te trzy): `prompts/get`, `resources/read`, `tools/call`. „Servers **MUST NOT** send `InputRequiredResult` responses on any other client requests."

### Bezpieczeństwo `requestState`
- **MUST** traktować jako dane kontrolowane przez atakującego.
- **MUST** chronić integralność (HMAC/AEAD) i **MUST** odrzucać stan, który nie przechodzi weryfikacji — jeśli wpływa na autoryzację, dostęp do zasobów lub logikę biznesową.
- Integralność **MAY** być pominięta tylko wtedy, gdy manipulacja może co najwyżej spowodować niepowodzenie żądania.
- **SHOULD** zawrzeć i weryfikować: uwierzytelnionego principal, krótki TTL, identyfikator żądania źródłowego (metoda + digest parametrów).
- Ostrzeżenie: te środki ograniczają okno replay, ale **nie gwarantują jednorazowości** — serwery wymagające jednorazowego użycia **MUST** egzekwować to po swojej stronie.

---

## 4. Elicitation — status

**Elicitation NIE jest wycofana.** Nie ma jej w rejestrze funkcji wycofanych (https://modelcontextprotocol.io/specification/2026-07-28/deprecated). Pozostaje funkcją aktywną, ale **dostarczaną wyłącznie przez MRTR**.

- „Servers **MAY** request information from a user during the processing of a client request, by sending an `InputRequiredResult` containing an `elicitation/create` request." — https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation
- Tryby: **form** (`mode: "form"`, domyślny gdy pominięty) i **url** (`mode: "url"`, wprowadzony w `2025-11-25`). Zdolność: `_meta.io.modelcontextprotocol/clientCapabilities.elicitation = { "form": {}, "url": {} }`; pusta zdolność ≡ tylko `form`. Klient **MUST** wspierać co najmniej jeden tryb.
- Wynik: `{ "action": "accept" | "decline" | "cancel", "content": {...} }` — w `inputResponses` przy ponowieniu.
- Schematy form: tylko płaskie obiekty z typami prymitywnymi (`string` z formatami `email`/`uri`/`date`/`date-time`, `number`/`integer`, `boolean`, enumy jedno- i wielokrotnego wyboru, wartości domyślne).
- Zakazy: serwer **MUST NOT** używać trybu form do haseł/kluczy API/tokenów/danych płatniczych; **MUST** używać trybu url.
- Klient dla trybu url: **MUST NOT** pobierać URL-a automatycznie, **MUST NOT** otwierać bez zgody, **MUST** pokazać pełny URL, **MUST** otworzyć w bezpieczny sposób (np. `SFSafariViewController`, **nie** `WKWebView`).

**Usunięte w 2026-07-28 (wprowadzone w 2025-11-25):**
- `notifications/elicitation/complete`
- pole `elicitationId` w żądaniach URL elicitation
Uzasadnienie: „the client learns the outcome of an out-of-band interaction by retrying the original request, so a server-initiated completion signal — and the identifier used to correlate it — no longer fit the protocol. Servers needing to correlate an elicitation across retries encode their own identifier in `requestState`." (changelog, poz. drobna 11)

---

## 5. Sampling — status i migracja

**Sampling jest WYCOFANY (deprecated) od `2026-07-28` na mocy SEP-2577.**

Treść ostrzeżenia w specyfikacji (verbatim): „**Deprecated**: The Sampling feature is deprecated as of protocol version `2026-07-28` (SEP-2577). Under the feature lifecycle policy, it remains in the specification for at least twelve months after this revision's release before it becomes eligible for removal. New implementations **SHOULD NOT** adopt it; existing implementations **SHOULD** migrate to integrating directly with LLM provider APIs."
— https://modelcontextprotocol.io/specification/2026-07-28/client/sampling

- **Rekomendowana migracja (oficjalna):** bezpośrednia integracja z API dostawców LLM. (rejestr wycofań + changelog)
- **Earliest removal:** „First revision released on or after 2027-07-28" (rejestr wycofań).
- Funkcja **wciąż działa** i jest dostarczana przez MRTR: `sampling/createMessage` w `inputRequests`, `CreateMessageResult` w `inputResponses`.
- Dodatkowo **deprecated** (SEP-2596, od `2025-11-25`, usunięcie „no later than the Sampling feature itself"): wartości `includeContext` = `"thisServer"` i `"allServers"`. Rekomendacja: pominąć pole (domyślnie `"none"`) lub użyć `"none"`.
- Zdolność `sampling.tools` (SEP-1577, z 2025-11-25) pozostaje: klient **MUST** zadeklarować `sampling.tools`, by otrzymywać żądania z narzędziami.

---

## 6. Tasks — przeniesione do rozszerzenia `io.modelcontextprotocol/tasks`

**SEP-2663** — *Tasks Extension*, **Final**, **Extensions Track**, utworzony 2026-04-27, autorzy Luca Chang, Caitie McCaffrey (Agents Working Group); sponsor Caitie McCaffrey; **Extension Identifier: `io.modelcontextprotocol/tasks`** — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2663
Repozytorium rozszerzenia: https://github.com/modelcontextprotocol/ext-tasks
Dokumentacja: https://modelcontextprotocol.io/extensions/tasks/overview

### Metody (dokładne nazwy)
| Metoda | Rola | Wynik |
|---|---|---|
| **`tasks/get`** | klient → serwer; odpytywanie o stan | `GetTaskResult` = `Result & DetailedTask & { resultType: "complete" }` |
| **`tasks/update`** | klient → serwer; dostarczenie `inputResponses` na zaległe `inputRequests` | puste potwierdzenie, `resultType: "complete"` |
| **`tasks/cancel`** | klient → serwer; anulowanie (kooperacyjne) | puste potwierdzenie, `resultType: "complete"` |
| ~~`tasks/list`~~ | **USUNIĘTE** | — |
| ~~blokujące `tasks/result`~~ | **ZASTĄPIONE** przez `tasks/get` (polling) | — |

Notyfikacja statusu: **`notifications/tasks`** — przenosi pełny `DetailedTask`; klient optuje przez `subscriptions/listen` z filtrem `taskIds?`.

### Kształt zadania
`CreateTaskResult` = `Result & Task & { resultType: "task" }` — płaski.
Pola `Task`: `taskId` (unikalny, generowany przez odbiorcę), `status`, `ttlMs` (`number | null`), `pollIntervalMs?`.
Warianty `DetailedTask`: `WorkingTask`, `InputRequiredTask` (dodaje **`inputRequests: InputRequests`**), `CompletedTask` (dodaje `result`), `FailedTask` (dodaje `error`), `CancelledTask`.

**Statusy:** `working` \| `input_required` \| `completed` \| `failed` \| `cancelled`. Terminalne: `completed`, `failed`, `cancelled`.

### Zdolność (capability)
```jsonc
// klient — w każdym żądaniu
"_meta": { "io.modelcontextprotocol/clientCapabilities": {
  "extensions": { "io.modelcontextprotocol/tasks": {} } } }

// serwer — w odpowiedzi server/discover
"capabilities": { "extensions": { "io.modelcontextprotocol/tasks": {} } }
```
W schemacie rozszerzenia: `export type TasksExtensionCapability = Record<string, never>;` — czyli **pusty obiekt**, brak pól konfiguracyjnych.
Tworzenie zadania jest **server-directed**: klient optuje raz przez zdolność, serwer decyduje per-żądanie. „allows servers to return task handles unsolicited without per-request opt-in" (changelog, poz. główna 6). Serwer **MUST NOT** zwrócić zadania klientowi, który nie zadeklarował wsparcia.

### `taskSupport` — USUNIĘTE
**⚠️ FLAG (korekta założenia):** wartości `taskSupport` (`"forbidden" | "optional" | "required"`, domyślnie `"forbidden"`) istniały w **rdzeniu** schematu `2025-11-25` (pole `Tool.execution`). W `2026-07-28`:
- `taskSupport` **nie występuje** w `schema/2026-07-28/schema.ts` rdzenia,
- `taskSupport` **nie występuje** w `schema/2026-07-28/schema.ts` rozszerzenia ext-tasks,
- pole `Tool.execution` / typ `ToolExecution` zostały **usunięte z rdzenia**.
Przyczyna: brak per-tool i per-request opt-in — decyzja należy wyłącznie do serwera.

### Wersje schematu rozszerzenia
| Wersja | Status | TypeScript | JSON Schema |
|---|---|---|---|
| `2026-07-28` | **Stable** | `schema/2026-07-28/schema.ts` | `schema/2026-07-28/schema.json` |
| `draft` | Development | `schema/draft/schema.ts` | `schema/draft/schema.json` |
→ https://github.com/modelcontextprotocol/ext-tasks (README)

**⚠️ FLAG:** strona https://modelcontextprotocol.io/extensions/client-matrix **nie zawiera wiersza dla Tasks** (wymienia MCP Apps, OAuth Client Credentials, Enterprise-Managed Authorization, Skills), mimo że https://modelcontextprotocol.io/extensions/tasks/overview linkuje do tej macierzy. Rozbieżność w dokumentacji.

**⚠️ FLAG:** SEP-2663 w swej treści odwołuje się do „the `2026-06-30` specification" — rewizja o tej dacie nigdy nie została opublikowana. Najprawdopodobniej pozostałość po roboczej nazwie; nie ma to wpływu na stan faktyczny.

---

## 7. MCP Apps — rozszerzenie `io.modelcontextprotocol/ui`

**SEP-1865** — *MCP Apps - Interactive User Interfaces for MCP*, **Status: Final**, **Type: Extensions Track**, utworzony **2025-11-21**, PR https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1865

### Daty i status wersji
| Zdarzenie | Data | Źródło |
|---|---|---|
| Ogłoszenie propozycji (SEP-1865) | 2025-11-21 | https://blog.modelcontextprotocol.io/posts/2025-11-21-mcp-apps/ |
| **Launch — „live as an official MCP extension"** | **2026-01-26** | https://blog.modelcontextprotocol.io/posts/2026-01-26-mcp-apps/ |
| Wydanie rdzenia 2026-07-28 | 2026-07-28 | https://blog.modelcontextprotocol.io/posts/2026-07-28/ |

Cytat z ogłoszenia: „Today, we're announcing that **MCP Apps are now live as an official MCP extension**. … This is the first official MCP extension, and it's ready for production."

**⚠️ FLAG (korekta założenia): MCP Apps NIE ma rewizji 2026-07-28.**
- Stabilna wersja specyfikacji rozszerzenia: **`2026-01-26`**, nagłówek `**Status:** Stable (2026-01-26)` — https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx
- `specification/draft/apps.mdx` → `**Status:** Draft` (wyprzedza stabilną)
- `specification/2026-07-28/apps.mdx` → **404**
- Wersja protokołu rozszerzenia jest przypięta niezależnie od rdzenia: `LATEST_PROTOCOL_VERSION = "2026-01-26"`.

### Identyfikator i negocjacja
- Identyfikator: **`io.modelcontextprotocol/ui`** (https://modelcontextprotocol.io/extensions/client-matrix, https://modelcontextprotocol.io/extensions/overview)
- Pole `extensions` w `ClientCapabilities` / `ServerCapabilities` jest **nowe w 2026-07-28** (changelog, poz. drobna 1) — MCP Apps korzysta z niego w kontekście rewizji 2026-07-28.
- Zdolność klienta: `extensions["io.modelcontextprotocol/ui"].mimeTypes` (tablica, REQUIRED wg specyfikacji rozszerzenia).
- Zdolność serwera: `capabilities.extensions["io.modelcontextprotocol/ui"] = {}` w odpowiedzi `server/discover`.

### `ui://` i `_meta.ui.resourceUri`
- Schemat URI: **`ui://`**. Reguły normatywne: „MUST use the `ui://` URI scheme to distinguish UI resources from other MCP resource types"; „URI MUST start with `ui://` scheme". Przykłady: `ui://weather-dashboard`, `ui://weather-server/dashboard-template`, `ui://charts/interactive`.
- **⚠️ FLAG:** brak formalnej gramatyki/ABNF dla `ui://` (reguły authority vs path, wielkość liter, znaki, unikalność). Sprawdzono: spec stabilną, draft, `/extensions/apps/overview.md`, `/extensions/apps/build.md`. Jedyna normatywna reguła to „MUST start with `ui://`".
- **`_meta.ui.resourceUri` występuje w DEFINICJACH NARZĘDZI, nie w wynikach narzędzi.** Wyniki trafiają do widoku przez notyfikację `ui/notifications/tool-result`.
  ```ts
  interface McpUiToolMeta {
    resourceUri?: string;                  // URI zasobu UI renderującego wyniki narzędzia
    visibility?: Array<"model" | "app">;   // domyślnie ["model", "app"]
  }
  ```
- Płaski wariant **`_meta["ui/resourceUri"]` jest DEPRECATED**: „The flat `_meta["ui/resourceUri"]` format is deprecated. Use `_meta.ui.resourceUri` instead. The deprecated format will be removed before GA."
- `visibility`: `"model"` = widoczne i wywoływalne przez agenta; `"app"` = wywoływalne przez aplikację tylko z tego samego serwera. „Host MUST NOT include tools in the agent's tool list when their visibility does not include `"model"`"; „Cross-server tool calls are always blocked for app-only tools."

### MIME type
**`text/html;profile=mcp-app`** — dokładny zapis: jeden średnik, **bez spacji**, parametr `profile`, wartość `mcp-app`, bez cudzysłowów.
„`mimeType` MUST be `text/html;profile=mcp-app`."
**⚠️ FLAG (sprzeczność wewnętrzna):** doc-comment pola `UIResource.mimeType` w **tym samym pliku** mówi „SHOULD be `text/html;profile=mcp-app`".

### Pola `_meta.ui` (zasoby)
| Pole | Typ | Semantyka / domyślne |
|---|---|---|
| `csp` | obiekt | patrz niżej |
| `permissions` | obiekt | `camera?`, `microphone?`, `geolocation?`, `clipboardWrite?` → atrybut `allow` iframe / Permission Policy. „Hosts MAY honor these… Apps SHOULD NOT assume permissions are granted." |
| `domain` | `string` | Wydzielony origin sandboxa. **Zależny od hosta** — „Servers MUST consult host-specific documentation". Przykłady: `a904794854a047f6.claudemcpcontent.com`, `www-example-com.oaiusercontent.com`. Pominięte → domyślny origin hosta. |
| `prefersBorder` | `boolean` | `true` = obramowanie+tło, `false` = brak, pominięte = decyzja hosta. |

Precedencja `_meta.ui` (tylko w **draft**): „When `_meta.ui` is present on **both**, the content-item value takes precedence. Hosts MUST check both locations, preferring the content item and falling back to the listing entry." Umiejscowienia: `resources/list` (statyczne domyślne) i `resources/read` (nadpisania per-odpowiedź).

### CSP
| Klucz | Mapuje na | Domyślnie gdy puste/pominięte |
|---|---|---|
| `connectDomains` | `connect-src` | „no external connections (secure default)" |
| `resourceDomains` | `img-src`, `script-src`, `style-src`, `font-src`, `media-src` | „no external resources (secure default)"; wildcardy subdomen obsługiwane |
| `frameDomains` | `frame-src` | „no nested iframes allowed (`frame-src 'none'`)" |
| `baseUriDomains` | `base-uri` | „only same origin allowed (`base-uri 'self'`)" |

**Domyślny restrykcyjny CSP (verbatim):**
```
default-src 'none';
script-src 'self' 'unsafe-inline';
style-src 'self' 'unsafe-inline';
img-src 'self' data:;
media-src 'self' data:;
object-src 'none';
connect-src 'none';
```
Wymogi: „Host MUST construct CSP headers based on declared domains"; „Host MAY further restrict but MUST NOT allow undeclared domains"; „Host SHOULD log CSP configurations for security review"; „Host MUST block connections to undeclared domains".
**⚠️ FLAG (niespójność):** przykładowy kod budujący CSP emituje `connect-src 'self' ${connectDomains}`, podczas gdy normatywny blok domyślny mówi `connect-src 'none'`.

### Sandbox / iframe — wymogi bezpieczeństwa
- „All View content MUST be rendered in sandboxed iframes with restricted permissions."
- „If the Host is a web page, it **MUST** wrap the View and communicate with it through an intermediate Sandbox proxy."
  1. „The Host and the Sandbox **MUST** have different origins."
  2. „The Sandbox **MUST** have the following permissions: `allow-scripts`, `allow-same-origin`."
  3. Sandbox **SHOULD** wysłać `ui/notifications/sandbox-proxy-ready`.
  4. Host **SHOULD** wysłać surowy HTML przez `ui/notifications/sandbox-resource-ready`.
  5. Sandbox **MUST** załadować HTML z CSP egzekwującym zadeklarowane domeny; `frame-src 'none'` i `base-uri 'self'` domyślnie; „Block dangerous features (`object-src 'none'`)"; restrykcyjne domyślne przy braku metadanych.
  6. „The Sandbox **MUST** forward messages sent by the Host to the View, and vice versa, for any method that doesn't start with `ui/notifications/sandbox-`. … The Host **MUST NOT** send any request or notification to the View before it receives an `initialized` notification."
  7. Sandbox **SHOULD NOT** tworzyć własnych żądań.
  8. Host **MAY** przekazywać wiadomości z widoku do serwera MCP Apps.
- Komunikacja: **JSON-RPC 2.0 przez `postMessage`**; `new MessageTransport(window.parent)`.
- **⚠️ FLAG:** specyfikacja **nie** przypina normatywnie wartości atrybutu `sandbox` **wewnętrznego** iframe (`sandbox-resource-ready` przyjmuje `sandbox?: string` opisane tylko jako „Optional override for inner iframe `sandbox` attribute"). Brak też normatywnego wymogu walidacji `targetOrigin`/`event.origin` w `postMessage`.
- Uwaga: `allow-scripts` + `allow-same-origin` jest bezpieczne **wyłącznie** dlatego, że reguła 1 wymusza różny origin; specyfikacja nie zawiera reguły „MUST NOT łączyć".

### Handshake i metody `ui/*`
Handshake jest wymagany: **`ui/initialize`** (View→Host) → `McpUiInitializeResult` (Host→View) → **`ui/notifications/initialized`** (View→Host). `appCapabilities` jest MUST w żądaniu `ui/initialize`.
- Żądania View→Host: `ui/initialize`, `ui/open-link`, `ui/message`, `ui/request-display-mode`, `ui/update-model-context`, `ui/resource-teardown` (+ `ui/download-file` w SDK)
- Notyfikacje Host→View: `ui/notifications/tool-input`, `ui/notifications/tool-input-partial`, `ui/notifications/tool-result`, `ui/notifications/tool-cancelled`, `ui/notifications/host-context-changed`
- Notyfikacje View→Host: `ui/notifications/initialized`, `ui/notifications/size-changed`
- Zarezerwowane dla sandbox proxy: `ui/notifications/sandbox-proxy-ready`, `ui/notifications/sandbox-resource-ready`
- Współdzielone z rdzeniem MCP: `tools/call`, `resources/read`, `notifications/message`, `ping`

### Wsparcie klientów (2026-09-19)
Claude (web), Claude Desktop, VS Code GitHub Copilot, Microsoft 365 Copilot, Goose, Postman, MCPJam, ChatGPT, Cursor, Archestra.AI, PostHog Code — https://modelcontextprotocol.io/extensions/client-matrix

---

## 8. `outputSchema` / `structuredContent` — zmiany w 2026-07-28

**SEP-2106** — *Tools `inputSchema` & `outputSchema` Conform to JSON Schema 2020-12*, **Final**, Standards Track, utworzony **2026-01-06**, autor John McBride, shepherd/sponsor Ola Hungerford — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2106

Cytat z changelogu (poz. drobna 10): „Loosen `inputSchema` and `outputSchema` to allow any JSON Schema 2020-12 keywords, and `structuredContent` to allow any JSON value. Add `$ref` resolution requirements and composition-keyword resource bounds."

| Element | 2025-11-25 | 2026-07-28 |
|---|---|---|
| `inputSchema` | ograniczone do `$schema`/`type`/`properties`/`required` | `type: "object"` nadal **wymagane** u korzenia, ale „any JSON Schema 2020-12 keyword may appear alongside `type` — including composition keywords (`oneOf`, `anyOf`, `allOf`, `not`), conditional keywords (`if`/`then`/`else`), reference keywords (`$ref`, `$defs`, `$anchor`)". Schemat: `additionalProperties: {}`, `required: ["type"]`. |
| `outputSchema` | „Currently restricted to `type: \"object\"` at the root level." | „This can be any valid JSON Schema 2020-12." Brak wymaganego `type` u korzenia; możliwy korzeń tablicowy (przykład `list_users` w dokumentacji). |
| `structuredContent` | `{ [key: string]: unknown }` | **Dowolna wartość JSON**: „object, array, string, number, boolean, or null" zgodna z `outputSchema`, jeśli zdefiniowano. |

### `$ref` — nowe wymogi normatywne
https://modelcontextprotocol.io/specification/2026-07-28/basic/index#ref-resolution
- „Implementations **MUST NOT** automatically dereference `$ref` values that resolve to a network URI."
- Tryb opt-in pobierania zdalnych `$ref` **MUST** być **domyślnie wyłączony**, **SHOULD** egzekwować allowlistę hostów (min. odrzucać loopback, link-local, adresy sieci prywatnych), limity czasu i rozmiaru, oraz logować dereferencjonowane URI.
- „Schemas that fail to validate due to an unresolved external `$ref` **SHOULD** be rejected rather than silently treated as permissive."

### Composition-keyword resource bounds — nowe
„Implementations **SHOULD** apply reasonable bounds, such as a maximum schema depth, a cap on the total number of subschemas, or a per-validation time budget, to prevent a malicious schema from acting as a Denial-of-Service vector against the validator."
→ https://modelcontextprotocol.io/specification/2026-07-28/basic/index#composition-keyword-resource-use

Dialekt: JSON Schema **2020-12** domyślnie (bez `$schema`), inne dialekty dozwolone przez jawne `$schema`; implementacje **MUST** wspierać 2020-12.

**⚠️ FLAG (konflikt źródeł) — fallback tekstowy:** strona `server/tools` mówi **SHOULD** („a tool that returns structured content SHOULD also return the serialized JSON in a TextContent block"), natomiast SEP-2106 mówi **MUST** dla ryzykownego podzbioru („servers using array or primitive `structuredContent` **MUST** also emit a `TextContent` block containing the serialized JSON").

### Resource links w wynikach narzędzi — BEZ ZMIAN
**⚠️ FLAG (korekta założenia):** nie znalazłem **żadnej** zmiany dotyczącej `resource_link` w wynikach narzędzi w 2026-07-28.
- Diff schematu: `ResourceLink` różni się wyłącznie komentarzami dokumentacyjnymi; `ContentBlock` jest **identyczny** między rewizjami.
- Changelog nie zawiera wpisu o `resource_link` (ani w części głównej, ani drobnej).
- Przykład i semantyka identyczne w obu rewizjach: `{ "type": "resource_link", "uri": "...", "name": "...", "description": "...", "mimeType": "..." }`; „Resource links returned by tools are not guaranteed to appear in the results of a `resources/list` request."
- Sprawdzone: oba `schema.json`, https://modelcontextprotocol.io/specification/2026-07-28/server/tools , https://modelcontextprotocol.io/specification/2025-11-25/server/tools , changelog 2026-07-28.

### Powiązane zmiany w wynikach (2026-07-28)
- **`resultType` wymagane** na każdym wyniku (SEP-2322).
- **`ttlMs` + `cacheScope` wymagane** na wynikach `tools/list`, `prompts/list`, `resources/list`, `resources/read`, `resources/templates/list` przez interfejs `CacheableResult` (SEP-2549). `ttlMs` = wskazówka świeżości w ms; `cacheScope` = `"public"` | `"private"`.
- **`x-mcp-header`** jako adnotacja właściwości schematu (SEP-2243).
- „Servers **SHOULD** return tools from `tools/list` in a deterministic order" (changelog, poz. drobna 3).

---

## 9. Adnotacje narzędzi (tool annotations)

**Zestaw w 2026-07-28 — dokładnie pięć pól, `ToolAnnotations`:**

| Pole | Typ | Opis / domyślna wartość (verbatim ze `schema.ts`) |
|---|---|---|
| `title` | `string` | „A human-readable title for the tool." |
| `readOnlyHint` | `boolean` | „If true, the tool does not modify its environment." **Default: false** |
| `destructiveHint` | `boolean` | „If true, the tool may perform destructive updates to its environment. If false, the tool performs only additive updates. (This property is meaningful only when `readOnlyHint == false`)" **Default: true** |
| `idempotentHint` | `boolean` | „If true, calling the tool repeatedly with the same arguments will have no additional effect on its environment. (This property is meaningful only when `readOnlyHint == false`)" **Default: false** |
| `openWorldHint` | `boolean` | „If true, this tool may interact with an \"open world\" of external entities. If false, the tool's domain of interaction is closed." **Default: true** |

Źródło: `https://github.com/modelcontextprotocol/specification/blob/main/schema/2026-07-28/schema.ts` (linie ~1912–1954).

Ostrzeżenie normatywne: „clients **MUST** consider tool annotations to be untrusted unless they come from trusted servers." — https://modelcontextprotocol.io/specification/2026-07-28/server/tools
Opis typu: „all properties in `ToolAnnotations` are **hints**. They are not guaranteed to provide a faithful description of tool behavior (including descriptive properties like `title`)."
Precedencja nazwy wyświetlanej: `title` → `annotations.title` → `name`.

### Czy jest NOWA adnotacja w 2026-07-28? — NIE
**⚠️ FLAG (korekta założenia): `untrustedHint` NIE ISTNIEJE.** Weryfikacja negatywna:
- `ToolAnnotations` w `2025-11-25` i `2026-07-28`: **identyczny** zestaw pól, typów i wartości domyślnych (diff programistyczny; zmieniło się tylko opakowanie komentarza `Tool` → `{@link Tool}`).
- Brak `untrustedHint` w obu `schema.json` i w korpusie dokumentacji.
- Changelog 2026-07-28 nie zawiera **żadnego** wpisu o adnotacjach.
- W indeksie SEP (stan 2026-09-19) **żaden** SEP nie dodaje adnotacji narzędzia.

**Propozycje, które NIE weszły** (stan na 2026-09-19):
| SEP | Propozycja | Status |
|---|---|---|
| [#1913](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1913) | Trust and Sensitivity Annotations | **open** (zaktualizowany 2026-09-14) |
| [#1984](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1984) | Comprehensive Tool Annotations for Governance/UX | **open** (zaktualizowany 2026-09-07) |
| [#1487](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1487) | `trustedHint` | Proposal (zamknięty jako issue) |
| [#1560](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1560) | `secretHint` | Proposal (zamknięty jako issue) |
| [#1561](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1561) | `unsafeOutputHint` | Proposal (zamknięty jako issue) |

Kontekst i pięć pytań oceniających nowe adnotacje: https://blog.modelcontextprotocol.io/posts/2026-03-16-tool-annotations/ (2026-03-16, Ola Hungerford, Sam Morrow, Luca Chang).

### Inne zmiany dotyczące adnotacji / metadanych wykonania
- **`Tool.execution` / `ToolExecution` USUNIĘTE z rdzenia** (m.in. pole `taskSupport`) — patrz sekcja 6.
- **`Tool._meta`** przemianowane z inline `object` na `"$ref": "#/$defs/MetaObject"`.
- Historycznie: `taskHint` zaproponowany jako adnotacja, ale „landed as `Tool.execution` instead" (PR #1854) — a `Tool.execution` zostało następnie usunięte z rdzenia w 2026-07-28.

**⚠️ FLAG (luka w dokumentacji):** strona https://modelcontextprotocol.io/specification/2026-07-28/server/tools **nie ma** sekcji wyliczającej pięć adnotacji ani ich wartości domyślnych — mówi tylko „`annotations`: Optional properties describing tool behavior". Normatywne wartości domyślne istnieją **wyłącznie** w `schema.ts` / `schema.json`.

---

## 10. Discovery — mechanizm `.well-known` / server card

### Co JEST w specyfikacji 2026-07-28: RPC `server/discover`
https://modelcontextprotocol.io/specification/2026-07-28/server/discover
- „Servers **MUST** implement it." — **obowiązkowe**.
- Żądanie nie niesie parametrów poza standardowym `_meta`.
- `DiscoverResult`:
  - `supportedVersions: string[]` — wersje protokołu wspierane przez serwer
  - `capabilities` — zdolności serwera (tools, resources, prompts, extensions…)
  - `_meta["io.modelcontextprotocol/serverInfo"]` — nazwa i wersja (serwer **SHOULD** dołączać)
  - `instructions?: string` — opcjonalne wskazówki naturalnym językiem dla LLM
  - `ttlMs` + `cacheScope` — wynik jest cacheowalny
- Wywołanie jest **opcjonalne dla klienta** — może wywołać dowolne RPC od razu i obsłużyć `UnsupportedProtocolVersionError`. Przydatne w dwóch scenariuszach: prezentacja informacji o serwerze oraz **sonda zgodności wstecznej na STDIO**.
- `serverInfo` jest self-reported i nieuwierzytelnione — „Clients SHOULD NOT use it to change their behavior, and SHOULD NOT rely on it for security decisions."

### Czego NIE MA w specyfikacji
**⚠️ FLAG (istotna korekta założenia):**
1. **`/.well-known/mcp.json` NIE ISTNIEJE** jako mechanizm MCP. Nie ma go w specyfikacji 2026-07-28 ani w żadnej opublikowanej rewizji. Jedyne trafienie to przypadkowe issue w repozytorium osób trzecich (`robhunter/agentdeals#329`), nie źródło normatywne.
2. **Mechanizm „MCP Server Cards" NIE jest częścią specyfikacji 2026-07-28.** To wciąż **otwarty** PR.

### MCP Server Cards — stan faktyczny (2026-09-19)
| Element | Stan |
|---|---|
| **SEP-2127** *MCP Server Cards - HTTP Server Discovery* | **PR OTWARTY** — utworzony 2026-01-21, ostatnia aktualizacja 2026-09-12, **niezmergowany** → https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2127 |
| SEP-1649 (poprzednik, „HTTP Server Discovery via .well-known") | **zamknięty** 2026-01-26, zastąpiony przez SEP-2127 |
| Repozytorium rozszerzenia | https://github.com/modelcontextprotocol/ext-server-card (alias `experimental-ext-server-card`) |
| Status w README repo | **„Status: Experimental. This work is for prototyping and feedback only, and is not an accepted or official MCP extension."** |
| Identyfikator rozszerzenia | `io.modelcontextprotocol/server-card` |
| Roadmap (2026-08-22) | „The Server Card Working Group continues to work through the `.well-known` metadata conventions for MCP servers" — czyli **prace w toku** |

### Proponowany (nieprzyjęty) mechanizm discovery wg SEP-2127 / repo ext-server-card
- **AI Catalog** pod `/.well-known/ai-catalog.json`, media type `application/ai-catalog+json`. Wpis: `{ identifier (wymagane, format `urn:air:{publisher}:{namespace}:{name}`), type: "application/mcp-server-card+json" (wymagane), url LUB data (dokładnie jedno) }`.
- **Server Card** pod **`GET <streamable-http-url>/server-card`** (rekomendowana, zarezerwowana lokalizacja) z `Accept: application/mcp-server-card+json`; media type `application/mcp-server-card+json`. Karta **MAY** być hostowana pod dowolnym niezarezerwowanym URI.
- **`.well-known` jest wprost ODRZUCONE dla samej karty:** „A `.well-known` URI (e.g., `/.well-known/mcp/server-card`)… `.well-known` is for _site-wide_ metadata, whereas an individual server's card is _application-level_ metadata."
- **Pola Server Card** (ze `schema.ts`): `$schema` (wymagane; dokładnie `https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json`), `name` (wymagane, reverse-DNS z jednym ukośnikiem, wzorzec `^[a-zA-Z0-9.-]+/[a-zA-Z0-9._-]+$`), `version` (wymagane, semver), `description` (wymagane, 1–100 znaków), `title?`, `websiteUrl?`, `repository?` (`url`, `source`, `subfolder?`, `id?`), `icons?`, `remotes?` (`type: "streamable-http" | "sse"`, `url`, `headers?`, `variables?`, `supportedProtocolVersions?`), `_meta?`.
- Karty **celowo nie zawierają** primitywów (tools/resources/prompts) ani `capabilities`/`extensions` — uzasadnienie: serwer jest dynamiczny, a statyczny manifest nie może tego rzetelnie reprezentować.
- Bezpieczeństwo: karty publiczne i tylko do odczytu; **MUST NOT** zawierać poświadczeń/topologii sieci; **MUST** być serwowane po HTTPS (TLS 1.2+) w produkcji; CORS `Access-Control-Allow-Origin: *` dopuszczalny; `Cache-Control: public, max-age=3600` + `ETag`; klienci **MUST NOT** traktować karty jako autorytatywnej dla decyzji bezpieczeństwa i **SHOULD** weryfikować jej twierdzenia wobec żywego `server/discover`.

### Odrębny byt: MCP Registry `server.json`
- MCP Registry jest w **preview**: https://modelcontextprotocol.io/registry/about
- `server.json` opisuje **wpisy rejestru**, w tym lokalnie instalowalne pakiety i konfigurację uruchomieniową; schema: https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md
- SEP-2127 explicite rozdziela oba formaty: „MCP Server Cards describe _remote_ MCP connectivity only. The MCP Registry's `server.json` separately describes registry entries… The Registry owns that schema; the Server Card extension does not define a `Server` superset or package-installation types."

---

## 11. Subskrypcje — `subscriptions/listen`

**Zastępuje:** `resources/subscribe`, `resources/unsubscribe` **oraz** endpoint HTTP GET. SEP-2575.
Dokumentacja: https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions

Cytat z changelogu (poz. główna 4): „Replace the HTTP GET endpoint and `resources/subscribe`/`resources/unsubscribe` with `subscriptions/listen`: a single long-lived POST-response stream for opted-in server-to-client change notifications."

### Semantyka
1. Klient wysyła **`subscriptions/listen`** jako zwykłe żądanie JSON-RPC (POST). Odpowiedzią jest **długo żyjący strumień SSE**.
2. Filtr `notifications` (wszystkie pola opcjonalne; pominięcie = brak subskrypcji tego typu):

| Pole | Typ | Znaczenie |
|---|---|---|
| `toolsListChanged` | `boolean` | `notifications/tools/list_changed` |
| `promptsListChanged` | `boolean` | `notifications/prompts/list_changed` |
| `resourcesListChanged` | `boolean` | `notifications/resources/list_changed` |
| `resourceSubscriptions` | `string[]` | `notifications/resources/updated` dla tych URI |

3. **Potwierdzenie (acknowledgment):** serwer **MUST** wysłać **`notifications/subscriptions/acknowledged`** jako **pierwszą** wiadomość, z `io.modelcontextprotocol/subscriptionId` w `_meta`, i **MUST NOT** wysłać żadnej notyfikacji przed nią. Pole `notifications` w potwierdzeniu odzwierciedla **podzbiór**, który serwer zgodził się honorować; nieobsługiwane typy są pomijane. Klient **SHOULD** porównać potwierdzenie z żądaniem.
4. **`io.modelcontextprotocol/subscriptionId`** = **ID JSON-RPC żądania `subscriptions/listen`**. Wszystkie notyfikacje na strumieniu noszą ten klucz. Na STDIO (jeden kanał) klienci **MUST** używać tego pola do demultipleksowania.
5. Serwer **MUST NOT** wysyłać typów, których klient nie zamówił.
6. **Wiele równoczesnych subskrypcji** jest dozwolone; każda identyfikowana ID swojego żądania.
7. **Zakończenie subskrypcji:**
   - klient: zamknięcie strumienia SSE (HTTP) albo `notifications/cancelled` wskazujące ID żądania (STDIO);
   - serwer: **SHOULD** wysłać pomyślny wynik `subscriptions/listen` (`resultType: "complete"` + `subscriptionId` w `_meta`) przed zamknięciem — sygnał **graceful closure**; zamknięcie transportu bez tego wyniku = nieoczekiwane rozłączenie;
   - zamknięcie transportu (timeout HTTP, TCP, wyjście procesu stdio).
8. **STDIO:** po zerwaniu i ponownym połączeniu klient **MUST** ponownie wysłać `subscriptions/listen` — serwer nie przechowuje stanu subskrypcji między połączeniami.
9. Notyfikacje **zakresowo związane z żądaniem** (`notifications/progress`, `notifications/message`) **NIE** płyną strumieniem `subscriptions/listen` — płyną wyłącznie na strumieniu odpowiedzi żądania, którego dotyczą.
10. Serwery **SHOULD** okresowo emitować linię komentarza SSE (`:`) jako keep-alive na długo żyjących strumieniach oraz nagłówek `X-Accel-Buffering: no`.

---

## 12. Deprecacje i usunięcia w 2026-07-28 — dokładna lista

### USUNIĘTE (breaking)
| Co | Źródło |
|---|---|
| `initialize` / `notifications/initialized` (handshake) | changelog poz. główna 2 (SEP-2575) |
| Nagłówek `Mcp-Session-Id` i protokolarny koncept sesji | changelog poz. główna 1 (SEP-2567) |
| Endpoint HTTP **GET** w Streamable HTTP | changelog poz. główna 4 (SEP-2575) |
| `resources/subscribe`, `resources/unsubscribe` | changelog poz. główna 4 (SEP-2575) |
| **`ping`** | changelog poz. główna 5 (SEP-2575) |
| **`logging/setLevel`** | changelog poz. główna 5 (SEP-2575) |
| **`notifications/roots/list_changed`** | changelog poz. główna 5 (SEP-2575) |
| Wznawianie SSE: `Last-Event-ID` i identyfikatory zdarzeń | changelog poz. główna 9 (SEP-2575) |
| `notifications/elicitation/complete` + pole `elicitationId` | changelog poz. drobna 11 |
| `tasks/list` oraz blokujące `tasks/result` (Tasks → rozszerzenie) | changelog poz. główna 6 (SEP-2663) |
| `Tool.execution` / `ToolExecution` (w tym `taskSupport`) z rdzenia | diff schematu 2025-11-25 → 2026-07-28 |
| Kod błędu `-32002` → zastąpiony przez `-32602`; `-32042` usunięty | changelog poz. drobna 6; https://modelcontextprotocol.io/specification/2026-07-28/basic/index#error-codes |

**Poziom logowania** jest teraz ustawiany per-request przez `io.modelcontextprotocol/logLevel` w `_meta`; „servers **MUST NOT** emit `notifications/message` for requests that did not include this field."

### ZDEPRECJONOWANE w 2026-07-28
| Funkcja | SEP | Od kiedy | Ścieżka migracji | Earliest removal |
|---|---|---|---|---|
| **Roots** | SEP-2577 | `2026-07-28` | Przekazywanie katalogów/plików przez parametry narzędzi, URI zasobów lub konfigurację serwera | pierwsza rewizja wydana 2027-07-28 lub później |
| **Sampling** | SEP-2577 | `2026-07-28` | Bezpośrednia integracja z API dostawców LLM | pierwsza rewizja wydana 2027-07-28 lub później |
| **Logging** | SEP-2577 | `2026-07-28` | `stderr` dla stdio; OpenTelemetry dla obserwowalności | pierwsza rewizja wydana 2027-07-28 lub później |
| **Dynamic Client Registration** (RFC 7591) | PR #2858 | `2026-07-28` | Client ID Metadata Documents | pierwsza rewizja wydana 2027-07-28 lub później |
| `includeContext: "thisServer"` / `"allServers"` | SEP-2596 | `2025-11-25` | Pomiń pole lub użyj `"none"` | razem z Sampling (SEP-2577) |
| **Transport HTTP+SSE** (2024-11-05) | SEP-2596 | `2025-03-26` | Streamable HTTP | 3 miesiące po osiągnięciu Final przez SEP-2596 |

→ rejestr: https://modelcontextprotocol.io/specification/2026-07-28/deprecated

### NIE zdeprecjonowane
**`progress`, `elicitation`, `resources`, `prompts`, `tools`, `completion` — wszystkie pozostają aktywne.**
- `notifications/progress` działa dalej i płynie na strumieniu odpowiedzi żądania; opt-in przez `progressToken` w `_meta` (unikalny wśród aktywnych żądań). https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress
- `notifications/message` (logging) nadal istnieje, ale ograniczony do żądań z `logLevel`.

**⚠️ FLAG (niuans):** rejestr wycofań ma sekcję „Removed" z tekstem „**No features have been removed under this policy yet.**" Należy rozróżnić: usunięcia wymienione wyżej to zmiany protokołu w samej rewizji 2026-07-28 (breaking changes), natomiast rejestr cyklu życia dokumentuje usunięcia **funkcji uprzednio zdeprecjonowanych w ramach tej polityki** — takich jeszcze nie było.

### Nowa polityka (SEP-2596)
- Stany funkcji: **Active**, **Deprecated**, **Removed**.
- Minimalne okno deprecjacji: **12 miesięcy** (lub 90 dni w ramach „expedited-removal exception").
- „The earliest removal marks when a feature becomes *eligible* for removal; the actual removal is a Core Maintainer decision taken during release preparation and may happen later."
- Rejestr wycofań: https://modelcontextprotocol.io/specification/2026-07-28/deprecated

---

## 13. Zmiany OAuth / autoryzacji w 2026-07-28

Baza: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization (strona rozbita na podstrony przez PR #2858 „Authorization spec split" — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2858)

### RFC 9207 `iss` — SEP-2468
**SEP-2468** *Recommend Issuer (iss) Parameter in MCP Auth Responses*, **Final**, Standards Track, utworzony **2026-03-25**, autorka Emily Lauber, sponsor @pcarleton — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2468

- Changelog (poz. drobna 7): „Authorization servers **SHOULD** include the `iss` parameter in authorization responses per RFC 9207, and MCP clients **MUST** validate a present `iss` against the recorded issuer before redeeming the authorization code."
- Klient **MUST** zapisać `issuer` z **zwalidowanych** metadanych wybranego AS i powiązać go z tym samym rekordem per-request, co PKCE `code_verifier` (i `state`).
- AS dołączający `iss` **MUST** ogłosić to przez `authorization_response_iss_parameter_supported: true`.
- Klient **MUST** zastosować walidację z RFC 9207 §2.4 przed przekazaniem kodu do jakiegokolwiek token endpointu:

| `authorization_response_iss_parameter_supported` | `iss` w odpowiedzi | Działanie klienta |
|---|---|---|
| `true` | obecny | Porównaj z zapisanym issuerem (simple string comparison, RFC 3986 §6.2.1) |
| `true` | **brak** | **Odrzuć odpowiedź** |
| `false` lub brak | obecny | Porównaj z zapisanym issuerem |
| `false` lub brak | brak | Kontynuuj |

- Klient **MUST NOT** stosować foldingu wielkości liter schematu/hosta, elizji domyślnego portu, ukośnika końcowego ani normalizacji percent-encoding przed porównaniem.
- Walidacja dotyczy **także odpowiedzi błędnych** — przy niezgodności klient **MUST NOT** działać na `error`, `error_description`, `error_uri` ani ich wyświetlać.
- Zapowiedź: „A future revision of this specification is expected to upgrade authorization server inclusion of `iss` from **SHOULD** to **MUST**."
- Nowa sekcja bezpieczeństwa „Mix-Up Attacks"; nowy wymóg walidacji metadanych: `issuer` w dokumencie **MUST** być identyczny z issuerem użytym do zbudowania well-known URL, inaczej klient **MUST NOT** użyć metadanych.

### `application_type` w DCR — SEP-837
- Changelog (poz. drobna 8): „Require MCP clients to specify an appropriate `application_type` during Dynamic Client Registration to avoid OpenID Connect redirect URI conflicts."
- Wymóg: **„MCP clients MUST specify an appropriate `application_type` during Dynamic Client Registration. Omitting it defaults to `\"web\"` under OIDC, which can conflict with native-style redirect URIs; non-OIDC servers safely ignore the parameter."**
- Wartości: **`"native"`** dla aplikacji desktopowych, mobilnych, CLI i lokalnie hostowanych web aplikacji dostępnych przez `localhost`; **`"web"`** dla zdalnych aplikacji przeglądarkowych spoza localhosta. Oba **SHOULD**.
- Klient **MUST** być przygotowany na błędy rejestracji z powodu ograniczeń redirect URI w OIDC; **MAY** ponowić z dostosowanym `application_type`.
- Uzasadnienie: AS zgodne z OIDC egzekwują ograniczenia redirect URI na podstawie `application_type` (OpenID Connect Dynamic Client Registration 1.0) i odrzucały `localhost` dla klientów desktop/CLI.
**⚠️ FLAG:** SEP-837 nie ma strony `/seps/837` (404) i nie występuje w indeksie SEP — pochodzi z okresu przed workflow PR-owym. Zweryfikowane przez tytuł PR, changelog i tekst normatywny.

### Client ID Metadata Documents (CIMD)
**⚠️ FLAG (istotna korekta założenia): CIMD NIE zostało wprowadzone w 2026-07-28.**
- CIMD weszło w **`2025-11-25`**: „Add support for OAuth Client ID Metadata Documents as a recommended client registration mechanism (**SEP-991**, PR #1296)" — https://modelcontextprotocol.io/specification/2025-11-25/changelog
- **SEP-991** *Enable URL-based Client Registration using OAuth Client ID Metadata Documents*, Final, Standards Track, utworzony **2025-07-07**; PR realizujący to **#1296** (zmergowany 2025-11-14), nie #2858.
- Pełna sekcja CIMD istnieje już na stronie autoryzacji **2025-11-25** (https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization).
- **Co nowego w 2026-07-28:** wyłącznie **deprecjacja DCR na rzecz CIMD**.
- **⚠️ FLAG (korekta):** PR **#2858** to „Authorization spec split" (podział strony autoryzacji), **nie** SEP CIMD. Changelog i rejestr wycofań cytują #2858 dla *deprecjacji DCR*, nie dla wprowadzenia CIMD.

Wymogi CIMD (2026-07-28, https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration):
- `client_id` **MUST** używać schematu `https` i zawierać komponent ścieżki, np. `https://example.com/client.json`.
- Dokument metadanych **MUST** zawierać co najmniej `client_id`, `client_name`, `redirect_uris`; wartość `client_id` **MUST** dokładnie zgadzać się z URL-em dokumentu.
- AS: **SHOULD** pobierać dokument przy URL-owym `client_id`; **MUST** walidować zgodność `client_id` z URL-em; **MUST** walidować redirect URI wobec dokumentu; **MUST** walidować strukturę JSON i wymagane pola; **SHOULD** cache'ować zgodnie z nagłówkami HTTP.
- Ogłaszanie wsparcia: `{ "client_id_metadata_document_supported": true }` w metadanych AS.
- Kolejność priorytetów: **pre-registered → CIMD → DCR → pytanie do użytkownika**.
- Podstawa: `draft-ietf-oauth-client-id-metadata-document-00`.

### Powiązanie poświadczeń z serwerem autoryzacji — SEP-2352
- Changelog (poz. drobna 9): „clients **MUST** key persisted credentials by the issuer identifier, **MUST NOT** reuse them with a different authorization server, and **MUST** re-register when the authorization server changes."
- Klienci używający poświadczeń pre-registered lub uzyskanych przez DCR **MUST** powiązać je z konkretnym AS, kluczując po `issuer`.
- Klient **MUST NOT** używać ponownie poświadczeń z innego AS i **MUST** zarejestrować się na nowo przy zmianie AS.
- Klient **SHOULD** zgłosić błąd, a nie po cichu próbować niezgodnych poświadczeń.
- Client ID oparte na CIMD są **przenośne** między AS — ponowna rejestracja nie jest potrzebna.
**⚠️ FLAG:** SEP-2352 nie ma strony `/seps/2352` i nie występuje w indeksie SEP; zweryfikowane przez tytuł PR („SEP-2352: Clarify authorization server binding and migration"), changelog i tekst normatywny.

### Deprecjacja DCR (RFC 7591)
- Changelog (Deprecated, poz. 4): „Deprecate the OAuth 2.0 Dynamic Client Registration Protocol (RFC7591) as a client registration mechanism in favor of Client ID Metadata Documents (PR #2858). It remains available for backwards compatibility with authorization servers that do not support Client ID Metadata Documents."
- Status: **ZDEPRECJONOWANE, nie usunięte.** DCR pozostaje opcją **MAY**.
- Rejestr wycofań: Deprecated in `2026-07-28`; **Earliest removal: pierwsza rewizja wydana 2027-07-28 lub później**.
- Blog: „DCR continues to work for backward compatibility, but will be removed in a future version of the MCP spec."

### Pozostałe zmiany autoryzacji
- **RFC 8707 (Resource Indicators) — BEZ ZMIAN.** Tekst identyczny w obu rewizjach: klienci **MUST** implementować RFC 8707, `resource` **MUST** być w żądaniu autoryzacji i tokenu, **MUST** identyfikować serwer MCP, **MUST** używać kanonicznego URI, **MUST** być wysyłany niezależnie od wsparcia AS.
- **Obsługa zakresów (scope) — ZMIENIONA:** akumulacja unii zakresów przy ponownej autoryzacji; „Servers **SHOULD** include all scopes required for the current operation in a single challenge. Challenging incrementally … degrades user experience."; nowe „Servers **MUST** account for scope hierarchies, where a broader scope implies narrower ones…"; `scopes_supported` przeformułowane jako „the minimal set of scopes necessary for basic functionality".
- **Nowa sekcja „Refresh Tokens"** (nieobecna w 2025-11-25; SEP-2207, Final, utworzony 2026-02-04): klient **MUST** chronić refresh tokeny (OAuth 2.1 §4.3), **SHOULD** dołączyć `refresh_token` do `grant_types`, **MAY** dodać `offline_access` gdy AS wymienia go w `scopes_supported`, **MUST NOT** zakładać ich wydania; serwer **SHOULD NOT** umieszczać `offline_access` w `WWW-Authenticate` ani w `scopes_supported` PRM.
- **Rozszerzona lista standardów:** dodano RFC 6750, RFC 8707, RFC 9207, OpenID Connect Discovery 1.0, OIDC Dynamic Client Registration 1.0.
- Rozszerzenia autoryzacyjne: `io.modelcontextprotocol/oauth-client-credentials`, `io.modelcontextprotocol/enterprise-managed-authorization` — https://github.com/modelcontextprotocol/ext-auth

---

## 14. Wsparcie SDK dla 2026-07-28 (stan 2026-09-19)

| SDK | Najnowsza wersja | Data | Mówi `2026-07-28`? | Pierwsza wersja z wsparciem |
|---|---|---|---|---|
| **TypeScript v1** `@modelcontextprotocol/sdk` | **1.30.0** | 2026-07-27 | **NIE** | — |
| **TypeScript v2** `@modelcontextprotocol/server` `/client` `/core` | **2.0.0** | 2026-07-27 | **TAK** (opt-in) | 2.0.0-beta.1 (2026-06-30) |
| **Python** `mcp` | **2.2.0** | 2026-09-07 | **TAK** | **2.0.0** (2026-07-28) |
| Python v1 (linia 1.x) | 1.30.0 | 2026-09-07 | NIE | — |
| **Rust `rmcp`** | **3.4.0** | 2026-09-15 | **TAK** | **3.0.0** (2026-07-28) |
| **Go** `github.com/modelcontextprotocol/go-sdk` | **v1.8.0** | tag 2026-09-04 / release 2026-09-14 | **TAK** | **v1.7.0** (2026-07-28) |
| **Java** `io.modelcontextprotocol.sdk:mcp` | **2.0.1** | 2026-08-19 | **NIE** | brak |
| **C#** `ModelContextProtocol` | **2.2.0** | 2026-08-13 | **TAK** | **2.0.0** (2026-07-28) |

### Rust `rmcp` — odpowiedź szczegółowa (pytanie wprost z zadania)
- **Aktualna wersja crate `rmcp`: `3.4.0`**, opublikowana **2026-09-15T15:44:08Z** (crates.io: `max_version = max_stable_version = 3.4.0`; 65 wersji; 27 579 718 pobrań łącznie). → https://crates.io/api/v1/crates/rmcp , https://docs.rs/rmcp
- **TAK — wspiera `2026-07-28`.** README (verbatim): „This SDK implements the stable MCP **`2026-07-28`** specification while remaining fully compatible with the **`2025-11-25`** release and earlier versions. Features introduced in `2026-07-28` — server discovery & negotiation, transport-neutral subscriptions, long-running tasks, response caching, multi-round-trip requests, and standard HTTP routing headers…" → https://github.com/modelcontextprotocol/rust-sdk
- **Pierwsza wersja ze wsparciem:** `3.0.0` (stabilna, crates.io **2026-07-28T22:52:36Z**); pierwsza w ogóle: `3.0.0-beta.1` (2026-07-23). Linia 2.x obsługiwała wyłącznie `2025-11-25`.
- **Oficjalność potwierdzona:** (1) README linia 1: „An **official** Rust Model Context Protocol SDK implementation with tokio async runtime"; (2) repozytorium w organizacji `modelcontextprotocol`; (3) https://modelcontextprotocol.io/docs/2026-07-28/sdk wymienia Rust jako **Tier 1**.
- **Czy istnieje oficjalna strona statusu Rust SDK?** `https://rust.sdk.modelcontextprotocol.io` **przekierowuje na https://docs.rs/rmcp**. Nie ma osobnej strony ze statusem/macierzą wersji specyfikacji.
- **⚠️ FLAG:** w README `rmcp` (ani w `crates/rmcp/README.md`) **nie ma tabeli zgodności wersji specyfikacji** — tylko proza i linki „MCP Spec:" przy funkcjach. Tabele takie mają **Go** (README, sekcja „Version Compatibility") i **Java** (CHANGELOG, tabela „Release lines"). TypeScript, Python, C# i Rust ich nie mają.
- **⚠️ FLAG (nieaktualne źródło):** blog z 2026-07-28 mówi „the Rust SDK supports the new spec in beta" — to odpowiadało `3.0.0-beta.x`; dziś `3.4.0` jest stabilne. 3.x jest zmianą łamiącą: przewodnik migracji → https://github.com/modelcontextprotocol/rust-sdk/discussions/969
- **MSRV:** `rust_version = 1.88` (wszystkie wersje 3.x).

### Pozostałe SDK — szczegóły
**TypeScript** (https://github.com/modelcontextprotocol/typescript-sdk):
- v1 `@modelcontextprotocol/sdk` 1.30.0 — brak wsparcia 2026-07-28; `dist-tags` = wyłącznie `{"latest":"1.30.0"}`, **brak taga `next`/beta**.
- v2 to **osobne nazwy pakietów**, nie tag: `@modelcontextprotocol/server`, `/client`, `/core` (wszystkie 2.0.0, 2026-07-27), plus `/node`, `/express`, `/fastify`, `/hono`, `/codemod`, `/server-legacy`.
- **Krytyczny niuans:** „Nothing in v2 puts a 2026-07-28 byte on the wire by default… Serving or speaking 2026-07-28 is always an explicit opt-in" — przez `versionNegotiation` `{mode:'auto'}` / `{pin:'2026-07-28'}`. → https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28.html
- v2 ESM-only, Node 20+, Bun, Deno; schematy narzędzi przez Standard Schema (Zod v4, Valibot, ArkType). v1.x otrzymuje poprawki i aktualizacje bezpieczeństwa przez min. 6 miesięcy po wydaniu v2.

**Python** (https://github.com/modelcontextprotocol/python-sdk):
- 2.2.0 (2026-09-07). v2.0.0 (2026-07-28): „It supports the 2026-07-28 revision of the Model Context Protocol and serves every earlier revision from the same server."
- v1 **nie wspiera**: `LATEST_PROTOCOL_VERSION = "2025-11-25"` w `src/mcp/types.py` (gałąź `v1.x`).
- API v2: `FastMCP` → `MCPServer`; pierwszej klasy `Client`; testy in-process.
- **⚠️ FLAG (sprzeczność):** notatki v2.0.0 mówiły, że v1.x dostanie „only security fixes", ale po tym wydano 1.29.1 i 1.30.0 z realnymi poprawkami i zmianami zachowania. Aktualny README mówi „critical bug fixes and security patches" — traktować „security fixes only" jako nieaktualne.

**Go**: v1.8.0; pierwsze wsparcie w **v1.7.0** („This release brings full support for protocol version 2026-07-28"). README ma oficjalną tabelę „Version Compatibility": `v1.7.0+ → 2026-07-28`. HTTP serwuje 2026-07-28 **tylko gdy `StreamableHTTPOptions.Stateless = true`**; inaczej negocjacja w dół do 2025-11-25. **⚠️ FLAG:** rozjazd dat tej samej wersji v1.8.0 — tag/commit 2026-09-04, publikacja release 2026-09-14.

**Java**: `io.modelcontextprotocol.sdk:mcp` **2.0.1** (2026-08-19). **NIE wspiera 2026-07-28.** CHANGELOG (verbatim): `2.0.x | 2.0.1 (2026-08-19) | 2025-11-25 | Active development`. Wersja 2.0.0: „First major release since 1.x, tracking the **2025-11-25** MCP specification." Java jest **Tier 2** na oficjalnej stronie SDK (Tier 2 = nowe funkcje protokołu „within 6 months"). **⚠️ FLAG: nie znalazłem publicznego roadmapu ani ETA dla 2026-07-28 w Java SDK.**

**C#**: `ModelContextProtocol` **2.2.0** (2026-08-13); pierwsze stabilne wsparcie w **2.0.0** (NuGet 2026-07-28T21:38:31Z). Ogłoszenie: „Announcing v2.0 of the official MCP C# SDK", Jeff Handley, **28 lipca 2026** — https://devblogs.microsoft.com/dotnet/announcing-v20-of-the-official-mcp-csharp-sdk/ . v2.0 jest zgodne wstecznie; `HttpServerTransportOptions.Stateless` domyślnie `true`; Roots/Sampling/Logging oznaczone `[Obsolete]` (MCP9005/9006); Tasks w `ModelContextProtocol.Extensions.Tasks`, MCP Apps w `ModelContextProtocol.Extensions.Apps`.

### Oficjalna skonsolidowana strona SDK × rewizja — NIE ISTNIEJE
**⚠️ FLAG:** `https://modelcontextprotocol.io/docs/sdk` przekierowuje na `https://modelcontextprotocol.io/docs/2026-07-28/sdk` i grupuje SDK **wyłącznie według tierów** — TypeScript / Python / C# / Go / **Rust** = **Tier 1**; Java / Ruby = Tier 2; Swift / PHP / Kotlin = Tier 3 — **bez kolumny rewizji specyfikacji i bez numerów wersji**.
`https://modelcontextprotocol.io/community/sdk-tiers` definiuje wymagania tierów (Tier 1: 100% conformance + nowe funkcje protokołu „before new spec version release"; Tier 2: 80% + „within 6 months"), ale nie jest macierzą zgodności.
Najbliższe oficjalne, zbiorcze stwierdzenie to akapit z bloga 2026-07-28: „All four Tier 1 SDKs speak `2026-07-28` as of today: TypeScript, Python, Go, C#. Beyond the Tier 1 set, the Rust SDK supports the new spec in beta." — **Java jest tam nieobecna, a wzmianka o Rust jest nieaktualna.**

---

## Zbiorcza lista pozycji niezweryfikowanych / sprzecznych

1. **`untrustedHint` nie istnieje** — ani w 2026-07-28, ani w 2025-11-25. `ToolAnnotations` jest niezmienione między rewizjami. Istnieją tylko propozycje: `trustedHint` (#1487), `secretHint` (#1560), `unsafeOutputHint` (#1561) — wszystkie „Proposal"; SEP #1913 i #1984 — oba **open/Draft**.
2. **CIMD nie jest nowością 2026-07-28** — weszło w 2025-11-25 (SEP-991 / PR #1296). Zmianą 2026-07-28 jest **deprecjacja DCR na rzecz CIMD**.
3. **PR #2858 to „Authorization spec split"**, nie SEP CIMD — cytowany dla deprecjacji DCR.
4. **MCP Apps nie ma rewizji 2026-07-28** — stabilna specyfikacja to `2026-01-26`, wersja protokołu rozszerzenia przypięta na `"2026-01-26"`; `draft` wyprzedza stabilną.
5. **`/.well-known/mcp.json` nie istnieje** jako mechanizm MCP. Pending SEP-2127 proponuje `/.well-known/ai-catalog.json` (AI Catalog) + Server Card pod `<streamable-http-url>/server-card`; to rozszerzenie ma status **Experimental**, a PR SEP-2127 jest **nadal otwarty** (aktualizacja 2026-09-12).
6. **`taskSupport` zostało usunięte** — istniało w rdzeniu 2025-11-25 (`Tool.execution`), nie ma go w rdzeniu 2026-07-28 ani w schemacie ext-tasks.
7. **`resource_link` w wynikach narzędzi — bez zmian** między rewizjami.
8. **Brak tabeli zgodności wersji specyfikacji w README Rust `rmcp`** — istnieje tylko proza. Tabele mają Go i Java.
9. **Brak publicznego ETA dla Java SDK** na 2026-07-28.
10. **Brak skonsolidowanej oficjalnej strony SDK × rewizja specyfikacji.**
11. **Sprzeczności wewnętrzne w specyfikacji MCP Apps:** MIME „MUST" vs „SHOULD"; domyślny CSP `connect-src 'none'` vs przykład kodu `connect-src 'self'`; brak normatywnego przypięcia atrybutu `sandbox` wewnętrznego iframe; brak normatywnej walidacji `targetOrigin` w `postMessage`.
12. **Konflikt źródeł — fallback tekstowy `structuredContent`:** strona `server/tools` mówi SHOULD, SEP-2106 mówi MUST dla tablic/prymitywów.
13. **Luka w dokumentacji:** strona `server/tools` nie dokumentuje adnotacji ani ich wartości domyślnych — istnieją wyłącznie w `schema.ts`/`schema.json`.
14. **Rozbieżność w macierzy rozszerzeń:** Tasks nie ma wiersza na https://modelcontextprotocol.io/extensions/client-matrix mimo linkowania z jego strony.
15. **Data Go v1.8.0:** tag 2026-09-04 vs publikacja release 2026-09-14.
16. **„Security fixes only" dla Python v1** — sprzeczne z późniejszymi wydaniami 1.29.1 / 1.30.0.
17. **SEP-2663 odwołuje się do nieistniejącej rewizji „2026-06-30"** — prawdopodobnie pozostałość robocza.
18. **SEP-837 i SEP-2352 nie mają stron `/seps/`** ani wpisów w indeksie — pochodzą sprzed workflow PR-owego; zweryfikowane przez tytuły PR, changelog i tekst normatywny.
19. **Brak formalnej gramatyki `ui://`** — jedyna reguła normatywna to „MUST start with `ui://`".
20. **GitHub REST API był limitowany (HTTP 403) dla tego IP** w trakcie badania; część weryfikacji GitHub oparto na `raw.githubusercontent.com`, stronach renderowanych i `releases.atom`. Żadna liczba w raporcie nie opiera się na snippecie wyszukiwarki.

---

## Źródła pierwotne

### Specyfikacja i dokumentacja MCP
- https://modelcontextprotocol.io/specification/2026-07-28/changelog — *Key Changes* (changelog 2026-07-28)
- https://modelcontextprotocol.io/specification/2026-07-28 — indeks specyfikacji 2026-07-28
- https://modelcontextprotocol.io/specification/2026-07-28/basic/index — protokół bazowy, `_meta`, kody błędów, JSON Schema usage, `$ref`, bezstanowość
- https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning — negocjacja wersji, rozszerzenia, zgodność wsteczna
- https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr — MRTR
- https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions — `subscriptions/listen`
- https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress — progress (niezdeprecjonowane)
- https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http — nagłówki, walidacja, zgodność wsteczna
- https://modelcontextprotocol.io/specification/2026-07-28/server/discover — `server/discover`
- https://modelcontextprotocol.io/specification/2026-07-28/server/tools — narzędzia, `outputSchema`, `structuredContent`, `x-mcp-header`
- https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/logging — Logging (deprecated)
- https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation — Elicitation
- https://modelcontextprotocol.io/specification/2026-07-28/client/sampling — Sampling (deprecated)
- https://modelcontextprotocol.io/specification/2026-07-28/client/roots — Roots (deprecated)
- https://modelcontextprotocol.io/specification/2026-07-28/deprecated — rejestr funkcji wycofanych
- https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization + `/authorization-server-discovery` + `/client-registration` + `/security-considerations`
- https://modelcontextprotocol.io/specification/2025-11-25/changelog — changelog 2025-11-25
- https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization — CIMD obecne już w 2025-11-25
- https://modelcontextprotocol.io/specification/draft/changelog — pusty (brak nowszej rewizji)
- https://github.com/modelcontextprotocol/specification/blob/main/schema/2026-07-28/schema.ts — źródło prawdy schema 2026-07-28
- https://github.com/modelcontextprotocol/specification/blob/main/schema/2025-11-25/schema.ts — schema 2025-11-25 (`taskSupport` obecne)

### SEP-y (pliki i PR-y)
- SEP-2575 *Make MCP Stateless* (Final) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2575
- SEP-2567 *Sessionless MCP via Explicit State Handles* (Final, 2026-03-11) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567
- SEP-2322 *Multi Round-Trip Requests* (Final, 2026-02-03) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2322
- SEP-2663 *Tasks Extension* (Final, Extensions Track, 2026-04-27) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2663
- SEP-2577 *Deprecate Roots, Sampling, and Logging* (Final, 2026-04-14) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2577
- SEP-2596 *Specification Feature Lifecycle and Deprecation Policy* (Final, 2026-04-17) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2596
- SEP-2106 *Tools `inputSchema` & `outputSchema` Conform to JSON Schema 2020-12* (Final, 2026-01-06) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2106
- SEP-2549 *TTL for List Results* (Final, 2026-04-09) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2549
- SEP-2243 *HTTP Header Standardization for Streamable HTTP Transport* (Final, 2026-02-04) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2243
- SEP-2468 *Recommend Issuer (iss) Parameter in MCP Auth Responses* (Final, 2026-03-25) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2468
- SEP-2164 *Standardize Resource Not Found Error Code* (Final, 2026-01-28) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2164
- SEP-2133 *Extensions* (Final) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2133
- SEP-1865 *MCP Apps* (Final, Extensions Track, 2025-11-21) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1865
- SEP-991 *URL-based Client Registration using OAuth Client ID Metadata Documents* (Final, 2025-07-07; realizacja PR #1296, zmergowany 2025-11-14) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/991
- PR #2858 „Authorization spec split" — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2858
- SEP-837 / PR #837 — `application_type` w DCR — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/837
- SEP-2352 / PR #2352 — powiązanie poświadczeń z AS — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2352
- SEP-2127 *MCP Server Cards* (PR **otwarty**, utworzony 2026-01-21, aktualizacja 2026-09-12) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2127
- SEP-1649 (poprzednik, zamknięty 2026-01-26) — https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1649
- SEP-1913 *Trust and Sensitivity Annotations* (open) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1913
- SEP-1984 *Comprehensive Tool Annotations for Governance/UX* (open) — https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1984
- SEP-1487 `trustedHint` / SEP-1560 `secretHint` / SEP-1561 `unsafeOutputHint` — https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1487 , /1560 , /1561
- Katalog SEP (42 pliki, stan 2026-09-19) — https://github.com/modelcontextprotocol/modelcontextprotocol/tree/main/seps

### Rozszerzenia
- https://modelcontextprotocol.io/extensions/overview — ramy rozszerzeń, negocjacja
- https://modelcontextprotocol.io/extensions/tasks/overview — Tasks
- https://github.com/modelcontextprotocol/ext-tasks — README + `schema/2026-07-28/schema.ts`
- https://modelcontextprotocol.io/extensions/apps/overview — MCP Apps
- https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx — **stabilna** specyfikacja MCP Apps
- https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/draft/apps.mdx — draft
- https://modelcontextprotocol.io/extensions/client-matrix — macierz wsparcia klientów
- https://github.com/modelcontextprotocol/ext-server-card — Server Cards (Experimental)
- https://github.com/modelcontextprotocol/ext-auth — rozszerzenia autoryzacyjne
- https://modelcontextprotocol.io/registry/about — MCP Registry (`server.json`)
- https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md

### Blog (oficjalny)
- „The 2026-07-28 Specification" — 2026-07-28 — https://blog.modelcontextprotocol.io/posts/2026-07-28/
- „The New MCP Roadmap" — 2026-08-22 — https://blog.modelcontextprotocol.io/posts/mcp-roadmap/
- „Beta SDKs for the 2026-07-28 MCP Spec Release Candidate Are Here" — 2026-06-29 — https://blog.modelcontextprotocol.io/posts/sdk-betas-2026-07-28/
- „The 2026-07-28 MCP Specification Release Candidate" — 2026-05-21 — https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/
- „Tool Annotations as Risk Vocabulary" — 2026-03-16 — https://blog.modelcontextprotocol.io/posts/2026-03-16-tool-annotations/
- „MCP Apps - Bringing UI Capabilities To MCP Clients" — 2026-01-26 — https://blog.modelcontextprotocol.io/posts/2026-01-26-mcp-apps/
- „MCP Apps: Extending servers with interactive user interfaces" — 2025-11-21 — https://blog.modelcontextprotocol.io/posts/2025-11-21-mcp-apps/
- „One Year of MCP: November 2025 Spec Release" — 2025-11-25 — https://blog.modelcontextprotocol.io/posts/2025-11-25-first-mcp-anniversary/

### Rejestry pakietów i SDK (wszystkie pobrane 2026-09-19)
- https://crates.io/api/v1/crates/rmcp — 3.4.0, 2026-09-15T15:44:08Z
- https://crates.io/api/v1/crates/rmcp/versions — 3.0.0 @ 2026-07-28T22:52:36Z; 3.0.0-beta.1 @ 2026-07-23T18:50:13Z
- https://docs.rs/rmcp
- https://github.com/modelcontextprotocol/rust-sdk — README („official… implements the stable MCP `2026-07-28` specification")
- https://github.com/modelcontextprotocol/rust-sdk/discussions/969 — przewodnik migracji 3.x
- https://registry.npmjs.org/@modelcontextprotocol/sdk — 1.30.0 @ 2026-07-27T17:56:01Z
- https://registry.npmjs.org/@modelcontextprotocol/server — 2.0.0 @ 2026-07-27T23:55:22Z
- https://registry.npmjs.org/@modelcontextprotocol/client — 2.0.0 @ 2026-07-27T23:55:22Z
- https://github.com/modelcontextprotocol/typescript-sdk — README v2
- https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28.html — opt-in
- https://pypi.org/pypi/mcp/json — 2.2.0; 2.0.0 @ 2026-07-28T13:45:28Z
- https://github.com/modelcontextprotocol/python-sdk — README v2
- https://github.com/modelcontextprotocol/python-sdk/blob/v1.x/src/mcp/types.py — `LATEST_PROTOCOL_VERSION = "2025-11-25"`
- https://proxy.golang.org/github.com/modelcontextprotocol/go-sdk/@latest — v1.8.0, 2026-09-04T08:08:52Z
- https://github.com/modelcontextprotocol/go-sdk — README, tabela „Version Compatibility"
- https://repo1.maven.org/maven2/io/modelcontextprotocol/sdk/mcp/maven-metadata.xml — 2.0.1, lastUpdated 20260819143251
- https://github.com/modelcontextprotocol/java-sdk/blob/main/CHANGELOG.md — `2.0.x | 2.0.1 (2026-08-19) | 2025-11-25`
- https://api.nuget.org/v3/registration5-gz-semver2/modelcontextprotocol/index.json — 2.0.0 @ 2026-07-28T21:38:31Z; 2.2.0 @ 2026-08-13T08:56:14Z
- https://devblogs.microsoft.com/dotnet/announcing-v20-of-the-official-mcp-csharp-sdk/ — Jeff Handley, 28 lipca 2026
- https://modelcontextprotocol.io/docs/2026-07-28/sdk — oficjalna lista SDK (tylko tiery)
- https://modelcontextprotocol.io/community/sdk-tiers — wymagania tierów

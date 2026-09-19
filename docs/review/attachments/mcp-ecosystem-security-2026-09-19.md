# Bezpieczeństwo ekosystemu MCP — stan na 2026-09-19

> Raport dla konsumenta: **realny serwer pamięci MCP** (Vestige).
> Każda teza ma URL i datę. Sekcja 8 jawnie oznacza rzeczy **niezweryfikowane**.
> Metoda: źródła pierwotne (NVD REST API, GitHub Security Advisories API, OSV.dev API, CVE Program `CNAsList.json`, repozytoria OWASP, arxiv.org/abs, spec MCP 2026-07-28). Blogi użyte wyłącznie jako tropy do dalszej weryfikacji.

---

## 0. Wnioski krytyczne (TL;DR)

1. **OWASP MCP Top 10 nadal istnieje tylko w edycji 2025.** W repozytorium nie ma katalogu `2026/`; lista to `MCP01:2025`–`MCP10:2025`, wersja dokumentu `v0.1`, roadmapa w fazie „Beta Release and Pilot Testing — We are here right now”. Ostatni commit: **2026-07-29**. ([owasp.org](https://owasp.org/www-project-mcp-top-10/), [repo](https://github.com/OWASP/www-project-mcp-top-10))
2. **Anthropic jest oficjalnym CNA dla MCP** — `CNA-2026-0052`, zakres obejmuje wprost „the Model Context Protocol (MCP) SDKs and reference servers”. To jedyna odpowiedź na pytanie „czy MCP ma własny CNA”. ([CVE Program CNAsList.json](https://raw.githubusercontent.com/CVEProject/cve-website/dev/src/assets/data/CNAsList.json))
3. **Skala CVE eksplodowała w 2026**: w oknie 2026-06-01 → 2026-09-19 NVD zwraca **191 CVE** pasujących do frazy „MCP server” — w tym **35 CRITICAL** i **84 HIGH**. ([NVD API](https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=MCP%20server&pubStartDate=2026-06-01T00:00:00.000&pubEndDate=2026-09-19T23:59:59.999))
4. **Są już CVE wymierzone w serwery pamięci MCP.** `CVE-2026-50027` (mcp-memory-service, CVSS 9.8) to brak uwierzytelnienia na `/api/documents/*` → nieuwierzytelniony odczyt/zapis/**trwałe kasowanie** wspomnień. To najbliższy wzorzec zagrożenia dla Vestige.
5. **Spec MCP przyjął w 2026-07-28 realne mechanizmy obronne, ale NIE na tool poisoning.** Są nagłówki `Mcp-Method`/`Mcp-Name`, `ttlMs`/`cacheScope`, twardnienie OAuth (`iss`, `application_type`). **Nie ma** hashowania definicji narzędzi, podpisanych manifestów narzędzi ani pola `untrustedHint` — to trzeba zbudować po stronie serwera.
6. **Oficjalny MCP Registry nie robi żadnego skanowania bezpieczeństwa** i wprost deklaruje, że **nie usuwa serwerów z podatnościami**. ([moderation policy](https://modelcontextprotocol.io/registry/moderation-policy))
7. **Najważniejszy wynik naukowy to wynik negatywny**: zatrucie **1,2%** korpusu pamięci obniża dokładność z 0,850 do 0,300, a pipeline screeningu przy 0,832 recall **odrzucił 0 z 360** zatrutych wspomnień. ([arXiv:2608.21230](https://arxiv.org/abs/2608.21230), 2026-08-21)

---

## 1. OWASP MCP Top 10 — status, wersja, dokładna lista

### 1.1 Identyfikacja projektu

| Pole | Wartość |
|---|---|
| Oficjalna strona | https://owasp.org/www-project-mcp-top-10/ |
| Repozytorium | https://github.com/OWASP/www-project-mcp-top-10 |
| Projekt lead | Vandana Verma Sehgal |
| `project.owasp.yaml` | `level: 2`, `type: documentation`, `audience: [builder, defender]` |
| Utworzone | 2025-06-18 |
| Ostatni push | **2026-07-29** |
| Metadane zaktualizowane | 2026-09-10 |
| Licencja | CC BY-NC-SA 4.0 |

### 1.2 Status — czy to nadal beta/incubator?

Na oficjalnej stronie projekt jest oznaczony jako **„Production Project” / Classification: Documentation**. Jednak **roadmapa na tej samej stronie mówi wprost**:

> „Phase 3 – Beta Release and Pilot Testing – **We are here right now**”

a nagłówek pliku `tab_top10.md` brzmi: `## OWASP Top 10 for Model Context Protocol version [v0.1]`.

**Interpretacja (jawnie oznaczona jako moja):** metadane rejestru OWASP mówią „Production”, ale treść merytoryczna jest autodeklarowana jako **v0.1 w fazie beta (Faza 3/5)**. Nie ma edycji 2026. ([tab_top10.md](https://raw.githubusercontent.com/OWASP/www-project-mcp-top-10/main/tab_top10.md))

### 1.3 Dokładna lista 10 kategorii (tytuły verbatim ze strony OWASP)

| ID | Tytuł oficjalny (verbatim) |
|---|---|
| MCP01:2025 | Token Mismanagement & Secret Exposure |
| MCP02:2025 | Privilege Escalation via Scope Creep |
| MCP03:2025 | Tool Poisoning |
| MCP04:2025 | Software Supply Chain Attacks & Dependency Tampering |
| MCP05:2025 | Command Injection & Execution |
| MCP06:2025 | Prompt Injection via Contextual Payloads |
| MCP07:2025 | Insufficient Authentication & Authorization |
| MCP08:2025 | Lack of Audit and Telemetry |
| MCP09:2025 | Shadow MCP Servers |
| MCP10:2025 | Context Injection & Over-Sharing |

> Uwaga na formatowanie: strona OWASP i `tab_top10.md` używają **`MCP1:2025`…`MCP9:2025`** (bez zera wiodącego) w `tab_top10.md`, a **`MCP01`…`MCP10`** w roadmapie i na stronie. Cytując, używaj formy z zerem — jest zgodna z nazwami plików.

### 1.4 Zmiany względem wcześniej opublikowanej listy — **ZNALEZIONO ROZBIEŻNOŚĆ**

To jest najważniejsze ustalenie w tej sekcji i jest **weryfikowalne**:

- Katalog `2025/` zawiera plik **`MCP06-2025–Intent-Flow-Subversion.md`**, którego frontmatter brzmi:
  `title: "MCP06:2025 – Intent Flow Subversion"`.
- Tymczasem **indeks i strona OWASP** tytułują MCP06 jako **„Prompt Injection via Contextual Payloads”**.
- **Link na oficjalnej stronie OWASP do MCP06 prowadzi do pliku, który nie istnieje.** Zweryfikowane bezpośrednio:

```
404  <-  MCP06-2025–Prompt-InjectionviaContextual-Payloads.md   (link ze strony OWASP)
200  <-  MCP06-2025–Intent-Flow-Subversion.md                   (plik faktyczny)
```

- Plik MCP06 dodano commitami `add-intent-flow-subversion` z **2026-01-12**. Treść opisuje goal hijacking, hidden instructions w `resources/` i `tool outputs`, oraz „Stealthy Persistence: attackers can inject meta-instructions into long-lived MCP contexts that alter the agent's behavior across multiple unrelated sessions” — czyli **bezpośrednio atak na trwałą pamięć**.

**Wniosek:** MCP06 zostało najprawdopodobniej przemianowane/przepisane („Intent Flow Subversion” → „Prompt Injection via Contextual Payloads”), ale **indeks i linki nie zostały zsynchronizowane**. Cytując OWASP MCP Top 10, trzeba to rozstrzygnąć świadomie.

### 1.5 Aktywność redakcyjna w 2026 (dowód, że lista jest utrzymywana)

| Data | Zmiana |
|---|---|
| 2026-07-29 | Merge PR #38 — `mcp03-static-detection-indicators` |
| 2026-07-29 | Merge PR #45 — `control/client-side-tool-risk-gating` |
| 2026-07-11 | Dodano kontrolę: client-side tool risk gating dla hostów MCP |
| 2026-06-06 | MCP03: dodano statyczne wskaźniki detekcji dla description-based tool poisoning |
| 2026-03-13 | Dodano MCPS jako rekomendowaną kontrolę kryptograficzną |
| 2026-03-10 | Dodano „References & Further Reading” do wszystkich 10 wpisów |
| 2026-01-12 | Dodano Intent Flow Subversion (MCP06) |

### 1.6 Pokrewny, nowszy projekt OWASP (kontekst)

OWASP GenAI Security Project prowadzi **równoległe** listy, których nie należy mylić z MCP Top 10:
- **OWASP Top 10 for Agentic Applications for 2026** — https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/ (potwierdzone, że strona istnieje)
- **CheatSheet — A Practical Guide for Securely Using Third-Party MCP Servers 1.0** — https://genai.owasp.org/resource/cheatsheet-a-practical-guide-for-securely-using-third-party-mcp-servers-1-0/
  ⚠️ **NIE ZWERYFIKOWANO**: dokładnej daty publikacji obu dokumentów (strony genai.owasp.org renderują treść po stronie klienta; nie udało się wyciągnąć daty ze źródła pierwotnego).

---

## 2. Znane CVE w ekosystemie MCP

### 2.1 CVE wskazane w zadaniu — wszystkie zweryfikowane w NVD

| CVE | Produkt | Typ | CVSS | Publikacja | GHSA |
|---|---|---|---|---|---|
| **CVE-2025-6514** | `mcp-remote` (npm) 0.0.5–0.1.15 | OS command injection przez `authorization_endpoint` | **9.6 CRITICAL** (v3.1) | 2025-07-09 | [GHSA-6xpm-ggf7-wc3p](https://github.com/advisories/GHSA-6xpm-ggf7-wc3p) |
| **CVE-2025-32711** | Microsoft 365 Copilot (EchoLeak) | AI command injection → wyciek danych | **9.3 CRITICAL** (MSFT) / 7.5 HIGH (NVD) | 2025-06-11 | — ([MSRC](https://msrc.microsoft.com/update-guide/vulnerability/CVE-2025-32711)) |
| **CVE-2025-49596** | MCP Inspector < 0.14.1 | Brak uwierzytelnienia client↔proxy → RCE | **9.4 CRITICAL** (v4.0) | 2025-06-13 | [GHSA-7f8r-222p-6f5g](https://github.com/modelcontextprotocol/inspector/security/advisories/GHSA-7f8r-222p-6f5g) |
| **CVE-2025-54135** | Cursor < 1.3.9 | Zapis `.cursor/mcp.json` bez zgody → RCE | 8.5 HIGH (GHSA) / **9.8 CRITICAL** (NVD) | 2025-08-05 | [GHSA-4cxx-hrm3-49rm](https://github.com/cursor/cursor/security/advisories/GHSA-4cxx-hrm3-49rm) |
| **CVE-2025-54136** | Cursor ≤ 1.2.4 | **Rug pull** — podmiana zatwierdzonego MCP na złośliwy | 7.2 HIGH (GHSA) / 8.8 HIGH (NVD) | 2025-08-02 | [GHSA-24mc-g4xr-4395](https://github.com/cursor/cursor/security/advisories/GHSA-24mc-g4xr-4395) |
| **CVE-2025-59536** | Claude Code < 1.0.111 | Code injection przed akceptacją trust dialog | **8.7 HIGH** (v4.0) | 2025-10-03 | [GHSA-4fgq-fpq9-mr3g](https://github.com/anthropics/claude-code/security/advisories/GHSA-4fgq-fpq9-mr3g) |
| **CVE-2026-21852** | Claude Code < 2.0.65 | Exfiltracja `ANTHROPIC_BASE_URL`/kluczy API **przed** trust promptem | 5.3 MEDIUM (v4.0) / 7.5 HIGH (NVD) | **2026-01-21** | [GHSA-jh7p-qr78-84p7](https://github.com/anthropics/claude-code/security/advisories/GHSA-jh7p-qr78-84p7) |
| **CVE-2026-88938** | `knowns` (npm) ≤ 0.33.0 | **Path traversal** w narzędziu MCP `code.find` — odczyt plików poza projektem | **7.1 HIGH** (v4.0) / 6.5 MEDIUM (v3.1) | **2026-09-10** | [GHSA-5cj9-fcqq-g2h7](https://github.com/knowns-dev/knowns/security/advisories/GHSA-5cj9-fcqq-g2h7) |
| **CVE-2026-53708** | IBM ContextForge / mcp-context-forge < 1.0.3 | **SSRF via DNS rebinding** (TOCTOU na walidacji URL) | 6.6 MEDIUM (v3.1) | **2026-09-14** | [GHSA-9hgc-g3w5-67cm](https://github.com/IBM/mcp-context-forge/security/advisories/GHSA-9hgc-g3w5-67cm) |

**Uwagi precyzyjne (ważne dla cytowania):**

- **CVE-2025-49596 nie jest „Anthropic MCP Inspector”** w sensie własności. Dotyczy `modelcontextprotocol/inspector` — narzędzia **deweloperskiego** do testowania serwerów MCP, utrzymywanego w organizacji `modelcontextprotocol`. Anthropic jest stewardem MCP (i od 2026 CNA), ale formułowanie „Anthropic MCP Inspector RCE” jest nieścisłe. ([NVD](https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2025-49596))
- **CVE-2026-88938 i CVE-2026-53708 są prawdziwe i bardzo świeże** — oba opublikowane w ciągu 10 dni przed datą raportu. SSVC dla CVE-2026-88938 wskazuje `exploitation: poc`. To nie są pozycje fikcyjne.
- **Statusy NVD**: większość z tych rekordów ma `vulnStatus: "Deferred"` lub `"Received"`, co oznacza, że NVD **nie przeprowadziło własnej analizy** — dane CVSS pochodzą od CNA, nie od NIST. CVE-2025-32711 i CVE-2025-54135/54136 mają status `"Modified"`/`"Analyzed"` i **podwójne metryki** (vendor vs NVD), które się rozjeżdżają (np. 8.5 vs 9.8). Cytuj oba.

### 2.2 Pakiety wskazane w zadaniu — status zweryfikowany przez OSV.dev i NVD

| Pakiet / produkt | Ustalenie |
|---|---|
| **git-mcp / mcp-server-git** | `mcp-server-git` (PyPI) — **4 CVE**: CVE-2025-68143 (nieograniczone `git_init` → repo w dowolnym miejscu), CVE-2025-68144 (argument injection w `git_diff`/`git_checkout` → nadpisanie plików), CVE-2025-68145 (brak walidacji ścieżki przy `--repository`), **CVE-2026-27735** (path traversal w `git_add`, 2026-02-26). Dodatkowo **MAL-2026-5478** — złośliwy pakiet npm `mcp-server-git` (2026-06-09). Osobny projekt `git-mcp-server` (cyanheads): **CVE-2026-85626**, argument injection w `git_log`/`git_diff`/`git_show` przez `--output=`, 8.7 HIGH, 2026-09-04. |
| **mcp-server-filesystem** | `@modelcontextprotocol/server-filesystem` (npm) — **2 CVE**: CVE-2025-53109 (path validation bypass via prefix matching + symlink), CVE-2025-53110 (colliding path prefix), oba 2025-07-01. `mcp-server-filesystem` (PyPI) — 0. |
| **Postgres MCP** | `awslabs.postgres-mcp-server` < 1.1.7 — **CVE-2026-85787** (niekompletna denylista SQL → modyfikacja danych poza trybem read-only, 7.1 HIGH, 2026-09-04) i **CVE-2026-87911** (`COPY ... TO PROGRAM` → **OS command injection w trybie read-only**, **9.0 CRITICAL**, 2026-09-09). Referencyjny `@modelcontextprotocol/server-postgres` — **0 advisories** (pakiet deprecated). |
| **mcp-remote** | CVE-2025-6514 (patrz 2.1). OSV zwraca dokładnie 1 wpis. |
| **Docker MCP Gateway** | **8 advisories**; 6 opublikowanych **2026-07-09**, w tym **2 CRITICAL**: CVE-2026-76099 (niezwalidowany bind mount z configu → root RCE przez Docker socket) i CVE-2026-76092 (nieuwierzytelniony dostęp do proxowanych narzędzi w trybie kontenera). Dalej: CVE-2026-76094 (**weryfikacja podpisów obrazów wyłączona domyślnie**, high), **CVE-2026-76097 (tool-name shadowing między agregowanymi serwerami — medium — bezpośrednio klasa rug pull/shadowing)**, CVE-2026-76095 (`file://` → odczyt plików hosta), CVE-2026-76093 (SSRF), CVE-2026-55887 (argument injection przez etykietę OCI, 2026-06-16), CVE-2025-64443 (DNS rebinding, 2025-12-03). |
| **Atlassian MCP** | `mcp-atlassian` (PyPI) — **5 CVE**: CVE-2026-73496 (arbitralny odczyt plików serwera przez `confluence_upload_attachment`, 7.7 HIGH, 2026-09-14), CVE-2026-73498 (j.w. przez brak `validate_safe_path`, 7.7 HIGH, 2026-08-12), CVE-2026-73497 (DNS-rebinding TOCTOU obchodzący wcześniejszy fix SSRF), CVE-2026-27825 (arbitralny zapis pliku → RCE), CVE-2026-27826 (SSRF przez niezwalidowane nagłówki `X-Atlassian-*`). Wszystkie naprawione w **0.22.0**. |
| **Supabase MCP** | **BRAK CVE i BRAK GHSA.** OSV.dev dla `@supabase/mcp-server-supabase`, `supabase-mcp`, `mcp-server-supabase` zwraca 0 wyników. Znane ryzyko ma charakter **projektowy, nie CVE**: serwer działa z `service_role`, który omija RLS. ⚠️ **NIE ZWERYFIKOWANO** źródła pierwotnego dla tego ryzyka — brak advisory do zacytowania. |

### 2.3 CVE dotyczące **serwerów pamięci** — najważniejsze dla Vestige

| CVE | Produkt | Co pozwala zrobić atakującemu | CVSS | Data |
|---|---|---|---|---|
| **CVE-2026-50027** | `mcp-memory-service` < 10.67.1 | **Wszystkie trasy `/api/documents/*` bez uwierzytelnienia**, mimo skonfigurowanego `MCP_API_KEY`/OAuth. Nieuwierzytelniony atakujący: **upload dowolnej treści do magazynu pamięci, odczyt dokumentów, trwałe kasowanie wspomnień** należących do uwierzytelnionych użytkowników. | **9.8 CRITICAL** (CWE-306) | 2026-08-14 |
| **CVE-2026-33010** | `mcp-memory-service` | Wildcard CORS **z** credentials → **cross-origin memory theft** | — | 2026-03-07 |
| **CVE-2026-49291** | `mcp-memory-service` | Klienci OAuth z prawem **read-only** mogą **zapisywać i kasować wspomnienia** przez `tools/call` (egzekwowanie tylko w warstwie prezentacji) | — | 2026-06-26 |
| **CVE-2026-29787** | `mcp-memory-service` | Ujawnienie informacji o systemie przez health endpoint | — | 2026-03-05 |
| **CVE-2026-49986** | Cortex MCP / `neuro-cortex-memory` < 3.17.1 | `CLAUDE_PROJECT_DIR` (ustawiany automatycznie przez Claude Code) traktowany jako zaufany checkout deweloperski → **wykonanie kodu z niezaufanego projektu** przy `open_visualization` | 7.1 HIGH (CWE-829) | 2026-08-14 |
| **CVE-2026-31240 / -31241 / -31245** | **mem0** (serwer) | Brak uwierzytelnienia i autoryzacji na API **zarządzania / tworzenia / kasowania pamięci** | — | 2026-05-12 |
| **CVE-2026-7597** | mem0 | Improper input validation | — | 2026-05-02 |
| **CVE-2026-54504** | MCP Documentation Server 1.13.0–1.13.1 | Web UI startuje domyślnie, `app.listen(PORT)` **bez hosta** → nieuwierzytelnione API zarządzania dokumentami na **wszystkich interfejsach** (semantic search) | **8.8 HIGH** (CWE-306/668) | 2026-09-17 |
| **CVE-2026-57441** | MCPVault < 0.11.4 | `PathFilter` kompiluje wzorce **case-sensitively** → na macOS/Windows warianty `.git`, `.obsidian`, `node_modules` przechodzą walidację | 8.4 HIGH | 2026-09-15 |
| **CVE-2026-45829/45830/45831/45833** | **ChromaDB** | Pre-auth code injection; dowolny uwierzytelniony użytkownik może czytać/pisać/kasować dane **w kolekcji dowolnego tenanta**; `SimpleRBACAuthorizationProvider` nie sprawdza zakresu uprawnień | — | 2026-05-18 / 2026-06-12 |

**Wzorzec, który się powtarza (istotny dla Vestige):**
1. **Uwierzytelnienie egzekwowane tylko w warstwie discovery (`tools/list`), a nie w `tools/call`** — CVE-2026-46519 (mcp-server-kubernetes, 8.8 HIGH) pokazuje to wprost: „The access control was effectively cosmetic”. Ten sam wzorzec w CVE-2026-49291 (mcp-memory-service).
2. **Bindowanie do wszystkich interfejsów zamiast localhost** — CVE-2026-54504, CVE-2026-50027.
3. **Brak rozróżnienia principal/sesji** — CVE-2026-52869 (MCP Python SDK: HTTP transports obsługują żądania sesji **bez weryfikacji uwierzytelnionego principal”), CVE-2026-67431 (Ruby SDK: session ID nie związany z właścicielem), CVE-2026-48529 (GitHub MCP: process-global singleton `RepoAccessCache` współdzielony między użytkownikami).

### 2.4 CVE opublikowane w 2026 Q2/Q3 (czerwiec–wrzesień 2026)

Zapytanie do NVD REST API (`keywordSearch=MCP server`, `pubStartDate=2026-06-01`, `pubEndDate=2026-09-19`) zwraca **191 rekordów**.

| Miesiąc | Liczba CVE |
|---|---|
| 2026-06 | 27 |
| 2026-07 | 51 |
| 2026-08 | 69 |
| 2026-09 (do 19.) | 44 |

| Severity | Liczba |
|---|---|
| CRITICAL | 35 |
| HIGH | 84 |
| MEDIUM | 53 |
| LOW | 19 |

**Najistotniejsze dodatkowe pozycje z tego okna (poza wymienionymi wyżej):**

- **CVE-2026-52870** (7.6 HIGH, 2026-07-15) — MCP Python SDK 1.23.0–1.27.2: handlery zadań nie wiążą zadania z sesją → **dowolny klient może wyliczać, czytać wyniki i anulować zadania innych klientów**. Naprawione w 1.27.2.
- **CVE-2026-59950 / CVE-2026-52869** — MCP Python SDK: transport WebSocket bez walidacji Host/Origin; transporty HTTP bez weryfikacji principal.
- **CVE-2026-63127** (8.2 HIGH, 2026-09-16) — **RMCP (oficjalny Rust SDK)**: pominięte pole `resource` z RFC 9728 → **atak mix-up**, złośliwy serwer MCP może opublikować metadane innego, legalnego zasobu. Naprawione w 2.0.0.
- **CVE-2026-67431 / -67432** (2026-07-29) — MCP Ruby SDK < 0.23.0: session ID nie związany z właścicielem → `tools/call` w sesji ofiary.
- **CVE-2026-25536** (2026-02-04) — MCP TypeScript SDK: **cross-client data leak** przez współdzieloną instancję server/transport.
- **CVE-2026-0621** (2026-01-05) — MCP TypeScript SDK: ReDoS.
- **CVE-2025-66414 / CVE-2025-66416** (2025-12-02) — TS i Python SDK: **DNS rebinding protection wyłączona domyślnie**.
- **CVE-2026-53710** (**10.0 CRITICAL**, 2026-09-15) — IBM ContextForge < 1.0.2: `python_sandbox_server` wystawia surowe `getattr` przez `safe_builtins` → ucieczka z sandboxa i wykonanie poleceń OS.
- **CVE-2026-32625** (9.6 CRITICAL, 2026-06-02) — LibreChat ≤ 0.8.3: `${VAR}` w URL-u serwera MCP rozwiązywane z `process.env` → wyciek `CREDS_KEY`, `JWT_SECRET`, `MONGO_URI` do serwera atakującego.
- **CVE-2026-48529** (2026-06-26) — GitHub MCP Server 0.22.0–1.1.2: `--lockdown-mode` używa procesowo-globalnego singletonu z poświadczeniami **pierwszego** użytkownika.
- **CVE-2026-47427** (7.5 HIGH, 2026-07-28) — GitHub MCP Server < 1.1.0: nil pointer dereference **przed** uwierzytelnieniem → DoS.
- **CVE-2026-76099 / CVE-2026-76092 / CVE-2026-76094 / CVE-2026-76097** — Docker MCP Gateway (patrz 2.2).
- **CVE-2026-61559 / -61560 / -61568** — `@zereight/mcp-gitlab` (SSRF, nieuwierzytelniony odczyt plików → eksfiltracja PAT, DNS rebinding; 2026-09-15/16).

**Rodziny produktów generujące najwięcej CVE w tym oknie:** Flowise (MCP custom node → RCE, wielokrotnie: CVE-2025-71336, CVE-2026-56274, -58057, -69263, -91931, -91932), IBM Langflow (CVE-2026-7755, -9135, -9077, -17623, -17626, -78575, -81941), MCPHub (7 CVE naraz: CVE-2026-79744…-79750, w tym 9.9 CRITICAL), OpenClaw (**590 wpisów w OSV** — największy wolumen w całym ekosystemie).

---

## 3. Rejestry, trackery i CNA

### 3.1 Czy MCP ma oficjalnego CNA? — **TAK**

**Anthropic, PBC jest CNA o identyfikatorze `CNA-2026-0052`.** Zakres (verbatim z rejestru CVE Program):

> „Vulnerabilities in software, services, and open-source projects developed, maintained, or distributed by Anthropic. This includes Claude Code, Claude Desktop, browser extensions, **the Model Context Protocol (MCP) SDKs and reference servers**, Anthropic API client SDKs, and related developer tooling published under the Anthropic, anthropic-experimental, and **modelcontextprotocol** GitHub organizations.”
>
> „**Frontier AI Researcher CVE Numbering Authorities (CNAs) Pilot Scope:** Vulnerabilities discovered by Anthropic that are not within another CNA's scope.”

- Kontakt: `security-cna@anthropic.com`
- Typ: `Vendor` + `AI Researcher (Frontier AI Researcher CNA Pilot)`
- Root: MITRE. Kraj: USA.
- Advisories: https://trust.anthropic.com/resources#6a44049fd659a87d0068f0c5 oraz https://red.anthropic.com/2026/cvd/
- Źródło pierwotne: https://raw.githubusercontent.com/CVEProject/cve-website/dev/src/assets/data/CNAsList.json

**Kluczowe zastrzeżenie:** zakres Anthropic obejmuje **własne** SDK i serwery referencyjne. **Nie istnieje CNA obejmujący cały ekosystem MCP** — czyli tysiące serwerów firm trzecich. Dlatego CVE dla `mcp-atlassian`, `knowns`, `mcp-memory-service` itd. nadają **VulDB**, **VulnCheck** i **GitHub** (`security-advisories@github.com`), a nie Anthropic. W rejestrze CNA nie ma innego podmiotu, którego zakres wspomina „Model Context Protocol” (zweryfikowano przeszukanie wszystkich 549 wpisów).

**Inne istotne CNA AI:** OpenAI — `OAI`, `CNA-2026-0026`.

### 3.2 GitHub Advisory Database dla `modelcontextprotocol/*` — **TAK, są wpisy**

Zweryfikowane przez OSV.dev API (który konsumuje GHSA):

| Pakiet | Liczba advisories | Przykłady |
|---|---|---|
| `@modelcontextprotocol/sdk` (npm) | 3 | CVE-2026-25536, CVE-2026-0621, CVE-2025-66414 |
| `@modelcontextprotocol/inspector` (npm) | 2 | CVE-2025-49596, CVE-2025-58444 |
| `@modelcontextprotocol/server-filesystem` (npm) | 2 | CVE-2025-53109, CVE-2025-53110 |
| `mcp` (PyPI, Python SDK) | 6 CVE + 6 PYSEC | CVE-2026-52869, -52870, -59950, -2025-53365/53366/66416 |
| `mcp-server-git` (PyPI) | 4 CVE + 4 PYSEC | CVE-2025-68143/68144/68145, CVE-2026-27735 |

**Zapytanie programistyczne (zalecane):**
```
POST https://api.osv.dev/v1/query
{"package":{"name":"mcp","ecosystem":"PyPI"}}

GET https://api.github.com/advisories?affects=<nazwa-pakietu>
```

### 3.3 Społecznościowy tracker

- **`mcp-security-project/mcp-cve-project`** — https://github.com/mcp-security-project/mcp-cve-project — „The Project shares all information on MCP related CVE's published”. 28 gwiazdek. **Społecznościowy, nieoficjalny.**
- ⚠️ **NIE ZWERYFIKOWANO**: istnienia repozytorium o nazwie `mcp-security-advisories` (sugerowanej w zadaniu). Nie znaleziono.

### 3.4 Oficjalny MCP Registry — **brak skanowania bezpieczeństwa**

Z polityki moderacji (verbatim):

> „The MCP Registry **does not** make guarantees about moderation, and consumers should **assume minimal-to-no moderation**.”
>
> „**What We Don't Remove:** … **Servers with security vulnerabilities** …”

Rejestr usuwa wyłącznie treści nielegalne, malware, spam i serwery całkowicie niedziałające. Jedyna kontrola to dowód własności przestrzeni nazw (DNS/HTTP). Status: **preview** (`v1.8.1`, 2026-08-06).

**Konsekwencja dla Vestige:** obecność serwera w oficjalnym MCP Registry **nie jest sygnałem bezpieczeństwa**. Nie ma tam podpisywania, atestacji ani SLSA/sigstore.

---

## 4. Badania naukowe (2026)

> Pełny przegląd ~130 prac: `docs/research/mcp-memory-poisoning-literature-2026-09-19.md` (w tym workspace). Poniżej pozycje najistotniejsze, **spot-checkowane przeze mnie bezpośrednio przez `arxiv.org/abs`**.

### 4.1 Prace wskazane w zadaniu — wszystkie zweryfikowane

| arXiv | Tytuł | Data | Kluczowe liczby |
|---|---|---|---|
| [2508.14925](https://arxiv.org/abs/2508.14925) | MCPTox: A Benchmark for Tool Poisoning Attack on Real-World MCP Servers | 2025-08-19 | 45 serwerów / 353 narzędzia / 1312 przypadków; **o1-mini ASR 72,8%**; refusal Claude-3.7-Sonnet **<3%** |
| [2508.13220](https://arxiv.org/abs/2508.13220) | MCPSecBench | v1 2025-08-17, **v3 2026-02-12** | 17 typów ataków / 4 powierzchnie; **wszystkie powierzchnie kompromitowalne**; obecne zabezpieczenia <30% skuteczności |
| [2508.10991](https://arxiv.org/abs/2508.10991) | MCP-Guard: Multi-Stage Defense-in-Depth | v1 2025-08-14, **v4 2026-01-08** | fine-tuned E5 **96,01% accuracy**; MCP-ATTACKBENCH = 70 448 próbek |
| [2601.17549](https://arxiv.org/abs/2601.17549) | Breaking the Protocol: Security Analysis of the MCP Specification… | **2026-01-24** ✅ | 847 scenariuszy; MCP **wzmacnia ASR o 23–41%** vs non-MCP; MCPSec redukuje ASR **52,8% → 12,4%** przy 8,3 ms latency. ⚠️ Preprint ma tylko ~13 KB — liczby nie są replikowane |
| [2601.01241](https://arxiv.org/abs/2601.01241) | MCP-SandboxScan: WASM-based Secure Execution… | v1 **2026-01-03**, v2 2026-06-22 ✅ | 1127 narzędzi w 71 repo; **886 z wrażliwymi uprawnieniami**; 12/33 re-eksekucji dało source-to-sink witness |
| [2509.10540](https://arxiv.org/abs/2509.10540) | EchoLeak: First Real-World Zero-Click Prompt Injection… | 2025-09-06 | CVE-2025-32711. ⚠️ **Praca jakościowa — NIE podaje ASR.** Każda cytowana liczba ASR dla EchoLeak jest wymyślona |
| [2512.06556](https://arxiv.org/abs/2512.06556) | Semantic Attacks on Tool-Augmented LLMs (tool poisoning / shadowing / rug pull) | v1 2025-12-06, v2 2026-05-21 | unsafe tool invocations **36% → 15%**, block rate **74%** |
| [2508.20412](https://arxiv.org/abs/2508.20412) | MindGuard | v1 2025-08-28, v3 2026-01-15 | 94–99% precision, 95–100% attribution, zerowy koszt tokenów |

### 4.2 Najnowsze prace 2026 (wrzesień 2026)

- [2609.18411](https://arxiv.org/abs/2609.18411) — Verifiable Action Card: ASR **68–100% → 0%** (2026-09-16)
- [2609.18217](https://arxiv.org/abs/2609.18217) — cross-channel fragmentation: **0%** w jednym kanale → **do 100%** przy dwóch kanałach eksfiltracji; **wszystkie 7 zbadanych narzędzi security MCP zawiodły** (2026-09-16)
- [2609.19100](https://arxiv.org/abs/2609.19100) — ruch MCP omija Suricata + RITA: **beacon score 0.0** (2026-09-16)
- [2609.17320](https://arxiv.org/abs/2609.17320) — Emergence World: 8 światów, 850 tys. wywołań LLM; **żaden świat nie był w pełni odporny**; agenci działali na wstrzykniętej treści **do 46 godzin później** (2026-09-15)
- [2609.14119](https://arxiv.org/abs/2609.14119) — spis rejestru MCP: **21 643 serwery** (2026-09-12)

⚠️ **Higiena cytowania:** wiele prac z 2025 było **wznawianych w 2026** (2508.13220 v3, 2508.10991 v4, 2508.20412 v3, 2512.06556 v2). To **nie są** nowe prace 2026.

---

## 5. Skannery i narzędzia (stan na 2026-09-19)

### 5.1 Trzy zmiany własności, które unieważniają starszą dokumentację

1. **`mcp-scan` (Invariant Labs) już nie istnieje jako produkt.** Zweryfikowane bezpośrednio:
   - `https://github.com/invariantlabs-ai/mcp-scan` → **HTTP 301** → `https://github.com/snyk/agent-scan`
   - PyPI `mcp-scan` 0.4.3 (2026-03-02) to **stub przekierowujący**: summary = „This package has been renamed to snyk-agent-scan…”, `requires_dist: ['snyk-agent-scan']`
   - Następca: **`snyk-agent-scan` 0.6.3** (2026-09-10), Apache-2.0, aktywnie utrzymywany
   - ⚠️ Skanowanie konfiguracji MCP **wykonuje polecenia w nich zawarte**; wymaga `SNYK_TOKEN`
2. **Promptfoo należy do OpenAI** — przejęcie ogłoszone **2026-03-09**. Nadal MIT i open source; wersja **0.123.1** (2026-09-18).
3. **Semgrep MA oficjalne reguły MCP** — 7 reguł MCP + 5 reguł agent-skill w `semgrep-rules/ai/ai-best-practices/`, dodane **2026-03-23** i **2026-05-10**. Licencja: **Semgrep Rules License v1.0** (nie jest OSI open source).

### 5.2 Tabela wersji (zweryfikowana z rejestrów maszynowych)

| Narzędzie | Wersja | Data | Licencja | Status |
|---|---|---|---|---|
| Snyk Agent Scan (ex `mcp-scan`) | **0.6.3** | 2026-09-10 | Apache-2.0 | bardzo aktywny |
| Promptfoo | **0.123.1** | 2026-09-18 | MIT | aktywny (OpenAI) |
| Semgrep MCP rules | ruleset | 2026-03-23 / 2026-05-10 | Semgrep Rules License v1.0 | aktywny |
| Cisco AI Defense MCP Scanner | **4.8.4** | 2026-08-28 | Apache-2.0 | aktywny |
| Docker MCP Gateway | **v0.43.3** | 2026-07-16 | MIT | aktywny, **8 advisories** |
| Microsoft Agent Governance Toolkit | **v4.1.0** | 2026-06-09 | MIT | aktywny |
| Agent Threat Rules (ATR) | **v4.0.0** | 2026-08-23 | MIT | aktywny |
| **ShieldCortex** (memory firewall) | **5.0.5** | 2026-09-13 | MIT | aktywny |
| MCP Registry | v1.8.1 | 2026-08-06 | NOASSERTION | **preview** |

### 5.3 Co wykrywają (istotne dla pamięci)

- **Snyk Agent Scan 0.6.x** — przeszedł z kodów błędów na **punktowane wskaźniki ryzyka (0–1000)**; kategorie MCP: `prompt_injection_tool_desc`, `untrusted_content`, `private_data`, `destructive_capabilities`. W 0.5.x były Tool Poisoning (E001), **Tool Shadowing (E002)**, **Toxic Flows**. Skanuje też agent skills.
- **Cisco mcp-scanner** — trzy silniki: **reguły YARA**, **analiza LLM** (LiteLLM; domyślnie gpt-5.2), **Cisco AI Defense inspect API**. Plus skanowanie zachowań kodu, `pip-audit`, **VirusTotal**, **skanowanie PyPI/npm w sandboxie Docker** (4.8.0). Wersja 4.8.3 dodała **detekcję dynamicznej rejestracji narzędzi**.
- **Promptfoo MCP** — plugin `mcp` obejmuje Function Discovery, Parameter Injection, Function Call Manipulation, **Tool Metadata Injection**, Privilege Escalation; dokumentacja wprost pokrywa **tool poisoning, rug pulls, tool shadowing, cross-server attacks**.
- **Microsoft Agent Governance Toolkit** — dostarcza `MCPSecurityScanner`, `MCPGateway`, **`MCPMessageSigner`**, `MCPSessionAuthenticator`, `MCPResponseScanner` + **detekcja rug-pull i schema-drift**.

### 5.4 Nowe narzędzia 2026 (poza listą z zadania)

- **Runtime enforcement:** `praxiom` (0.2.0, 2026-09-07 — weryfikacja polityk przy każdym wywołaniu narzędzia), `mcp-zero-trust-layer` (0.6.0, 2026-09-17), `agent-airlock` (0.10.7, 2026-09-19 — warstwa deny-by-default), `avakill` (AGPL-3.0, stan z 2026-03-08).
- **Standard detekcji:** **Agent Threat Rules (ATR)** — „Sigma dla agentów AI”, 10 kategorii, MIT, v4.0.0 (2026-08-23).
- **Skanery:** `mcphound` (0.1.5, 2026-08-31, SARIF).
- **Bezpieczeństwo pamięci agentów:** **ShieldCortex 5.0.5** (2026-09-13, MIT) — deklaruje „trustworthy memory + memory firewall for AI agents; runtime tool gates on Claude Code, OpenClaw, Hermes”. **To jedyne znalezione narzędzie klasy „memory firewall”.**
- **Threat intel:** Miasma worm (czerwiec 2026) — złośliwe konfiguracje MCP w **73 repozytoriach GitHub**, w tym w Microsoft `azure/durabletask`.

### 5.5 Docker MCP Gateway — model bezpieczeństwa

`docs/security.md` dokumentuje: Bearer auth na transportach HTTP, zdalne URL-e **tylko publiczne HTTPS**, **domyślnie włączona weryfikacja podpisów** obrazów z przestrzeni `mcp/` (przypięte do digestu), domyślnie read-only bindy hosta z allowlistami, `no-new-privileges`, sekrety z Docker Desktop. Katalog: przegląd PR, efemeryczne buildy + testy init/funkcjonalne/list-tools — ale Docker sam nazywa to **„best-effort”** i **nie ujawnia, jakiego skanera używa**.

⚠️ Uwaga: **CVE-2026-76094 dotyczy wprost tego, że weryfikacja podpisów była wyłączona domyślnie** — czyli deklaracja z `docs/security.md` jest nowsza niż stan, którego dotyczyły advisories.

---

## 6. Mechanizmy obronne na poziomie protokołu

**Aktualna rewizja specyfikacji MCP: `2026-07-28`** (zastępuje `2025-11-25`). Źródło: https://modelcontextprotocol.io/specification/2026-07-28/changelog

### 6.1 Co ZOSTAŁO przyjęte (adoptowane)

| Mechanizm | SEP | Znaczenie bezpieczeństwa |
|---|---|---|
| **`Mcp-Method` i `Mcp-Name` jako wymagane nagłówki** na POST Streamable HTTP + `x-mcp-header` dla parametrów narzędzi | [SEP-2243](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2243) | Umożliwia routing i inspekcję na poziomie pośrednika/firewalla **bez parsowania ciała JSON-RPC** → realna podstawa dla kontroli per-narzędzie. Dodano też `HeaderMismatchError` (kod `-32020`) |
| **`ttlMs` + `cacheScope` na wynikach `tools/list`, `prompts/list`, `resources/list`, `resources/read`** (`CacheableResult`) | [SEP-2549](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2549) | Serwer podaje świeżość definicji narzędzi; klient może cache'ować i **wykryć zmianę definicji** = fundament pod detekcję rug pull po stronie klienta. Dodatkowo: „Servers **SHOULD** return tools from `tools/list` in a **deterministic order**” |
| **Walidacja `iss` (RFC 9207)** — klient MUSI zweryfikować `iss` wobec zapisanego wystawcy przed wymianą kodu | [SEP-2468](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2468) | Obrona przed **mix-up attacks** |
| **Poświadczenia klienta związane z wystawcą** — MUSZĄ być kluczowane po `issuer`, ZAKAZ ponownego użycia z innym AS | [SEP-2352](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2352) | Ogranicza reuse tokenów między serwerami |
| **`application_type` w Dynamic Client Registration** | [SEP-837](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/837) | Zapobiega konfliktom redirect URI w OIDC |
| **Usunięcie `Mcp-Session-Id`** i sesji protokołu; stan przez jawnie mintowane handle w argumentach narzędzi | [SEP-2567](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567) | Eliminuje całą klasę ataków na przechwycenie sesji (por. CVE-2026-67431, CVE-2026-52869) |
| **Protokół bezstanowy** — usunięto handshake `initialize`; wersja i capability w `_meta`; dodano `server/discover` | [SEP-2575](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2575) | Powierzchnia ataku na stan sesji zredukowana |
| **Multi Round-Trip Requests (MRTR)** — zastępuje serwer-inicjowane `sampling`/`elicitation`/`roots/list`; wymagane `resultType` | [SEP-2322](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2322) | Server-initiated requests to klasyczny wektor injection; MRTR czyni je jawnymi w odpowiedzi na konkretne żądanie |
| **Deprecjacja DCR** na rzecz Client ID Metadata Documents | [PR #2858](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2858) | Uporządkowanie rejestracji klienta |
| **Deprecjacja Roots, Sampling, Logging** | [SEP-2577](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2577) | Redukcja powierzchni |

### 6.2 Czego **NIE MA** — krytyczne dla Vestige

**Zweryfikowany wynik negatywny.** W specyfikacji `2026-07-28` **nie występuje**:

- ❌ **hashowanie / pinning definicji narzędzi** (tool definition hashing)
- ❌ **`untrustedHint`** — pole nie istnieje w specyfikacji ani w propozycjach SEP
- ❌ **podpisane manifesty narzędzi** (signed tool manifests)
- ❌ jakikolwiek mechanizm na poziomie protokołu przeciw **tool poisoning** czy **rug pull**

Potwierdza to struktura oficjalnego dokumentu **Security Best Practices** dla tej rewizji: https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices

Sekcje dotyczą wyłącznie: Confused Deputy, Token Passthrough, SSRF, SSRF against Authorization Servers, State Handle Hijacking, Local MCP Server Compromise, OAuth Authorization URL Validation, stdio Transport Security in Proxy Scenarios, Mix-Up Attacks, Localhost Redirect URI Impersonation, CIMD Trust Policies, Scope Minimization.

**To cała lista. Nie ma ani jednej sekcji o integralności definicji narzędzi.**

**Wniosek — najważniejszy dla rekomendacji:** integralność warstwy narzędzi w MCP 2026-07-28 jest **odpowiedzialnością implementacji serwera i klienta**, nie protokołu. `ttlMs`/`cacheScope` i deterministyczna kolejność `tools/list` to **jedyne** elementy, na których można oprzeć własną detekcję zmian definicji narzędzi.

### 6.3 Inicjatywy poza specyfikacją

- **MCPS: Cryptographic Security Layer for the Model Context Protocol** — draft IETF, sygnatury w konwencji RFC 8785: https://datatracker.ietf.org/doc/draft-sharif-mcps-secure-mcp/ (⚠️ **NIE ZWERYFIKOWANO** statusu i daty draftu w źródle pierwotnym IETF)

---

## 7. Zagrożenia dla systemów pamięci — **sekcja kluczowa dla Vestige**

### 7.1 Ataki na trwałą pamięć agenta (wszystkie zweryfikowane przez `arxiv.org/abs`)

| arXiv | Tytuł | Data | Kluczowe liczby |
|---|---|---|---|
| [2608.21230](https://arxiv.org/abs/2608.21230) | **Utility Under Attack: Agent Memory Poisoning and the Limits of Content Screening and Provenance Ranking** | **2026-08-21** | Zatrucie **1,2%** LongMemEval → dokładność **0,850 → 0,300**. Pipeline screeningu przy write-time o recall **0,832** dla indirect prompt injection **odrzucił 0 z 360** zatrutych wspomnień. Ważenie provenance statystycznie nierozróżnialne od braku obrony (**p=0,80**); mocniejsza waga działa tylko przez wykluczenie niezaufanej treści, a gdy dowody same są niezaufane → recall dowodów **0**, dokładność **0,0417**. **Wniosek autorów: brak użytecznego ustawienia addytywnego członu provenance — potrzebne ograniczenia typu bounded occupancy.** |
| [2609.13889](https://arxiv.org/abs/2609.13889) | When Malicious Instructions Persist: Persistent Memory Poisoning Attack on Harness-Based Agents (PMPA) | **2026-09-12** | ISR/C-ASR **73,7%/55,5%** (OpenClaw), **66,9%/81,7%** (Claude Code). **Obrona na poziomie promptu przestaje działać po zatruciu pamięci** |
| [2607.06595](https://arxiv.org/abs/2607.06595) | When Agents Remember Too Much: Memory Poisoning Attacks on LLM Agents | **2026-07-06** | ~**98%** injection rate, ~**60%** activation rate |
| [2607.05189](https://arxiv.org/abs/2607.05189) | When Claws Remember but Do Not Tell: Stealthy Memory Injection in Persistent Personal Agents | **2026-07-06** | **87,5%** (OpenClaw + GPT-5.4), **71,4%** (Claude Code SDK + Sonnet 4.6). **Transferuje się na Mem0**, NanoClaw, Hermes Agent |
| [2605.09033](https://arxiv.org/abs/2605.09033) | ShadowMerge: Poisoning Attack on **Graph-Based** Agent Memory via Relation-Channel Conflicts | 2026-05-09 (v3 2026-05-15) | **93,8%** średni ASR **na Mem0**, **+50,3 pkt** nad najlepszym baseline. Atakuje **krawędzie grafu wiedzy**, nie treść węzłów |
| [2604.02623](https://arxiv.org/abs/2604.02623) | Poison Once, Exploit Forever: Environment-Injected Memory Poisoning Attacks on Web Agents (eTAMP) | 2026-04-03 (v2 04-07) | **32,5%** GPT-5-mini, 23,4% GPT-5.2, 19,5% GPT-OSS-120B; **do 8× ASR** przy „Frustration Exploitation”. **Wstrzyknięcie wyłącznie przez środowisko — bez dostępu do pamięci** |
| [2605.15338](https://arxiv.org/abs/2605.15338) | Hidden in Memory | 2026-05-14 | **99,8%** insertion (GPT-5.5), 95% Kimi-K2.6; 60–89% akcji zgodnych z intencją atakującego |
| [2605.26154](https://arxiv.org/abs/2605.26154) | MemMorph | 2026-05-24 | do **85,9%** ASR przy **tylko 3** wstrzykniętych rekordach |
| [2608.03844](https://arxiv.org/abs/2608.03844) | MAFIA | 2026-08-04 | do **90,7%** ASR przy jednoczesnym stłumieniu detekcji audytowej **83,3% → ≤7,4%** |
| [2605.01970](https://arxiv.org/abs/2605.01970) | Trojan Hippo | 2026-05-03 | **85–100%** ASR vs frontier OpenAI/Google; aktywacja po **100 nieszkodliwych sesjach**; obrony redukują do 0–5% **wysokim kosztem użyteczności** |
| [2606.10742](https://arxiv.org/abs/2606.10742) | MemVenom | 2026-06-09 | do **99,15%** ASR na web agentach z GPT-5, multimodalny |
| [2609.00523](https://arxiv.org/abs/2609.00523) | PipePoison | 2026-09-01 | +19,1 pp utilization rate; +16 pkt nad baseline na **niewidzianych** ofiarach; przeżywa 8 obron |

### 7.2 Obrony — z liczbami (i ich ograniczeniami)

| arXiv | Nazwa | Data | Wynik |
|---|---|---|---|
| [2609.08747](https://arxiv.org/abs/2609.08747) | MemSentry | 2026-09-08 | 91,7% accuracy, 0,908 macro-F1, **100% detekcji** zagrożeń klasy external-quarantine |
| [2605.14421](https://arxiv.org/abs/2605.14421) | MemLineage | 2026-05-14 | **jedyna** konfiguracja sprowadzająca wszystkie trzy kolumny memory-poisoning do **zerowego ASR**; sub-ms overhead |
| [2606.12703](https://arxiv.org/abs/2606.12703) | SMSR | 2026-06-10 | certyfikowana granica; ASR **65,3% → 5,3%** |
| [2609.02265](https://arxiv.org/abs/2609.02265) | CAPTURE | 2026-09-02 | ogranicza zatrucie do **11,5%**, akceptuje 83,5% prawdziwych aktualizacji; **przy adaptacyjnym atakującym → 24,7%** |
| [2606.24322](https://arxiv.org/abs/2606.24322) | Twierdzenie o rozdzielności (maszynowo zweryfikowane) | 2026-06-23 | **Żadna obrona oparta na treści ani na lineage nie jest sound przy „laundering”; konieczne jest write-time origin binding** |

### 7.3 Awarie ładu — **bez udziału atakującego**

- [2609.08258](https://arxiv.org/abs/2609.08258) (2026-09-08) — zbadano **5 systemów pamięci agentów**: **żaden nie egzekwuje unieważniania (revocation) domyślnie**; unieważnione fakty **przechodzą rankingiem ponad swoimi następcami** w 9 scenariuszach × 9 modeli.
- [2609.01836](https://arxiv.org/abs/2609.01836) (2026-09-01) — endogenous authorization laundering: piszący tworzą fałszywy autorytet dla **do 50,2%** nieautoryzowanych żądań; wykonawcy działają na nim w **98,6%** prób.
- [2606.29279](https://arxiv.org/abs/2606.29279) (2026-06-28) — **nazywa wprost mem0 i LangMem**: konsolidacja przepisuje ostrożnościowe uwagi w pewne stwierdzone fakty; agenci słuchają **pewności frazy, nie źródła**; tagi „unverified” są ignorowane; „do not trust this” **działa odwrotnie**.
- [2605.22842](https://arxiv.org/abs/2605.22842) (2026-05-12) — Misattribution Gap: **4 klasyfikatory bezpieczeństwa**, w tym jeden trenowany na memory poisoning, dały **ZERO detekcji** na 510 checkpointach; agenci cytowali wstrzyknięty dokument jako autorytet normatywny w **59/65** przypadkach.
- [2606.30566](https://arxiv.org/abs/2606.30566) — ⚠️ **v1 raportowało AUC 0,9904, ale prerejestrowana replikacja (v2, N=4 360, 13 modeli) wykazała 100% FALSE POSITIVES.** **Nie cytuj liczby z v1.**

### 7.4 Zatruwanie RAG i grafów wiedzy

- [arXiv:2407.12784](https://arxiv.org/abs/2407.12784) — **AgentPoison** (2024-07-17): pierwotny backdoor pamięci/RAG.
- [arXiv:2505.18543](https://arxiv.org/abs/2505.18543) (2025-05-24) — pierwszy benchmark zatruwania RAG: 13 ataków / 7 obron; **wszystkie ówczesne obrony zawodzą**.
- [2607.17535](https://arxiv.org/abs/2607.17535) (2026-07-20) — Salience Induction: **83,3%** ASR, edycje zachowujące prawdziwość; najlepsza obrona zatrzymuje 75,7%.
- [2608.20756](https://arxiv.org/abs/2608.20756) (2026-08-21, EMNLP 2026 Findings) — Vis-Poison: 40,16–65,40% ASR.
- [2608.21095](https://arxiv.org/abs/2608.21095) (2026-08-21, ICSEA 2026) — Trustworthy RAG: 91% acc / 100% precision, ale **podmiany encji niewykrywalne**.
- [2608.17153](https://arxiv.org/abs/2608.17153) — rozumowanie System-2 **obniża** wyciek, ale **podnosi** ogólny sukces ataku (0,233 → 0,298).

### 7.5 Co to znaczy konkretnie dla Vestige

1. **Powierzchnia ataku to zapis, nie tylko odczyt.** eTAMP ([2604.02623](https://arxiv.org/abs/2604.02623)) działa **bez dostępu do pamięci** — wystarczy treść środowiskowa. `smart_ingest` przyjmujący dowolny tekst z sesji agenta jest wektorem.
2. **Screening treści jest empirycznie bezwartościowy w tej klasie.** [2608.21230](https://arxiv.org/abs/2608.21230): 0/360. Nie buduj obrony na klasyfikatorze treści.
3. **Ważenie provenance bez ograniczeń nie działa** (p=0,80). Potrzebne **bounded occupancy** — limity udziału treści z danego źródła/pochodzenia w korpusie.
4. **`origin binding` w momencie zapisu jest koniecznością, nie opcją** — [2606.24322](https://arxiv.org/abs/2606.24322) dowodzi maszynowo, że przy „laundering” obrony oparte na treści/lineage **nie mogą** być sound.
5. **Konsolidacja pamięci sama tworzy fałszywy autorytet.** Dotyczy wprost `dream`, `reflect` i FSRS-6 consolidation w Vestige — [2606.29279](https://arxiv.org/abs/2606.29279), [2609.01836](https://arxiv.org/abs/2609.01836).
6. **Brak egzekwowania revocation to domyślny stan rynku** — [2609.08258](https://arxiv.org/abs/2609.08258). Vestige ma `temporal invalidate`; **trzeba zweryfikować, czy unieważnione wspomnienia faktycznie przegrywają rankingiem**.
7. **Minimalna dawka jest bardzo mała:** 1,2% zatrutego korpusu ([2608.21230](https://arxiv.org/abs/2608.21230)), 3 rekordy w MemMorph ([2605.26154](https://arxiv.org/abs/2605.26154)). Przy >700 wspomnieniach w Vestige to kwestia kilku wpisów.
8. **Relewantne CVE wzorcowe:** CVE-2026-50027 (brak auth na API dokumentów), CVE-2026-49291 (egzekwowanie tylko w `tools/list`), CVE-2026-33010 (wildcard CORS z credentials), CVE-2026-49986 (zaufanie do `CLAUDE_PROJECT_DIR`). **Każde z tych czterech ma bezpośredni odpowiednik w architekturze serwera pamięci MCP.**

---

## 8. Czego NIE udało się zweryfikować (jawne luki)

| Pozycja | Status |
|---|---|
| Data publikacji OWASP „CheatSheet — Securely Using Third-Party MCP Servers 1.0” oraz „OWASP Top 10 for Agentic Applications 2026” | **NIE ZWERYFIKOWANO** — genai.owasp.org renderuje treść klientowo; strony istnieją (HTTP 200), daty nieodczytane ze źródła pierwotnego |
| CVE/GHSA dla **Supabase MCP** | **NIE ZNALEZIONO** — OSV.dev zwraca 0 dla wszystkich wariantów nazwy. Znane ryzyko (`service_role` omija RLS) **nie ma advisory do zacytowania** |
| Repozytorium `mcp-security-advisories` | **NIE ZNALEZIONO**. Istnieje `mcp-security-project/mcp-cve-project` (społecznościowe, 28★) |
| NSA MCP guidance („Security Design Considerations for AI-Driven Automation Leveraging MCP”, rzekomo maj 2026) | **NIE ZWERYFIKOWANO** — nsa.gov zwraca HTTP 403; tytuł/data potwierdzone wyłącznie pośrednio |
| Silnik skanujący Docker MCP Catalog | **NIE UJAWNIONY** przez Docker; firma nazywa proces „best-effort” |
| Wersje naprawione dla 7 z 8 advisories Docker MCP Gateway | **NIE OPUBLIKOWANE** (`first_patched_version: none`) |
| Stabilne (nie-prerelease) wydanie `docker/mcp-gateway` | **NIE ISTNIEJE** — wszystkie wydania oznaczone jako prerelease |
| Status i data draftu IETF `draft-sharif-mcps-secure-mcp` | **NIE ZWERYFIKOWANO** w źródle pierwotnym IETF |
| Dokładne daty publikacji ~120 z ~130 prac arXiv | Pochodzą z rekordów Atom arXiv API, nie z indywidualnych stron `abs/`. 18 najważniejszych (w tym **wszystkie 8 wymienionych w tym raporcie**) zweryfikowałem bezpośrednim fetch-em `arxiv.org/abs` |
| Promptfoo MCP Proxy — wersja i data | **NIE ZWERYFIKOWANO** |
| Produkt MCP Trail of Bits | **NIE ZNALEZIONO** |
| Podpisywanie/atestacja/SLSA/sigstore w oficjalnym MCP Registry | **NIE ISTNIEJE** |
| Meta arXiv:2608.23601 (StateTune) | ⚠️ **ANOMALIA METADANYCH** — tytuł i abstrakt przeplatane z rekordem 2606.12703 w feedzie Atom. Nie cytować bez ponownego pobrania |

**Uwaga metodologiczna:** zapytanie NVD `keywordSearch=MCP server` daje 191 trafień, ale **nie jest wyczerpujące** — CVE opisujące MCP bez frazy „MCP server” (np. część pozycji OpenClaw) nie wchodzą do zbioru. Traktuj 191 jako **dolną granicę**, nie pełny spis.

---

## 9. Źródła pierwotne

### OWASP
- https://owasp.org/www-project-mcp-top-10/ — strona oficjalna (dostęp 2026-09-19)
- https://github.com/OWASP/www-project-mcp-top-10 — repo; utworzone 2025-06-18, ostatni push **2026-07-29**
- https://raw.githubusercontent.com/OWASP/www-project-mcp-top-10/main/project.owasp.yaml
- https://raw.githubusercontent.com/OWASP/www-project-mcp-top-10/main/tab_top10.md — `version [v0.1]`
- https://raw.githubusercontent.com/OWASP/www-project-mcp-top-10/main/2025/MCP06-2025–Intent-Flow-Subversion.md — `title: "MCP06:2025 – Intent Flow Subversion"`
- https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/
- https://genai.owasp.org/resource/cheatsheet-a-practical-guide-for-securely-using-third-party-mcp-servers-1-0/

### CVE / rejestry
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2025-6514 — 2025-07-09, CVSS 9.6
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2025-32711 — 2025-06-11, CVSS 9.3/7.5
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2025-49596 — 2025-06-13, CVSS 9.4
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2025-54135 — 2025-08-05, CVSS 8.5/9.8
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2025-54136 — 2025-08-02, CVSS 7.2/8.8
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2025-59536 — 2025-10-03, CVSS 8.7
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2026-21852 — 2026-01-21, CVSS 5.3/7.5
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2026-88938 — 2026-09-10, CVSS 7.1
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2026-53708 — 2026-09-14, CVSS 6.6
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2026-50027 — 2026-08-14, CVSS 9.8
- https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2026-49986 — 2026-08-14, CVSS 7.1
- https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=MCP%20server&pubStartDate=2026-06-01T00:00:00.000&pubEndDate=2026-09-19T23:59:59.999 — 191 wyników
- https://raw.githubusercontent.com/CVEProject/cve-website/dev/src/assets/data/CNAsList.json — Anthropic `CNA-2026-0052`
- https://api.osv.dev/v1/query — OSV.dev (maszynowe zapytania pakietowe)
- https://api.github.com/repos/docker/mcp-gateway/security-advisories — 8 advisories
- https://github.com/mcp-security-project/mcp-cve-project — społecznościowy tracker
- https://github.com/advisories/GHSA-84hp-mqvj-3p8h — mcp-memory-service
- https://github.com/advisories/GHSA-g9rg-8vq5-mpwm — mcp-memory-service CORS

### Specyfikacja MCP
- https://modelcontextprotocol.io/specification/2026-07-28/changelog — rewizja **2026-07-28**
- https://modelcontextprotocol.io/specification/2026-07-28/server/tools
- https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices
- https://modelcontextprotocol.io/registry/moderation-policy — „assume minimal-to-no moderation”; nie usuwa serwerów z podatnościami
- https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2243 (SEP-2243 `Mcp-Method`/`Mcp-Name`)
- https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2549 (SEP-2549 `ttlMs`/`cacheScope`)
- https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2468 (SEP-2468 `iss` / RFC 9207)
- https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567 (SEP-2567 usunięcie `Mcp-Session-Id`)
- https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2575 (SEP-2575 bezstanowość, `server/discover`)
- https://datatracker.ietf.org/doc/draft-sharif-mcps-secure-mcp/ — MCPS draft

### arXiv (zweryfikowane bezpośrednim fetch-em `arxiv.org/abs`)
- https://arxiv.org/abs/2608.21230 — Utility Under Attack (2026-08-21)
- https://arxiv.org/abs/2609.13889 — PMPA (2026-09-12)
- https://arxiv.org/abs/2605.09033 — ShadowMerge (2026-05-09, v3 05-15)
- https://arxiv.org/abs/2607.06595 — When Agents Remember Too Much (2026-07-06)
- https://arxiv.org/abs/2607.05189 — When Claws Remember but Do Not Tell (2026-07-06)
- https://arxiv.org/abs/2604.02623 — Poison Once, Exploit Forever (2026-04-03)
- https://arxiv.org/abs/2601.17549 — Breaking the Protocol (2026-01-24)
- https://arxiv.org/abs/2601.01241 — MCP-SandboxScan (2026-01-03, v2 06-22)
- https://arxiv.org/abs/2508.14925 — MCPTox (2025-08-19)
- https://arxiv.org/abs/2508.13220 — MCPSecBench (v3 2026-02-12)
- https://arxiv.org/abs/2508.10991 — MCP-Guard (v4 2026-01-08)
- https://arxiv.org/abs/2509.10540 — EchoLeak (2025-09-06)
- https://arxiv.org/abs/2512.06556 — Semantic Attacks / rug pull (v2 2026-05-21)
- https://arxiv.org/abs/2508.20412 — MindGuard (v3 2026-01-15)
- https://arxiv.org/abs/2407.12784 — AgentPoison (2024-07-17)
- https://arxiv.org/abs/2505.18543 — RAG poisoning benchmark (2025-05-24)

### Raporty towarzyszące (w tym workspace)
- `docs/research/mcp-memory-poisoning-literature-2026-09-19.md` — przegląd ~130 prac
- `MCP_SECURITY_TOOLING_2026-09-19.md` — pełne bloki per narzędzie

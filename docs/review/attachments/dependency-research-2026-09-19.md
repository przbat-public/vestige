# Research stanu zależności — 2026-09-19

Wszystkie wersje zweryfikowane przez `crates.io/api/v1/crates/<name>`, `raw.githubusercontent.com`
oraz oficjalne changelogi. Wersje projektu odczytane z `Cargo.lock` (commit working tree).

## TL;DR — 3 najważniejsze ryzyka

1. **CVE-2026-11822 / CVE-2026-11824 (FTS5 heap overflow)** — naprawione w SQLite **3.53.2** (2026-06-03).
   Projekt bundluje SQLite **3.51.3** przez `rusqlite 0.39.0` → **narażony**. Wymaga wrogiego SQL
   + `SQLITE_DBCONFIG_DEFENSIVE` wyłączonego + FTS5 włączonego. FTS5 **jest** włączony w `bundled`.
   Remediacja: `rusqlite >= 0.40.1`.
2. **fastembed**: 6.0.0 i 7.0.1 **wymagają `ort = "=2.0.0-rc.13"`**. Projekt ma własny exact pin
   `=2.0.0-rc.12` (dla `late-interaction`) → bump fastembed bez bumpu ort = błąd rozwiązania zależności.
3. **git2 0.21.0**: `default = []` — `ssh`/`https` **zniknęły z defaultów**. Obecny zapis
   `git2 = { version = "0.20", features = ["vendored-openssl"] }` po bumpie **cicho straci** transport SSH/HTTPS.

---

## 1. rusqlite

| | wersja | data |
|---|---|---|
| projekt (Cargo.lock) | **0.39.0** | 2026-03-15 |
| latest | **0.40.2** | 2026-08-08 |

Ścieżka: 0.40.0 (2026-05-26) → 0.40.1 (2026-06-06) → 0.40.2 (2026-08-08).

**Uwaga metodologiczna:** `Changelog.md` w repo jest **przestarzały** — zawiera wpisy tylko do
`0.14.0 (2018-08-17)` i jawnie odsyła do GitHub Releases ("For version 0.15.0 and above, see Releases page").
Źródło: https://raw.githubusercontent.com/rusqlite/rusqlite/master/Changelog.md
Właściwe źródło: https://github.com/rusqlite/rusqlite/releases.atom

### 0.40.0 vs 0.39 — breaking changes (wszystkie oznaczone „Breaking changes" w release notes)

- **Replace `VTab` macros by constructors** (#1823) — największa zmiana; makra vtab zastąpione konstruktorami.
- **Fix `VTab::best_index`** (#1824) — breaking.
- **Fix `VTab::connect` / `create`** (#1826, #1832) — breaking.
- **Fix `vtab::dequote`** (#1835) — breaking.
- **Allow opting out of using `sqlite-wasm-rs` on `wasm32-unknown-unknown`** (#1828, #1829) — breaking.
- Asserts on `VTab::connect` aux and args (#1825).

### 0.40.0 vs 0.39 — pozostałe

- Bump bundled SQLite → **3.53.1** (#1848); bundled SQLCipher → **4.14.0** (#1837).
- `sqlite3_set_errmsg` (#1752), `impl From for FromSqlError` (#1833).
- Wsparcie `UtcDateTime` dla chrono/jiff/time (#1843, #1844).
- **Fix UB w `ToSqlOutput::from_rc`** (#1839) — bugfix bezpieczeństwa pamięci.
- `Derive Default` dla `SeriesTabCursor`/`ArrayTabCursor` (#1830), bump `sqlite3-parser` (#1838).

### 0.40.1

- **Bump bundled SQLite → 3.53.2** (#1853).
- **Fix SQL injection when SAVEPOINT name is tainted** (#1854) — istotne bezpieczeństwo.
- Bump `hashlink` (#1855), fix clippy (#1852).

### 0.40.2

- **Lower MSRV to 1.88.0** (jedyna zmiana). MSRV projektu to 1.91 → bez konfliktu.

### Odpowiedź na pytanie o API breakage dla wzorców projektu

- **Connection / Statement / params**: brak zmian w 0.40.x — `Connection`, `Statement`, `params!`,
  `query_row` bez zmian. (Uwaga: `TryFrom for Value` był breakingiem już w **0.39.0**, #1819/#1817 —
  projekt jest już na 0.39, więc to nie dotyczy.)
- **hooks**: brak zmian w 0.40.x. Historycznie: 0.39.0 „Clear hooks only for owning connections" (#1785),
  0.38.0 „Check Connection is owned when registering Closure as hook" (#1764).
- **blob**: brak zmian w 0.40.x.
- **vtab**: **całkowicie przepisane** w 0.40.0 — jeśli projekt nie używa `vtab` feature, bump jest
  praktycznie bezbolesny. Projekt (`vestige-core`) nie włącza `vtab`.
- Project features: `bundled-sqlite = ["rusqlite/bundled"]` (default w `vestige-core`),
  `chrono` + `serde_json` — wszystkie nadal istnieją w 0.40.2.

Źródła: https://github.com/rusqlite/rusqlite/releases.atom · https://crates.io/api/v1/crates/rusqlite

---

## 2. libsqlite3-sys / bundled SQLite

| | wersja | data | bundled SQLite |
|---|---|---|---|
| projekt | **0.37.0** (via rusqlite 0.39.0) | 2026-03-15 | **3.51.3** |
| latest | **0.38.2** | 2026-08-08 | **3.53.2** |

Mapowanie zweryfikowane w `Cargo.toml` tagów rusqlite + `sqlite3.h` (`#define SQLITE_VERSION`):

| rusqlite | libsqlite3-sys | SQLITE_VERSION |
|---|---|---|
| v0.39.0 | 0.37.0 | `"3.51.3"` (3051003) |
| v0.40.0 | 0.38.0 | `"3.53.1"` (3053001) |
| v0.40.1 | 0.38.1 | `"3.53.2"` (3053002) |
| v0.40.2 | 0.38.2 | `"3.53.2"` (3053002) |

**FTS5 w bundled:** `libsqlite3-sys/build.rs` (tag v0.40.2) ustawia
`-DSQLITE_ENABLE_FTS5` (linia 159) oraz `-DSQLITE_ENABLE_FTS3` (157), `-DSQLITE_ENABLE_JSON1` (160),
`-DSQLITE_ENABLE_RTREE` (163). Projekt włącza `bundled` (`vestige-mcp/Cargo.toml:73`,
`vestige-core` default `bundled-sqlite`) → **FTS5 jest kompilowany w binarium projektu**.

Źródła: https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/sqlite3/sqlite3.h ·
https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/build.rs

### SQLite — wydania w 2026 (https://www.sqlite.org/changes.html)

| data | wersja | uwagi |
|---|---|---|
| 2026-01-09 | **3.51.2** | fix deadlocka w broken-posix-lock detection; fixy EXISTS-to-JOIN |
| 2026-03-06 | **3.52.0** | **WITHDRAWN** — wszystkie funkcje przeniesione do 3.53.0 |
| 2026-03-13 | **3.51.3** | **Fix WAL-reset database corruption bug** + drobne fixy |
| 2026-04-09 | **3.53.0** | główne wydanie (patrz niżej) |
| 2026-05-05 | **3.53.1** | fixy problemów zgłoszonych przez użytkowników |
| 2026-06-03 | **3.53.2** | fixy + **poprawki bezpieczeństwa FTS5 (CVE-2026-11822/11824)** |
| 2026-06-26 | **3.53.3** | „Fixes for problems in 3.53.0 … mostly coming from AIs" |
| 2026-07-24 | **3.53.4** | jw. — najnowsze wydanie SQLite |

**WAL:** WAL-reset corruption bug naprawiony **zarówno w 3.51.3, jak i 3.53.0** → projekt na 3.51.3
**ma już tę poprawkę**. Brak dalszych zmian WAL w 3.53.x.

**3.53.0 — najważniejsze (bo to pierwszy bump feature'owy względem 3.51.3):**
- Nowe SQL: `ALTER TABLE` dodawanie/usuwanie `NOT NULL` i `CHECK`; `REINDEX EXPRESSIONS`;
  `TEMP` triggery mogą modyfikować main schema; `VACUUM INTO` z `reserve=N`.
- Nowe funkcje: `json_array_insert()`, `jsonb_array_insert()`.
- Nowe C API: `sqlite3_str_truncate()`, `sqlite3_str_free()`, `sqlite3_carray_bind_v2()`,
  `SQLITE_PREPARE_FROM_DDL`, `SQLITE_UTF8_ZT`, `SQLITE_LIMIT_PARSER_DEPTH`, `SQLITE_DBCONFIG_FP_DIGITS`.
- **Zmiana zachowania (ryzyko dla DB pamięci):** float ↔ text — rounding domyślnie do
  **17 cyfr znaczących zamiast 15**. Może zmienić tekstową reprezentację liczb zmiennoprzecinkowych
  (np. embeddingi serializowane do JSON).
- Query planner: EXCEPT/INTERSECT/UNION zawsze sort-and-merge; liczne ulepszenia join order.
- QRF (Query Result Formatter) + duże zmiany CLI (`.mode`, `.indexes`) — **nie dotyczy** użycia jako biblioteka.
- „Potential incompatibility": niecytowane średniki na końcu dot-commands — tylko CLI.
- **FTS5**: brak zmian funkcjonalnych odnotowanych w 3.53.x (poza fixami bezpieczeństwa w 3.53.2).
- JavaScript/WASM: nowy VFS `opfs-wl`.

### CVE w 2026 (https://www.sqlite.org/cves.html — strona ostatnio aktualizowana 2026-08-01)

| CVE | fix | opis |
|---|---|---|
| **CVE-2026-11822**, **CVE-2026-11824** | **3.53.2 (2026-06-03)** | Atakujący mogący wykonać dowolny SQL (np. SQL injection), przy `SQLITE_DBCONFIG_DEFENSIVE` **wyłączonym** i **FTS5 włączonym**, może wywołać zapis poza końcem bufora heap. Dwa oddzielne CVE dla tego samego problemu. |
| CVE-2026-51296, -51297, -51300, -51302, -51303, -51304 | — | **„Not a bug in SQLite"** — niereprodukowalne, „appear to be AI hallucinations" (analiza JFrog). |
| CVE-2025-70873 | 3.52.0 (2026-03-06) | Rozszerzenie `zipfile` (nie w standardowym SQLite) — OOB read przy złośliwym ZIP. |
| CVE-2025-7709 | 3.50.3 (2025-07-17) | Zepsuty indeks FTS5 → odczyt poza granicami tablicy (integer overflow). |
| CVE-2025-6965 | 3.50.2 (2025-06-28) | Integer overflow → OOB read przy wstrzykniętym SQL. |
| CVE-2025-3277, CVE-2025-29087 | 3.49.1 (2025-02-18) | `concat_ws()` — zapis poza końcem tablicy. |

**Ocena ekspozycji projektu:** 3.51.3 zawiera fixy do 3.50.3 włącznie, więc CVE-2025-* **nie dotyczą**.
Otwarte pozostają **CVE-2026-11822 / CVE-2026-11824** — projekt jest na 3.51.3, fix jest w 3.53.2.
Warunek: `SQLITE_DBCONFIG_DEFENSIVE` musi być wyłączony (SQLite domyślnie go nie włącza, rusqlite
również nie) — czyli warunek jest spełniony. Wymagany jest jednak atakujący zdolny wstrzyknąć SQL.

---

## 3. usearch

| | wersja | data |
|---|---|---|
| projekt | **2.25.2** | 2026-05-02 |
| latest | **2.26.2** | 2026-08-31 |

**Uwaga:** repo **nie ma pliku CHANGELOG** (`CHANGELOG.md` i `rust/CHANGELOG.md` → HTTP 404).
Jedyne pierwotne źródło to GitHub Releases: https://github.com/unum-cloud/usearch/releases

### 2.25.3 (2026-05-24) — patch
- Fix: Refuse operations without reserved thread contexts (#757); Checked arithmetic for allocation sizes (#763);
  Preserve hash lookup capacity across thread reserves (#765); Guard quantized casts against zero-magnitude inputs (#758);
  Refuse missing metrics in C change-metric API (#760); Short-circuit self-renames (#761);
  **Keep `vectors_lookup_` capacity after `clear()`** (#759).
- Fix: Bounded probe w `equal_iterator_gt::operator++`; Resize cast buffer w `change_metric`;
  Eager-reserve thread contexts w `index_dense_gt::make` (#755); Restore `ring_gt::try_push` return value.
- Python/JS/C-ABI: mutex na współbieżny dostęp, GIL release, fixy OOM.

### 2.26.0 (2026-07-10) — minor
- **Add: Expose `stats()` in Rust SDK** (#768).
- **Add: Rust compact binding** (#771).
- Fix: **Reclaim `slot_lookup_` tombstones under churn** (#769).
- Fix: **Mix integer key hashes with SplitMix64** (#773).
- Reszta: CI/JNI/C#/JS build plumbing.

### 2.26.1 (2026-08-22) — patch
- Fix: Respect Cargo-selected Windows CRT (#778).
- Fix: **Reserve the requested member capacity** (#777).
- Fix: **Bound file-controlled sizes in `view()`** (#776) — hartowanie na złośliwy/ uszkodzony plik indeksu.
- Make: Pin citations, deps, CI versions.

### 2.26.2 (2026-08-31) — patch
- Improve: Native enums i PEP 604 unions (Python).
- Make: Ship a Python source distribution.
- Fix: **Retry Linux file mapping without `O_NOATIME`** (#784).

### Czy format serializacji / recall się zmienił? (pytanie o sidecar `vestige.hnsw`)

**Nie znaleziono ogłoszonej zmiany formatu** w 2.25.3–2.26.2 — żaden release note nie wspomina
o zmianie formatu ani o niekompatybilności. Dodatkowo, z analizy źródeł (`include/usearch/index_dense.hpp`, main):

- Format ma nagłówek `index_dense_head_t` z `magic_t = char[7]` + trójką `version_major/minor/patch`
  (`std::uint16_t`), zapisywaną przy `save_to_stream` z `USEARCH_VERSION_*`.
- Istnieje jawny shim kompatybilności wstecznej `fix_pre_2_10_metadata()` dla plików < 2.10.
- `serialized_length()` = `dimensions_length + matrix_length + sizeof(index_dense_head_buffer_t) + typed_->serialized_length()`
  — **`slot_lookup_` nie jest częścią serializacji**. Jest to runtime'owy `flat_hash_multi_set_gt`,
  odbudowywany przez `reindex_keys_()` (pętla `slot_lookup_.try_emplace(...)`) oraz przy `add`.
  Zatem fix „Mix integer key hashes with SplitMix64" (2.26.0) działa **tylko in-memory** i nie
  unieważnia zapisanego pliku.

**Wniosek:** istniejący sidecar powinien się wczytać. **Zastrzeżenie:** brak jawnej gwarancji
stabilności formatu w dokumentacji — powyższe to wniosek z kodu źródłowego, nie z deklaracji
producenta. Bezpieczne podejście: zachować fallback „rebuild-from-SQLite", który projekt już ma.
**Nie zweryfikowano** oficjalnego stanowiska o kompatybilności formatu 2.25.x ↔ 2.26.x.

Źródła: https://github.com/unum-cloud/usearch/releases.atom ·
https://raw.githubusercontent.com/unum-cloud/usearch/main/include/usearch/index_dense.hpp

---

## 4. ts-rs

| | wersja | data |
|---|---|---|
| projekt | **12.0.1** | 2026-01-31 |
| latest (crates.io) | **12.0.1** | 2026-01-31 |

**Brak 13.x — także jako pre-release.** Pełna lista wersji z crates.io: `12.0.1`, `12.0.0` (2026-01-31),
`11.1.0` (2025-10-14), `11.0.1`, `11.0.0` … Brak jakiejkolwiek wersji 13.x ani alpha/beta/RC.

**Brak 13.x w rozwoju:** `ts-rs/Cargo.toml` na branchu `main` ma `version = "12.0.1"`,
`rust-version = "1.78.0"`. `CHANGELOG.md` na `main` ma sekcje `# master` → `### Breaking`,
`### Features`, `### Fixes` — **wszystkie puste**.

### 12.0.0 (2026-01-31) — breaking (projekt już na 12.0.1, więc to kontekst historyczny)
- Zmiana generowanego typu unit structs → `Record<symbol, never>` (#431).
- Zmiana `HashMap` → `{ [key in K]: V }` gdy `K` nie jest enumem (#446);
  dla `K` = enum → `{ [key in K]?: V }`. Migracja: env var `TS_RS_USE_V11_HASHMAP` (do usunięcia w przyszłości).
- Programmatic configuration of binding generation (#460).
- Features: `TS_RS_LARGE_INT` env var (#448), wsparcie `arrayvec` (#469), wsparcie `jiff` (#458).
- Fixes: brak warninga dla `#[serde(borrow)]` (#471) i `#[serde(crate = "..")]` (#447);
  fix trait bounds dla `#[ts(optional)]` na `Option<Generic>` (#454); fix parsowania
  comma-separated serde attributes (#466).

### Atrybuty używane przez projekt (`export_to`, `rename_all`, `optional`) — stan i roadmap

- **Nie zweryfikowano** żadnych oczekujących (pending) zmian dla tych trzech atrybutów.
  Nie znaleziono otwartego milestone'u ani PR dla 13.x; `CHANGELOG` na `main` jest pusty.
  GitHub REST API (`api.github.com`) jest rate-limited (403) z tego IP, a strona issues HTML
  nie zwróciła listy zadań — stąd brak pełnej weryfikacji roadmapy.
- Fakty potwierdzone: `#[ts(export_to)]` przyjmuje dowolne wyrażenie (od 11.0.0) i od 10.0.0
  wiele typów może eksportować do tego samego pliku. `#[ts(rename_all)]` i `#[ts(optional)]`
  działają bez zmian w 12.x; `#[ts(optional)]` + `#[ts(type)]` został naprawiony w 11.0.1 (#416),
  a `Option<Generic>` w 12.0.0 (#454).
- Projekt używa `features = ["chrono-impl", "no-serde-warnings"]` — oba nadal istnieją.

Źródła: https://crates.io/api/v1/crates/ts-rs ·
https://github.com/Aleph-Alpha/ts-rs/releases.atom ·
https://raw.githubusercontent.com/Aleph-Alpha/ts-rs/main/CHANGELOG.md ·
https://raw.githubusercontent.com/Aleph-Alpha/ts-rs/main/ts-rs/Cargo.toml

---

## 5. axum / hyper / tower / tower-http

| crate | projekt | latest | data latest |
|---|---|---|---|
| axum | **0.8.9** | **0.8.9** | 2026-04-14 |
| hyper | **1.9.0** (transitive) | **1.11.1** | 2026-08-28 (CHANGELOG: 2026-08-27) |
| tower | **0.5.3** | **0.5.3** | 2026-01-12 |
| tower-http | **0.6.10** | **0.7.1** | 2026-08-31 |

### axum — czy jest 0.9 / 1.0 alpha/beta/RC?

**Nie.** crates.io nie ma **żadnej** wersji 0.9.x ani 1.0.x — ostatni pre-release w historii to
`0.8.0-rc.1` (2024-12-17). `axum/Cargo.toml` na branchu `main` ma `version = "0.8.9"`.

Natomiast **breaking changes są już zmergowane na `main`** i czekają w sekcji `# Unreleased`
changeloga (czyli następne wydanie będzie prawdopodobnie 0.9.0):
- **breaking:** Router fallbacks są teraz poprawnie mergowane dla zagnieżdżonych routerów (#3158).
- **breaking:** `#[from_request(via(Extractor))]` używa typu rejection ekstraktora zamiast
  `axum::response::Response` (#3261).
- **breaking:** `axum::serve` stosuje domyślny `header_read_timeout` hypera (#3478).
- **breaking:** output future `axum::serve` — usunięcie `io::Result` (nigdy nie zwracał `Err`)
  i typ uninhabited bez `with_graceful_shutdown` (#3601).
- **changed:** `serve` ma dodatkowy argument generyczny, działa z dowolnym typem body (#3205).
- Dodatki (nie-breaking): `ListenerExt::limit_connections` (#3489), `MethodRouter::method_filter` (#3586),
  `serve::Executor` + `Serve::with_executor` (#3704), `IntoResponseParts` dla `Redirect` (#3721),
  `sse::Event::raw` (#3829), `RawPathParams::from_request_extensions` (#3757),
  `ConnectionLifetimeLimits` / `MaxConnectionAge` (#3779), `MethodFilter::QUERY` (#3801);
  bump `matchit` (#3702); fix `HEAD` + `content-length: 0` (#3742).

**Wersja 0.8.9 (2026-04-14) — co zawiera:** `WebSocketUpgrade::{requested_protocols, set_selected_protocol}` (#3597);
**MSRV podniesiony do 1.80** (#3620); fix connect endpoint w `MethodRouter` (#3656);
lepszy komunikat błędu przy przekroczeniu limitu multipart (#3611).

**Ocena dla projektu:** projekt używa `axum 0.8` z `default-features = false`,
features `["json", "query", "tokio", "http1", "ws"]` — bez zmian. Brak presji na migrację.

### hyper 1.10 / 1.11 — co się zmieniło

**Brak breaking changes** — 1.10.0 i 1.11.0 to wydania minor z bugfixami i dodatkami:

- **v1.11.1 (2026-08-27/28):** http1 — wykrywanie `TE: trailers` bez rozróżniania wielkości liter
  i z innymi wartościami (#4152); rozpoznawanie `\n\r\n` jako terminatora nagłówka w partial-read
  fast path (#4147, fix #4145); flush buforowanych bajtów przed yield (#4143);
  eviction pooled conn przy `Connection: close` po stronie requestu (#4110).
- **v1.11.0 (2026-07-20):** http1 — odrzucanie `content-length` gdy przychodzi przed
  `transfer-encoding` (#4124, fix #4123); `append` dla powtarzanych trailer values (#4118);
  do `max_headers` trailerów (#4108); ścisłe egzekwowanie `max_buf_size` (#4093, fix #4081);
  flush przed shutdown (#4018). http2 — unikanie buforowania `Upgraded` writes bez send capacity (#4102).
  Feature: `rt::ReadBufCursor::initialized_unfilled()` (#4115).
- **v1.10.1 (2026-05-29):** fix busy loop gdy peer half-close i otwarte body (#4086, fix #4085).
- **v1.10.0 (2026-05-27):** http1 — błąd gdy dispatcher dropowany w trakcie body (#4069);
  fix czytania dużych body na 32-bit (#4056); fix rzadkiego missed write wakeup (#…).
  http2 — `reset_stream_duration()` client option (#4068), `header_table_size()` na server builder (#4062).

Ostatnia zmiana **breaking** w hyperze to v1.8.0 (HTTP/2 client connection nie przyjmuje już executora).

### tower-http 0.7.0 (2026-06-15) — breaking changes

Wydanie 0.7.1 (2026-08-31) jest **nie-breaking** względem 0.7.0 (dodatki + fixy, patrz niżej).

**Breaking (jawne w release notes):**
1. **compression:** middleware obsługuje teraz `*` wildcard oraz `identity;q=0` wg RFC 9110 §12.5.3.
   Żądania, które wcześniej spadały do identity (np. `*;q=0` albo `identity;q=0` bez innej
   akceptowalnej encoding), dostają teraz **406 Not Acceptable** (#693).
2. **compression:** próg `SizeAbove` z `u16` → **`u64`** (minimalne rozmiary > 64 KiB) (#704).
3. **Usunięto** dorozumiane no-op features `tokio` i `async-compression` (#628) — trzeba je usunąć
   z `Cargo.toml`, jeśli są włączane.
4. **trace/classify:** gRPC error message trafia do outputu tracingu; `GrpcCode` i `GrpcFailureClass`
   są teraz `#[non_exhaustive]`; `GrpcStatus` eksportowany z modułu `classify` (#422).
5. **follow-redirect:** `FollowRedirect` **forwarduje `Extensions`** do przekierowanych żądań
   (wcześniej je gubił). `Standard` policy gubi extensions przy cross-origin.
   Opt-out: `FollowRedirectLayer::preserve_extensions(false)`; wybiórczo
   `FilterCredentials::allow_extension::<T>()` / `keep_all_extensions()` (#706).
6. **follow-redirect:** filtrowanie nagłówków i extensions jest teraz **kumulatywne** —
   wartość zgubiona na jednym hopie nie wraca na późniejszych; `FilterCredentials` nie wyśle już
   `Cookie`/`Authorization` do same-origin celu osiągniętego po cross-origin hopie (#706).
7. **services:** odrzucanie trailing slash dla ścieżek plików — żądania plików z końcowym `/`
   zwracają **404 Not Found** zamiast serwować plik (#678).

**Pozostałe (nie-breaking):** MSRV z 1.64 → **1.65** (#684); nowe: `csrf::CsrfLayer` (#699),
`DeadlineBody` + `RequestBodyDeadlineLayer`/`ResponseBodyDeadlineLayer` (#688),
strong ETag w `ServeDir` z `If-Match`/`If-None-Match` wg RFC 9110 (#691),
`Backend` trait + `ServeDir::with_backend()` (#684), `html_as_default_extension` (#519),
`redirect_path_prefix` (#486), `ValidateRequestHeaderLayer::has_header_value()` (#360),
`UnsyncBoxBody::new()` (#537), `Default` dla `limit::ResponseBody` (#679);
`trace::DefaultOn*` jawnie parentują eventy do spanu żądania (#690); CORS — złagodzone defaulty `Vary` (#674).

**tower-http 0.7.1 (2026-08-31):** `ServeDir::redirect_to_trailing_slash()` (#728);
`ignore_multi_range_requests()` (#727); `const fn` konstruktory w request-id (#716);
**behavioral change:** `ServeDir::try_call` propaguje błędy I/O gdy brak fallbacku, zamiast 404 (#718);
decompression — fix gubionych trailerów (#722, regresja z 0.7.0) i fix cichego truncate (#712).

Źródła: https://raw.githubusercontent.com/tokio-rs/axum/main/axum/CHANGELOG.md ·
https://github.com/tokio-rs/axum/releases.atom · https://raw.githubusercontent.com/tokio-rs/axum/main/axum/Cargo.toml ·
https://raw.githubusercontent.com/hyperium/hyper/master/CHANGELOG.md ·
https://github.com/tower-rs/tower-http/releases.atom · https://crates.io/api/v1/crates/tower-http

---

## 6. tokio

| | wersja | data |
|---|---|---|
| projekt | **1.52.3** | 2026-05-08 |
| latest | **1.53.1** | 2026-07-20 |

### 1.52.4 (2026-07-16)
- Fixed: runtime — nie pomijaj drivera gdy `before_park` zaplanuje pracę (#8222). **Tylko fix, brak security.**

### 1.53.0 (2026-07-17)
- Added: `fs` — `From<OwnedFd>` i `From<OwnedHandle>` dla `File` (#8266); metrics — task schedule
  latency metric (#7986); net — metody `SocketAddr` dla Unix sockets (#8144).
- Changed: **`sync::mpsc::{Receiver, UnboundedReceiver}` zwalniają teraz waker przy dropie, nawet
  gdy nadal istnieją nadawcy** (#8095) — zmiana zachowania, potencjalnie istotna dla kodu
  polegającego na starym zachowaniu; `time::timeout_at()` z `#[track_caller]` (#8077);
  io — `#[inline]` na implach traitów IO (#8242); net — `UCred::pid` na FreeBSD (#8086), wsparcie NuttX (#8259).
- Fixed: io — zero-length reads nie są traktowane jako EOF w `Chain` (#8251); runtime — unikanie
  nielegalnego stanu w `FastRand` (#8078); sync — wybudzanie odbiorcy mpsc gdy `reserve[_many]`
  zwraca permit (#8260); time — fix stack overflow w konstruktorze runtime (#8093);
  time (alt timer) — timery zostają w tym samym runtime po `.reset()` (#8169);
  taskdump — brak podwójnego wake (#8043).
- IO uring (unstable): io-uring dla `fs::try_exists` (#8080) i rename (#7800).
- **Brak zmian breaking.**

### 1.53.1 (2026-07-20)
- Fixed: **signal — przywrócenie MSRV przez usunięcie `OnceLock::wait` z handlera Windows** (#8300).
  To fix regresji MSRV wprowadzonej w 1.53.0.
- Fixed (unstable): time — fix race przy anulowaniu i wstawianiu alt timera (#8252).

### RustSec advisory dla tokio w 2026

**Nie znaleziono żadnego advisory dla crate'a `tokio` (rdzeń 1.x).** Katalog `crates/` w
`rustsec/advisory-db` zawiera wyłącznie crate'y `tokio-*` (tokio-codec, tokio-compat,
tokio-current-thread, tokio-executor, tokio-fs, tokio-io, tokio-postgres, tokio-process, tokio-proto,
tokio-reactor, tokio-rustls, tokio-signal, tokio-sync, tokio-tar, tokio-tcp, tokio-threadpool,
tokio-timer, tokio-tls, tokio-udp, tokio-uds) — **brak wpisu `tokio`**. Wpis `tokio-tar` dotyczy
innego crate'a.

**Podsumowanie:** 1.52.4 / 1.53.0 / 1.53.1 **nie zawierają fixów bezpieczeństwa** i **nie zawierają
breaking changes**. Bump jest bezpieczny.

Źródła: https://raw.githubusercontent.com/tokio-rs/tokio/master/tokio/CHANGELOG.md ·
https://github.com/tokio-rs/tokio/releases.atom · https://github.com/rustsec/advisory-db/tree/main/crates ·
https://rustsec.org/advisories/

---

## 7. clap

| | wersja | data |
|---|---|---|
| projekt | **4.6.1** | 2026-04-15 |
| latest | **4.6.7** | 2026-09-14 |

### clap 5 — status

**Nie wydany.** `CHANGELOG.md` na `master` zawiera sekcję:

```
## 5.0.0 - TBD
*available through `unstable-v5` feature flag*
```

Planowane breaking changes w 5.0.0:
- `ArgPredicate` → `#[non_exhaustive]`.
- *(help)* domyślny `Command::term_width` → „source format"; domyślny `Command::max_term_width` → 100.
- *(derive)* `Vec<Vec<T>>` — traktowane jako zbieranie wystąpień (occurrences).
- *(derive)* warianty `ValueEnum` używają całego doc comment, nie summary, dla `PossibleValue::help`.
- *(derive)* domyślne dla deferringu zmienione na `#[command(defer = true)]`.
- *(derive)* Feature: grupowanie wartości wg wystąpienia z `Vec<Vec<T>>`.

Czyli: clap 5 istnieje tylko za feature flagą `unstable-v5`, bez wydania.

### Zmiany w 4.6.x (projekt 4.6.1 → 4.6.7) — wszystkie nie-breaking

| wersja | data | zmiana |
|---|---|---|
| **4.6.2** | 2026-07-15 | Fix *(help)*: mów „alias" gdy jest tylko jeden |
| **4.6.3** | 2026-07-20 | Fix *(derive)*: `"literal".function()` jako wartości atrybutów |
| **4.6.4** | 2026-07-21 | Internal: update do syn v3 |
| **4.6.5** | 2026-07-31 | Fix *(help)*: poprawne oznaczanie opcjonalnych `value_names` z `num_args` |
| **4.6.6** | 2026-08-06 | Feature: `Command::get_overridden_usage` |
| **4.6.7** | 2026-09-14 | Feature *(derive)*: atrybut `#[command(defer = <bool>)]` — opt-in do lazy initialisation subcommands |

Uwaga historyczna: **4.6.0 (2026-03-12) podniósł MSRV do 1.85** (zmiana „Compatibility") —
projekt jest już na 4.6.1, więc to nie dotyczy.

Źródła: https://raw.githubusercontent.com/clap-rs/clap/master/CHANGELOG.md ·
https://github.com/clap-rs/clap/releases.atom · https://crates.io/api/v1/crates/clap

---

## 8. ort (ONNX Runtime Rust bindings)

| | wersja | data |
|---|---|---|
| projekt | **2.0.0-rc.12** (exact pin) | 2026-03-05 |
| latest | **2.0.0-rc.13** | 2026-07-28 |
| stable | **brak** (`max_stable_version = None` na crates.io) | — |

### rc.13 vs rc.12 — breaking changes

Z release notes https://github.com/pykeio/ort/releases/tag/v2.0.0-rc.13 :

1. **Execution provider clarity — breaking:** struktury EP (np. `ep::CUDA`) są teraz **bramkowane
   w czasie kompilacji** przez odpowiednie feature flagi. Wcześniej flagi EP zmieniały tylko
   zachowanie runtime — to był częsty punkt bólu.
2. **Link-time error — breaking w praktyce:** ort **zgłosi błąd na etapie linkowania**, jeśli
   `download-binaries` jest włączone i żadne binarium nie zawiera wszystkich żądanych EP.
   Wcześniej nonsensowna kombinacja (np. `cuda` + `coreml`) **po cichu spadała do CPU-only**.
   Nowy feature `lax-feature-matching` przywraca fallback do najbliższego pasującego zestawu.
3. **ONNX Runtime 1.28:** rc.13 **przeskakuje 4 wersje ONNX Runtime do v1.28** (nowe operatory,
   bugfixy, poprawki bezpieczeństwa, wydajność).
4. **Tylko CUDA 13:** rc.13 dostarcza wyłącznie binaria CUDA 13 — ONNX Runtime **zdeprecjonował CUDA 12**.
5. **Custom operator ergonomics — breaking:** przeprojektowany interfejs operatorów, np.
   `type Kernel<'attr> = ort::operator::BoxedKernel<'attr>;`.

Kontekst z rc.12 (2026-03-05): multiversioning (obsługa ORT v1.17–v1.24 przez feature flagi `api-*`;
**jeśli używasz `default-features = false`, włącz `api-24`**), automatyczny wybór urządzenia
(`SessionBuilder::with_auto_device`, ORT ≥ 1.22), CUDA 12 + CUDA 13, `SessionBuilder` error recovery.

### Którą wersję biblioteki C targetuje rc.13?

**ONNX Runtime 1.28.** Potwierdzone podwójnie:
- Release notes: „rc.13 skips ahead 4 ONNX Runtime versions to v1.28".
- `Cargo.toml` rc.13, pole `description`: **„A safe Rust wrapper for ONNX Runtime 1.28"**.

Domyślne features rc.13: `["std", "ndarray", "tracing", "download-binaries", "tls-native", "copy-dylibs", "api-27"]`.
fastembed ustawia `default-features = false, features = ["ndarray", "std", "api-24"]`.

**Uwaga:** ONNX Runtime wydał od tego czasu **1.28.1 (2026-08-18)**, **1.28.2 (2026-09-03)**,
**1.29.0 (2026-08-12)**, **1.29.1 (2026-09-10)** i **1.30.0 (2026-09-10)** — ort rc.13 pozostaje
przypięty do linii 1.28. **Nie zweryfikowano**, czy binaria pobierane przez `download-binaries`
zostały zaktualizowane do 1.28.1/1.28.2.

### Czy stable 2.0.0 jest planowane lub wydane?

- **Nie wydane:** crates.io `max_version = 2.0.0-rc.13`, `max_stable_version = None`.
- **Planowane:** release notes rc.11 (2026-04-15) mówią wprost: „the next big release of ort should
  be, finally, **2.0.0** 🎉 … I would really like to not have to do another major release right
  after". Mimo to od 2026-04-15 wydano tylko rc.13 (2026-07-28).
- **Nie zweryfikowano** daty wydania stable 2.0.0 ani tego, czy jest oficjalnie zaplanowana
  (brak publicznego roadmapu w zweryfikowanych źródłach).

### Alternatywne bindingi ONNX w Rust

- **`tract`** — czysto-Rustowy inference engine (bez FFI do ONNX Runtime), wspiera WASM.
  Wymieniony jako rekomendowana alternatywa w release notes ort v2.0.0-rc.5 (2024-08-18).
- **`wonnx`** — inference ONNX na WebGPU/WASM, również wymieniony tamże.
- Sam ort utrzymuje backend `backends/tract` w workspace (widoczny w `exclude` w `Cargo.toml` rc.13).
- **Nie zweryfikowano** aktualnych wersji ani dat wydania `tract`/`wonnx` — nie były przedmiotem
  tego researchu; powyższe pochodzi z release notes ort z 2024-08-18 i z układu repo ort.

Źródła: https://github.com/pykeio/ort/releases/tag/v2.0.0-rc.13 ·
https://raw.githubusercontent.com/pykeio/ort/v2.0.0-rc.13/Cargo.toml ·
https://crates.io/api/v1/crates/ort · https://github.com/microsoft/onnxruntime/releases.atom

---

## 9. fastembed ⚠️ KRYTYCZNE

| | wersja | data |
|---|---|---|
| projekt | **5.13.4** | (pin `5.13`) |
| latest | **7.0.1** | 2026-09-16 |

Ścieżka: **6.0.0** (2026-08-16) → 6.0.1 (2026-08-23) → 6.0.2 (2026-08-27) → 6.0.3 (2026-09-07) →
**6.1.0** (2026-09-12) → **7.0.0** (2026-09-16) → **7.0.1** (2026-09-16).

**Uwaga metodologiczna:** repo **nie ma pliku CHANGELOG.md** — `CHANGELOG.md` w root zwraca 404,
podobnie jak brak alternatywnej ścieżki. Release notes żyją **wyłącznie** w GitHub Releases.
Wersje potwierdzone niezależnie przez crates.io API.

### 6.0.0 (2026-08-16) — BREAKING

- **`fastembed::Error` zmieniło się z type aliasu `anyhow::Error` na error enum** (#278).
  To jedyna pozycja w sekcji „BREAKING". Dokumentacja: „Error handling reference".
- Konsekwencja: kod robiący `?`/`anyhow::Context` na błędach fastembed, albo dopasowujący
  `anyhow::Error`, wymaga refaktoryzacji. Enum (7.0.1, `#[non_exhaustive]`, 16 wariantów):
  `ModelRetrieval`, `TokenizerConfig`, `Tokenization`, `EmptyTokenizations`, `ImageDecode`,
  `ImageTransform`, `InvalidArgument`, `InvalidShape`, `Io`, `Ort`, `OrtBuilder`, `OrtSession`,
  `Other`, `OutputKeyMissing`, `PreprocessorConfig`, `TensorExtraction`.

### 7.0.0 (2026-09-16) — BREAKING

1. **`UserDefinedSparseModel::new` przyjmuje rodzinę `SparseModel` jako trzeci argument:**
   `new(onnx_file, tokenizer_files, model)`.
2. **Image embeddings dla nie-kwadratowych wejść różnią się** od wcześniejszych wydań — dotyczy
   wszystkich modeli obrazowych.
3. **Qwen3-VL image embeddings różnią się od 5.16.1–6.1.0.**
4. **Modele candle** (`Qwen3TextEmbedding`, `Qwen3VLEmbedding`, `NomicV2MoeTextEmbedding`) cache'ują
   teraz pod **`FASTEMBED_CACHE_DIR` (domyślnie `.fastembed_cache`)** zamiast `~/.cache/huggingface/hub`.
   **Istniejące wagi są ponownie pobierane**, chyba że ustawiono `HF_HOME`.
5. **`fastembed::InitOptions` to teraz deprecated alias `TextInitOptions`** i **emituje warning**.
   Kod projektu używający `InitOptions` będzie się kompilował, ale z warningiem.

Pozostałe w 7.0.0: fixy Qwen3-VL MRoPE, image preprocessing (aspect-preserving `shortest_edge` resize,
poprawna kolejność width/height, CHW layout), `Error::OutputKeyMissing` zamiast paniki dla sparse
models, BGE-M3 `try_new_from_user_defined`/`try_new_from_path` respektują `disable_cpu_fallback`
i `dimension_overrides`, tokenizer loading akceptuje `pad_token` jako obiekt `AddedToken`.
API additions: `SparseTextEmbedding::try_new_from_user_defined`, `UserDefinedSparseModel::with_idf_file`,
`NomicBertModel` re-eksportowany z publicznym konstruktorem. **MSRV = 1.88** (projekt: 1.91 ✓).

### 7.0.1 (2026-09-16) — patch
- `docs: Update version in README.md (#302)` — **tylko dokumentacja**.

### Który ort wymagają 6.0.0 i 7.x? (kluczowe dla projektu)

**Oba wymagają `ort = "=2.0.0-rc.13"`.** Zweryfikowane w `Cargo.toml` tagów:

| tag | ort requirement | tokenizers | candle |
|---|---|---|---|
| **v5.13.4** (projekt) | `=2.0.0-rc.12` | 0.22.2 | 0.10.2 |
| **v6.0.0** | **`=2.0.0-rc.13`** | 0.22.2 | 0.11.0 |
| **v7.0.1** | **`=2.0.0-rc.13`** | **0.23.2** | 0.11.0 |

Bump ort → rc.13 nastąpił już w **5.17.4 (2026-07-28)**: „Bump ort to 2.0.0-rc.13(#275)".

⚠️ **Projekt ma własny exact pin `ort = "=2.0.0-rc.12"`** (`crates/vestige-core/Cargo.toml`, dla
feature'a `late-interaction`). Dwa sprzeczne exact piny (`=2.0.0-rc.12` vs `=2.0.0-rc.13`)
**uniemożliwią rozwiązanie zależności** — bump fastembed do ≥6.0.0 **wymaga** jednoczesnego
podniesienia pinu ort do `=2.0.0-rc.13`.

⚠️ **tokenizers:** fastembed ≥6.0.3 wymaga `tokenizers 0.23.2`, a projekt pinuje `tokenizers = "0.22"`.
0.22 i 0.23 są semver-niekompatybilne → cargo skompiluje **dwie kopie** (jedną dla fastembed,
jedną dla `late-interaction`), albo trzeba podnieść pin projektu do 0.23.

### Czy feature flagi projektu są nadal poprawne?

**TAK — `hf-hub-rustls-tls` i `ort-download-binaries-rustls-tls` są nadal prawidłowymi nazwami
w 6.0.0 i 7.0.1.** Sekcja `[features]` w obu tagach zawiera:
```toml
hf-hub-rustls-tls = ["hf-hub", "hf-hub?/rustls-tls"]
ort-download-binaries-rustls-tls = ["ort/download-binaries", "ort/tls-rustls"]
```
Domyślne features w 7.0.1 to `["ort-download-binaries-native-tls", "hf-hub-native-tls", "image-models"]`
— identycznie jak w 5.13.4, więc `default-features = false` + dwie flagi rustls daje ten sam efekt
(native-tls zamienione na rustls, `image-models` wyłączone). **Bez zmian w konfiguracji features.**

### Czy `TextEmbedding` / `EmbeddingModel` się zmieniły? Czy modele zostały usunięte?

- **`TextEmbedding` nadal istnieje** w 7.0.1 (docs.rs: `struct.TextEmbedding.html`), z metodami
  `try_new`, `try_new_from_user_defined`, `embed`, `list_supported_models`, `get_model_info`,
  `get_default_pooling_method`, `get_quantization_mode`, `model_name`, `dimensions`.
  Re-eksport w `lib.rs` 7.0.1: `pub use crate::text_embedding::{...}`.
- **`EmbeddingModel` nadal re-eksportowany** z crate root w 7.0.1:
  `pub use crate::models::text_embedding::EmbeddingModel;` — identycznie jak w 5.13.4.
- **`nomic-embed-text-v1.5` NIE został usunięty.** W `src/models/text_embedding.rs` tagu **v7.0.1**:
  - linia 33–34: `/// nomic-ai/nomic-embed-text-v1` → `NomicEmbedTextV1`
  - linia 35–36: `/// nomic-ai/nomic-embed-text-v1.5` → **`NomicEmbedTextV15`** ✓
  - linia 37–38: `/// Quantized v1.5 nomic-ai/nomic-embed-text-v1.5` → **`NomicEmbedTextV15Q`** ✓
  - linia 219–222: mapowanie `model_code: "nomic-ai/nomic-embed-text-v1.5"` ✓
  - warianty są też obecne w match armach (linie 611–613, 660–662).
- Odpowiednik typu w projekcie (`NGEmbeddingModel::NomicEmbedTextV15`) to alias lokalny —
  wariant enuma `EmbeddingModel::NomicEmbedTextV15` **istnieje w 7.0.1**.
- **`InitOptions`** nadal istnieje jako `pub type InitOptions = TextInitOptions;` (lib.rs 7.0.1, linia 108)
  — **z deprecation warningiem** (patrz breaking change #5 z 7.0.0).

**Podsumowanie migracji 5.13.4 → 7.0.1:** zmiana obsługi błędów (`Error` enum), bump ort do rc.13
(wymuszony przez exact pin), potencjalny bump tokenizers do 0.23, deprecation `InitOptions`.
Features bez zmian, `NomicEmbedTextV15` bez zmian.

Źródła: https://github.com/Anush008/fastembed-rs/releases.atom ·
https://raw.githubusercontent.com/Anush008/fastembed-rs/v7.0.1/Cargo.toml ·
https://raw.githubusercontent.com/Anush008/fastembed-rs/v6.0.0/Cargo.toml ·
https://raw.githubusercontent.com/Anush008/fastembed-rs/v5.13.4/Cargo.toml ·
https://raw.githubusercontent.com/Anush008/fastembed-rs/v7.0.1/src/lib.rs ·
https://raw.githubusercontent.com/Anush008/fastembed-rs/v7.0.1/src/models/text_embedding.rs ·
https://docs.rs/fastembed/7.0.1/fastembed/ · https://crates.io/api/v1/crates/fastembed

---

## 10. git2 / notify / criterion / lru / tokenizers

### git2 — major-version breakage ⚠️

| | wersja | data |
|---|---|---|
| projekt | **0.20.4** (pin `0.20`, feature `vendored-openssl`) | 2026-02-02 |
| latest | **0.21.0** | 2026-05-18 |

**Breaking changes w 0.21.0** (źródło: `CHANGELOG.md`, https://raw.githubusercontent.com/rust-lang/git2-rs/master/CHANGELOG.md):

1. ❗ **`ssh`, `https` i `cred` NIE są już domyślnymi feature'ami.** Było `default = ["ssh", "https"]`,
   jest **`default = []`**. Trzeba je włączyć jawnie, jeśli zależy się na credential helperach
   lub transporcie (#1168).
   **Konsekwencja dla projektu:** obecny zapis `git2 = { version = "0.20", features = ["vendored-openssl"] }`
   polega na domyślnych `ssh` + `https`. Po bumpie do 0.21 **cicho straci** obsługę SSH i HTTPS.
   Poprawny zapis: `features = ["vendored-openssl", "ssh", "https"]`.
2. ❗ **`CredentialHelper` i zależność `url` są za feature'em `cred`.** Włączenie `ssh` lub `https`
   pociąga `cred` tranzytywnie (#1168).
3. ❗ **Edycja 2021** (#1173).
4. ❗ **Wiele akcesorów stringów zwraca teraz `Result<&str, Error>` / `Result<Option<&str>, Error>`
   zamiast `Option<&str>`** — pozwala odróżnić brak wartości od nie-UTF-8 (#1241).
5. ❗ **`BlameHunk::final_signature`, `final_committer`, `orig_signature`, `orig_committer`
   zwracają teraz `Option`** — unikanie segfaultów przy brakującej sygnaturze (#1254).
6. Bump `libgit2-sys` 0.18.4 → **libgit2 1.9.3** (#1242).

Dodatki (nie-breaking): eksperymentalne wsparcie SHA256 za feature `unstable-sha256` + warianty
`*_ext` z `ObjectFormat` (#1206); `opts::set_cache_max_size()`/`get_cached_memory()` (#1188);
`Repository::object_format()` + enum `ObjectFormat` (#1204); `Repository::set_config()` (#1208);
`merge_file()` + `MergeFileInput` (#1210); `Repository::refdb_compress()` (#1221); publiczny typ
`Refdb` (#1228); `Revspec::into_objects()` (#1230); `BlameHunk::final_committer/orig_committer/summary`
(#1231); `Clone` dla `Reference` (#1233); `Repository::author_from_env()`/`committer_from_env()` (#1237);
`impl From<Utf8Error> for Error` (#1239).

**`vendored-openssl` nadal istnieje** w 0.21.0: `vendored-openssl = ["openssl-sys/vendored", "libgit2-sys/vendored-openssl"]` ✓.
**Uwaga:** sam `vendored-openssl` **nie** włącza `https` — trzeba dodać `https` jawnie.

Aktualizacje libgit2 (poza 0.21.0, wydania `libgit2-sys`): 0.18.5+1.9.4 (2026-05-29),
0.18.6+1.9.5 (2026-07-22), 0.18.7+1.9.6 (2026-07-22), **0.18.8+1.9.7 (2026-08-21)** — najnowsze.

### notify

| | wersja | data |
|---|---|---|
| projekt | **8.2.0** | 2025-08-03 |
| latest **stable** | **8.2.0** | 2025-08-03 |
| latest **pre-release** | **9.0.0-rc.5** | 2026-08-30 |

**notify 9.0.0 nie jest jeszcze wydane** — dostępne jest `9.0.0-rc.5`. Breaking changes z linii 9.0.0-rc:
- **rc.5 (2026-08-30):** `CHANGE: update to edition 2024`; FreeBSD — natywny inotify dla FreeBSD 14.5+,
  w przeciwnym razie kqueue (`freebsd_inotify` feature do cross-compile); `Config::with_fsevent_latency` (macOS);
  `Watcher::watch_with` + `WatchPathConfig::with_dereference_symlinks` (#255).
- **rc.4 (2026-05-02):** **`CHANGE: preserve watched path representation in `Event.paths` i
  `Watcher::watched_paths`; relatywne ścieżki watch dają teraz relatywne ścieżki eventów
  konsekwentnie na wszystkich backendach** (#453, #740) — istotna zmiana zachowania. Fixy kqueue/Windows.
- **rc.3 (2026-04-16):** **`CHANGE: raise MSRV to 1.88`**; **`#[must_use]` na API builderów,
  konstruktorów i getterów** (`Config`, `PathOp`, `Error`) — może generować nowe warningi;
  `Watcher::watched_paths`; normalizacja ścieżek Windows (#375).
- **rc.2 (2026-02-14)**, **rc.1 (2026-01-25)** — poza zakresem pobranego fragmentu changeloga;
  **nie zweryfikowano** szczegółów.

Projekt używa `notify = "8"` → **8.2.0 pozostaje latest stable**, brak presji na migrację.

### criterion

| | wersja | data |
|---|---|---|
| projekt | **0.5.1** (dev-dependency, `html_reports`) | 2023-05-26 |
| latest | **0.8.2** | 2026-02-04 |

⚠️ **Repo przeniesione:** `bheisler/criterion.rs` → **`criterion-rs/criterion.rs`**
(crates.io `repository` dla 0.8.2 wskazuje nowe repo). Stary `CHANGELOG.md` w `bheisler/...`
jest **przestarzały** (kończy się na 0.7.0). Właściwy: https://raw.githubusercontent.com/criterion-rs/criterion.rs/master/CHANGELOG.md

**Breaking changes na ścieżce 0.5.1 → 0.8.2:**

- **0.6.0 (2025-05-17):**
  - MSRV → **1.80**.
  - **Feature `real_blackbox` nie ma już żadnego efektu** — criterion zawsze używa
    `std::hint::black_box()`. **Kod używający `criterion::black_box()` powinien przejść na
    `std::hint::black_box()`** — to najważniejsza zmiana dla istniejących benchmarków.
  - `clap` dependency odpięty.
  - Fixed: poprawne wykrywanie gnuplot na niektórych binariach Windows.
  - Added: async benchmarking z `tokio::runtime::Handle` (nie tylko `Runtime`).
- **0.7.0 (2025-07-25):** bump `criterion-plot` dla wyrównania zależności (bez breaking API).
- **0.8.0 (2025-11-29):**
  - **BREAKING: usunięto wsparcie `async-std`.**
  - MSRV → **1.86**, stable → 1.91.1.
  - Added: throughput na stronie summary; `Throughput::ElementsAndBytes`;
    alloca-based memory layout randomisation.
  - Fixed: bug plottowania NaN.
- **0.8.1 (2025-12-07):** fix linku homepage.
- **0.8.2 (2026-02-04):** fix — nie buduj alloca na nieobsługiwanych targetach; fix paniki przy
  uniform iteration durations; wykluczenie skryptów deweloperskich z paczki.

Nowe features w 0.8.2: `default = ["rayon", "plotters", "cargo_bench_support"]`,
`stable = ["csv_output", "html_reports", "async_futures", "async_smol", "async_tokio"]`.
**Uwaga:** projekt włącza `features = ["html_reports"]` — feature nadal istnieje ✓, ale domyślnie
`html_reports` jest **poza** `default` (od 0.4.0), więc jawne włączenie pozostaje potrzebne.

### lru

| | wersja | data |
|---|---|---|
| projekt | **0.16.4** | 2026-04-13 |
| latest | **0.18.4** | 2026-09-03 |

Zmiany (źródło: https://raw.githubusercontent.com/jeromefroe/lru-rs/master/CHANGELOG.md):

- **0.16.3 (2026-01-07):** fix Stacked Borrows violation w `IterMut` — **fix soundness**.
- **0.16.4 (2026-04-13):** add `get_or_insert_with_key` i warianty (projekt już to ma).
- **0.17.0 (2026-04-14):** **breaking** — `hashbrown` → 0.17.0, **MSRV → 1.85.0**.
- **0.18.0 (2026-04-27):** **breaking** — „Fix unconstrained lifetime in `get_or_insert_mut_ref`"
  (zmiana sygnatury).
- **0.18.1 (2026-07-09):** add `find_and_promote`.
- **0.18.2 (2026-08-03):** **„Fix panic-safety unsoundness in `pop` method"** — istotny fix
  bezpieczeństwa pamięci (panic safety).
- **0.18.3 (2026-08-27):** add `sparse` constructor.
- **0.18.4 (2026-09-03):** add `retain` method.

**Uwaga:** dwie wersje 0.17.0 i 0.18.0 wydane dzień po dniu (2026-04-14/2026-04-27) — 0.17.0 to
głównie bump hashbrown + MSRV, 0.18.0 to fix lifetime. Projekt na 0.16.4 nie ma fixów
`get_or_insert_mut_ref` ani soundness `pop`. Rekomendacja: bump do **0.18.4** (wymaga MSRV 1.85 —
projekt ma 1.91 ✓).

### tokenizers

| | wersja | data |
|---|---|---|
| projekt | **0.22.2** (optional, `late-interaction`) | 2025-12-02 |
| latest | **0.23.2** | 2026-09-03 |

- **0.23.0 nigdy nie zostało opublikowane.** Release notes v0.23.1: „0.23.0 only ever shipped as
  rc0 because the release pipeline itself was broken … There is no functional 0.23.0 published —
  we tag 0.23.1 directly so users don't accidentally pull a never-shipped version."
  Potwierdzone na crates.io: wersje to 0.22.2 → **0.23.1** (2026-04-27) → **0.23.2** (2026-09-03),
  bez 0.23.0.
- **0.23.1 (2026-04-27):** „first proper stable release in the 0.23 line". Ogłoszone breaking changes
  dotyczą **wyłącznie Pythona**: „Drop Python 3.9 (#1952) — requires-python = \">=3.10\"; 3.9 users
  stay on 0.22.x". Reszta to wydajność (BPE / added-vocab hot paths), Node multi-platform wheels,
  Python 3.14 / 3.14t, type hints.
- **0.23.2 (2026-09-03):** **„This is the last v0 release, we are moving to v1!!"** — zapowiedź v1.
  Zmiany: `daachorse` 1.0.1 → 3.0.0 (#2050); perf — unikanie pełnego klonowania vocab w
  `get_vocab_size()` (#2074); `daachorse`-related; bindings — jednokrotne pozyskanie read-locka
  modelu na wywołanie zamiast per pre-token (#2072); bump pyo3 do 0.29 (#2115); CI.
- **Rust-side breaking changes: NIE ZWERYFIKOWANO.** Plik `tokenizers/CHANGELOG.md` w repo jest
  **przestarzały** — ostatni wpis to `[0.13.2]`. Nie znalazłem żadnego udokumentowanego breakingu
  dla API Rusta między 0.22 a 0.23. Jedyna ogłoszona zmiana breaking dotyczy Pythona.
- **Istotne dla projektu:** fastembed ≥6.0.3 wymaga `tokenizers 0.23.2`. Projekt pinuje `0.22`
  w feature `late-interaction` → patrz punkt 9 (dwie semver-niekompatybilne kopie).

Źródła: https://crates.io/api/v1/crates/{git2,notify,criterion,lru,tokenizers} ·
https://raw.githubusercontent.com/rust-lang/git2-rs/master/CHANGELOG.md ·
https://raw.githubusercontent.com/notify-rs/notify/main/notify/CHANGELOG.md ·
https://raw.githubusercontent.com/criterion-rs/criterion.rs/master/CHANGELOG.md ·
https://raw.githubusercontent.com/jeromefroe/lru-rs/master/CHANGELOG.md ·
https://github.com/huggingface/tokenizers/releases.atom ·
https://raw.githubusercontent.com/huggingface/tokenizers/main/tokenizers/CHANGELOG.md

---

## Zestawienie zbiorcze

| # | crate | projekt | latest | data latest | breaking? |
|---|---|---|---|---|---|
| 1 | rusqlite | 0.39.0 | 0.40.2 | 2026-08-08 | TAK (vtab) + SQL injection fix w 0.40.1 |
| 2 | libsqlite3-sys | 0.37.0 | 0.38.2 | 2026-08-08 | nie (ale CVE FTS5) |
| 3 | usearch | 2.25.2 | 2.26.2 | 2026-08-31 | nie (format niezmieniony) |
| 4 | ts-rs | 12.0.1 | 12.0.1 | 2026-01-31 | brak nowszej wersji |
| 5 | axum | 0.8.9 | 0.8.9 | 2026-04-14 | 0.9 staged na main, niewydane |
| 5 | hyper | 1.9.0 | 1.11.1 | 2026-08-28 | nie |
| 5 | tower | 0.5.3 | 0.5.3 | 2026-01-12 | — |
| 5 | tower-http | 0.6.10 | 0.7.1 | 2026-08-31 | TAK (0.7.0) |
| 6 | tokio | 1.52.3 | 1.53.1 | 2026-07-20 | nie, brak security |
| 7 | clap | 4.6.1 | 4.6.7 | 2026-09-14 | nie (clap 5 za `unstable-v5`) |
| 8 | ort | 2.0.0-rc.12 | 2.0.0-rc.13 | 2026-07-28 | TAK (EP gating, ORT 1.28, CUDA 13) |
| 9 | fastembed | 5.13.4 | 7.0.1 | 2026-09-16 | TAK (6.0.0 Error enum; 7.0.0 cache/InitOptions) |
| 10 | git2 | 0.20.4 | 0.21.0 | 2026-05-18 | TAK (default features, Result zwroty) |
| 10 | notify | 8.2.0 | 8.2.0 (9.0.0-rc.5) | 2025-08-03 (rc 2026-08-30) | 9.x to pre-release |
| 10 | criterion | 0.5.1 | 0.8.2 | 2026-02-04 | TAK (black_box, async-std) |
| 10 | lru | 0.16.4 | 0.18.4 | 2026-09-03 | TAK (lifetime, hashbrown) |
| 10 | tokenizers | 0.22.2 | 0.23.2 | 2026-09-03 | Python-only; v1 zapowiedziane |

## Czego NIE zweryfikowano

- ts-rs: publiczna roadmapa / otwarte PR dla `export_to`, `rename_all`, `optional` dla przyszłego 13.x
  (api.github.com rate-limited 403; strona issues HTML nie zwróciła listy).
- usearch: oficjalna deklaracja o kompatybilności formatu serializacji 2.25.x ↔ 2.26.x
  (wniosek wyłącznie z analizy źródeł; brak CHANGELOG w repo).
- ort: data wydania stable 2.0.0; czy binaria `download-binaries` w rc.13 zostały zaktualizowane
  do ONNX Runtime 1.28.1/1.28.2.
- tract / wonnx: aktualne wersje i daty wydań.
- tokenizers: ewentualne breaking changes po stronie API Rusta między 0.22 a 0.23
  (Rust changelog w repo jest przestarzały — ostatni wpis 0.13.2).
- notify 9.0.0-rc.1 / rc.2: szczegóły zmian (poza pobranym fragmentem changeloga).

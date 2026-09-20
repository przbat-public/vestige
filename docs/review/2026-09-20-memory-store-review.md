# Przegląd 13 wspomnień z żywego magazynu — 2026-09-20

Pytanie, na które odpowiada ten dokument: **czy ktoś, kto czyta te wspomnienia bez
kontekstu, wiedziałby, o co chodzi?**

Materiał: `~/Library/Application Support/com.vestige.core/vestige.db`, odczyt wyłącznie
do odczytu (13 wierszy, wszystkie zapisane 2026-09-20 między 15:37:37 a 15:37:51).
Treści czytane dwiema drogami: bezpośrednio z SQLite oraz przez działający serwer MCP
(`POST /mcp`, `search` na trzech poziomach szczegółowości) — bo to, co widzi czytelnik,
zależy od drogi odczytu.

> **Status usterek po wdrożeniu napraw.** U1 (fałszywe „Correction") — domknięte szerzej, niż
> zakładał ten dokument: po pomiarze okazało się, że nawet zaostrzony detektor oznacza 39 z 156
> par jako korekty, więc **ścieżka automatyczna przestała wycofywać wspomnienia w ogóle**
> (`GateDecision::Contradiction`, krawędź `contradicts`, `valid_until` tylko na wyraźne żądanie).
> U2 (scalanie) — domknięte: podobna treść trafia do osobnego wspomnienia i zostaje powiązana.
> U3 (bramka na ścieżce `codebase`), U4 (tytuł decyzji) i U5 (tagi `entity:*`) — w toku; status
> zostanie dopisany po weryfikacji. U6 (`source` u czytelnika) — domknięte: widok `summary`
> i dashboard.
> Trzy wspomnienia w magazynie użytkownika nadal noszą fałszywe `valid_until` — ich poprawienie
> jest zapisem do danych i czeka na zgodę.

---

## 1. Odpowiedź

**Częściowo — i podział nie wypada tam, gdzie powinien.** Sześć wspomnień czyta się bez
zarzutu i to są dokładnie te, których bramka samoistności **nigdy nie sprawdziła**
(3 decyzje i 3 wzorce zapisane przez `codebase`). Siedem, które bramka sprawdziła i przepuściła,
czyta się gorzej: same w sobie są zrozumiałe, ale nie mówią, **czego dotyczą** — projekt
`nes-emulator-stm32` istnieje wyłącznie w polu `source`, którego domyślny odczyt nie zwraca.
Dwa wspomnienia są nieczytelne wprost, bo pod nagłówkiem o jednym zdarzeniu siedzi doklejona
treść o innym, a to samo zdarzenie istnieje w drugiej, konkurencyjnej wersji.

| id | typ | ocena dla czytelnika bez kontekstu | co przeszkadza |
|---|---|---|---|
| `c225a8c7` | concept | **tak** (wzór do naśladowania) | brak daty w treści; 3 śmieciowe tagi |
| `d7446713` | decision | **tak** | tytuł ucięty w połowie wyrazu; projekt tylko w `source` |
| `97bbe4ca` | decision | **tak** | tytuł ucięty („…jako ”); ścieżki plików w treści zamiast kotwic |
| `973f329d` | decision | **tak** | tytuł ucięty („…: instal”); „panel” nienazwany |
| `127860f3` | pattern | **tak** (ma „kiedy stosować / kiedy nie”) | brak nazwy projektu |
| `da17bf5d` | pattern | **tak** | brak nazwy projektu |
| `ae68d8d5` | pattern | **częściowo** | reguła dobra, ale pod nią doklejony `[Updated 2026-09-20]` o **innym** zdarzeniu (intro i tester) — czytelnik szuka związku i go nie znajduje; ten sam materiał jest osobno jako `7e7dff16` |
| `ed25287e` | event | **częściowo** | objaw→przyczyna→lekcja czyta się dobrze, ale nie wiadomo, co to za panel i jaki projekt; data tylko w metadanych |
| `23fec144` | event | **częściowo** | jak wyżej |
| `d59cea00` | event | **częściowo** | jak wyżej |
| `7e7dff16` | event | **częściowo** | brak nazwy własnego narzędzia („skrypt sterujący padem”) i projektu |
| `b96153d2` | event | **nie** | druga wersja `ed25287e` + doklejona lekcja o łączu debugowania, która ma własne wspomnienie `d59cea00`; czytelnik nie wie, która wersja obowiązuje |
| `5925e4a8` | event | **nie** | druga wersja `23fec144` + doklejony blok o porównywaniu klatki bajt w bajt (ta sama treść w `ae68d8d5`) |

Co czyta się dobrze i warto powtarzać:

- **Decyzje** mają kształt ADR (Context / Decision / Alternatives / Affected Files) i dzięki
  temu niosą własny kontekst: `d7446713` podaje konkret (Castlevania III wchodzi do intra,
  poziom nie startuje) i regułę z uzasadnieniem. To najmocniejsza grupa w magazynie.
- **Wzorce** kończą się zdaniem „stosować, gdy… / nie stosować, gdy…” — czytelnik wie, kiedy
  regułę zastosować, a to jest właśnie ta część, której nie ma w kodzie.
- **`c225a8c7`** jest najlepszym wspomnieniem w magazynie: nazywa podmiot (emulator NES na
  mikrokontrolerze z panelem na jednym przewodzie), podaje wynik pomiaru i uogólnia go na
  regułę („każdy projekt z wyświetlaczem na jednym przewodzie zacznij od policzenia czasu
  transmisji jednej klatki”). Tak powinny wyglądać pozostałe.

---

## 2. Co realnie dostaje czytelnik (dowód z odczytu)

`search` zwraca różne pola zależnie od `detail_level` (`tools/search_unified/format.rs`):

| poziom | pola |
|---|---|
| `brief` | `id`, `nodeType`, `tags`, `retentionStrength`, `combinedScore` — **bez treści** |
| `summary` (domyślny) | `id`, `content`, `tags`, `createdAt`, `recordedAt`, `updatedAt`, `epistemicStatus`, `memorySystem`, wyniki dopasowania — **bez `source`** |
| `full` | + `source`, `validFrom`/`validUntil`, `provenance`, `stability`, `reps`… |

Skutek: przy domyślnym odczycie nazwa projektu jest **niedostępna** — a to jedyne miejsce,
w którym w ogóle występuje. Treść mówi „panel”, „płytka”, „emulator”, „gra”. Czytelnik
otrzymuje `temporalHint` w rodzaju „from very recent (< 1 hour)”, który z czasem traci
sens, a sama treść nie zawiera żadnej daty.

Dashboard jest pod tym względem gorszy: `MemoryMetadataFooter.tsx` pokazuje
`createdAt` / `recordedAt` / `updatedAt` / `lastAccessedAt` (dobrze — to jest ta trójka
zegarów), ale `source` nie jest renderowane **nigdzie** w interfejsie; jedyne wystąpienie
`source` to pole formularza w `AddMemoryDialog.tsx`, czyli zapis bez odczytu. Człowiek
przeglądający pamięć nie widzi więc nawet tego, co widzi agent przy `detail_level=full`.

---

## 3. Usterki, które to powodują

### U1. Fałszywy werdykt „Correction” wycofuje prawdziwe wspomnienia

W magazynie jest 13 × `create`, 3 × `edit` i **6 × `supersede`** — te 6 dotyczy 3 wspomnień,
w tym 2 decyzji. Każde z tych 3 ma `valid_until` ustawione ~1,3 s po utworzeniu, czyli
w momencie nadejścia kolejnego, **niezwiązanego** zapisu:

```
973f329d (decision)  created 15:37:37.699  valid_until 15:37:38.806  <- b96153d2 (event o pasach)
97bbe4ca (decision)  created 15:37:37.469  valid_until 15:37:38.926  <- 5925e4a8 (event o rejestrach)
5925e4a8 (event)     created 15:37:38.926  valid_until 15:37:39.650  <- c225a8c7 (concept o transmisji)
```

Przyczyna: `advanced/prediction_error/gate.rs:153` wycofuje pamięć, gdy
`appears_contradictory && similarity >= correction_threshold`, gdzie próg wynosi **0.70**
(`constants.rs:13`), a `appears_contradictory` to wartość logiczna — **pewność detektora nie
jest sprawdzana**. Dwie pamięci o tym samym projekcie łatwo przekraczają 0.70 cosinusa.

Detektor (uruchomiony na prawdziwych parach z magazynu) myli się we **wszystkich trzech**
przypadkach:

| para | werdykt | dowód |
|---|---|---|
| `973f329d` ← `b96153d2` | `positive=true, confidence=0.6` | `CorrectionPhrase("w rzeczywistości")` — `nlp/contradiction.rs:284` traktuje zwykły zwrot „w rzeczywistości” jako frazę korekty |
| `97bbe4ca` ← `5925e4a8` | `positive=true, confidence=0.85` | `NegationScope("nie … może utrzymać zmiennej plikowej w rejestrze procesora przez")` — przeczenie dotyczy tego, czego **kompilator nie potrafi**, a nie tego wspomnienia; wiąże je wspólna fraza techniczna obecna w obu tekstach |
| `5925e4a8` ← `c225a8c7` | `positive=true, confidence=0.85` | `NegationScope("nie … emulacja procesora")` — przeciwstawienie w **jednym** zdaniu („wąskim gardłem okazała się transmisja obrazu, a nie emulacja procesora”) |

Skutki widoczne dla czytelnika: `temporal current` ukrywa trzy wspomnienia, które są nadal
prawdziwe; oś czasu twierdzi, że decyzja wygasła 1,3 s po podjęciu; w magazynie nie ma
śladu, dlaczego tak się stało (powód „Superseded by new memory: Correction” nic nie wyjaśnia).
To jest dokładnie odwrotność celu „czytelnik ma wiedzieć, jak zmieniał się obraz w czasie” —
oś czasu kłamie.

### U2. Ciche scalanie dokleja obcą treść pod cudzy nagłówek

`storage/sqlite/smart_ingest.rs:191-199` (`UpdateType::Merge | UpdateType::Append`) skleja
treść nową z istniejącą: `"{existing}\n\n[Updated {data}]\n{new}"`. W magazynie są 3 takie
wiersze (`ae68d8d5`, `5925e4a8`, `b96153d2`), każdy z rewizją `kind=edit`,
`reason="smart_ingest: merged with similar memory"`. Skutki:

- łamie własną zasadę atomowości (`AGENTS.md`: jedno wspomnienie = jeden fakt);
- pod nagłówkiem wzorca o diagnostyce zawieszonego emulatora leży zdarzenie o testerze,
  a pod zdarzeniem o pasach — lekcja o łączu debugowania;
- powstają konkurencyjne wersje tego samego: `ed25287e` ↔ `b96153d2` oraz
  `23fec144` ↔ `5925e4a8`; doklejone bloki to trzecie wystąpienie treści, która ma już
  własne wspomnienia (`7e7dff16`, `d59cea00`).

`memory_connections` = **0** — nic nie wiąże tych wersji ani nie wskazuje następcy.

### U3. Bramka samoistności nie obejmuje połowy drzwi zapisu

`self_contained` = `NULL` dla **6 z 13** wspomnień — wszystkich zapisanych przez `codebase`
(3 decyzje + 3 wzorce). Bramka jest wołana wyłącznie z `smart_ingest/{execute,batch}.rs`
(`detect_with_anchors`), a `tools/codebase_unified.rs` nie woła jej wcale. `NULL` znaczy
„nie sprawdzono”, nie „sprawdzone i czyste”, więc połowa magazynu nigdy nie przeszła
kontroli, którą uznajemy za warunek wartości wspomnienia.

Przy okazji ta sama ścieżka łamie drugą regułę z `AGENTS.md` („ścieżki plików trzymaj poza
treścią, należy do `code_refs`/`source`”): wstawia je do treści jako `## Files:` /
`## Affected Files:` i nie tworzy kotwic — `code_refs` = **0**, mimo że 6 wspomnień wymienia
`src/main.c`, `src/mapper.c`, `src/cpu6502.c`, `tutorial-pl`.

### U4. Tytuł decyzji to 50 bajtów ucięte w środku wyrazu

`tools/codebase_unified.rs:277`: `&decision[..decision.floor_char_boundary(50)]` — bez
wielokropka, bez granicy wyrazu i bez rozróżnienia bajtów od znaków (w polskim 50 bajtów to
~40 znaków). Efekt, widoczny jako pierwsza linia wspomnienia:

```
# Decision: Materiał zaczyna się od stanowiska pracy: instal
# Decision: Kod dydaktyczny powstaje obok produkcyjnego, jako
# Decision: Każdy obsługiwany układ kartridża implementuje
```

Nagłówek, który wygląda na ucięty, jest pierwszą rzeczą, jaką widzi czytelnik — i to on
trafia do wyszukiwania jako reprezentacja decyzji.

### U5. Śmieciowe tagi `entity:*` w tekstach polskich

`preprocessing/entities.rs:71-73` uznaje za nazwę własną każdy wyraz z wielkiej litery na
początku zdania, a lista `STOP_PROPER` (~50 słów) jest **wyłącznie angielska**. W polskim
każde zdanie zaczyna się z wielkiej litery, więc:

- 7 zdarzeń ma dokładnie po trzy śmieciowe tagi (`entity:objaw`, `entity:przyczyna`,
  `entity:lekcja`) — bo tyle zdań ma ta forma;
- `c225a8c7` ma `entity:wniosek`, `entity:kierunek`, `entity:ka…`.

Razem **21 z 59 tagów (36%)** to szum, powtarzalny w każdym wspomnieniu — czyli dokładnie
ten rodzaj tagu, który nie odróżnia niczego, a wpływa na filtry tagów i grupowanie.

### U6. `source` jest polem „zapisz i zapomnij”

Zapis ustawia je (`AddMemoryDialog.tsx`), odczyt przez agenta pokazuje tylko przy
`detail_level=full`, a dashboard nie renderuje go nigdzie. To jedyne miejsce, w którym
występuje nazwa projektu — więc ani człowiek, ani agent przy domyślnym odczycie nie ma
jak powiązać wspomnienia z projektem.

---

## 4. Propozycja kolejności napraw

| # | naprawa | gdzie | ryzyko |
|---|---|---|---|
| F1 | przestać wycofywać wspomnienia na podstawie słabego dowodu: usunąć/przewartościować `"w rzeczywistości"` z leksykonu `CorrectionPhrase`; wymagać, by `NegationScope` dotyczył **tego samego podmiotu i predykatu**, a nie dowolnej wspólnej frazy; uwzględnić `confidence` w bramce (np. ≥0,8) i podnieść `correction_threshold` powyżej 0,70 | `nlp/contradiction.rs:284`, `advanced/prediction_error/gate.rs:153`, `constants.rs:13` | wysokie — dotyka zachowania zapisu; wymaga testów na trzech parach z §3 jako fixture i przejścia `nlp_baseline` |
| F2 | domyślnie **nie** scalać: zapisać jako osobne wspomnienie i połączyć relacją (dziś `memory_connections` = 0), a scalanie zostawić tylko dla `similarity ≥ 0.92` i tego samego zdarzenia | `storage/sqlite/smart_ingest.rs:191-199` | średnie |
| F3 | bramka samoistności także na ścieżce `codebase`, z tym samym kontraktem (zapisz + oznacz, nigdy nie odrzucaj), plus `files` → `code_refs` zamiast `## Files:` w treści | `tools/codebase_unified.rs` | średnie |
| F4 | tytuł decyzji: pierwsze zdanie albo 120 znaków z wielokropkiem, nigdy w środku wyrazu | `tools/codebase_unified.rs:277` | niskie |
| F5 | nie tworzyć `entity:*` z wyrazu stojącego na początku zdania ani po etykiecie z dwukropkiem, chyba że występuje też w środku zdania; dodać listę stop dla PL | `preprocessing/entities.rs:71-73` | niskie |
| F6 | `source` w `summary` i w dashboardzie; pokazać też `validFrom`/`validUntil` | `search_unified/format.rs`, `MemoryMetadataFooter.tsx` | niskie |
| F7 | przepisać 13 wspomnień: dopisać projekt i datę, rozdzielić doklejone bloki, usunąć duplikaty, zdjąć fałszywe `valid_until` | magazyn użytkownika — **wymaga zgody**, to zapis do danych | średnie |

F7 jest jedyną pozycją dotykającą danych użytkownika; F1–F6 to kod i testy.

---

## 5. Materiał dowodowy

- 13 wierszy, `knowledge_nodes`; rozkład `self_contained`: 7 × `1`, 6 × `NULL`, 0 × oznaczone.
- `memory_revisions`: `create` 13, `edit` 3, `supersede` 6; 22 wiersze łącznie.
- `memory_connections` = 0, `code_refs` = 0.
- `valid_until` ustawione w 3 wierszach, wszystkie `<= created_at + 1,3 s`; `valid_from` = `NULL` we wszystkich 13.
- 3 wiersze z doklejonym `[Updated 2026-09-20]`.
- Tagi: 38 tematycznych, 21 `entity:*`.
- Sonda detektora (usunięta po diagnozie) na trzech parach z magazynu: 3/3 `positive`,
  confidence 0,6 / 0,85 / 0,85 — werdykty i dowody w §3/U1.

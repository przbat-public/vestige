# Projekt: wspomnienia samodzielne, wartościowe i osadzone w czasie

> **Kierunek (od użytkownika):** idziemy w stronę wspomnień samodzielnych i wartościowych. Każde
> wspomnienie musi nieść informację, **kiedy zostało zachowane**, bo czytelnik w przyszłości musi
> widzieć, jak zmieniał się jego obraz w czasie. Świadomie nie optymalizujemy pod benchmarki.
>
> **Podstawa:** synteza czterech badań (`docs/review/attachments/memory-quality-SYNTHESIS.md`)
> i weryfikacja w tym repozytorium (odsyłacze `plik:linia` w tekście).
>
> **Status:** zatwierdzony do wdrożenia. Fala 1 w toku.
>
> **Dwie decyzje podjęte przy starcie (z uzasadnieniem, do cofnięcia jeśli się nie zgodzisz):**
> 1. **`recorded_at` jako osobna kolumna**, nie przemianowanie `created_at`. Semantyka `created_at`
>    jest już nośna (kolejność, migracje, dashboard, `FreshnessKey`), a zmiana jest wtedy addytywna
>    i odwracalna: nowa kolumna + backfill `= created_at`. Przemianowanie intencji bez zmiany nazwy
>    byłoby tańsze, ale nie da się go cofnąć i nie widać go w schemacie.
> 2. **Kwarantanna jest opt-in, nie domyślna.** Domyślne zachowanie przy nierozwiązanym odniesieniu:
>    zapisz, **oznacz** (`self_contained: false`) i **pokaż ostrzeżenie w wyniku**, żeby czytelnik
>    wiedział, że wpis wymaga kontekstu. Uzasadnienie wprost z naszych badań: wspomnienie ukryte jest
>    **cichą porażką** — a jednym z najmocniejszych ustaleń całego przeglądu jest to, że cicha porażka
>    jest gorsza od głośnej. Kwarantanna (wpis niewidoczny dla wyszukiwania, widoczny w audycie)
>    zostaje dostępna dla wywołującego, który jej chce, ale nie jest domyślnym losem wspomnienia,
>    o którego zapisanie użytkownik poprosił.

---

## 0. Definicja: co znaczy „wartościowe wspomnienie"

Cztery warunki, **każdy sprawdzalny maszynowo** — to zarazem kryterium bramki zapisu i miara sukcesu:

| # | Warunek | Sprawdzenie |
|---|---|---|
| 1 | **Samodzielne** | brak nierozwiązanych zaimków i deiksy dyskursywnej („as discussed", „the fix") |
| 2 | **Zakotwiczone** | każde odniesienie rozwiązuje się do istniejącego obiektu: `path@commit#symbol`, nazwana encja, data absolutna |
| 3 | **Uzasadnione** | wskazuje przyszłą decyzję, którą zmienia, i zawiera *lekcję*, nie tylko opis |
| 4 | **Osadzone w czasie** | trzy zegary jawnie rozdzielone i widoczne dla czytelnika (§1) |

**Sukces mierzymy wskaźnikiem samodzielności i wskaźnikiem użycia, nie wynikiem benchmarku.**
Uzasadnienie w §7 — i nie jest to wymówka: badanie produkcyjne pokazuje system z 78,8% trafności,
który cytuje w rozmowie 7,9% tych faktów (luka 71 punktów), a benchmarki mierzą trafność w wąskim
zakresie, nie użyteczność.

---

## 1. Trzy zegary — decyzja projektowa

| zegar | znaczenie | pole | stan dzisiaj |
|---|---|---|---|
| **czas zdarzenia** (valid time) | kiedy fakt był prawdziwy w świecie | `valid_from` / `valid_until` | istnieje, opcjonalne, ustawiane przez temporal anchoring (`memory/node.rs:232-235`) |
| **czas zapisu** (record time) | kiedy *my* to zapisaliśmy | **`recorded_at`** (nowe) | **brak jako odrębne pole** — rolę pełni `created_at`, mylone z `updated_at` |
| **czas dostępu** | kiedy ostatnio po to sięgnęliśmy | `last_accessed` | istnieje, **odświeżane przez wyszukiwanie** — nie wolno na nim opierać wniosków o wieku |

Decyzja: `recorded_at` staje się **osobną, niemutowalną** kolumną. `created_at` zostaje jako czas
powstania wiersza (zgodność wstecz), `updated_at` jako czas ostatniej zmiany treści.
**Test niezmienniczości:** po 100 wyszukaniach, dowolnej liczbie wzmocnień i przebiegów decay
`recorded_at` się nie zmienia; dziś nie ma czego takiego testować, bo pole nie istnieje.

---

## 2. Schemat — migracja V17 (następna wolna; zarejestrowanych jest 16)

1. **`knowledge_nodes.recorded_at`** — `TEXT NOT NULL`, backfill `= created_at` dla istniejących wierszy.
2. **`code_refs`** — kotwice kodu: `node_id, repo_remote, commit_sha, path, symbol, hint_line,
   content_hash, resolved_at, verdict` (`fresh` / `stale` / `orphaned` / `unchecked`).
   Dziś jest tylko `CodeEntity.line_number: Option<u32>` (`codebase/types/code_entity.rs:21`) —
   schemat potrafi wyrazić **pozycję**, ale nie **tożsamość ani rewizję**.
3. **`memory_revisions`** — append-only: `node_id, recorded_at, kind` (`create` / `edit` /
   `supersede` / `invalidate` / `quarantine`), `old_content, new_content, reason, actor`.
   **To jest brakujący element Twojego wymagania**: w migracjach są `state_transitions`,
   `consolidation_history`, `dream_history`, `retention_snapshots`, ale **nie ma historii treści** —
   `update_node_content` nadpisuje wspomnienie bez śladu, więc poprzedni obraz przepada.
4. **Supersesja** — rekomendacja: rozszerzyć istniejące `connections` o typ `supersedes` z metadanymi
   (`recorded_at`, `reason`, `evidence`), zamiast tworzyć równoległą tabelę. Wzorzec już istnieje
   w schemacie decyzji: `supersedes` i `validUntil` (`codebase_unified.rs:88-96`), tylko bramki
   z innych narzędzi z niego nie korzystają.
5. **`knowledge_nodes.quarantine_reason`** (+ wykluczenie z wyszukiwania) — dla wpisów odroczonych (§3.3).

Zakres migracji w dokumentacji i w bramce metadanych przechodzi na **v1–v17**.

---

## 3. Zapis: kontrakt, bramka, odroczenie

1. **Kontrakt zapisu** (`AGENTS.md` i opisy schematów narzędzi). Dziś `AGENTS.md:318` **nakazuje**
   wpisywać `Files: [paths]` w treści każdego wspomnienia o naprawie błędu — czyli system produkuje
   klasę „plik + linia" z definicji. Zmiana:
   - `BUG_FIX`: `Root cause` + `Lesson` (reguła na przyszłość), a ścieżki **nie w treści**, tylko w `code_refs`;
   - `DECISION` / `CODE_CHANGE`: `supersedes` + `validUntil` zamiast nadpisywania;
   - zasada higieny „when in doubt, save" → **„zapisz, gdy umiesz wskazać decyzję, którą to zmienia"**.
2. **Bramka samodzielności** w istniejącym kanale ostrzeżeń (`smart_ingest/compound.rs::detect_compound_content`
   → `compound_content_warning`): deiksa dyskursywna, nierozwiązany zaimek (coref zwrócił brak),
   czas względny bez kotwicy, naga ścieżka bez `code_ref`, brak podmiotu.
   Uwaga: `coref.rs` rozwiązuje zaimki **wewnątrz wklejonej treści** — a Twój przypadek to brak
   poprzednika w tekście, czego regex zapisu nie naprawi. Dlatego ostrzeżenie musi być *działaniem*,
   nie etykietą.
3. **Odroczenie zamiast skrawka** (MemReader, jedyna znana praktyka, która *zmniejsza* liczbę złych
   wspomnień): najpierw ograniczone wyszukiwanie brakującego poprzednika w zapisanej pamięci;
   znaleziony → przepisz i zapisz; nieznaleziony → **kwarantanna**: wpis audytowalny, ale wyłączony
   z wyszukiwania. Dziś ścieżka zapisu ma dwa zakończenia — zapisz albo zapisz i ostrzeż — i oba zapisują.
4. **Lista odmów**: treść wyprowadzalna z repozytorium (zawartość plików, architektura, układy
   katalogów, numery wersji, liczby testów) → odrzuć i wskaż `AGENTS.md`. Tak robi Claude Code:
   *„skips anything it can derive from the codebase, such as architecture, file paths, or debugging fixes"*.
   **Drugi, niezależny powód, żeby trzymać ścieżki poza treścią wspomnień:** w tym samym audycie
   10 134 wpisów ścieżki plików **nie wystąpiły jako problem starzenia się**, tylko w kategorii
   „security / privacy leaks" (130 wpisów, 2,1%), obok adresów IP i identyfikatorów czatu — czyli
   magazyn, który automatycznie wyciąga ścieżki, gromadzi **wewnętrzną mapę topologii maszyny**.
   Audytorzy klasyfikują to jako wyciek, nie jako szum. To argument niezależny od tego, że ścieżki gniją.

   **A gniją po cichu — w obie strony.** Najczystszy znany przypadek (claude-code #95095): plik
   pamięci zawierający jedną linię — przypiętą ścieżkę pluginu. Po podbiciu wersji stare kopie
   zostały, więc przypięcie „nadal się rozwiązuje — do starej kopii": *„każda sesja ładowała reguły
   z 0.10.2 i umiejętności z 0.11.0 jednocześnie… nic w żadnym interfejsie nie sygnalizowało, że
   załadowane reguły są o wersję do tyłu"*. A gdy cel zostanie usunięty: *„nierozwiązywalny import
   nie objawia się niczym… cały wspólny zestaw reguł znika bez sygnału"*. **Zapisana ścieżka zawodzi
   cicho w obie strony — rozkłada się w nieprawdę albo w nicość, nigdy w błąd.** Stąd reguła:
   odwołanie do pliku może występować **wyłącznie jako niewstrzykiwana proweniencja**, nigdy wewnątrz
   tekstu wspomnienia, który trafia do kontekstu.
5. **Wyjątek na znaczniki czasu** w przyszłym walidatorze ugruntowania: absolutna data nigdy nie
   występuje dosłownie w transkrypcji, więc bez wyjątku walidator uzna każde poprawnie zakotwiczone
   `valid_until` za halucynację i **dwie funkcje zaczną ze sobą walczyć**.
6. **Ścieżka odrzucenia (`Reject`) — największa dźwignia, jaką mamy.** W tym repozytorium
   `GateDecision` zna wyłącznie `Create`, `Update`, `Supersede` i `Merge`
   (`advanced/prediction_error/decision.rs`), a `smart_ingest` oferuje `forceCreate` — ale **nie ma
   sposobu powiedzieć „tego faktu nie warto przechowywać"**. Audyt 10 134 wpisów w produkcyjnym
   mem0 (issue #4573: dwa przebiegi dedup, potem ręczne przeczytanie 6 264 ocalałych, 224 przetrwały
   — 97,8% śmieci) wskazuje jako dwie główne remedies **bramkę jakości między ekstrakcją a zapisem**
   i właśnie **akcję REJECT w prompcie decyzyjnym**. Ich własny wniosek: dedup nie był zawodzącą
   warstwą (po dwóch przebiegach wciąż 96,9% śmieci przy ręcznym czytaniu).

   **Zewnętrzne potwierdzenie, że nasza reguła atomowości jest wyborem, nie oczywistością.**
   Prompt ekstrakcji mem0 (`ADDITIVE_EXTRACTION_PROMPT`) jest **świadomie napisany pod maksymalny
   recall** i głosi coś przeciwnego: *„Contextually Rich, Not Atomic"*, *„15–80 words"*, z przykładami
   *„Extract ALL Dimensions — Don't Miss Secondary Info"*. Maintainer mem0 w notce triage'owej do
   issue #5730 (czerwiec 2026) potwierdza zarówno sam problem, jak i jego mechanizm: *„The
   precision/noise concern is legitimate and consistent with #4573. A stronger model following the
   high-recall prompt more faithfully is a real observed pattern."* Czyli „jeden fakt na wspomnienie"
   w Vestige **nie jest konwencją, której nikt nie egzekwuje — jest świadomym odejściem od
   najczęściej kopiowanego promptu w ekosystemie**, a 97,8% śmieci to dowód, co produkuje domyślne
   ustawienie. mem0 dodatkowo **usunął** furtkę nadpisania promptu (PR #4805, zamieniona na
   append-only `custom_instructions`), więc użytkownicy stracili dźwignię do zmniejszenia recallu.
   Warto też odnotować, gdzie jesteśmy z przodu: pipeline mem0 jest **ADD-only** i nie zastępuje
   nieaktualnych faktów (issue #4956/#5867) — czyli nie ma inwalidacji czasowej ani wykrywania
   sprzeczności, które Vestige już ma.

---

## 4. Odczyt: pokazać czas i proweniencję

1. **Renderowanie czasu do treści.** Dziś daty są w JSON-ie wyniku
   (`search_unified/format.rs:29-38`: `createdAt`, `updatedAt`, `lastAccessed`, `validFrom`, `validUntil`),
   ale nie w tekście, który czyta człowiek. Każdy wynik dostaje zwięzły wiersz proweniencji:
   `zapisano 2026-09-19 · ważne od 2026-09-01 · zastąpione 2026-09-25 przez <id> · kotwica: fresh`.
2. **`as_of` w wyszukiwaniu** — „co uważaliśmy 1 września". Możliwe dopiero, gdy record time jest
   niemutowalny. To bezpośrednia odpowiedź na „jak zmieniał się obraz w czasie" po stronie zapytania.
3. **`expandable` wyzwalane porażką**, nie tylko budżetem: gdy odniesienia nie da się rozwiązać,
   wynik sam prosi o rozwinięcie, zamiast oddawać skrawek.
4. **Dashboard**: te same pola w widoku wspomnienia (DTO już je niesie — `wire/` bez zmian kontraktu).

---

## 5. Obraz w czasie: supersesja i historia

1. Każda operacja (`create` / `edit` / `supersede` / `invalidate` / `quarantine`) zapisuje wiersz
   w `memory_revisions` — append-only, z czasem zapisu i powodem.
2. `temporal history` i `memory_changelog` renderują **linię czasu obrazu** (co uważaliśmy kiedy),
   a nie tylko stan bieżący. Dziś changelog czyta przejścia stanu (`states.rs:315`), więc pokazuje
   cykl życia, ale nie zmianę treści ani zastąpienie.
3. **Edycja zachowuje poprzednią wersję.** Dziś `update_node_content` nadpisuje bez śladu.
4. **RODO:** erasure musi kasować także rewizje i kotwice — inaczej „trwałe usunięcie" byłoby nieprawdą.
   To trzeba zapisać wprost w projekcie i przetestować.

---

## 6. Konserwacja

1. **Audyt rot kotwic** (raport-only, nigdy automatyczna naprawa): rozwiąż `code_refs` względem
   zapisanego SHA; porażka → `stale`/`orphaned`, demote i kolejka do przeglądu. Podstawa dowodowa:
   nieaktualne odniesienia do elementów kodu w **23,0% z 356 repozytoriów**.
   **Kluczowanie musi być na weryfikowalnej nieaktualności, nigdy na wieku ani liczbie odczytów.**
   To nie preferencja, to pomiar: automatyczny przebieg wycofywania nigdy nieużywanych wspomnień
   zwrócił **zero kandydatów** przy progu 30 dni (claude-code #92998), a „liczby odczytów to zły
   test" potwierdza niezależne źródło. Na zdrowym magazynie przebieg kluczowany po wieku **nie ma
   na czym pracować** — dokładnie odwrotnie, niż podpowiada intuicja.
2. **Ewikcja musi być świadoma supersesji — inaczej tworzy „pewnych siebie i nieprawdziwych".**
   Zapisane wprost: *„wspomnienie, które poprawia wcześniejsze, jest zawsze nowsze niż to, co
   poprawia — więc obcinanie pojemności wyrzuca najpierw supersesje, zostawiając załadowaną
   i autorytatywną wersję nieaktualną"*. Każdy mechanizm limitowania pojemności lub retencji
   w Vestige, który nie respektuje łańcucha zastąpień, odtworzy ten błąd.
3. **Ktoś musi być właścicielem uzgadniania — w obie strony.** Użytkownik, który **zbudował**
   system plikowy (a więc nie jest stromy w sporze „pliki kontra baza"), zgłasza te same klasy awarii:
   przepełnienie indeksu ukrywające własny ogon, pliki sieroce niewidoczne dla nowych sesji,
   synchronizacja nadpisująca nowszą pamięć z innej maszyny, nieaktualne wpisy wskazujące na usunięte
   pliki, duplikaty z równoległych sesji. **Zmiana nośnika przenosi te awarie, nie usuwa ich** —
   różnicą jest to, czy cokolwiek odpowiada za uzgadnianie. Dwa mechanizmy, które u niego zadziałały:
   blokada zakończenia sesji przy istniejących sierotach („złapała trzy prawdziwe pierwszego dnia")
   oraz bramka rozmiaru przy zapisie. Wniosek dla nas: bramki przy zapisie **uzupełniają** przebieg
   porządkowy, nie zastępują go, a magazyn i indeks wymagają uzgadniania w obu kierunkach.
4. **Pomiar `dream`/`reflect` przed/po wskaźniku samodzielności** — nie zakładamy, że pomagają;
   dane pokazują, że destylacja może zejść poniżej poziomu „brak pamięci".
5. **Nie** dodajemy wygaszania po wieku ani „większego streszczacza".

---

## 7. Czego świadomie nie robimy

- **Nie gonimy benchmarków.** Twoja intuicja ma potwierdzenie w danych: jeden kontrolowane badanie
  czynnikowe pokazuje, że metoda wyszukiwania rusza wynik LoCoMo o ~20 punktów, a strategia zapisu
  o 3–8, a surowe fragmenty rozmów biją ekstrakcję faktów. **Ale** ten spór nie jest rozstrzygnięty
  (trzy raporty producentów twierdzą odwrotnie, a system z własnym harnessem odmawia twierdzenia
  o ablacji), więc nie będziemy też ogłaszać, że „zapis jest ważniejszy". Uzasadnieniem tych zmian
  jest **zrozumiałość i rozwiązywalność odniesień**, a nie wzrost wyniku.
- **Nie dodajemy zależności modelowych** — cała walidacja jest deterministyczna. To nie jest
  oszczędność: **lepszy model nie jest dźwignią**. W audycie 10k wpisów przejście z `gemma2:2b` na
  Claude Sonnet 4.6 zmieniło odsetek śmieci z 95,4–98,5% tylko na 89,6%, a wniosek autora brzmiał:
  *„lepszy model wierniej wykonuje prompt ekstrakcji, więc ekstrahuje bardziej bezkrytycznie; wąskim
  gardłem jest prompt, nie model"*. Niezależnie potwierdzone w drugim produkcyjnym raporcie, gdzie
  zmiana modelu na większy „nie zrobiła zauważalnej różnicy", a podwoiła opóźnienie.
- **Nie polegamy na dedupie jako warstwie jakości.** W tym samym audycie dwa przebiegi dedup
  (hash + cosine) zostawiły 6 264 wpisy, z których po ręcznym czytaniu przetrwały 224.
  Dedup usuwa duplikaty; nie usuwa wpisów, które od początku nie powinny powstać.
- **Nie zmieniamy kontraktu drutu** bez jednoczesnej aktualizacji bramki metadanych, dokumentacji
  i generowanych typów.

---

## 8. Kryteria odbioru

| Kryterium | Jak zmierzyć |
|---|---|
| Wskaźnik samodzielności | odsetek nowych wpisów bez ostrzeżenia bramki — **zmierzyć przed wdrożeniem**, żeby mieć punkt odniesienia |
| Rozwiązywalność kotwic | odsetek `code_refs` w stanie `fresh` po 30 dniach |
| Niemutowalność czasu zapisu | test: 100 wyszukiwań + strengthen + decay → `recorded_at` bez zmian |
| Odtwarzalność obrazu | dla losowego wspomnienia: pełna linia czasu z `memory_revisions` (treść przed i po) |
| Erasure | po `erase` nie zostaje ani treść, ani rewizje, ani kotwice |
| Bramki | `fmt`, `clippy --workspace --all-targets -D warnings`, testy lib, e2e, dashboard, metadane — zielone po każdej fali |

---

## 9. Kolejność wdrożenia i ryzyka

| Fala | Zakres | Dlaczego w tej kolejności |
|---|---|---|
| **1** | `recorded_at` + `memory_revisions` (V17), renderowanie proweniencji, nowy kontrakt zapisu | najtańsze, największy zysk: bez tego nie ma czego renderować |
| **2** | bramka samodzielności + kwarantanna + lista odmów | odcina klasę „skrawków" u źródła |
| **3** | `code_refs`, werdykty `fresh/stale/orphaned`, audyt rot | usuwa „plik + linię" na rzecz symbolu i rewizji |
| **4** | `as_of`, linia czasu w `temporal`/`changelog`, `expandable` wyzwalane porażką | domyka „jak zmieniał się obraz" |
| **5** | pomiar wskaźników + A/B na `dream`/`reflect` | żeby nie wdrożyć na wiarę |

**Ryzyka:** migracja i backfill na istniejącej bazie (odwracalna: nowe kolumny są addytywne);
koszt trzech nowych tabel; kwarantanna a UX (wpis „znika" — musi być widoczny w audycie);
erasure kontra historia (rozstrzygnięte w §5.4); pokusa, by „przy okazji" dodać wygaszanie po wieku (nie).

---

## 10. Czego ten projekt nie obiecuje

Nie obiecuje lepszych wyników w benchmarkach ani tego, że `dream`/`reflect` poprawią pamięć — to
trzeba zmierzyć. Obiecuje coś węższego i sprawdzalnego: **wpis, który za pół roku da się przeczytać
bez naszej rozmowy, z jawnym czasem zapisu i widoczną historią zmian.**

---

## 11. Załącznik: reguły bramki zapisu (fala 2) — specyfikacja wykonawcza

Bramka rozszerza istniejący kanał ostrzeżeń, którym jest dziś `compound_content_warning`
(`smart_ingest/compound.rs::detect_compound_content` → `smart_ingest/execute.rs:249/328/384`).
Nie tworzy nowego mechanizmu: **dokłada drugi, ustrukturyzowany komunikat** o innym kształcie.

### 11.1 Kształt danych

Dziś ostrzeżenie to `String`. Dodajemy obok niego obiekt:

```json
{
  "self_contained": {
    "ok": false,
    "findings": [
      { "kind": "discourse_deixis", "span": "the fix", "hint": "nazwij, co zostało naprawione" },
      { "kind": "bare_code_reference", "span": "search.rs:112", "hint": "użyj code_ref (path@commit#symbol)" }
    ],
    "requires_context": true
  }
}
```

Zgodnie z decyzją z nagłówka: wpis **zostaje zapisany**, otrzymuje znacznik `self_contained: false`
i ostrzeżenie trafia do odpowiedzi oraz do renderowanego wyniku. Kwarantanna pozostaje opcją
wywołującego (`strict: true`), nie domyślnym losem.

### 11.2 Reguły (każda zwraca `kind`, `span` i podpowiedź naprawy)

| `kind` | Co wykrywa | Warunek zapalenia | Uwaga |
|---|---|---|---|
| `discourse_deixis` | „as discussed", „as mentioned", „the above", „the fix", „the bug", „last time", „earlier", „we decided", „the file", „the function", „the test" | fraza występuje **i** w tym samym wpisie nie ma nazwanej encji/podmiotu, który mógłby być jej poprzednikiem | To nie zaimki — `coref.rs` ich nie widzi, bo to frazy rzeczownikowe |
| `unresolved_pronoun` | zaimek, którego `coref.rs` **nie** rozwiązał | `CorefResult` musi zacząć raportować nierozwiązane (`unresolved: Vec<String>`) — dziś milczy o porażce | Cisza przy porażce jest tu tym samym błędem, co wszędzie |
| `relative_time` | „today", „yesterday", „recently", „soon", „last week", „next Friday" | **brak** absolutnego znacznika czasu w treści i brak `valid_from`/`valid_until` | Letta zakazuje tego wprost w swoim promptcie |
| `bare_code_reference` | `\S+\.(rs\|ts\|tsx\|py\|md\|json\|toml\|yml\|yaml)`, `path:linia`, `src/...` | brak odpowiadającego `code_ref` (fala 3) | Dodatkowy powód: ścieżki to też klasa wycieku (mapa topologii maszyny) |
| `no_subject` | brak jakiejkolwiek nazwanej encji **i** brak pierwszej osoby | treść nie mówi, o czym jest | Najtańszy wskaźnik „to jest fragment czyjegoś myślenia" |
| `derivable_from_repo` | zawartość plików, opis architektury, układ katalogów, numer wersji, liczba testów | heurystyka frazowa + wykrycie bloku kodu | Odrzucenie, nie ostrzeżenie: repozytorium jest źródłem prawdy |

### 11.3 Ścieżka odrzucenia (`Reject`) — brakująca zdolność

`GateDecision` (`advanced/prediction_error/decision.rs`) zna `Create`, `Update`, `Supersede`, `Merge`.
Dodajemy wariant `Reject { reason, findings }`, używany **wyłącznie** dla `derivable_from_repo` i dla
treści pustej/bez treści. `smart_ingest` zwraca wtedy `decision: "reject"` **i nic nie zapisuje**,
a komunikat mówi wprost, dlaczego i co zrobić zamiast tego (wskaż `AGENTS.md` / repozytorium).

Zakres zamierzony jako wąski: odrzucenie jest dla rzeczy, których **nie należy** przechowywać.
Wszystko, co da się naprawić (deiksa, zaimek, czas względny, naga ścieżka), dostaje ostrzeżenie
i zostaje zapisane — inaczej bramka zamieniłaby jeden problem (śmieci) na gorszy (cicha utrata).

### 11.4 Przypadki testowe, które muszą failować przed wdrożeniem

1. „BUG FIX: naprawiłem to, co omawialiśmy; Files: src/search.rs:112" → `discourse_deixis` +
   `bare_code_reference`; wpis **zapisany** ze znacznikiem, ostrzeżenie w odpowiedzi.
2. „He said the migration failed" bez poprzednika → `unresolved_pronoun` (dziś `coref.rs` milczy).
3. „Deploy dopiero w przyszłym tygodniu" bez kotwicy → `relative_time`; ta sama treść z
   `valid_from` → brak ostrzeżenia.
4. „Zawartość `Cargo.toml` to \[…\]" → `derivable_from_repo` → **`Reject`**, nic nie zapisane.
5. „Postgres 16" → **ostrzeżenie `probable_version_claim`, nie odrzucenie** (wersja starzeje się
   z każdym commitem, ale wzorzec „nazwa + liczba" nie odróżnia jej od „Sali 101"/„punktu 3", więc
   odrzucanie kosztowałoby utratę prawdziwego wspomnienia przez fałszywy alarm). Jawnie zapisane
   wersje — `v1.2.3`, `1.2.3`, `version 1.2` — są nadal odrzucane, a „version 2024-11-05" nie,
   bo to etykieta datowa, a nie twierdzenie o produkcie.
6. „Nie symuluj bazy w testach integracyjnych, bo testy przechodziły, a migracja padła" → **brak**
   ostrzeżeń (to wzorcowe wspomnienie: lekcja, nie opis).
7. Wpis z `code_ref` (fala 3) i nazwanym podmiotem → brak ostrzeżeń.

---

## 12. Załącznik: jak mierzymy skutek (fala 5) — specyfikacja

§0 mówi, że sukces mierzymy „wskaźnikiem samodzielności i wskaźnikiem użycia". To za mało, żeby
cokolwiek rozstrzygnąć, więc poniżej definicja wykonawcza. Zasada nadrzędna: **mierzymy przed
wdrożeniem i po nim, na tym samym magazynie**, bo bez punktu odniesienia każda liczba jest anegdotą.

### 12.1 Cztery liczby i ich pułapki

| miara | definicja | pułapka, którą trzeba obejść |
|---|---|---|
| **samodzielność** | udział wspomnień z `self_contained = 1` w oknie, wobec `= 0` (oflagowane) | `NULL` znaczy „bramka nie chodziła" i **nie wolno** go liczyć jako czystego; trzy kategorie raportujemy osobno |
| **rozwiązywalność kotwic** | rozkład werdyktów `code_refs`: `fresh` / `stale` / `orphaned` / `unchecked` | `unchecked` to brak repozytorium albo brak commita — awaria **środowiska**, nie kodu; mieszanie go z `orphaned` zafałszuje obraz w obie strony |
| **użycie** | udział wspomnień pobranych co najmniej raz po zapisie (z logu dostępów) | liczby odczytów są **złym** sygnałem do retencji (badania) i dobrym do wartości; nie używamy ich do wygaszania, tylko do oceny |
| **odrzucenia i flagi** | udział zapisów odrzuconych i oflagowanych, w rozbiciu na `kind` reguły | wysoki odsetek odrzuceń nie jest sukcesem sam w sobie: jeśli dominuje `no_subject` na dobrych wpisach, bramka jest za głośna i trzeba ją zawęzić |

### 12.2 A/B na `dream`/`reflect` — jedyny uczciwy sposób

Badania pokazują, że destylacja potrafi zejść **poniżej** poziomu „brak pamięci" (0 z 121 refleksji
trafiło w cel, a agenci na surowych epizodach mieli dwukrotnie wyższą dokładność niż ci pod przymusem
konsolidacji). Dlatego:

1. **Dwie próby na tym samym korpusie**: z przebiegiem i bez, ten sam wsad, ten sam seed.
2. **Mierzymy samodzielność i użycie**, a nie wynik benchmarku — benchmark nie jest celem tego projektu (§7).
3. **Wynik negatywny jest wynikiem.** Jeśli przebieg nie poprawia żadnej miary, zapisujemy to
   i **nie** włączamy go szerzej; „nic nie zaszkodziło" to nie to samo co „pomogło".
4. **Żadnych wniosków z jednego przebiegu**: ten sam kierunek w trzech kolejnych albo raport mówi
   „nierozstrzygnięte".

### 12.3 Czego nie mierzymy i dlaczego

- **Nie mierzymy LoCoMo/LongMemEval jako kryterium akceptacji.** Jeden kontrolowane badanie pokazuje,
  że metoda wyszukiwania rusza wynik o ~20 punktów, a strategia zapisu o 3–8, a surowy dialog bije
  ekstrakcję faktów — czyli te benchmarki mierzą coś innego niż to, co budujemy. Zostają jako straż
  regresji, nie jako cel.
- **Nie mierzymy „czy wspomnienie było prawdziwe"** — nie mamy źródła prawdy, a udawanie, że mamy,
  byłoby najgorszym rodzajem metryki.
- **Nie ogłaszamy wzrostu bez kontroli.** Każdy raport z wynikiem musi nieść informację, **jaka zmiana**
  go wywołała i czym to potwierdzono — tak jak binarka kontrolna w pomiarze FactConsolidation, gdzie
  identyczny wynik na wersji sprzed zmiany obalił pokusę przypisania sobie poprawy.

# Jakość wspomnień — synteza czterech badań

> **Pytanie, na które odpowiada ten dokument:** dlaczego zapisane wspomnienia są bezużytecznymi
> skrawkami — odnoszą się do kodu przez plik i linię albo zakładają, że czytający pamięta rozmowę
> z agentem — i jak ten problem rozwiązano gdzie indziej.
>
> **Metoda:** cztery niezależne badania internetowe pod różnymi kątami, każde z linkami do źródeł
> i jawnym oznaczeniem, co jest relacją pierwszoosobową, co wypowiedzią maintainera, co dokumentacją
> producenta, a co pracą naukową. Pełne materiały:
> [`memory-quality-write-time-extraction.md`](memory-quality-write-time-extraction.md),
> [`memory-quality-code-anchoring.md`](memory-quality-code-anchoring.md),
> [`memory-quality-context-repair-and-eval.md`](memory-quality-context-repair-and-eval.md),
> [`memory-quality-anti-patterns.md`](memory-quality-anti-patterns.md).
>
> **Data:** 19 września 2026.

---

## 1. To nie jeden problem, ale trzy różne

Zgłaszany objaw „bezużyteczne skrawki" rozpada się na trzy odrębne wady, które mają różne przyczyny
i różne lekarstwa:

| # | Wada | Objaw | Jak częsta w źródłach |
|---|---|---|---|
| **A** | Wspomnienie niezrozumiałe bez rozmowy | „the fix", „as discussed", „the retry logic", „today" | 3 niezależne źródła; najbliżej Twojego opisu |
| **B** | Odniesienie do kodu przez pozycję | „plik X, linia 123" — linia gnije przy pierwszym wstawieniu wyżej | udokumentowane projektowo u producentów; **brak pierwszoosobowej relacji o awarii** |
| **C** | Wpis bezwartościowy już przy zapisie | transientny stan, godzina, „jestem w podróży w tym tygodniu" | **6 niezależnych źródeł — najczęstsza awaria w praktyce** |

Wniosek porządkujący: **C jest większym problemem niż A i B razem**, a w naszym przypadku jest
dodatkowo generowany polityką. Dopiero potem sensowne jest naprawianie A i B.

---

## 2. Niewygodne ustalenie: część tego produkuje sam Vestige

To nie jest wina modelu. Kanoniczne instrukcje repozytorium **nakazują** oba antywzorce:

- **`AGENTS.md:318`** — bramka `BUG_FIX` wymaga treści
  `"BUG FIX: [error]\nRoot cause: [why]\nSolution: [fix]\nFiles: [paths]"`. Bramki `DECISION`
  i `CODE_CHANGE` przekazują `files: [paths]`, a pipeline `smart_ingest` dodatkowo taguje ścieżki
  jako `entity:*`. System nie tylko dopuszcza wspomnienia „plik + linia" — **produkuje je z definicji**.
- **Sekcja higieny pamięci** — „*When in doubt, save. Prediction Error Gating handles dedup. Lost
  knowledge is permanent*" — jest dokładnie odwrotna do tego, na czym konwerguje cała praktyka
  („jeśli nie umiesz wskazać przyszłej decyzji, którą to zmienia, nie zapisuj"). Dedup chroni przed
  duplikatami; nie robi nic z wpisami od początku bezwartościowymi.
- **Schemat nie potrafi wyrazić tożsamości kodu.** `CodeEntity.line_number`
  (`crates/vestige-core/src/codebase/types/code_entity.rs:21`) przechowuje pozycję, a przy odczycie
  `codebase_unified.rs:194/294` spłaszcza listę plików do markdownu (`## Files:`), podczas gdy
  `codebase` staje się tagiem. Nie ma gdzie zapisać rewizji ani symbolu — czyli dokładnie tego, co
  przeżywa refaktor.
- **Jest jednak dobry zaczyn:** `remember_decision_v2` już ma `supersedes` i `validUntil`
  (`codebase_unified.rs:88-96`), a `smart_ingest` już zwraca ostrzeżenie `compound_content_warning`,
  na które instrukcja każe reagować. Wzorzec ADR „zastąp, nie kasuj" jest w schemacie — bramka
  `BUG_FIX` po prostu z niego nie korzysta.

---

## 3. Na czym konwerguje branża

**Reguła samodzielności jest zapisana wprost w promptach, które trafiają do produkcji:**

- **Mem0** — *„Self-Contained / Every memory must be understandable on its own. Replace all pronouns
  with specific names or 'User.'"* oraz: *„User went to Paris last week" is useless 6 months later.
  „User went to Paris the week of May 15, 2023" is meaningful forever.*
  ([prompts.py](https://raw.githubusercontent.com/mem0ai/mem0/main/mem0/configs/prompts.py))
- **Graphiti** — najmocniejsza bramka zapisu, jaką znaleziono: *„If a phrase would not be
  distinguishable when read alone later, do NOT extract it"*, plus wymóg kwalifikowania posiadacza
  („Nisha's dad", nie „dad").
  ([extract_nodes.py](https://raw.githubusercontent.com/getzep/graphiti/main/graphiti_core/prompts/extract_nodes.py))
- **Letta** — wprost o czasie względnym: *„do not write 'today' or 'recently'… because 'today' and
  'recently' are relative, and the memory is persisted indefinitely"*.
  ([sleeptime.txt](https://raw.githubusercontent.com/letta-ai/letta/0.7.0/letta/prompts/system/sleeptime.txt))

**Czego świadomie nie zapisywać:**

- **Claude Code** — *„Claude skips anything it can derive from the codebase, such as architecture,
  file paths, or debugging fixes"*; wyklucza też „information that changes frequently".
  ([memory](https://code.claude.com/docs/en/memory))
- **Cursor** — *„Reference files instead of copying their contents — this keeps rules short and
  prevents them from becoming stale as code changes"*. ([rules](https://cursor.com/docs/rules))
- **Devin/Cascade** — producent odradza zaufanie własnej automatycznej pamięci: *„write it as a Rule
  or add it to AGENTS.md in your repo rather than relying on auto-generated Memories"*.
  ([docs](https://docs.devin.ai/desktop/cascade/memories))

**Kotwiczenie w kodzie — jedyne działające rozwiązanie tej klasy:**

- **GitHub Copilot** weryfikuje cytaty do kodu **w czasie odczytu**, na bieżącej gałęzi, i stosuje
  zasadę *„Information retrieval is an asymmetrical problem: It's hard to solve, but easy to verify"*.
  Świadomie **odrzucił** offline'owe porządkowanie (dedup, konflikty, wygaszanie) jako zbyt kosztowne.
  ([blog](https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/))
- **legendary-mcp** kotwiczy jako `{file, symbol?, lines?, commit, content_hash}`, rozwiązując
  **symbol przed hashowaniem**, dzięki czemu „zmiany wyłącznie białych znaków nie unieważniają
  wspomnienia, a symbol, który się przesunął, pozostaje świeży"; do wyniku trafia werdykt
  `fresh` / `stale` / `orphaned`. ([concepts](https://ashhadahsan.github.io/legendary/concepts/))
- **Codex** w swoim `AGENTS.md` kotwiczy regułę do commita i PR-a, nie do ścieżki:
  „…as in `3c7f013f9735` / `#16630`". ([AGENTS.md](https://github.com/openai/codex/blob/main/AGENTS.md))
- Standardy trwałości odrzucają poziom linii: **FORCE11** specyfikuje do numeru rewizji i **odmówił**
  schodzenia niżej, a **Zenodo** wymaga DOI dla konkretnej wersji.
  ([FORCE11](https://doi.org/10.7717/peerj-cs.86), [Zenodo](https://zenodo.org/help/versioning))

---

## 4. Niewygodne dane, które przestawiają priorytety

Cztery pomiary mówią, czego **nie** robić i czego nie obiecywać:

1. **Zapis rusza wyniki znacznie słabiej niż wyszukiwanie.** Metoda wyszukiwania zmienia dokładność
   LoCoMo o 20 punktów, strategia zapisu o 3–8, a surowe trzyrundowe fragmenty rozmów (zero wywołań
   LLM) biją ekstrakcję faktów w stylu Mem0. ([arXiv 2603.02473](https://ar5iv.labs.arxiv.org/html/2603.02473))
   → Pracę przy zapisie trzeba uzasadniać **zrozumiałością i rozwiązywalnością odniesień**, nie
   wzrostem benchmarku.
2. **Destylacja potrafi zaszkodzić.** Użyteczność pamięci „najpierw rośnie, potem spada i może zejść
   poniżej poziomu braku pamięci", a agenci na surowych epizodach osiągają dwukrotnie wyższą
   dokładność niż ci pod przymusem konsolidacji; w innym badaniu **0 z 121 refleksji** trafia w
   właściwy obiekt. ([arXiv 2605.12978](https://arxiv.org/abs/2605.12978),
   [arXiv 2605.29463](https://arxiv.org/abs/2605.29463))
   → `dream` i `reflect` **nie mogą być zakładane jako pomocne** — trzeba je zmierzyć przed/po.
3. **Trafność to nie użyteczność.** Ten sam system, który ma 78,8% trafności w pytaniach wprost,
   cytuje w rozmowie tylko 7,9% tych faktów — luka 71 punktów. ([arXiv 2608.24189](https://arxiv.org/abs/2608.24189))
4. **Rot adresów jest policzalny:** nieaktualne odniesienia do elementów kodu w **23,0% z 356
   repozytoriów**. ([arXiv 2606.09090](https://arxiv.org/abs/2606.09090))

**Najważniejszy wynik negatywny: metryka samodzielności wspomnienia nie istnieje.** Żaden benchmark
pamięci agentów (LoCoMo, LongMemEval v1/v2, MemoryAgentBench, MemConflict, AgentMemBench, PERMA,
DeMem) nie ocenia wpisu pod kątem zrozumiałości bez rozmowy źródłowej. Konstrukt jest sformalizowany
o jedno pole dalej — **decontextualization** (Choi i in., TACL 2021: interpretowalne w pustym
kontekście przy zachowaniu znaczenia) — ale nigdy nie przeniesiono go do pamięci.
([TACL](https://aclanthology.org/2021.tacl-1.27/)) To znaczy, że jeśli zbudujemy taką kontrolę,
będziemy mieli coś, czego nie ma żaden z badanych systemów — i że nie da się jej „pożyczyć" z literatury.

---

## 5. Plan dla Vestige, w kolejności wartości do kosztu

Wszystkie punkty to **bramki na istniejącej maszynerii**, nie nowe podsystemy.

1. **Rozwiązywalność odniesień (najpierw).** Przy zapisie zapisuj `path@commit#symbol`, a numer linii
   traktuj jako podpowiedź; przy odczycie i w istniejącym cyklu `dream`/konsolidacji rozwiązuj
   odniesienie względem zapisanego SHA i **wstawiaj werdykt do tekstu wyniku** (`fresh` / `stale` /
   `orphaned`); przy porażce degraduj i kolejkuj do przeglądu, nigdy nie oddawaj jako fakt.
   Deterministyczne, bez modelu, najmocniejsza baza dowodowa (23,0%). Wymaga pola `code_ref` —
   dziś schemat ma tylko `line_number`.
2. **Zmiana kontraktu zapisu.** `BUG_FIX` zamiast `Files: [paths]` powinien wymagać
   `Root cause` + `Lesson` (reguła na przyszłość), a ścieżki przenosić do `code_ref`. `DECISION`
   i `CODE_CHANGE` mają użyć `supersedes`/`validUntil`, które **już są** w schemacie. To najtańsza
   zmiana o największym efekcie: odcina klasę C u źródła.
3. **Bramka samodzielności przy zapisie.** Rozszerzyć `compound.rs` (ta sama ścieżka ostrzeżenia,
   którą model już dostaje) o wykrywanie **deiksy nie związanej z żadną nazwaną encją** oraz fraz
   odnoszących się do rozmowy („as discussed", „the above", „the fix", „last time"). Uwaga: obecne
   `coref.rs` rozwiązuje **zaimki wewnątrz wklejonej treści** — a zgłaszany przypadek to brak
   poprzednika w tekście, czego żaden regex zapisu nie naprawi. Do tego lista odmów dla treści
   wyprowadzalnych z repo (zawartość plików, architektura, opis katalogów), wskazująca na `AGENTS.md`.
4. **Rozszerzenie „expandable" z budżetowego na wyzwalane porażką** — gdy odniesienie nie da się
   rozwiązać, wynik powinien sam poprosić o rozwinięcie, zamiast oddawać niezrozumiały skrawek.
5. **Pomiar `dream`/`reflect` przed/po wskaźniku samodzielności**, zanim uznamy je za pomocne.
   Świadomie **nie** dodawać wygaszania po wieku ani „większego streszczacza" — dane mówią, że to
   właśnie ten rodzaj konsolidacji szkodzi.

---

## 6. Czego nie udało się ustalić (jawne luki)

- **Brak pierwszoosobowej publicznej relacji** o wspomnieniu „plik:linia", które zawiodło po
  refaktorze. Mechanizm jest udokumentowany na poziomie projektowym u producentów; anegdoty nie ma.
  Nie należy więc pisać, że to „powszechnie zgłaszany" problem.
- **„Line-number rot" nie jest terminem przyjętym w literaturze** — citable są *link rot*
  i *reference rot*.
- **Nikt nie mierzy samodzielności wspomnienia** (§4) — nie ma gotowej metryki do zaadaptowania.
- **Brak dowodu, że automatyczne dociąganie zerwanego odniesienia przy odczycie cokolwiek naprawia.**
  Działająca naprawa przy odczycie to *przeformułuj i zapytaj ponownie* (IRCoT +21 punktów
  wyszukiwania), nigdy „podążaj za wskaźnikiem".
- **MemoryOS nie publikuje swojego promptu ekstrakcji** — twierdzenia o nim są niezweryfikowane.

---

## 7. Jak to czytać

Sekcja 2 mówi, że część problemu jest w naszym własnym kontrakcie zapisu, więc **pierwsze dwie
zmiany z §5 są tańsze i skuteczniejsze niż cokolwiek po stronie wyszukiwania**. Sekcja 4 mówi, że
nie należy obiecywać poprawy benchmarków, a `dream`/`reflect` trzeba najpierw zmierzyć. Sekcja 6
mówi, czego nie wiemy — i to też jest wynik.

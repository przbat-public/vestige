//! Entity extraction via heuristic/regex analysis.
//!
//! Extracts proper nouns, URLs, emails, file paths, and other named entities
//! from text content. Returns normalized entity tags prefixed with `entity:`.
//!
//! # Two questions, two entry points
//!
//! This module answers two different questions, and they have different
//! answers. They are separate functions on purpose; merging them back into one
//! is how the store got three junk tags per Polish memory.
//!
//! * **"Which names does this memory deserve a tag for?"** —
//!   [`extract_entities`]. Strict: a capital that sentence position already
//!   explains earns nothing, so `Objaw:`, `Przyczyna:` and a word that opens
//!   exactly one sentence produce no `entity:*` tag.
//! * **"Does this text name its subject, and who is the actor here?"** —
//!   [`extract_mentioned_names`]. Permissive: a memory that opens with its
//!   subject ("Marek said the migration failed", "Alice manages the Auth
//!   Team") does name something even though that capital is positional, and
//!   coreference and relation extraction need the name's text, not a boolean.
//!
//! Tags must come from [`extract_entities`]. NLP consumers that ask the second
//! question must read [`extract_mentioned_names`], or they will treat a memory
//! that opens with its subject as naming nothing.
//!
//! No model downloads, no ONNX — pure Rust, sub-millisecond latency.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// An extracted entity with its type and surface form.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtractedEntity {
    /// The raw text of the entity as it appears in content
    pub text: String,
    /// Category: person, organization, url, email, path, date, monetary, misc
    pub entity_type: EntityType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Person,
    Organization,
    Url,
    Email,
    FilePath,
    Monetary,
    ProperNoun,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "org",
            Self::Url => "url",
            Self::Email => "email",
            Self::FilePath => "path",
            Self::Monetary => "monetary",
            Self::ProperNoun => "noun",
        }
    }
}

// Each `expect()` below documents WHY the regex can't fail at runtime — the
// pattern is a compile-time string literal vetted by tests. If any of these
// ever panics, the developer who edited the literal needs to know which one
// they broke; an explicit message beats `unwrap()`'s "called Option::unwrap on
// a None value" trace.
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s)<>\]]+").expect("entities.rs URL regex literal"));

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
        .expect("entities.rs EMAIL regex literal")
});

static FILE_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s(])(/[a-zA-Z0-9_.\-]+(?:/[a-zA-Z0-9_.\-]+)+|[a-zA-Z]:\\[^\s]+|[a-zA-Z0-9_\-]+(?:/[a-zA-Z0-9_.\-]+){2,})")
        .expect("entities.rs FILE_PATH regex literal")
});

static MONETARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\$€£¥]\s?\d[\d,]*(?:\.\d{1,2})?|\d[\d,]*(?:\.\d{1,2})?\s?(?:USD|EUR|GBP|PLN|JPY)")
        .expect("entities.rs MONETARY regex literal")
});

// A capitalized word *anywhere* in the text: one uppercase letter, then
// letters, digits, or internal hyphens/apostrophes — `Vestige`, `MongoDB`,
// `STM32`, `FSRS-6`, `Każdy`. Deliberately not `[A-Z][a-z]+`: the old ASCII
// class stopped at the first non-ASCII letter, which is how the Polish
// sentence opener "Każdy" was emitted as the truncated tag `entity:ka`, and it
// never saw an acronym (`NES`) or an identifier (`STM32`) at all.
//
// The context anchors the old pattern used (`^`, `[.!?]\s+`, `,\s+`, a list of
// English function words) are gone on purpose: they *created* the defect by
// treating every sentence start as a name, and their word list only worked for
// English. Position is judged later, per occurrence, by `opens_a_slot` and
// `label_colon_follows` — see `extract_proper_nouns`.
static PROPER_NOUN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\p{Lu}[\p{L}\p{N}]*(?:[-'’][\p{L}\p{N}]+)*")
        .expect("entities.rs PROPER_NOUN regex literal")
});

// The tail of a label phrase applied to the text after a candidate run: the run
// itself or one lowercase word, closed by a colon — `Objaw:`, `Root cause:`,
// `Affected files:`. One word, not two, because a real name can be followed by a
// short colon clause ("NES na mikrokontrolerze: …" names NES), and a longer
// label phrase is a heading, which the slot rule already covers.
static LABEL_COLON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[^\S\n]*(?:\p{Ll}[\p{L}\p{N}'’-]*[^\S\n]*){0,1}:")
        .expect("entities.rs LABEL_COLON regex literal")
});

/// Upper bound on the words one name may span, carried over from the old
/// multi-word window.
const MAX_PROPER_NOUN_WORDS: usize = 4;

/// A single letter is never worth a tag, and it is the most common sentence
/// opener in Polish (`W`, `Z`, `O`, `A`, `I`), so it is not a candidate at all.
const MIN_PROPER_NOUN_WORD_LEN: usize = 2;

/// Punctuation that ends a slot: sentence terminators, the colon and semicolon
/// that open a label or clause, line breaks, and dashes used as either. A
/// capital after one of these is the writer's convention, not the word's name.
const SLOT_BOUNDARY_CHARS: &[char] = &['.', '!', '?', '…', ':', ';', '\n', '\r', '-', '–', '—'];

/// Delimiters that wrap or mark up a word instead of opening a slot, so a
/// capital after one is still the first word of its slot: `"Vestige …` opens a
/// sentence, `the tool (Vestige)` does not.
const OPENING_DELIMITERS: &[char] = &[
    '"', '\'', '“', '”', '„', '«', '»', '(', '[', '{', '*', '_', '#', '>', '|',
];

/// Words that look like proper nouns but aren't, when they lead a sentence:
/// English determiners, pronouns, quantifiers and connectives. A word here also
/// must not *lead* a run, so "The Sophie Wilson" is tagged as Sophie Wilson.
const STOP_PROPER: &[&str] = &[
    "The", "This", "That", "These", "Those", "There", "Here", "It", "He", "She", "They", "We",
    "You", "I", "What", "Which", "Who", "When", "Where", "Why", "How", "And", "But", "Or", "Not",
    "If", "So", "Then", "Also", "Just", "Only", "Still", "Even", "However", "Because", "Since",
    "While", "After", "Before", "Some", "Any", "All", "Each", "Every", "Most", "New", "Old",
    "Good", "Bad", "First", "Last", "Many", "Much", "More", "Few", "Less",
];

/// The Polish half of the stop vocabulary: first the labels of the report shape
/// this project recommends ("Objaw: … Przyczyna: … Lekcja: …") and the headings
/// a decision or pattern carries, then the connectives that open Polish
/// sentences.
///
/// This list is a supplement, not the fix. The structural rule in
/// [`extract_proper_nouns`] already drops every entry here in the position the
/// list exists for, and it does so for a language nobody has written a list
/// for; splitting the list is what keeps a language-specific table from
/// masquerading as the mechanism. It stays because a word on it is also refused
/// where something else *would* have kept it: a label word must not be
/// resurrected by a second mention that is itself a label.
const STOP_PROPER_PL: &[&str] = &[
    "Objaw",
    "Przyczyna",
    "Lekcja",
    "Wniosek",
    "Wnioski",
    "Kontekst",
    "Decyzja",
    "Alternatywy",
    "Skutek",
    "Naprawa",
    "Rozwiązanie",
    "Problem",
    "Cel",
    "Uwaga",
    "Wynik",
    "Wyniki",
    "To",
    "Ta",
    "Te",
    "Ten",
    "Tym",
    "Tam",
    "Tak",
    "Jest",
    "Są",
    "Był",
    "Była",
    "Było",
    "Były",
    "Nie",
    "Ale",
    "Lub",
    "Oraz",
    "Czy",
    "Jak",
    "Gdy",
    "Kiedy",
    "Jeśli",
    "Jeżeli",
    "Więc",
    "Zatem",
    "Dlatego",
    "Jednak",
    "Potem",
    "Następnie",
    "Wtedy",
    "Teraz",
    "Tutaj",
    "Każdy",
    "Każda",
    "Każde",
    "Wszystkie",
    "Wszystko",
    "Wszyscy",
    "Żaden",
    "Żadna",
    "Można",
    "Należy",
    "Trzeba",
    "Warto",
];

/// True when the word is stop vocabulary in any language the extractor carries,
/// rather than a name.
fn is_stop_word(word: &str) -> bool {
    STOP_PROPER.contains(&word) || STOP_PROPER_PL.contains(&word)
}

/// One name-shaped run found in the text, with the positional facts the
/// earn-the-tag rule needs.
struct ProperNounCandidate {
    /// The run as it should be tagged, with any leading determiner removed.
    text: String,
    /// The run's first word — what a second mention has to repeat for this
    /// mention to count as corroboration.
    head: String,
    /// True when the run sits where every sentence-opening language puts a
    /// capital on an ordinary word: the start of the text, of a sentence, of a
    /// line, or of a clause after `:` or `;`.
    opens_a_slot: bool,
    /// True when a colon closes the run, making it a `Label:` phrase.
    label: bool,
    /// True when the run's own shape cannot come from capitalising the first
    /// letter of an ordinary word (see [`shape_is_self_evidencing`]).
    self_evidencing: bool,
    /// True when two or more capitalised words stand in a row in the source.
    multi_word: bool,
}

/// Extract entities from text content.
///
/// Returns up to `max_entities` entities, prioritized by type:
/// URLs > Emails > File paths > Monetary > Proper nouns.
///
/// This is the strict, tag-earning answer: the proper-noun half is the set of
/// names that earned a tag, not every name the text mentions. A name that
/// appears only as the opening word of a later sentence is deliberately absent
/// (see [`extract_proper_nouns`]). A consumer asking "does this text name its
/// subject, and who is the actor?" wants [`extract_mentioned_names`] instead.
pub fn extract_entities(content: &str, max_entities: usize) -> Vec<ExtractedEntity> {
    let mut entities = Vec::new();
    let mut seen_texts: HashSet<String> = HashSet::new();

    let mut add = |text: String, etype: EntityType| {
        let lower = text.to_lowercase();
        if !seen_texts.contains(&lower) {
            seen_texts.insert(lower);
            entities.push(ExtractedEntity {
                text,
                entity_type: etype,
            });
        }
    };

    for m in URL_RE.find_iter(content) {
        add(m.as_str().to_string(), EntityType::Url);
    }

    for m in EMAIL_RE.find_iter(content) {
        add(m.as_str().to_string(), EntityType::Email);
    }

    for cap in FILE_PATH_RE.captures_iter(content) {
        if let Some(m) = cap.get(1).or(cap.get(0)) {
            let path = m.as_str().trim();
            if path.contains('/') || path.contains('\\') {
                add(path.to_string(), EntityType::FilePath);
            }
        }
    }

    for m in MONETARY_RE.find_iter(content) {
        add(m.as_str().to_string(), EntityType::Monetary);
    }

    // Proper nouns — capitalized runs that earned the tag (see below).
    extract_proper_nouns(content, &mut seen_texts, &mut entities);

    entities.truncate(max_entities);
    entities
}

/// Extract the names a text mentions, wherever they stand.
///
/// This is the permissive answer, for consumers that need to know *who or what
/// the text is about* rather than what deserves a tag:
///
/// * coreference needs the actor's text to resolve "She" → "Alice", and Alice
///   is usually the first word of the memory;
/// * relation extraction needs the subject's text before the verb, or "Alice
///   manages the Auth Team" yields no triple;
/// * the ingest gate's `has_named_subject` asks whether the memory names
///   anything a later reader could look up.
///
/// It differs from [`extract_entities`] in exactly one way: a sentence-initial
/// name counts even without a second mention. Label runs are still excluded —
/// `Objaw:` names the slot the writer is filling in, not a subject — which is
/// what keeps the permissive list from handing the gate back the junk that made
/// every Polish report look like it named something.
///
/// Never build `entity:*` tags from this list: see the module docs for why the
/// two questions are kept apart.
pub fn extract_mentioned_names(content: &str, max_names: usize) -> Vec<ExtractedEntity> {
    let mut names = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for candidate in proper_noun_candidates(content) {
        if candidate.label || !seen.insert(candidate.text.to_lowercase()) {
            continue;
        }
        names.push(ExtractedEntity {
            entity_type: classify_proper_noun(&candidate.text),
            text: candidate.text,
        });
    }

    names.truncate(max_names);
    names
}

/// Extract the proper nouns that earned a tag.
///
/// # Why a capital has to be earned
///
/// Capitalisation on its own says nothing about a word: every language that
/// capitalises sentence openings puts a capital on the first word of a
/// sentence and on the word after a `Label:` regardless of what that word
/// names. A vocabulary list cannot fix that, because the list would have to
/// exist once per language — `STOP_PROPER` is English, so in a Polish store it
/// could never fire, and the shape this project recommends ("Objaw: …
/// Przyczyna: … Lekcja: …") produced three junk tags per memory, identical
/// across unrelated memories.
///
/// Position does transfer. A capital that sits at the start of a slot — the
/// text, a sentence, a line, a clause after `:` or `;` — is explained by the
/// orthography, so it is not evidence on its own; the same capitalised form
/// standing where sentence capitalisation cannot reach is evidence, because
/// nothing else in the orthography put a capital there. A language that
/// capitalises nouns mid-sentence (German) is the known limit of that argument:
/// there an ordinary noun still looks like a name, and only a per-language list
/// can help — which is why [`STOP_PROPER_PL`] is filed as a supplement and not
/// as the mechanism. So a run is emitted when it has at least one mention that
/// position cannot explain, and dropped when every mention is positional. That
/// is the corroboration rule: the form has to appear somewhere it could only
/// appear if it were a name. The cost is a genuine name mentioned exactly once
/// as the opening word of a memory — it earns its tag as soon as the text names
/// it again, and a store that never does gets no tag rather than a wrong one.
///
/// Three shapes are self-evidencing and need no second mention, because no
/// sentence-opening rule can produce them: acronyms and identifiers carrying
/// several capitals or digits (`NES`, `STM32`, `MongoDB`), and two or more
/// capitalised words in a row (`Jan Kowalski`, `Castlevania III`) — a one-word
/// sentence opening cannot explain the second capital.
///
/// A label is the one position the shapes above do not rescue: `Objaw:` and
/// `## Affected Files:` are section names, so the run is dropped there unless
/// the text also names the same form where a label cannot be.
fn extract_proper_nouns(
    content: &str,
    seen_texts: &mut HashSet<String>,
    entities: &mut Vec<ExtractedEntity>,
) {
    let candidates = proper_noun_candidates(content);

    // The whole language independence of this function is in these five lines:
    // a form counts as corroborated when it occurs somewhere that is neither a
    // slot start nor a label — no vocabulary, no language model.
    let corroborated: HashSet<&str> = candidates
        .iter()
        .filter(|candidate| !candidate.opens_a_slot && !candidate.label)
        .map(|candidate| candidate.head.as_str())
        .collect();

    for candidate in &candidates {
        let is_corroborated = corroborated.contains(candidate.head.as_str());
        let earned = if candidate.self_evidencing {
            // `NES`, `STM32`, `MongoDB`: nothing here is a sentence-opening
            // convention, not even in a label.
            true
        } else if candidate.label {
            is_corroborated
        } else if !candidate.opens_a_slot {
            // Where no sentence can open, the capital belongs to the word.
            true
        } else {
            // At a text, sentence or line opening: a second mention settles it,
            // and so do two capitalised words in a row, which a one-word
            // sentence opening cannot explain.
            candidate.multi_word || is_corroborated
        };
        if !earned {
            continue;
        }

        let lower = candidate.text.to_lowercase();
        if seen_texts.contains(&lower) {
            continue;
        }

        let etype = classify_proper_noun(&candidate.text);
        seen_texts.insert(lower);
        entities.push(ExtractedEntity {
            text: candidate.text.clone(),
            entity_type: etype,
        });
    }
}

/// Find every name-shaped run, in text order, with the positional facts the
/// earn-the-tag decision needs.
fn proper_noun_candidates(content: &str) -> Vec<ProperNounCandidate> {
    let tokens: Vec<(usize, usize, &str)> = PROPER_NOUN_RE
        .find_iter(content)
        .filter(|m| {
            // A token inside a longer word is not a token: `mcDonald` must not
            // yield `Donald`, nor `d'Artagnan` a bare `Artagnan`. A path
            // segment is not a token either — `tutorial-pl/README.md` belongs
            // to the file-path entity, and an anchor is not a name.
            m.as_str().chars().count() >= MIN_PROPER_NOUN_WORD_LEN
                && !content[..m.start()].chars().next_back().is_some_and(|c| {
                    c.is_alphanumeric() || matches!(c, '\'' | '’' | '-' | '/' | '\\')
                })
        })
        .map(|m| (m.start(), m.end(), m.as_str()))
        .collect();

    let mut candidates = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        // Consecutive capitalised words on one line are a single run — `Jan
        // Kowalski`, `Castlevania III` — because a run is what the tag should
        // name. A line break ends it: what follows is a heading, a list item or
        // a new thought, and `## Context` followed by `Niepełna …` is not one
        // name.
        let mut span = 1;
        while span < MAX_PROPER_NOUN_WORDS
            && index + span < tokens.len()
            && content[tokens[index + span - 1].1..tokens[index + span].0]
                .chars()
                .all(|c| c.is_whitespace() && c != '\n' && c != '\r')
        {
            span += 1;
        }

        let multi_word = span > 1;
        let run = &tokens[index..index + span];
        // A determiner or sentence opener must not lead the name: the text "The
        // Sophie Wilson" names Sophie Wilson, and the article is not part of it.
        let run = match run.iter().position(|t| !is_stop_word(t.2)) {
            Some(first) => &run[first..],
            None => {
                index += span;
                continue;
            }
        };

        let start = run[0].0;
        let end = run[run.len() - 1].1;
        let text = &content[start..end];
        candidates.push(ProperNounCandidate {
            text: text.to_string(),
            head: run[0].2.to_string(),
            opens_a_slot: opens_a_slot(content, start),
            label: label_colon_follows(content, end),
            self_evidencing: text.split_whitespace().any(shape_is_self_evidencing),
            multi_word,
        });
        index += span;
    }

    candidates
}

/// True when the capital could belong to the orthography rather than to the
/// word: the run sits where every sentence-opening language puts a capital on
/// an ordinary word — the start of the text, of a sentence, of a line, or of a
/// clause after `:` or `;`.
fn opens_a_slot(content: &str, start: usize) -> bool {
    // Spaces and tabs are invisible, so drop them first; a line break must stay
    // visible, because it opens a slot on its own — markdown headings, list
    // items, and the `[Updated …]` blocks our own ingest appends.
    let head =
        content[..start].trim_end_matches(|c: char| c.is_whitespace() && c != '\n' && c != '\r');
    // Wrapping markup only decorates the word: `"Vestige …` is still the first
    // word of its sentence, `## Context` is still a heading, while
    // `the tool (Vestige)` is mid-sentence.
    let head = head.trim_end_matches(|c: char| OPENING_DELIMITERS.contains(&c));
    match head.chars().next_back() {
        None => true,
        Some(c) => SLOT_BOUNDARY_CHARS.contains(&c),
    }
}

/// True when the run opens a `Label:` phrase, the other position where a
/// capital means nothing — the word names the slot the writer is filling in,
/// not something a later reader could look up.
fn label_colon_follows(content: &str, end: usize) -> bool {
    LABEL_COLON_RE.is_match(&content[end..])
}

/// True when the word's capitals or digits cannot come from capitalising the
/// first letter of an ordinary word: acronyms (`NES`, `MCP`), identifiers
/// (`STM32`, `FSRS-6`) and internally capitalised brands (`MongoDB`) are names
/// in every orthography, so one mention is enough.
fn shape_is_self_evidencing(word: &str) -> bool {
    word.chars().any(|c| c.is_numeric()) || word.chars().filter(|c| c.is_uppercase()).count() > 1
}

fn classify_proper_noun(text: &str) -> EntityType {
    let words: Vec<&str> = text.split_whitespace().collect();

    // Single capitalized word with >= 3 chars that ends in common org suffixes
    if words.len() == 1 {
        let w = words[0];
        if w.ends_with("Corp") || w.ends_with("Inc") || w.ends_with("Ltd") || w.ends_with("LLC") {
            return EntityType::Organization;
        }
    }

    // Multi-word with org indicators
    let lower = text.to_lowercase();
    if lower.contains("team")
        || lower.contains("group")
        || lower.contains("corp")
        || lower.contains("inc")
        || lower.contains("company")
        || lower.contains("dept")
        || lower.contains("department")
        || lower.contains("university")
        || lower.contains("institute")
        || lower.contains("foundation")
    {
        return EntityType::Organization;
    }

    // 2-3 capitalized words → likely a person name
    if words.len() >= 2
        && words.len() <= 3
        && words
            .iter()
            .all(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
    {
        return EntityType::Person;
    }

    EntityType::ProperNoun
}

/// Convert extracted entities to auto-tags with `entity:` prefix.
///
/// Tags are slugified: "John Smith" → "entity:john-smith".
/// Capped at `max_tags` (default 10).
pub fn entities_to_tags(entities: &[ExtractedEntity], max_tags: usize) -> Vec<String> {
    entities
        .iter()
        .take(max_tags)
        .filter_map(|e| {
            let slug = slugify(&e.text);
            if slug.is_empty() || slug.len() > 60 {
                return None;
            }
            Some(format!("entity:{}", slug))
        })
        .collect()
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == ' ' || c == '_' || c == '.' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_urls() {
        let entities = extract_entities("Check https://github.com/vestige for details", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Url && e.text.contains("github.com"))
        );
    }

    #[test]
    fn test_extract_emails() {
        let entities = extract_entities("Contact user@example.com for help", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Email && e.text == "user@example.com")
        );
    }

    #[test]
    fn test_extract_file_paths() {
        let entities = extract_entities("Edit the file /src/main.rs to fix the bug", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::FilePath)
        );
    }

    #[test]
    fn test_extract_monetary() {
        let entities = extract_entities("The cost was $1,500.00 per month", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Monetary)
        );
    }

    #[test]
    fn test_extract_proper_nouns() {
        let entities =
            extract_entities("The project was designed by Sophie Wilson at Cambridge", 20);
        let names: Vec<&str> = entities
            .iter()
            .filter(|e| matches!(e.entity_type, EntityType::Person | EntityType::ProperNoun))
            .map(|e| e.text.as_str())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n.contains("Sophie") || n.contains("Wilson") || n.contains("Cambridge")),
            "Expected proper nouns, got: {:?}",
            names
        );
    }

    #[test]
    fn test_stop_words_filtered() {
        let entities = extract_entities("The quick brown fox. This is not an entity.", 10);
        let texts: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
        assert!(!texts.contains(&"The"), "Should filter stop words");
        assert!(!texts.contains(&"This"), "Should filter stop words");
    }

    #[test]
    fn test_entities_to_tags() {
        let entities = vec![
            ExtractedEntity {
                text: "John Smith".to_string(),
                entity_type: EntityType::Person,
            },
            ExtractedEntity {
                text: "https://example.com".to_string(),
                entity_type: EntityType::Url,
            },
        ];
        let tags = entities_to_tags(&entities, 10);
        assert!(tags.contains(&"entity:john-smith".to_string()));
    }

    #[test]
    fn test_max_entities_cap() {
        let content = "Alice, Bob, Carol, Dave, Eve, Frank, Grace, Heidi, Ivan, Judy, Karl, Lisa";
        let entities = extract_entities(content, 5);
        assert!(entities.len() <= 5);
    }

    #[test]
    fn test_empty_content() {
        let entities = extract_entities("", 10);
        assert!(entities.is_empty());
    }

    #[test]
    fn test_dedup_entities() {
        let entities = extract_entities("Email user@test.com and also user@test.com again", 10);
        let emails: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Email)
            .collect();
        assert_eq!(emails.len(), 1, "Duplicate emails should be deduped");
    }

    #[test]
    fn test_mixed_entity_types() {
        let content =
            "Contact user@test.com at https://example.com for the $500 report on /var/log/app.log";
        let entities = extract_entities(content, 20);
        let types: Vec<EntityType> = entities.iter().map(|e| e.entity_type).collect();
        assert!(types.contains(&EntityType::Email));
        assert!(types.contains(&EntityType::Url));
        assert!(types.contains(&EntityType::Monetary));
    }

    #[test]
    fn test_organization_detection() {
        let content = "The project was led by the Engineering Department at Stanford University";
        let entities = extract_entities(content, 20);
        let orgs: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Organization)
            .collect();
        assert!(
            !orgs.is_empty(),
            "Should detect organization, got entities: {:?}",
            entities
        );
    }

    #[test]
    fn test_entity_type_as_str() {
        assert_eq!(EntityType::Person.as_str(), "person");
        assert_eq!(EntityType::Organization.as_str(), "org");
        assert_eq!(EntityType::Url.as_str(), "url");
        assert_eq!(EntityType::Email.as_str(), "email");
        assert_eq!(EntityType::FilePath.as_str(), "path");
        assert_eq!(EntityType::Monetary.as_str(), "monetary");
        assert_eq!(EntityType::ProperNoun.as_str(), "noun");
    }

    #[test]
    fn test_slugify_special_chars() {
        let entities = vec![ExtractedEntity {
            text: "user@test.com".to_string(),
            entity_type: EntityType::Email,
        }];
        let tags = entities_to_tags(&entities, 10);
        assert!(!tags.is_empty());
        assert!(tags[0].starts_with("entity:"));
        assert!(!tags[0].contains('@'), "Slug should not contain @");
    }

    #[test]
    fn test_entities_to_tags_max_cap() {
        let entities: Vec<_> = (0..20)
            .map(|i| ExtractedEntity {
                text: format!("Entity{}", i),
                entity_type: EntityType::ProperNoun,
            })
            .collect();
        let tags = entities_to_tags(&entities, 5);
        assert_eq!(tags.len(), 5);
    }

    #[test]
    fn test_eur_currency() {
        let entities = extract_entities("The price is €1,200.50 plus tax", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Monetary),
            "Should detect EUR currency"
        );
    }

    #[test]
    fn test_case_insensitive_dedup() {
        let entities = extract_entities("Contact admin@TEST.COM and also admin@test.com again", 10);
        let emails: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Email)
            .collect();
        assert_eq!(
            emails.len(),
            1,
            "Case-insensitive dedup should collapse identical emails"
        );
    }

    // ========================================================================
    // Regression fixtures: real Polish memories from a live 13-memory store
    // (docs/review/2026-09-20-memory-store-review.md §U5). 21 of that store's
    // 59 tags were `entity:*` noise; every memory written in the store's own
    // recommended shape carried exactly three of them, one per sentence.
    // ========================================================================

    /// Store memory `d59cea00` verbatim: the "Objaw: … Przyczyna: … Lekcja: …"
    /// shape. Each sentence's first capital used to become a tag, so all seven
    /// events in the store shared `entity:objaw`, `entity:przyczyna`,
    /// `entity:lekcja` — identical tags across unrelated memories, which is
    /// what made them harmful rather than merely useless.
    const POLISH_LABEL_MEMORY: &str = "Objaw: po wgraniu nowej wersji programu płytka zachowywała się jak przed zmianą. Przyczyna: łącze debugowania było zajęte przez inny proces, a narzędzie do wgrywania nie zgłosiło błędu i zakończyło się powodzeniem. Lekcja: po wgraniu resetuj układ osobną komendą i sprawdź, że zachowanie naprawdę się zmieniło; brak komunikatu o błędzie nie znaczy, że operacja się odbyła.";

    /// Store memory `c225a8c7` verbatim: a Polish concept whose tags were
    /// `entity:wniosek`, `entity:kierunek` and `entity:ka` — the last one
    /// truncated, because the old ASCII `[a-z]+` class stopped at the `ż`.
    /// `NES` is the one genuine name in the text and must survive.
    const POLISH_CONCEPT_MEMORY: &str = "Wniosek z pomiarów emulatora NES na mikrokontrolerze z panelem na jednym przewodzie: wąskim gardłem okazała się transmisja obrazu, a nie emulacja procesora. Kierunek optymalizacji wyznacza więc nie szybkość liczenia, a liczba bajtów wysyłanych do panelu i to, czy procesor czeka na koniec transmisji. Każdy projekt z wyświetlaczem na jednym przewodzie warto zacząć od policzenia czasu transmisji jednej klatki, bo ta liczba ustala górny limit klatek na sekundę.";

    /// The tags the ingest path would attach: the same call sequence
    /// `preprocessing::preprocess` uses (`MAX_ENTITIES` then `MAX_AUTO_TAGS`).
    fn entity_tags(content: &str) -> Vec<String> {
        entities_to_tags(&extract_entities(content, 20), 10)
    }

    #[test]
    fn test_polish_label_memory_produces_no_entity_tags() {
        let tags = entity_tags(POLISH_LABEL_MEMORY);
        for junk in ["entity:objaw", "entity:przyczyna", "entity:lekcja"] {
            assert!(
                !tags.contains(&junk.to_string()),
                "`{junk}` is a label, not a name; got {tags:?}"
            );
        }
        assert!(
            tags.is_empty(),
            "a memory that names nothing must carry no entity tags, got {tags:?}"
        );
    }

    #[test]
    fn test_polish_concept_memory_keeps_only_the_real_name() {
        let tags = entity_tags(POLISH_CONCEPT_MEMORY);
        for junk in [
            "entity:wniosek",
            "entity:kierunek",
            "entity:ka",
            "entity:każdy",
        ] {
            assert!(
                !tags.contains(&junk.to_string()),
                "`{junk}` is the first word of a sentence, not a name; got {tags:?}"
            );
        }
        assert!(
            tags.contains(&"entity:nes".to_string()),
            "NES is a real name in mid-sentence position and must survive, got {tags:?}"
        );
    }

    /// The rule is "earn it by appearing where sentence position cannot explain
    /// the capital" — so a name mentioned only as the opening word of the text
    /// yields nothing, while the same name mentioned again mid-sentence yields
    /// its tag. Both halves matter: the first is the defect, the second is what
    /// stops the fix from decaying into "disable proper nouns".
    #[test]
    fn test_sentence_initial_name_is_kept_only_when_corroborated() {
        let alone = entity_tags("Vestige przechowuje wspomnienia i porządkuje je w grafie.");
        assert!(
            alone.is_empty(),
            "a lone sentence-initial capital is the orthography's, not a name; got {alone:?}"
        );

        let corroborated = entity_tags(
            "Vestige przechowuje wspomnienia. Każde wspomnienie trafia najpierw do Vestige.",
        );
        assert!(
            corroborated.contains(&"entity:vestige".to_string()),
            "the second, mid-sentence mention is proof the capital belongs to the word; got {corroborated:?}"
        );
    }

    /// The mirror of the defect, in both languages: names, brands and
    /// identifiers must still produce tags. This is the test that fails if
    /// someone "fixes" the noise by turning proper-noun extraction off.
    #[test]
    fn test_real_names_survive_the_position_filter() {
        let english = entity_tags(
            "The project was designed by Sophie Wilson at Cambridge. The team then moved the \
             store to MongoDB, and the firmware now runs on an STM32 board.",
        );
        for expected in [
            "entity:sophie-wilson",
            "entity:cambridge",
            "entity:mongodb",
            "entity:stm32",
        ] {
            assert!(
                english.contains(&expected.to_string()),
                "`{expected}` is a real name and must survive, got {english:?}"
            );
        }

        // Polish: the name is mid-sentence, where no orthography puts a capital
        // by default, so it is evidence on its own.
        let polish = entity_tags(
            "Konfigurację panelu opisał Jan Kowalski. W kolejnym kroku Jan Kowalski dodał pomiar \
             czasu transmisji jednej klatki.",
        );
        assert!(
            polish.contains(&"entity:jan-kowalski".to_string()),
            "a mid-sentence Polish name must survive, got {polish:?}"
        );
    }

    /// A colon after the run marks a slot label ("Root cause:", "Objaw:").
    /// The capital there belongs to the template, so a second mention that is
    /// also a label is still not evidence.
    #[test]
    fn test_label_colon_is_never_evidence() {
        let tags = entity_tags(
            "Root cause: the shadow buffer was compared with itself. Then, Root cause: the same \
             mistake again.",
        );
        assert!(
            tags.is_empty(),
            "a word followed by a label colon is template vocabulary, got {tags:?}"
        );
    }

    /// The two entry points answer two different questions, and the difference
    /// is the whole point of having two: the same memory that earns no tag for
    /// its opening word still *mentions* it, so the gate and relation extraction
    /// keep the subject while the tag list stays clean. A read of the strict
    /// list cannot answer "does this name its subject?" — that is what the
    /// permissive one is for.
    #[test]
    fn test_mentioned_names_are_permissive_where_tags_are_strict() {
        let content = "Objaw: gra nie wchodziła do poziomu. Marek said the fix was in the tester.";

        let tags = entity_tags(content);
        assert!(
            tags.is_empty(),
            "neither the label nor the sentence-opening name earns a tag, got {tags:?}"
        );

        let mentions: Vec<String> = extract_mentioned_names(content, 10)
            .into_iter()
            .map(|entity| entity.text)
            .collect();
        assert!(
            mentions.contains(&"Marek".to_string()),
            "a memory that opens with its subject mentions it, got {mentions:?}"
        );
        assert!(
            !mentions.contains(&"Objaw".to_string()),
            "a label names the slot being filled in, not a subject, got {mentions:?}"
        );
    }

    /// The store's other real shape: what the `codebase` tool writes is
    /// markdown — a `# Decision:` title, `## Context` / `## Alternatives
    /// Considered:` headings, and an `[Updated …]` block our own ingest
    /// appends. A heading run is label vocabulary even when it is two
    /// capitalized words, and no run may swallow the first word of the
    /// paragraph under it (`## Context` + `Niepełna …` is not one name — the
    /// real defect produced `entity:contextniepełna` that way). The one real
    /// name in the text still has to come out.
    #[test]
    fn test_markdown_headings_are_not_entities() {
        let tags = entity_tags(
            "# Decision: Każdy obsługiwany układ kartridża implementuje\n\n\
             ## Context\n\n\
             Niepełna implementacja przechodzi ekran tytułowy. Castlevania III pokazała to \
             wprost: emulator doszedł do intra, a poziom nie startował.\n\n\
             ## Alternatives Considered:\n\
             - Implementować tylko to, czego używa konkretna gra\n\n\
             ## Affected Files:\n\
             - src/mapper.c\n\n\
             [Updated 2026-09-20]\n\
             Objaw: gra nie wchodziła do poziomu, tylko w kółko odtwarzała intro.",
        );
        for junk in [
            "entity:decisionkażdy",
            "entity:contextniepełna",
            "entity:alternatives-considered",
            "entity:affected-files",
            "entity:updated",
            "entity:objaw",
        ] {
            assert!(
                !tags.contains(&junk.to_string()),
                "`{junk}` is a section heading or label, not a name; got {tags:?}"
            );
        }
        assert_eq!(
            tags,
            vec!["entity:castlevania-iii".to_string()],
            "the title of the game is the only name in this memory"
        );
    }
}

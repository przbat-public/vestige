//! Lightweight language detection for NLP routing.
//!
//! Detects whether a piece of text is **Polish**, **English**, or **mixed/
//! unknown**. This is intentionally not a general-purpose language identifier
//! — we only need to pick the right per-language lexicon for the heuristic
//! detectors. Trading recall for the other ~7000 languages of the world is
//! a deliberate scope decision; the price is a method that we can ship as
//! ~70 lines of pure Rust and update without thinking about dependencies.
//!
//! ## Algorithm
//!
//! The detector counts three signals over the lowercased input:
//!
//! 1. **Polish-specific diacritics**: `ą ć ę ł ń ó ś ź ż`. Any occurrence is
//!    a strong PL signal — these characters don't appear in native English.
//! 2. **Polish digraphs / function words**: `nie`, `że`, `czy`, `jak`,
//!    `ich`, `dla`, `się`, etc. These catch Polish text written without
//!    diacritics (which is common on poorly-configured keyboards).
//! 3. **English function words**: `the`, `and`, `is`, `are`, `that`, etc.
//!
//! The classifier then decides:
//!
//! - **PL** if the diacritic count ≥ 1, OR the PL function-word density is
//!   higher than the EN density.
//! - **EN** if the EN function-word density is higher and no PL diacritics.
//! - **Unknown** for very short text (< 3 words), or texts with no signal.
//!
//! ## Why not `whatlang` or `lingua`?
//!
//! Both are excellent crates, but they pull in language models (~10MB
//! compressed) and tens of thousands of training tokens for languages
//! Vestige doesn't care about. For PL/EN classification on our domain
//! (memory snippets averaging 100-500 chars), this 70-LOC method
//! hits >95% accuracy in our eval suite — see `eval/data/language.rs`.

use std::collections::HashSet;
use std::sync::OnceLock;

/// The languages the NLP layer routes on.
///
/// Extending this enum requires:
/// 1. Add per-language lexicons in `contradiction.rs`, `opinion.rs`,
///    `future_relevance.rs`.
/// 2. Add evaluation examples in `eval/data/`.
/// 3. Update [`detect_language`] with the new signal set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// English.
    English,
    /// Polish.
    Polish,
    /// Mixed text (e.g. code comments with English keywords + Polish prose),
    /// or text too short to classify.
    Unknown,
}

/// Detect the dominant language of a piece of text.
///
/// Returns [`Language::Unknown`] if the text is empty, fewer than 3 words,
/// or has no language-specific signal.
pub fn detect_language(text: &str) -> Language {
    if text.trim().is_empty() {
        return Language::Unknown;
    }

    // Cheap pre-filter: Polish diacritics are a one-shot signal. If we see
    // any, we're done.
    let has_polish_diacritic = text
        .chars()
        .any(|c| matches!(c, 'ą' | 'ć' | 'ę' | 'ł' | 'ń' | 'ó' | 'ś' | 'ź' | 'ż'
                 | 'Ą' | 'Ć' | 'Ę' | 'Ł' | 'Ń' | 'Ó' | 'Ś' | 'Ź' | 'Ż'));

    // Tokenize on ASCII word boundaries. We don't need a real tokenizer —
    // unicode word segmentation would be more correct but adds a dep for
    // marginal benefit on this task. Single-character tokens are dropped
    // because they conflate "i" (PL `and` / EN `I`) and "a" (EN article /
    // a fragment of an apostrophe-stripped foreign word like "Postgres'a")
    // — that ambiguity used to flip short Polish memories to English.
    let lowered = text.to_lowercase();
    let words: Vec<&str> = lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .collect();

    if words.len() < 3 {
        // Single PL diacritic is enough to lock the language even on very
        // short input (e.g. "nie wiem" — short, but unambiguous).
        if has_polish_diacritic {
            return Language::Polish;
        }
        return Language::Unknown;
    }

    let pl_fn_words = polish_function_words();
    let en_fn_words = english_function_words();

    let mut pl_hits = 0_usize;
    let mut en_hits = 0_usize;
    for w in &words {
        if pl_fn_words.contains(*w) {
            pl_hits += 1;
        }
        if en_fn_words.contains(*w) {
            en_hits += 1;
        }
    }

    // Densities, not raw counts, so we don't get fooled by length.
    let total = words.len() as f32;
    let pl_density = pl_hits as f32 / total;
    let en_density = en_hits as f32 / total;

    if has_polish_diacritic {
        return Language::Polish;
    }

    // Require a meaningful signal — < 5% density on both sides means we
    // can't tell.
    const SIGNAL_THRESHOLD: f32 = 0.05;
    if pl_density < SIGNAL_THRESHOLD && en_density < SIGNAL_THRESHOLD {
        return Language::Unknown;
    }

    if pl_density > en_density {
        Language::Polish
    } else if en_density > pl_density {
        Language::English
    } else {
        Language::Unknown
    }
}

fn polish_function_words() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        [
            // Core function words & particles (skip single-char ones —
            // they're stripped by the tokenizer).
            "nie", "się", "to", "tym", "tego", "tej", "tych", "tę", "ta",
            "ten", "te", "ci", "ich", "ją", "go", "mu", "jej",
            // Conjunctions
            "lub", "ale", "albo", "lecz", "bo", "ponieważ", "więc",
            "oraz", "czy", "jeśli", "gdy", "kiedy", "jak", "żeby",
            "że", "iż",
            // Prepositions
            "na", "we", "ze", "do", "od", "po", "przed", "za",
            "dla", "bez", "przy", "nad", "pod", "przez",
            // Modifiers / common adverbs
            "tylko", "tak", "tu", "tam", "tutaj", "też", "także",
            "jeszcze", "już", "może", "trzeba", "warto", "raczej",
            // Negation/correction (also used by contradiction.rs)
            "nigdy", "żaden", "żadna", "żadne",
            // Common verbs (1st/3rd person sing)
            "jest", "był", "była", "było", "byli", "były", "będzie",
            "musi", "musisz", "muszę", "trzeba", "mam", "masz", "ma",
            "mają", "robię", "robi", "robisz", "dotyczy", "wynosi",
            "powinno", "powinien", "powinna",
            // Question/relative
            "który", "która", "które", "kto",
        ]
        .into_iter()
        .collect()
    })
}

fn english_function_words() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        [
            // Articles (skip `a` — single-char, tokenizer drops it).
            "the", "an",
            // Pronouns (skip `I` — single-char `i` is dropped; we rely on
            // density of other markers like `the`, `and`, `to`).
            "you", "he", "she", "it", "we", "they", "this", "that",
            "these", "those", "my", "your", "his", "her", "its", "our",
            "their", "me", "him", "us", "them",
            // Common verbs
            "is", "are", "was", "were", "be", "been", "being", "have",
            "has", "had", "do", "does", "did", "will", "would", "could",
            "should", "may", "might", "can", "must",
            // Conjunctions
            "and", "or", "but", "so", "yet", "for", "nor", "because", "if",
            "when", "while", "though", "although", "since", "unless",
            // Prepositions
            "in", "on", "at", "to", "from", "with", "by", "about", "into",
            "through", "during", "before", "after", "between", "of",
            // Negation
            "not", "no", "never",
        ]
        .into_iter()
        .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_polish_with_diacritics() {
        let text = "Pamiętaj że trzeba sprawdzić to później.";
        assert_eq!(detect_language(text), Language::Polish);
    }

    #[test]
    fn detects_polish_without_diacritics() {
        // Polish written without diacritics ("polskawe") — happens a lot
        // in chat / commit messages.
        let text = "Pamietaj ze trzeba sprawdzic to pozniej i nie zapomniec o tym.";
        assert_eq!(detect_language(text), Language::Polish);
    }

    #[test]
    fn detects_english() {
        let text = "The mitochondria is the powerhouse of the cell.";
        assert_eq!(detect_language(text), Language::English);
    }

    #[test]
    fn unknown_for_empty() {
        assert_eq!(detect_language(""), Language::Unknown);
        assert_eq!(detect_language("   \n\t  "), Language::Unknown);
    }

    #[test]
    fn unknown_for_very_short_without_diacritics() {
        // Two words, no diacritics, no function words → can't tell.
        assert_eq!(detect_language("hello world"), Language::Unknown);
    }

    #[test]
    fn polish_short_with_diacritic() {
        // Single Polish diacritic on short text — should lock to Polish even
        // when the word count is below the normal threshold.
        assert_eq!(detect_language("już dziś"), Language::Polish);
    }

    #[test]
    fn handles_code_snippets_with_english_keywords() {
        // Code with function names — should classify as EN.
        let text = "const result = foo.map(x => x.bar).filter(Boolean);";
        // It's actually mostly identifiers — likely Unknown is fine here.
        let lang = detect_language(text);
        assert!(matches!(lang, Language::English | Language::Unknown));
    }

    #[test]
    fn polish_dominates_mixed_text() {
        // Polish prose with some English technical terms.
        let text = "Naprawiłem bug w komponencie React. Trzeba sprawdzić czy useState działa poprawnie.";
        assert_eq!(detect_language(text), Language::Polish);
    }

    #[test]
    fn english_dominates_mixed_text() {
        // English with one Polish word — should still classify as EN.
        let text = "I fixed the bug in the React component. The useState hook was misbehaving.";
        assert_eq!(detect_language(text), Language::English);
    }

    #[test]
    fn deterministic_across_calls() {
        // Same input → same output, every time.
        let text = "To jest test który sprawdza determinizm detektora języka.";
        let first = detect_language(text);
        for _ in 0..10 {
            assert_eq!(detect_language(text), first);
        }
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            detect_language("Pamiętaj że trzeba sprawdzić."),
            detect_language("PAMIĘTAJ ŻE TRZEBA SPRAWDZIĆ.")
        );
    }

    #[test]
    fn capital_polish_diacritic_detected() {
        // Capital diacritic — easy to miss in a naive impl.
        assert_eq!(detect_language("ŁADNIE wykonane zadanie."), Language::Polish);
    }

    #[test]
    fn long_text_classifies_correctly() {
        // Realistic Vestige memory content.
        let pl = "Naprawiono problem z deserializacją Pub/Sub event'ów: handler oczekiwał \
                  base64-encoded data, ale ostatnia wersja library zwraca już zdekodowany \
                  payload. Trzeba pamiętać o tej zmianie przy upgrade'ach.";
        assert_eq!(detect_language(pl), Language::Polish);

        let en = "Fixed Pub/Sub event deserialization issue: the handler expected \
                  base64-encoded data, but the latest library version returns \
                  pre-decoded payload. Need to remember this when upgrading.";
        assert_eq!(detect_language(en), Language::English);
    }
}

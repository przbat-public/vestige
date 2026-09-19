//! NegEx-style negation scope detection.
//!
//! The legacy contradiction heuristic was symmetric substring matching:
//! "does the new memory contain the word `not` that the old one doesn't?".
//! This loses two important pieces of information:
//!
//! 1. **What is being negated.** "I do *not* like X" negates "like X", not
//!    "do" or "I".
//! 2. **The scope of the negation.** "Don't do X but feel free to do Y"
//!    negates X, not Y. Terminator tokens (`but`, `however`, `jednak`)
//!    end the scope.
//!
//! This module implements a port of the **NegEx** algorithm (Chapman et al.,
//! "A simple algorithm for identifying negated findings and diseases in
//! discharge summaries", 2001). NegEx was designed for clinical text, but
//! the underlying mechanism — a sliding window from a negation trigger
//! to the next clause boundary — generalises cleanly to any prose.
//!
//! ## Algorithm
//!
//! For each negation trigger `t` found in the text:
//!
//! 1. Find the **scope window** starting *after* `t`, up to:
//!    - the next punctuation that ends a clause (`.`, `!`, `?`, `;`),
//!    - the next clause boundary token (`but`, `however`, `ale`, `jednak`,
//!      `lecz`, `chociaż`, `although`, …),
//!    - or a maximum of `MAX_SCOPE_WORDS` tokens.
//! 2. Record the trigger + scope as a [`NegationScope`].
//!
//! Special case: triggers that act **backwards** (Polish "X nie jest Y" — the
//! negation actually scopes over the preceding constituent). For our use
//! case (memory snippets, mostly English with some Polish), the forward
//! sweep is sufficient — backward scope adds parsing complexity for
//! marginal recall.
//!
//! ## Limits we accept
//!
//! - **No syntactic parsing.** "Not only X but Y" looks like "not [scope: X
//!   but Y]" to us. NegEx the original paper had the same limit; the
//!   downstream consumer must be ready for occasional false positives.
//! - **No constituency awareness.** "I don't think it's broken" reads as
//!   negation of "think it's broken" rather than "it's broken". This
//!   matters less than it sounds — both readings imply doubt.
//! - **No stem matching for Polish.** "nie pamiętam" and "nie pamiętała"
//!   are detected because we list `nie` (the trigger); the verb form
//!   appears inside the scope and is just text.

use std::sync::OnceLock;

use super::language::Language;

/// Maximum number of words a negation scope can span.
///
/// NegEx used 5-6 in clinical text; we use 8 because Vestige memories tend
/// to be longer and use more clauses. Going higher gives diminishing returns
/// and more false positives.
pub const MAX_SCOPE_WORDS: usize = 8;

/// A detected negation scope.
///
/// Holds the byte offsets of the trigger and of the scope window, plus the
/// extracted scope text (with the trigger removed, for downstream semantic
/// comparison against another memory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegationScope {
    /// Byte offset (start) of the trigger.
    pub trigger_start: usize,
    /// Byte offset (end, exclusive) of the trigger.
    pub trigger_end: usize,
    /// Byte offset (start) of the scope window (the words after the trigger).
    pub scope_start: usize,
    /// Byte offset (end, exclusive) of the scope window.
    pub scope_end: usize,
    /// The trigger token, lowercased.
    pub trigger: String,
    /// The scope text, with surrounding whitespace trimmed.
    pub scope_text: String,
}

/// Find all negation scopes in `text` for the given `language`.
///
/// Returns an empty vec if no triggers are found. Pure function — no
/// allocations on the hot path beyond the result.
pub fn find_negation_scopes(text: &str, language: Language) -> Vec<NegationScope> {
    let triggers = triggers_for(language);
    let terminators = terminators_for(language);

    let mut scopes = Vec::new();

    // Tokenize on whitespace + punctuation, keeping byte offsets.
    // Tokenize the ORIGINAL text so `trigger_start`/`scope_end` are offsets into `text`.
    // Lowercasing the whole string first (the previous behaviour) moves offsets whenever a
    // character changes byte length when lowercased (U+0130, U+212A, U+1E9E), so
    // `text.get(scope_start..scope_end)` silently returned "" and the negation signal
    // vanished. Comparison is case-insensitive per token instead.
    let words = tokenize_with_offsets(text);
    if words.is_empty() {
        return scopes;
    }

    let mut i = 0;
    while i < words.len() {
        let (word_start, word_end, word) = words[i];

        // Check single-token and bigram triggers.
        // Case-insensitive, apostrophe-normalised comparison: `don\u{2019}t` (U+2019) must
        // match the `don't` trigger exactly like the ASCII spelling does.
        let word_lc = normalize_token(word);
        let single_match = triggers.iter().any(|t| !t.contains(' ') && *t == word_lc);
        let bigram_match = if i + 1 < words.len() {
            let bigram = format!("{} {}", word_lc, normalize_token(words[i + 1].2));
            triggers.iter().any(|t| t.contains(' ') && *t == bigram)
        } else {
            false
        };

        if single_match || bigram_match {
            // Determine trigger span.
            let (trigger_start, trigger_end, trigger_token) = if bigram_match && !single_match {
                let (_, end_2, _) = words[i + 1];
                (
                    word_start,
                    end_2,
                    format!("{} {}", word_lc, normalize_token(words[i + 1].2)),
                )
            } else {
                (word_start, word_end, word_lc.clone())
            };

            // Walk forward, collecting up to MAX_SCOPE_WORDS or until a
            // terminator/sentence-end.
            let scope_start = trigger_end;
            let mut scope_end = trigger_end;
            let consumed_for_bigram = if bigram_match && !single_match { 2 } else { 1 };
            let scope_word_start_idx = i + consumed_for_bigram;

            for &(ws, we, w) in words
                .iter()
                .skip(scope_word_start_idx)
                .take(MAX_SCOPE_WORDS)
            {
                // Sentence-end punctuation between previous scope_end and current word.
                if has_sentence_break(text, scope_end, ws) {
                    break;
                }
                // Clause-boundary token.
                if terminators.contains(&w) {
                    break;
                }
                scope_end = we;
            }

            // Map byte offsets back into the original (case-preserving) text.
            let scope_text = text
                .get(scope_start..scope_end)
                .unwrap_or("")
                .trim()
                .to_string();

            scopes.push(NegationScope {
                trigger_start,
                trigger_end,
                scope_start,
                scope_end,
                trigger: trigger_token,
                scope_text,
            });

            i += consumed_for_bigram;
            continue;
        }

        i += 1;
    }

    scopes
}

fn triggers_for(language: Language) -> &'static [&'static str] {
    static EN: OnceLock<Vec<&'static str>> = OnceLock::new();
    static PL: OnceLock<Vec<&'static str>> = OnceLock::new();
    static ALL: OnceLock<Vec<&'static str>> = OnceLock::new();

    let en = EN.get_or_init(|| {
        vec![
            "not",
            "no",
            "never",
            "neither",
            "nor",
            "without",
            "don't",
            "doesn't",
            "didn't",
            "won't",
            "wouldn't",
            "can't",
            "cannot",
            "couldn't",
            "shouldn't",
            "isn't",
            "aren't",
            "wasn't",
            "weren't",
            "hasn't",
            "haven't",
            "hadn't",
            "do not",
            "does not",
            "did not",
            "will not",
            "would not",
            "can not",
            "could not",
            "should not",
            "is not",
            "are not",
            "was not",
            "were not",
            "has not",
            "have not",
            "had not",
            "no longer",
        ]
    });
    let pl = PL.get_or_init(|| {
        vec![
            // Polish negation pivots on `nie` + verb / `nie` + adjective.
            "nie", "nigdy", "żaden", "żadna", "żadne", "żadni", "żadnych", "bez", "ani",
        ]
    });
    let all = ALL.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(en.iter().copied());
        v.extend(pl.iter().copied());
        v
    });

    match language {
        Language::English => en.as_slice(),
        Language::Polish => pl.as_slice(),
        Language::Unknown => all.as_slice(),
    }
}

fn terminators_for(language: Language) -> &'static [&'static str] {
    static EN: OnceLock<Vec<&'static str>> = OnceLock::new();
    static PL: OnceLock<Vec<&'static str>> = OnceLock::new();
    static ALL: OnceLock<Vec<&'static str>> = OnceLock::new();

    let en = EN.get_or_init(|| {
        vec![
            "but",
            "however",
            "although",
            "though",
            "yet",
            "still",
            "nevertheless",
            "nonetheless",
            "except",
            "besides",
        ]
    });
    let pl = PL.get_or_init(|| {
        vec![
            "ale",
            "jednak",
            "lecz",
            "chociaż",
            "choć",
            "pomimo",
            "natomiast",
            "tylko",
            "zaś",
            "wprawdzie",
        ]
    });
    let all = ALL.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(en.iter().copied());
        v.extend(pl.iter().copied());
        v
    });

    match language {
        Language::English => en.as_slice(),
        Language::Polish => pl.as_slice(),
        Language::Unknown => all.as_slice(),
    }
}

/// Lowercase a single token and fold the typographic apostrophe to ASCII, so trigger
/// tables written with `'` keep matching text typed with U+2019.
fn normalize_token(token: &str) -> String {
    token.to_lowercase().replace('\u{2019}', "'")
}

/// Tokenize the **original** text, returning `(byte_start, byte_end, word)` per
/// alphanumeric run (ASCII and typographic apostrophes count as word characters).
/// Punctuation is dropped, and the offsets index `text` — never a lowercased copy of
/// it, whose byte lengths can differ.
fn tokenize_with_offsets(text: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;

    for (i, c) in text.char_indices() {
        if c.is_alphanumeric() || c == '\'' || c == '\u{2019}' {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            out.push((s, i, &text[s..i]));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len(), &text[s..]));
    }
    out
}

/// Returns true if there's a sentence-ending punctuation between the two
/// byte offsets.
fn has_sentence_break(text: &str, from: usize, to: usize) -> bool {
    if from >= to || to > text.len() {
        return false;
    }
    text[from..to]
        .chars()
        .any(|c| matches!(c, '.' | '!' | '?' | ';'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_scope(scope: &NegationScope, expected_trigger: &str, expected_scope_contains: &str) {
        assert_eq!(scope.trigger, expected_trigger, "trigger mismatch");
        assert!(
            scope.scope_text.contains(expected_scope_contains),
            "scope_text {:?} should contain {:?}",
            scope.scope_text,
            expected_scope_contains
        );
    }

    #[test]
    fn empty_text_no_scopes() {
        assert!(find_negation_scopes("", Language::English).is_empty());
        assert!(find_negation_scopes("   ", Language::English).is_empty());
    }

    #[test]
    fn text_without_negation_no_scopes() {
        assert!(
            find_negation_scopes("This is a regular fact about cats.", Language::English)
                .is_empty()
        );
    }

    #[test]
    fn english_simple_not() {
        let scopes = find_negation_scopes("I do not like broccoli.", Language::English);
        assert_eq!(scopes.len(), 1);
        // "do not" is detected as a bigram; "do" alone would also fire but the
        // bigram takes precedence by ordering.
        assert!(scopes[0].trigger.contains("not"));
        assert!(scopes[0].scope_text.contains("broccoli"));
    }

    #[test]
    fn english_dont_contraction() {
        let scopes = find_negation_scopes("Don't deploy on Friday.", Language::English);
        assert_eq!(scopes.len(), 1);
        assert_scope(&scopes[0], "don't", "deploy");
    }

    #[test]
    fn scope_text_survives_length_changing_lowercase() {
        // Regression: offsets were computed on `text.to_lowercase()` and then used to
        // slice `text`, so any character that changes byte length when lowercased made
        // `text.get(scope_start..scope_end)` return "" — the negation signal vanished
        // and the contradiction detector scored the pair as agreeing.
        // U+212A (3 bytes) lowercases to `k` (1 byte); U+0130 lowercases to two chars.
        let scopes = find_negation_scopes(
            "The sensor reports 300 \u{212A} and does not use the legacy protocol.",
            Language::English,
        );
        assert_eq!(scopes.len(), 1, "negation trigger must still be found");
        assert!(
            !scopes[0].scope_text.is_empty(),
            "scope_text must not silently become empty"
        );
        assert!(scopes[0].scope_text.contains("use the legacy protocol"));

        let scopes = find_negation_scopes(
            "\u{0130}stanbul rollout is not approved for production yet.",
            Language::English,
        );
        assert_eq!(scopes.len(), 1);
        assert!(scopes[0].scope_text.contains("approved for production"));
    }

    #[test]
    fn typographic_apostrophe_matches_ascii_trigger() {
        // `don't` typed with U+2019 must match the same trigger as the ASCII spelling.
        let ascii = find_negation_scopes("Don't use the legacy API.", Language::English);
        let typographic =
            find_negation_scopes("Don\u{2019}t use the legacy API.", Language::English);
        assert_eq!(ascii.len(), 1);
        assert_eq!(
            typographic.len(),
            1,
            "U+2019 apostrophe must not hide the trigger"
        );
        assert_eq!(typographic[0].trigger, ascii[0].trigger);
        assert!(typographic[0].scope_text.contains("legacy API"));
    }

    #[test]
    fn polish_nie_scopes_following_words() {
        let scopes =
            find_negation_scopes("Nie używaj tej biblioteki w produkcji.", Language::Polish);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].trigger, "nie");
        assert!(scopes[0].scope_text.contains("używaj"));
    }

    #[test]
    fn scope_ends_at_terminator_but() {
        let scopes = find_negation_scopes(
            "Do not use feature X but feel free to use feature Y.",
            Language::English,
        );
        assert_eq!(scopes.len(), 1);
        // Scope should include "use feature X" but stop before "but".
        let scope = &scopes[0];
        assert!(scope.scope_text.contains("feature X"));
        assert!(!scope.scope_text.contains("feature Y"));
    }

    #[test]
    fn scope_ends_at_terminator_jednak() {
        let scopes =
            find_negation_scopes("Nie używaj X jednak Y jest w porządku.", Language::Polish);
        assert_eq!(scopes.len(), 1);
        let scope = &scopes[0];
        assert!(scope.scope_text.contains("X"));
        assert!(!scope.scope_text.contains("Y"));
    }

    #[test]
    fn scope_ends_at_sentence_break() {
        let scopes = find_negation_scopes("Do not deploy. Always test first.", Language::English);
        assert_eq!(scopes.len(), 1);
        let scope = &scopes[0];
        assert!(scope.scope_text.contains("deploy"));
        assert!(!scope.scope_text.contains("test"));
    }

    #[test]
    fn multiple_scopes_in_one_text() {
        let scopes = find_negation_scopes("Don't use X. Don't use Y either.", Language::English);
        assert_eq!(scopes.len(), 2);
        assert!(scopes[0].scope_text.contains("X"));
        assert!(scopes[1].scope_text.contains("Y"));
    }

    #[test]
    fn max_scope_words_limit() {
        // Long sentence — scope must cap at MAX_SCOPE_WORDS words.
        let text = "Not one two three four five six seven eight nine ten eleven twelve.";
        let scopes = find_negation_scopes(text, Language::English);
        assert_eq!(scopes.len(), 1);
        // The scope holds at most MAX_SCOPE_WORDS words.
        let word_count = scopes[0].scope_text.split_whitespace().count();
        assert!(
            word_count <= MAX_SCOPE_WORDS,
            "scope had {word_count} words, expected at most {MAX_SCOPE_WORDS}"
        );
    }

    #[test]
    fn polish_nigdy_trigger() {
        let scopes = find_negation_scopes(
            "Nigdy nie commituj kluczy API do repozytorium.",
            Language::Polish,
        );
        // Both `nigdy` and `nie` are triggers — should detect at least one.
        assert!(!scopes.is_empty(), "expected at least one scope");
        assert!(scopes.iter().any(|s| s.scope_text.contains("commituj")));
    }

    #[test]
    fn unknown_language_uses_combined_lexicons() {
        let scopes = find_negation_scopes("not used; nie używać.", Language::Unknown);
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn byte_offsets_are_valid_char_boundaries() {
        // Polish text with multi-byte characters — offsets must be valid.
        let text = "Nie używaj tej źle skonfigurowanej biblioteki.";
        let scopes = find_negation_scopes(text, Language::Polish);
        assert_eq!(scopes.len(), 1);
        let s = &scopes[0];
        assert!(text.is_char_boundary(s.trigger_start));
        assert!(text.is_char_boundary(s.trigger_end));
        assert!(text.is_char_boundary(s.scope_start));
        assert!(text.is_char_boundary(s.scope_end));
    }

    #[test]
    fn deterministic_for_same_input() {
        let text = "Do not panic. Never deploy on Friday.";
        let first = find_negation_scopes(text, Language::English);
        for _ in 0..5 {
            assert_eq!(find_negation_scopes(text, Language::English), first);
        }
    }

    #[test]
    fn trigger_at_end_of_text_no_panic() {
        // Trigger with no following content — should produce a scope with
        // empty/short scope_text, not panic.
        let scopes = find_negation_scopes("don't", Language::English);
        assert_eq!(scopes.len(), 1);
        assert!(scopes[0].scope_text.is_empty());
    }

    #[test]
    fn punctuation_around_trigger_handled() {
        let scopes = find_negation_scopes("(Don't do X.) See also Y.", Language::English);
        assert_eq!(scopes.len(), 1);
        assert!(scopes[0].scope_text.contains("X"));
    }

    #[test]
    fn long_realistic_polish_memory() {
        let text = "Naprawiono błąd: nie używaj starego API do pobierania użytkowników, \
                    ponieważ zwraca nieaktualne dane. Zamiast tego użyj nowego endpointu.";
        let scopes = find_negation_scopes(text, Language::Polish);
        // Should find `nie` scoping over "używaj starego API".
        assert!(!scopes.is_empty());
        assert!(
            scopes
                .iter()
                .any(|s| s.scope_text.contains("starego") || s.scope_text.contains("używaj"))
        );
    }
}

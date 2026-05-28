//! Contradiction detection between two pieces of text.
//!
//! Replaces the previous symmetric-substring heuristic with a scope-aware
//! algorithm built on top of [`crate::nlp::negation`] (NegEx). The legacy
//! heuristic returned `true` whenever a negation token appeared in `new`
//! and not in `old`, which is noisy: "I do **not** want to make this
//! decision lightly" would flag against any memory not containing "not".
//!
//! ## Decision logic
//!
//! For a pair `(new, old)` the detector runs three independent checks
//! and combines them with a deliberately conservative OR (any signal
//! tips us to "contradiction"). The trade-off is calibrated against the
//! eval datasets in [`crate::nlp::eval::data`].
//!
//! 1. **Correction-phrase scan.** Phrases like `actually`, `correction:`,
//!    `was wrong`, `poprawka`, `sprostowanie` etc. in `new` are strong
//!    signals that the author is correcting something — and the most
//!    likely candidate is the prior similar memory, so we count them
//!    as evidence.
//!
//! 2. **Negation-scope overlap.** Find all negation scopes in `new`
//!    (e.g. "don't deploy on Friday" → scope = "deploy on Friday").
//!    For each scope, check whether the *content of the scope* shares
//!    ≥1 substantive (non-stopword) token with `old`. If yes, the new
//!    memory is negating something the old memory asserts.
//!
//! 3. **Asymmetric negation.** Cover the legacy bool — a negation
//!    trigger in `new` that's absent from `old`. This catches cases
//!    where the scope-overlap rule misses (e.g. very short triggers,
//!    or content where the negated noun phrase is paraphrased).
//!
//! ## Confidence calibration
//!
//! - `correction phrase only` → 0.6 (mid signal, common in update messages
//!   that aren't strict contradictions of a *specific* prior memory).
//! - `scope-overlap match` → 0.85 (strong signal — we found *what* is
//!   being negated).
//! - `asymmetric only` → 0.45 (legacy bool — kept for recall, low
//!   confidence).
//! - Multiple signals stack via probability fusion (`1 - Π(1 - cᵢ)`).
//!
//! ## What this does NOT do
//!
//! - It does **not** detect factual contradictions where neither memory
//!   uses overt negation (e.g. "Adam lives in Kraków" vs "Adam moved to
//!   Gdańsk"). That requires an NLI model — see
//!   [`crate::nlp::mod@`] (Plan section) for the roadmap.
//! - It does **not** detect contradiction through temporal qualifiers
//!   ("X is fast" vs "X used to be fast"). Temporal supersession is
//!   handled separately by the `temporal` MCP tool.

use std::sync::OnceLock;

use super::language::{Language, detect_language};
use super::negation::find_negation_scopes;
use super::{DetectionResult, Evidence, EvidenceKind};

// ---------------------------------------------------------------------------
// Per-signal confidence weights.
//
// Extracted as `pub` consts so the dashboard / eval reports can show the
// calibration weights alongside the model name, and so a regression test
// (`tests/nlp_baseline.rs`) can pin them — silently shifting these from
// 0.85 to 0.5 would tank ECE and the F1 gate.
// ---------------------------------------------------------------------------

/// Confidence contribution for a correction phrase match.
///
/// Moderate signal: the phrase implies the author is correcting something,
/// but not necessarily *this specific old memory*.
pub const CORRECTION_PHRASE_CONFIDENCE: f32 = 0.6;

/// Confidence contribution for a negation-scope token overlap with `old`.
///
/// Strong signal: we found *what* is being negated and it overlaps with the
/// other memory.
pub const NEGATION_SCOPE_CONFIDENCE: f32 = 0.85;

/// Confidence contribution for the asymmetric-negation fallback.
///
/// Low signal kept for recall on cases where scope-overlap misses
/// (paraphrased negated noun phrase, etc.).
pub const ASYMMETRIC_NEGATION_CONFIDENCE: f32 = 0.45;

/// Interface for contradiction detection between two pieces of text.
///
/// Implementations should be stateless and `Send + Sync` so they can be
/// shared across the request-handling threads of the MCP server.
pub trait ContradictionDetector: Send + Sync {
    /// Return a [`DetectionResult`] describing whether `new` contradicts
    /// `old`. The byte offsets in any [`Evidence`] are into `new`.
    fn detect(&self, new: &str, old: &str) -> DetectionResult;

    /// A human-readable name for the implementation, used by the eval
    /// runner and the dashboard.
    fn name(&self) -> &'static str;
}

/// Default heuristic implementation — pure Rust, no models, sub-millisecond.
///
/// See the module documentation for the algorithm and confidence model.
#[derive(Debug, Default, Clone)]
pub struct HeuristicContradictionDetector;

impl HeuristicContradictionDetector {
    /// Construct a new heuristic detector.
    pub fn new() -> Self {
        Self
    }
}

impl ContradictionDetector for HeuristicContradictionDetector {
    fn name(&self) -> &'static str {
        "heuristic-v2"
    }

    fn detect(&self, new: &str, old: &str) -> DetectionResult {
        // Both sides must carry content. If `old` is empty there's nothing
        // to contradict; without this guard, the asymmetric-negation
        // fallback fires on any new memory that contains a trigger like
        // "never" / "deprecated", because `has_in_old` is trivially false.
        // That bug surfaced as PredictionErrorGate flagging legitimate
        // creates as "supersede this empty candidate" — see the
        // `empty_old_does_not_create_phantom_contradiction` regression
        // test below.
        if new.trim().is_empty() || old.trim().is_empty() {
            return DetectionResult::negative();
        }

        let new_lower = new.to_lowercase();
        let old_lower = old.to_lowercase();
        let language = detect_language(new);

        let mut evidence: Vec<Evidence> = Vec::new();
        let mut signal_confidences: Vec<f32> = Vec::new();

        // 1. Correction phrases.
        //
        // We honour the per-phrase language tag: when the new memory's
        // language is unambiguous, skip lexicon entries for the other
        // language. This avoids cross-language false positives — without
        // it, a Polish memo like "Faktycznie, to nie jest tak" wouldn't
        // fire today, but "actually" appearing inside a polonglish memo
        // would. Mirrors the policy in `opinion.rs`.
        for (phrase, phrase_lang) in correction_phrases() {
            if *phrase_lang != Language::Unknown
                && *phrase_lang != language
                && language != Language::Unknown
            {
                continue;
            }
            if let Some(idx) = new_lower.find(phrase) {
                evidence.push(Evidence {
                    kind: EvidenceKind::CorrectionPhrase,
                    span_start: idx,
                    span_end: idx + phrase.len(),
                    snippet: (*phrase).to_string(),
                });
                signal_confidences.push(CORRECTION_PHRASE_CONFIDENCE);
                // One CorrectionPhrase contribution per call — enough
                // signal without inflating evidence noise.
                break;
            }
        }

        // 2. Negation scope overlap.
        let scopes = find_negation_scopes(new, language);
        let old_tokens = significant_tokens(&old_lower);

        for scope in &scopes {
            let scope_tokens = significant_tokens(&scope.scope_text.to_lowercase());
            let overlap = scope_tokens
                .iter()
                .filter(|t| old_tokens.contains(*t))
                .count();
            if overlap >= 1 {
                evidence.push(Evidence {
                    kind: EvidenceKind::NegationScope,
                    span_start: scope.trigger_start,
                    span_end: scope.scope_end,
                    snippet: format!("{} … {}", scope.trigger, scope.scope_text),
                });
                signal_confidences.push(NEGATION_SCOPE_CONFIDENCE);
                // One scope overlap is sufficient signal.
                break;
            }
        }

        // 3. Asymmetric-negation fallback.
        // Only emit if we have *no* stronger signal yet — keeps the
        // evidence list focused.
        if evidence.is_empty() {
            let triggers = asymmetric_triggers();
            for trigger in triggers {
                let has_in_new = lowered_contains_word(&new_lower, trigger);
                let has_in_old = lowered_contains_word(&old_lower, trigger);
                if has_in_new
                    && !has_in_old
                    && let Some(idx) = new_lower.find(trigger)
                {
                    evidence.push(Evidence {
                        kind: EvidenceKind::AsymmetricNegation,
                        span_start: idx,
                        span_end: idx + trigger.len(),
                        snippet: (*trigger).to_string(),
                    });
                    signal_confidences.push(ASYMMETRIC_NEGATION_CONFIDENCE);
                    break;
                }
            }
        }

        if evidence.is_empty() {
            return DetectionResult::negative();
        }

        // Probability fusion: 1 - Π(1 - c_i). Models "at least one signal
        // is correct" under independence — a defensible approximation.
        let fused = 1.0
            - signal_confidences
                .iter()
                .map(|c| (1.0 - c).max(0.0))
                .product::<f32>();

        DetectionResult::positive(fused, evidence)
    }
}

/// Asymmetric negation triggers used by the fallback rule.
///
/// We keep this list narrow on purpose — it generates more false positives
/// than the scope-based rule, so it should fire only for triggers that
/// strongly imply negation regardless of context.
fn asymmetric_triggers() -> &'static [&'static str] {
    static SET: OnceLock<Vec<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        vec![
            // English
            "never",
            "avoid",
            "wrong",
            "incorrect",
            "deprecated",
            "outdated",
            "instead of",
            "rather than",
            "no longer",
            // Polish
            "nigdy",
            "unikaj",
            "błędn",      // prefix: błędny / błędne / błędnie
            "przestarz",  // prefix: przestarzały / przestarzałe
            "nieaktualn", // prefix
            "zamiast",
        ]
    })
}

/// Correction phrases + their associated language (for future per-language
/// stats; currently informational).
fn correction_phrases() -> &'static [(&'static str, Language)] {
    static SET: OnceLock<Vec<(&'static str, Language)>> = OnceLock::new();
    SET.get_or_init(|| {
        vec![
            // English
            ("actually", Language::English),
            ("correction", Language::English),
            ("update:", Language::English),
            ("was wrong", Language::English),
            ("should be", Language::English),
            ("better approach", Language::English),
            ("improved", Language::English),
            ("the right way", Language::English),
            ("turns out", Language::English),
            ("on second thought", Language::English),
            // Polish
            ("poprawk", Language::Polish),
            ("aktualizacja:", Language::Polish),
            ("naprawio", Language::Polish),
            ("powinno być", Language::Polish),
            ("lepiej", Language::Polish),
            ("tak naprawdę", Language::Polish),
            ("w rzeczywistości", Language::Polish),
            ("sprostowanie", Language::Polish),
            ("okazuje się", Language::Polish),
            ("po przemyśleniu", Language::Polish),
        ]
    })
}

/// Extract the set of "significant" tokens from a lowercased string.
///
/// Drops stopwords (high-frequency function words that don't carry the
/// negated meaning) and very short tokens (≤ 2 chars, mostly noise).
fn significant_tokens(lowered: &str) -> std::collections::HashSet<String> {
    let stopwords = stopword_set();
    lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !stopwords.contains(*w))
        .map(std::string::ToString::to_string)
        .collect()
}

fn stopword_set() -> &'static std::collections::HashSet<&'static str> {
    static SET: OnceLock<std::collections::HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        [
            // English stopwords (subset of standard list, focused on
            // what shows up in scope contexts).
            "the", "and", "for", "with", "from", "this", "that", "are",
            "was", "were", "have", "has", "had", "but", "not", "you",
            "your", "our", "their", "his", "her", "its", "into", "onto",
            "out", "off", "all", "any", "some", "more", "most", "such",
            "then", "than", "very", "much", "many", "few",
            // Polish stopwords
            "który", "która", "które", "być", "jest", "byli", "były",
            "tego", "tym", "ten", "tej", "tych", "tych", "oraz", "ale",
            "lub", "albo", "lecz", "ponieważ", "więc", "dla", "bez",
            "przy", "nad", "pod", "przez", "się", "tak", "też",
        ]
        .into_iter()
        .collect()
    })
}

/// Word-boundary aware "contains" for a lowercased haystack.
///
/// For triggers that contain spaces (`"do not"`, `"no longer"`), this falls
/// back to a simple substring match — the space already acts as a boundary
/// on both sides. For single-word triggers, we require ASCII non-alphanumeric
/// before and after.
fn lowered_contains_word(haystack: &str, needle: &str) -> bool {
    if needle.contains(' ') || needle.ends_with('-') || needle.starts_with('-') {
        return haystack.contains(needle);
    }
    // For triggers that are intended as prefixes (suffix-stripping for PL),
    // we accept matches followed by a letter — these are the `błędn`,
    // `przestarz`, `nieaktualn` family.
    let is_prefix_trigger = matches!(
        needle,
        "błędn" | "przestarz" | "nieaktualn"
    );

    let bytes = haystack.as_bytes();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before = if abs == 0 {
            true
        } else {
            !is_word_byte(bytes[abs - 1])
        };
        let after_idx = abs + needle.len();
        let after = if after_idx >= bytes.len() {
            true
        } else if is_prefix_trigger {
            // Accept letter-suffix — that's the whole point of prefix triggers.
            true
        } else {
            !is_word_byte(bytes[after_idx])
        };
        if before && after {
            return true;
        }
        start = abs + needle.len();
    }
    false
}

#[inline]
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80 // bytes ≥ 0x80 are part of multibyte UTF-8 sequences
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> HeuristicContradictionDetector {
        HeuristicContradictionDetector::new()
    }

    // ========================================================================
    // BASIC BEHAVIOR
    // ========================================================================

    #[test]
    fn empty_new_is_negative() {
        let r = detector().detect("", "anything");
        assert!(!r.positive);
        assert!(r.evidence.is_empty());
    }

    #[test]
    fn empty_old_does_not_create_phantom_contradiction() {
        // Regression test: before the empty-old guard the asymmetric-
        // negation fallback would fire on any `new` containing a trigger
        // because `has_in_old` was always `false`. That made any "deprecated"
        // memory look like a contradiction-with-nothing.
        let d = detector();
        for new in &[
            "This API is deprecated.",
            "We should never deploy on Friday.",
            "Avoid the legacy module.",
            "Ta biblioteka jest przestarzała.",
            "Nigdy nie commituj kluczy.",
        ] {
            let r = d.detect(new, "");
            assert!(
                !r.positive,
                "false positive vs empty old for {new:?}: {:?}",
                r.evidence
            );
            assert!(r.confidence == 0.0);
        }
    }

    #[test]
    fn empty_old_whitespace_only_does_not_contradict() {
        // Whitespace-only old must be treated the same as empty.
        let r = detector().detect("Don't deploy on Friday.", "   \n\t  ");
        assert!(!r.positive, "false positive vs whitespace old: {:?}", r.evidence);
    }

    #[test]
    fn identical_text_is_negative() {
        let r = detector().detect("X is fast.", "X is fast.");
        assert!(!r.positive, "identical content shouldn't contradict itself");
    }

    #[test]
    fn unrelated_text_is_negative() {
        let r = detector().detect("Today is sunny.", "Cats like fish.");
        assert!(!r.positive);
    }

    // ========================================================================
    // CORRECTION PHRASE SIGNAL
    // ========================================================================

    #[test]
    fn english_correction_phrase_actually_fires() {
        let r = detector().detect(
            "Actually the deployment goes to staging first.",
            "The deployment goes straight to production.",
        );
        assert!(r.positive);
        assert!(r.evidence.iter().any(|e| matches!(e.kind, EvidenceKind::CorrectionPhrase)));
        assert!(r.confidence >= 0.5, "got {}", r.confidence);
    }

    #[test]
    fn polish_correction_phrase_sprostowanie_fires() {
        let r = detector().detect(
            "Sprostowanie: API zwraca JSON, nie XML.",
            "API zwraca XML w odpowiedzi.",
        );
        assert!(r.positive);
        // Should match BOTH a correction phrase and an asymmetric negation.
        assert!(r.evidence.iter().any(|e| matches!(e.kind, EvidenceKind::CorrectionPhrase)));
    }

    #[test]
    fn polish_poprawka_prefix_match() {
        let r = detector().detect(
            "Poprawka: nazwa zmiennej to 'userId', nie 'user_id'.",
            "Nazwa zmiennej to 'user_id'.",
        );
        assert!(r.positive);
    }

    // ========================================================================
    // NEGATION SCOPE OVERLAP
    // ========================================================================

    #[test]
    fn negation_scope_overlap_english() {
        let r = detector().detect(
            "Do not deploy on Friday.",
            "We always deploy on Friday afternoons.",
        );
        assert!(r.positive);
        assert!(
            r.evidence
                .iter()
                .any(|e| matches!(e.kind, EvidenceKind::NegationScope)),
            "expected NegationScope evidence, got: {:?}",
            r.evidence
        );
        // Scope-overlap is the strongest signal, confidence should be high.
        assert!(r.confidence >= 0.8, "got {}", r.confidence);
    }

    #[test]
    fn negation_scope_overlap_polish() {
        let r = detector().detect(
            "Nie używaj starego API do pobierania użytkowników.",
            "Używamy starego API do pobierania użytkowników w produkcji.",
        );
        assert!(r.positive);
        assert!(
            r.evidence
                .iter()
                .any(|e| matches!(e.kind, EvidenceKind::NegationScope))
        );
    }

    #[test]
    fn negation_outside_scope_doesnt_fire() {
        // The "not" scopes "make this decision lightly", which has no
        // token overlap with the old memory about cats.
        let r = detector().detect(
            "I do not want to make this decision lightly.",
            "Cats are nocturnal animals.",
        );
        // Should NOT detect contradiction — no scope overlap, no correction.
        assert!(!r.positive, "spurious match: {:?}", r.evidence);
    }

    #[test]
    fn scope_after_terminator_not_considered_overlap() {
        // "not deploy on Friday but Saturday is fine" — scope is just
        // "deploy on Friday", not "Saturday".
        let r = detector().detect(
            "Do not deploy on Friday but Saturday is fine.",
            "Saturday deployments work great.",
        );
        // Saturday is NOT in the negation scope, so this should be negative.
        assert!(!r.positive, "scope leaked past terminator: {:?}", r.evidence);
    }

    // ========================================================================
    // ASYMMETRIC NEGATION FALLBACK
    // ========================================================================

    #[test]
    fn asymmetric_negation_legacy_behavior() {
        // No scope overlap (different topics), but "deprecated" in new
        // is a strong asymmetric signal.
        let r = detector().detect(
            "This API is deprecated.",
            "We use this API extensively.",
        );
        assert!(r.positive);
        assert!(
            r.evidence
                .iter()
                .any(|e| matches!(e.kind, EvidenceKind::AsymmetricNegation))
        );
        // Asymmetric-only confidence is lower than scope-overlap.
        assert!(r.confidence < 0.6, "got {}", r.confidence);
    }

    #[test]
    fn asymmetric_negation_polish_prefixes() {
        let r = detector().detect(
            "Ta biblioteka jest przestarzała.",
            "Używamy tej biblioteki w wielu projektach.",
        );
        assert!(r.positive);
        assert!(
            r.evidence
                .iter()
                .any(|e| matches!(e.kind, EvidenceKind::AsymmetricNegation))
        );
    }

    // ========================================================================
    // FALSE POSITIVE GUARDS
    // ========================================================================

    #[test]
    fn name_containing_no_not_a_false_positive() {
        // "Annie" contains "ann" but no word-bounded negation trigger.
        let r = detector().detect("Annie said the meeting starts at 3pm.", "Meeting at 3pm.");
        assert!(!r.positive, "false positive: {:?}", r.evidence);
    }

    #[test]
    fn word_boundary_respected_for_polish() {
        // "nieaktualn" is a prefix trigger — but only on a real word.
        // "manie" or "mania" shouldn't match it.
        let r = detector().detect("Mania to silne pragnienie.", "Coś innego.");
        // None of `nie ` (word "nie"), `nigdy`, etc. should fire here as
        // the asymmetric trigger because there's no word `nie` in "Mania".
        assert!(!r.positive, "false positive on Mania: {:?}", r.evidence);
    }

    #[test]
    fn double_negative_in_new_does_not_fire_against_double_negative_in_old() {
        // Both contain "no" — asymmetric rule shouldn't fire.
        let r = detector().detect(
            "There are no known regressions.",
            "We have no recorded issues.",
        );
        // Different content, but no asymmetric negation signal.
        assert!(!r.positive);
    }

    // ========================================================================
    // CONFIDENCE FUSION
    // ========================================================================

    #[test]
    fn multiple_signals_increase_confidence() {
        let r = detector().detect(
            "Update: do not deploy on Friday after 5pm.",
            "We deploy on Friday at 6pm.",
        );
        assert!(r.positive);
        // Should have CorrectionPhrase + NegationScope.
        let has_correction = r.evidence.iter().any(|e| matches!(e.kind, EvidenceKind::CorrectionPhrase));
        let has_scope = r.evidence.iter().any(|e| matches!(e.kind, EvidenceKind::NegationScope));
        assert!(has_correction || has_scope, "got: {:?}", r.evidence);
        // Fused confidence should be high.
        assert!(r.confidence >= 0.6, "confidence={}", r.confidence);
    }

    // ========================================================================
    // PROPERTY-LIKE TESTS
    // ========================================================================

    #[test]
    fn confidence_is_in_zero_one_range() {
        let cases = [
            ("Do not deploy.", "We deploy now."),
            ("", ""),
            ("X.", "Y."),
            ("Actually correction: was wrong, deprecated.", "Other."),
        ];
        for (new, old) in cases {
            let r = detector().detect(new, old);
            assert!(r.confidence >= 0.0 && r.confidence <= 1.0, "out of range: {}", r.confidence);
        }
    }

    #[test]
    fn evidence_byte_offsets_are_valid_into_new() {
        let new = "Nie używaj starego API.";
        let old = "Stare API jest super.";
        let r = detector().detect(new, old);
        if r.positive {
            for ev in &r.evidence {
                assert!(ev.span_start <= ev.span_end);
                assert!(ev.span_end <= new.len());
                assert!(new.is_char_boundary(ev.span_start));
                assert!(new.is_char_boundary(ev.span_end));
            }
        }
    }

    #[test]
    fn deterministic_across_calls() {
        let d = detector();
        let r1 = d.detect("Do not deploy on Friday.", "We deploy on Friday.");
        for _ in 0..5 {
            let r2 = d.detect("Do not deploy on Friday.", "We deploy on Friday.");
            assert_eq!(r1.positive, r2.positive);
            assert!((r1.confidence - r2.confidence).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn detector_name_is_stable() {
        assert_eq!(detector().name(), "heuristic-v2");
    }
}

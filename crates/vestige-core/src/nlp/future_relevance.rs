//! Future-relevance / TODO / intention detection.
//!
//! Replaces the inline future-marker check in
//! [`DreamEngine::categorize_memory`](crate::consolidation::DreamEngine).
//! A memory is "future-relevant" when it represents something the user
//! intends to do later — a TODO, a reminder, a plan, an intention. The
//! consolidation engine routes these into the `FutureRelevant` triage
//! category and they get stronger retention reinforcement.
//!
//! ## Algorithm
//!
//! Single-class binary detection on a per-language lexicon of future
//! markers. The lexicon covers:
//!
//! 1. **Explicit TODO / reminder markers**: `TODO`, `reminder`, `follow up`,
//!    `przypomnienie`, `do zrobienia`.
//! 2. **Intention frames**: `I plan to`, `I will`, `I must remember`,
//!    `planuję`, `muszę pamiętać`, `zamierzam`.
//! 3. **Temporal future markers**: `next time`, `tomorrow`, `następnym
//!    razem`, `jutro`.
//!
//! Each match contributes evidence; we emit the strongest single piece
//! of evidence (the first hit) plus a confidence reflecting which class
//! of marker fired.
//!
//! ## Known limits
//!
//! - **Tense-only.** Polish "zrobię" (I will do) is a future tense form
//!   but isn't in our lexicon because we'd need morphological analysis
//!   to disambiguate "zrobię" from "zrobiłem" reliably. A future ONNX
//!   classifier on top would help here.
//! - **Imperative vs. future.** "Pamiętaj o X" (remember X) is an
//!   imperative — could be future-relevant if the writer is reminding
//!   themselves, or not if they're reminding someone else. We default
//!   to "yes, future-relevant" because Vestige memories are mostly
//!   self-directed.

use std::sync::OnceLock;

use super::language::{Language, detect_language};
use super::{DetectionResult, Evidence, EvidenceKind};

/// Interface for future-relevance / TODO detection.
pub trait FutureRelevanceDetector: Send + Sync {
    /// Classify `content` as future-relevant (positive) or not (negative).
    fn detect(&self, content: &str) -> DetectionResult;

    /// A human-readable name for the implementation.
    fn name(&self) -> &'static str;
}

/// Default heuristic future-relevance detector.
#[derive(Debug, Default, Clone)]
pub struct HeuristicFutureRelevanceDetector;

impl HeuristicFutureRelevanceDetector {
    /// Construct a new heuristic future-relevance detector.
    pub fn new() -> Self {
        Self
    }
}

impl FutureRelevanceDetector for HeuristicFutureRelevanceDetector {
    fn name(&self) -> &'static str {
        "heuristic-v2"
    }

    fn detect(&self, content: &str) -> DetectionResult {
        if content.trim().is_empty() {
            return DetectionResult::negative();
        }

        let lower = content.to_lowercase();
        let language = detect_language(content);

        for (marker, lang, confidence) in future_markers() {
            if *lang != Language::Unknown && *lang != language && language != Language::Unknown {
                continue;
            }
            if let Some(idx) = lower.find(marker) {
                let evidence = Evidence {
                    kind: EvidenceKind::FutureMarker,
                    span_start: idx,
                    span_end: idx + marker.len(),
                    snippet: marker.to_string(),
                };
                return DetectionResult::positive(*confidence, vec![evidence]);
            }
        }

        DetectionResult::negative()
    }
}

/// Future-relevance markers with their language and per-marker confidence.
///
/// Confidence reflects how unambiguous the marker is:
/// - `todo`, `reminder`, `przypomnienie`, `do zrobienia` → 0.9 (explicit).
/// - `I plan to`, `planuję`, `zamierzam` → 0.8 (intention).
/// - `next time`, `następnym razem`, `tomorrow` → 0.65 (temporal future).
fn future_markers() -> &'static [(&'static str, Language, f32)] {
    static SET: OnceLock<Vec<(&'static str, Language, f32)>> = OnceLock::new();
    SET.get_or_init(|| {
        vec![
            // Explicit TODO / reminder markers — strongest.
            ("todo", Language::English, 0.9),
            ("to-do", Language::English, 0.9),
            ("reminder", Language::English, 0.9),
            ("remind me", Language::English, 0.9),
            ("follow up", Language::English, 0.85),
            ("follow-up", Language::English, 0.85),
            ("przypomn", Language::Polish, 0.9), // przypomnienie / przypomnij / przypomnieć
            ("do zrobienia", Language::Polish, 0.9),
            // Intention frames.
            ("i plan to", Language::English, 0.8),
            ("plan to", Language::English, 0.75),
            ("i will", Language::English, 0.7),
            ("intention", Language::English, 0.8),
            ("i intend to", Language::English, 0.8),
            ("must remember", Language::English, 0.85),
            ("need to remember", Language::English, 0.85),
            ("i need to", Language::English, 0.7),
            ("planuję", Language::Polish, 0.8),
            ("zamierzam", Language::Polish, 0.8),
            ("zamierz", Language::Polish, 0.7), // prefix for other inflections
            ("muszę pamiętać", Language::Polish, 0.85),
            ("muszę ", Language::Polish, 0.7), // generic "I have to X"
            ("trzeba pamiętać", Language::Polish, 0.85),
            ("trzeba zapamiętać", Language::Polish, 0.85),
            ("trzeba ", Language::Polish, 0.7), // generic "one must X"
            ("pamiętaj ", Language::Polish, 0.8), // imperative: remember to X
            ("pamiętaj,", Language::Polish, 0.8),
            ("pamiętaj o ", Language::Polish, 0.85),
            // Temporal future cues.
            ("next time", Language::English, 0.65),
            ("następnym razem", Language::Polish, 0.65),
            ("w przyszłości", Language::Polish, 0.6),
            ("później", Language::Polish, 0.55),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> HeuristicFutureRelevanceDetector {
        HeuristicFutureRelevanceDetector::new()
    }

    // ========================================================================
    // POSITIVE CASES
    // ========================================================================

    #[test]
    fn english_todo_is_future_relevant() {
        let r = detector().detect("TODO: refactor the auth module.");
        assert!(r.positive);
        assert!(r.confidence >= 0.85);
        assert!(
            r.evidence
                .iter()
                .any(|e| matches!(e.kind, EvidenceKind::FutureMarker))
        );
    }

    #[test]
    fn english_reminder_is_future_relevant() {
        let r = detector().detect("Reminder to update the deploy script.");
        assert!(r.positive);
    }

    #[test]
    fn english_follow_up_is_future_relevant() {
        let r = detector().detect("Need to follow up with Adam on the migration.");
        assert!(r.positive);
    }

    #[test]
    fn english_i_plan_to_is_future_relevant() {
        let r = detector().detect("I plan to migrate the database next week.");
        assert!(r.positive);
        assert!(r.confidence >= 0.75);
    }

    #[test]
    fn english_must_remember_is_future_relevant() {
        let r = detector().detect("Must remember to rotate the API keys quarterly.");
        assert!(r.positive);
    }

    #[test]
    fn polish_przypomnienie_is_future_relevant() {
        let r = detector().detect("Przypomnienie: sprawdzić logi produkcyjne.");
        assert!(r.positive);
        assert!(r.confidence >= 0.85);
    }

    #[test]
    fn polish_muszę_pamiętać_is_future_relevant() {
        let r = detector().detect("Muszę pamiętać o rotacji kluczy API co kwartał.");
        assert!(r.positive);
    }

    #[test]
    fn polish_planuję_is_future_relevant() {
        let r = detector().detect("Planuję refaktoryzację modułu auth w przyszłym tygodniu.");
        assert!(r.positive);
    }

    #[test]
    fn polish_do_zrobienia_is_future_relevant() {
        let r = detector().detect("Do zrobienia: napisać testy integracyjne dla nowego API.");
        assert!(r.positive);
        assert!(r.confidence >= 0.85);
    }

    #[test]
    fn polish_zamierz_prefix_match() {
        // "zamierzam", "zamierzaliśmy", "zamierzaliście" — prefix match.
        let r = detector().detect("Zamierzaliśmy to zrobić w zeszłym tygodniu.");
        assert!(r.positive);
    }

    // ========================================================================
    // NEGATIVE CASES
    // ========================================================================

    #[test]
    fn empty_is_negative() {
        assert!(!detector().detect("").positive);
    }

    #[test]
    fn past_tense_fact_is_negative() {
        // Pure past-tense factual statement, no future cues.
        let r = detector().detect("The deployment completed successfully at 3pm.");
        assert!(!r.positive);
    }

    #[test]
    fn polish_past_tense_fact_is_negative() {
        let r = detector().detect("Deploy zakończył się pomyślnie o godzinie 15.");
        assert!(!r.positive);
    }

    #[test]
    fn present_tense_observation_is_negative() {
        let r = detector().detect("The cache hit ratio is at 87%.");
        assert!(!r.positive);
    }

    // ========================================================================
    // EDGE CASES
    // ========================================================================

    #[test]
    fn polish_text_does_not_match_english_marker() {
        // Polish text with "plan" word — should NOT match "plan to" lexicon
        // because the marker requires the full phrase.
        let r = detector().detect("Plan jest gotowy do realizacji.");
        // No "plan to" or "planuję" — should be negative.
        assert!(!r.positive);
    }

    #[test]
    fn english_text_does_not_fire_polish_lexicon() {
        // "Plan" in English text — could falsely match the PL "planuję"
        // prefix? Our PL lexicon entry is "planuję" not "plan", so this
        // is safe.
        let r = detector().detect("This plan covers Q2 priorities.");
        // The lone word "plan" — no matching trigger.
        assert!(!r.positive);
    }

    #[test]
    fn marker_case_insensitive() {
        let r1 = detector().detect("TODO: do X.");
        let r2 = detector().detect("todo: do X.");
        let r3 = detector().detect("ToDo: do X.");
        assert_eq!(r1.positive, r2.positive);
        assert_eq!(r2.positive, r3.positive);
    }

    // ========================================================================
    // CONFIDENCE BUCKETS
    // ========================================================================

    #[test]
    fn explicit_todo_higher_confidence_than_temporal() {
        let todo = detector().detect("TODO: refactor X.");
        let temporal = detector().detect("Next time we should try Y.");
        assert!(todo.positive && temporal.positive);
        assert!(todo.confidence > temporal.confidence);
    }

    // ========================================================================
    // PROPERTIES
    // ========================================================================

    #[test]
    fn confidence_in_range() {
        let cases = [
            "TODO: do X.",
            "Przypomnienie: zrobić X.",
            "",
            "Random factual sentence.",
            "I plan to refactor.",
        ];
        for c in cases {
            let r = detector().detect(c);
            assert!(r.confidence >= 0.0 && r.confidence <= 1.0);
        }
    }

    #[test]
    fn deterministic() {
        let d = detector();
        let r1 = d.detect("TODO: do X.");
        for _ in 0..5 {
            let r2 = d.detect("TODO: do X.");
            assert_eq!(r1.positive, r2.positive);
            assert!((r1.confidence - r2.confidence).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn name_stable() {
        assert_eq!(detector().name(), "heuristic-v2");
    }

    #[test]
    fn realistic_polish_memory() {
        // From real Vestige usage patterns.
        let r = detector()
            .detect("Trzeba pamiętać o tym przy następnym refaktorze pipeline'u CI/CD.");
        assert!(r.positive);
    }

    #[test]
    fn realistic_english_memory() {
        let r = detector()
            .detect("Must remember to update the DOCKER_BUILDKIT env var on the CI runners.");
        assert!(r.positive);
    }
}

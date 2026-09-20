//! Natural language heuristics for the cognitive engine.
//!
//! This module replaces the ad-hoc `to_lowercase().contains(...)` checks that
//! used to live inline in [`PredictionErrorGate`](crate::advanced::prediction_error::PredictionErrorGate),
//! [`DreamEngine::categorize_memory`](crate::consolidation::DreamEngine), and the
//! `is_opinion` helper inside the confidence MCP tool. The new architecture has
//! three goals:
//!
//! 1. **Pluggability**. Detectors are traits, so the heuristic implementation
//!    we ship today can be swapped for an ONNX NLI cross-encoder, a fine-tuned
//!    multi-task classifier, or an LLM-as-judge sidecar without changing call
//!    sites.
//! 2. **Measurability**. The [`eval`] submodule holds hand-curated multilingual
//!    datasets and a runner that computes precision/recall/F1, calibration
//!    (Expected Calibration Error), and per-language breakdowns. **No detector
//!    upgrade ships without an eval delta** — that's the whole point.
//! 3. **Explainability**. Detectors return [`DetectionResult`] with structured
//!    [`Evidence`], not a bare `bool`. The cognitive engine and the dashboard
//!    can surface *why* a memory was flagged as contradictory/opinion/future-
//!    relevant, which is critical when the user disagrees with the gate's
//!    decision.
//!
//! ## Three detectors
//!
//! | Detector | What it answers | Default impl |
//! |---|---|---|
//! | [`ContradictionDetector`] | Does `new` contradict `old`? | [`HeuristicContradictionDetector`] (NegEx scope + asymmetric negation + correction phrases) |
//! | [`OpinionDetector`] | Is this content an opinion (vs. a fact)? | [`HeuristicOpinionDetector`] (first-person opinion frames, hedges) |
//! | [`FutureRelevanceDetector`] | Is this a TODO / intention / future plan? | [`HeuristicFutureRelevanceDetector`] (future markers, planning idioms) |
//!
//! All three default implementations are pure Rust, sub-millisecond, no model
//! downloads, no allocations on hot paths. They cover **English and Polish**
//! out of the box — the two languages the Vestige user base writes in.
//!
//! ## How to add a new detector implementation
//!
//! 1. Implement the relevant trait in a new file under `nlp/`.
//! 2. Add a baseline measurement to `tests/nlp_baseline.rs` using
//!    [`eval::EvalRunner`] so the regression is detectable.
//! 3. Wire it through `vestige-mcp` behind a feature flag (`nli-onnx`, `llm-judge`, …).
//! 4. Document the trade-off (latency, model size, accuracy) in the rustdoc.
//!
//! ## Architectural rationale
//!
//! Why not just bolt on an NLI model directly? Three reasons:
//!
//! - **Eval first**. Without ground-truth datasets we can't tell whether a
//!   shiny new model is better than the heuristic. The cost of an unmeasured
//!   "upgrade" is silent quality regression that surfaces months later as
//!   "Vestige feels worse but I can't say why".
//! - **Heuristics are good enough for many cases**. The current
//!   substring-based code catches ~50% of contradictions; with NegEx scope
//!   detection and proper word boundaries we expect ~70%. NLI models add
//!   another ~15-20% on top — useful but not free (latency, memory, model
//!   download). We want the option, not the requirement.
//! - **Composition**. The real production pipeline will probably be:
//!   heuristic → if uncertain, NLI → if still uncertain, LLM-as-judge. The
//!   trait architecture is exactly what enables this layered composition.

pub mod contradiction;
pub mod eval;
pub mod future_relevance;
pub mod language;
pub mod negation;
pub mod opinion;

// Re-exports for the public API.
pub use contradiction::{ContradictionDetector, HeuristicContradictionDetector};
pub use future_relevance::{FutureRelevanceDetector, HeuristicFutureRelevanceDetector};
pub use language::{Language, detect_language};
pub use negation::{NegationScope, find_negation_scopes};
pub use opinion::{HeuristicOpinionDetector, OpinionDetector};

// ---------------------------------------------------------------------------
// Default-detector factories.
//
// Call sites in `PredictionErrorGate`, `DreamEngine`, and the confidence MCP
// tool go through these wrappers instead of naming `HeuristicXDetector`
// directly. The point: when we wire in an ONNX or LLM backend behind a
// feature flag, the switch happens in one file (here), not 3+ call sites.
// The heuristic detectors are ZSTs, so the indirection is free; the
// `'static` lifetime lets callers store the reference.
// ---------------------------------------------------------------------------

static CONTRADICTION: HeuristicContradictionDetector = HeuristicContradictionDetector;
static OPINION: HeuristicOpinionDetector = HeuristicOpinionDetector;
static FUTURE_RELEVANCE: HeuristicFutureRelevanceDetector = HeuristicFutureRelevanceDetector;

/// Return the process-wide default [`ContradictionDetector`].
///
/// Currently always the heuristic implementation. A future feature flag
/// (`nli-onnx`, `llm-judge`) will swap this without touching call sites.
#[must_use]
pub fn default_contradiction_detector() -> &'static dyn ContradictionDetector {
    &CONTRADICTION
}

/// Return the process-wide default [`OpinionDetector`].
#[must_use]
pub fn default_opinion_detector() -> &'static dyn OpinionDetector {
    &OPINION
}

/// Return the process-wide default [`FutureRelevanceDetector`].
#[must_use]
pub fn default_future_relevance_detector() -> &'static dyn FutureRelevanceDetector {
    &FUTURE_RELEVANCE
}

/// Result of any NLP detection.
///
/// `positive` is the binary verdict (yes / no), `confidence` is a calibrated
/// probability in `[0, 1]`, and `evidence` is a structured list of spans
/// in the original text that drove the decision. The cognitive engine maps
/// `positive` onto its existing bool-returning APIs, but downstream consumers
/// (dashboard, MCP tools, future LLM-as-judge layers) want the richer signal.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// The binary verdict.
    pub positive: bool,
    /// Calibrated probability of the positive class in `[0, 1]`.
    ///
    /// For heuristic detectors this is a discrete approximation (e.g. 0.0,
    /// 0.5, 0.7, 1.0 buckets); for model-based detectors it's the actual
    /// model probability. Either way it should be calibrated against the
    /// eval datasets in [`eval::data`].
    pub confidence: f32,
    /// Structured evidence spans that drove the decision.
    ///
    /// Empty when `positive == false` and we want to record "no evidence";
    /// non-empty when `positive == true` to support explanation surfaces.
    pub evidence: Vec<Evidence>,
}

impl DetectionResult {
    /// Build a positive result with `confidence` and an evidence list.
    pub fn positive(confidence: f32, evidence: Vec<Evidence>) -> Self {
        Self {
            positive: true,
            confidence: confidence.clamp(0.0, 1.0),
            evidence,
        }
    }

    /// Build a negative result.
    pub fn negative() -> Self {
        Self {
            positive: false,
            confidence: 0.0,
            evidence: Vec::new(),
        }
    }

    /// Build a negative result with a confidence (used by detectors that
    /// score everything on a scale, e.g. NLI models that output
    /// `(entailment, neutral, contradiction)` probabilities).
    pub fn negative_with_confidence(confidence: f32) -> Self {
        Self {
            positive: false,
            confidence: confidence.clamp(0.0, 1.0),
            evidence: Vec::new(),
        }
    }
}

/// One piece of evidence supporting a [`DetectionResult`].
///
/// Holds the kind of evidence and the **byte offsets** into the original
/// text. Callers that need the substring should slice on a char boundary
/// (see [`Evidence::snippet_from`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// What kind of evidence this is.
    pub kind: EvidenceKind,
    /// Byte offset of the start of the matched span in the source text.
    pub span_start: usize,
    /// Byte offset of the end (exclusive) of the matched span.
    pub span_end: usize,
    /// The matched text — small (just the trigger), not the whole scope.
    pub snippet: String,
}

impl Evidence {
    /// Extract the snippet from the original text, gracefully handling
    /// byte offsets that aren't on a char boundary (clamps to the nearest
    /// valid boundary). Useful for display in the dashboard.
    pub fn snippet_from<'a>(&self, text: &'a str) -> &'a str {
        let start = clamp_to_boundary(text, self.span_start);
        let end = clamp_to_boundary(text, self.span_end);
        &text[start..end]
    }
}

/// The kind of evidence a detector produced.
///
/// Variants carry no payload — the byte span and snippet on [`Evidence`]
/// are sufficient context for any consumer. The variants exist to let the
/// dashboard render them differently (red highlight for negation, yellow
/// for correction, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EvidenceKind {
    /// A negation token (`not`, `never`, `nie`, `nigdy`, …) that appears
    /// in `new` and is absent from `old`. The asymmetric form, retained
    /// from the legacy heuristic.
    AsymmetricNegation,
    /// A negation trigger combined with a scope window that overlaps
    /// content from the other memory — the NegEx-style detection.
    NegationScope,
    /// A correction phrase (`actually`, `correction:`, `update:`,
    /// `poprawka`, `aktualizacja:`, …) in `new`.
    CorrectionPhrase,
    /// An opinion frame (`I think`, `myślę`, …).
    OpinionFrame,
    /// A hedge or epistemic marker (`maybe`, `chyba`, …).
    Hedge,
    /// A future-tense / TODO / intention marker.
    FutureMarker,
}

impl EvidenceKind {
    /// Stable identifier for callers and payloads.
    ///
    /// Spelled out rather than derived from `Debug`: this string travels in a
    /// tool response, so renaming a variant must not silently rename the wire.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AsymmetricNegation => "asymmetric_negation",
            Self::NegationScope => "negation_scope",
            Self::CorrectionPhrase => "correction_phrase",
            Self::OpinionFrame => "opinion_frame",
            Self::Hedge => "hedge",
            Self::FutureMarker => "future_marker",
        }
    }
}

fn clamp_to_boundary(text: &str, mut idx: usize) -> usize {
    if idx > text.len() {
        idx = text.len();
    }
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_result_positive_clamps_confidence() {
        let result = DetectionResult::positive(1.5, vec![]);
        assert!((result.confidence - 1.0).abs() < f32::EPSILON);

        let result = DetectionResult::positive(-0.2, vec![]);
        assert!(result.confidence.abs() < f32::EPSILON);
    }

    #[test]
    fn detection_result_negative_is_zero_confidence_no_evidence() {
        let result = DetectionResult::negative();
        assert!(!result.positive);
        assert!(result.confidence.abs() < f32::EPSILON);
        assert!(result.evidence.is_empty());
    }

    #[test]
    fn evidence_snippet_handles_multibyte_chars() {
        let text = "polski tekst z błędami i poprawką";
        // Find the exact byte offsets of "poprawką" — multibyte chars
        // mean character indices != byte indices.
        let span_start = text.find("poprawką").expect("substring exists");
        let span_end = span_start + "poprawką".len();
        let evidence = Evidence {
            kind: EvidenceKind::CorrectionPhrase,
            span_start,
            span_end,
            snippet: "poprawką".to_string(),
        };
        let snippet = evidence.snippet_from(text);
        assert_eq!(snippet, "poprawką");
    }

    #[test]
    fn evidence_snippet_clamps_out_of_range_indices() {
        let text = "short";
        let evidence = Evidence {
            kind: EvidenceKind::FutureMarker,
            span_start: 100,
            span_end: 200,
            snippet: String::new(),
        };
        assert_eq!(evidence.snippet_from(text), "");
    }

    #[test]
    fn evidence_snippet_clamps_unicode_boundary_correctly() {
        // ł is a 2-byte UTF-8 char; landing inside it must clamp.
        let text = "błąd";
        let evidence = Evidence {
            kind: EvidenceKind::AsymmetricNegation,
            span_start: 0,
            // 2 is in the middle of `ł` (b=1 byte, ł=2 bytes, so 2 lands in middle of ł)
            span_end: 2,
            snippet: String::new(),
        };
        let snippet = evidence.snippet_from(text);
        // Must be valid UTF-8 — clamped to either "b" or "b…boundary".
        assert!(snippet.is_char_boundary(0));
    }
}

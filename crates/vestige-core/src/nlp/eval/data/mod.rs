//! Curated, hand-labeled datasets bundled into the test binary.
//!
//! Datasets are versioned with the crate (no external files) so:
//!
//! 1. CI doesn't need fixture-loading infrastructure.
//! 2. Every commit that changes detector behavior also changes the eval
//!    delta, which makes the diff actionable.
//! 3. Adding a regression test is just adding a new [`super::dataset::Example`]
//!    to the relevant file.
//!
//! ## Authoring rules
//!
//! - **Balance.** Positive/negative ratio per language should stay between
//!   40/60 and 60/40 to avoid degenerate accuracy metrics.
//! - **Rationale is mandatory.** Every example carries a `rationale` field
//!   explaining *why* it has the label. When a future eval delta surprises
//!   us, the rationale is the difference between "we know" and "we guess".
//! - **No proper nouns in negative cases.** Word-boundary heuristics often
//!   trip on substrings of names. Negative examples should be the
//!   ones that exercise edge cases of the detection algorithm itself.
//! - **Mirror language pairs.** Many examples come as a pair (EN + PL of
//!   the same scenario) — useful for spotting per-language regressions.
//!
//! ## Dataset sizes
//!
//! Per task we aim for 60+ examples (≥ 30 EN, ≥ 30 PL, ~50/50 positive/
//! negative). Going much higher pulls in maintenance cost without a
//! proportional accuracy gain at this scale; the next ROI step is moving
//! to an ONNX classifier, not adding the 200th example.

mod contradiction;
mod future_relevance;
mod opinion;

use super::dataset::Dataset;

/// Get the contradiction eval dataset.
pub fn contradiction_dataset() -> &'static Dataset {
    &contradiction::DATASET
}

/// Get the opinion eval dataset.
pub fn opinion_dataset() -> &'static Dataset {
    &opinion::DATASET
}

/// Get the future-relevance eval dataset.
pub fn future_relevance_dataset() -> &'static Dataset {
    &future_relevance::DATASET
}

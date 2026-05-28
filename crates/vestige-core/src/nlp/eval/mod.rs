//! Evaluation infrastructure for the NLP detectors.
//!
//! Without ground-truth datasets and a runner that computes calibrated
//! metrics, every "this model is better" claim is wishful thinking. This
//! module is the foundation that makes detector upgrades measurable.
//!
//! ## What lives here
//!
//! - [`Example`] / [`Label`] / [`Dataset`] — the labeled-test-case
//!   primitives. Datasets are bundled as compiled-in Rust data (see
//!   [`data`]), so they're versioned with the code and run in CI
//!   without any data-loading machinery.
//! - [`EvalRunner`] — runs a detector across a dataset and produces an
//!   [`EvalReport`] with precision, recall, F1, accuracy, ECE
//!   (Expected Calibration Error), and a per-language breakdown.
//! - [`metrics`] — pure functions for the metrics. Tested independently
//!   so the runner can rely on them.
//!
//! ## Workflow
//!
//! ```rust,ignore
//! use vestige_core::nlp::{HeuristicContradictionDetector};
//! use vestige_core::nlp::eval::{EvalRunner, data};
//!
//! let detector = HeuristicContradictionDetector::new();
//! let dataset = data::contradiction_dataset();
//! let report = EvalRunner::run_contradiction(&detector, &dataset);
//! println!("F1: {:.3}", report.f1);
//! println!("ECE: {:.3}", report.ece);
//! ```
//!
//! ## How to add a regression
//!
//! When a real-world false positive / false negative is reported, add it
//! as a new [`Example`] to the appropriate dataset module under [`data`].
//! Rerun the baseline (`tests/nlp_baseline.rs`) — if the metric regresses
//! below the asserted threshold, the test fails and the fix is required
//! before merge.

pub mod data;
pub mod metrics;

mod dataset;
mod runner;

pub use dataset::{Dataset, Example, Label};
pub use metrics::{CalibrationBucket, EvalReport, LanguageMetrics};
pub use runner::EvalRunner;

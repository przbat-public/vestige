//! Eval runner — feeds a detector through a dataset, builds the report.
//!
//! Three entry points, one per detector type. Internally they all share
//! [`finalize_report`] — only the prediction call differs.

use std::collections::HashMap;

use crate::nlp::contradiction::ContradictionDetector;
use crate::nlp::future_relevance::FutureRelevanceDetector;
use crate::nlp::language::Language;
use crate::nlp::opinion::OpinionDetector;

use super::dataset::Dataset;
use super::metrics::{
    EvalReport, FailureRecord, LanguageMetrics, accuracy, confusion_matrix,
    expected_calibration_error, f1_score, precision, recall,
};

/// Number of equal-width buckets in the calibration curve.
pub const CALIBRATION_BUCKETS: usize = 10;

/// Eval runner — stateless, three associated functions.
pub struct EvalRunner;

impl EvalRunner {
    /// Run a [`ContradictionDetector`] against a contradiction-pair dataset.
    pub fn run_contradiction<D: ContradictionDetector>(
        detector: &D,
        dataset: &Dataset,
    ) -> EvalReport {
        let mut samples: Vec<Sample> = Vec::with_capacity(dataset.len());
        for ex in dataset.examples {
            let result = detector.detect(ex.text_a, ex.text_b);
            samples.push(Sample {
                id: ex.id,
                language: ex.language,
                rationale: ex.rationale,
                predicted: result.positive,
                confidence: result.confidence,
                truth: ex.label.is_positive(),
            });
        }
        finalize_report(dataset, detector.name(), samples)
    }

    /// Run an [`OpinionDetector`] against an opinion dataset.
    pub fn run_opinion<D: OpinionDetector>(detector: &D, dataset: &Dataset) -> EvalReport {
        let mut samples: Vec<Sample> = Vec::with_capacity(dataset.len());
        for ex in dataset.examples {
            let result = detector.detect(ex.text_a);
            samples.push(Sample {
                id: ex.id,
                language: ex.language,
                rationale: ex.rationale,
                predicted: result.positive,
                confidence: result.confidence,
                truth: ex.label.is_positive(),
            });
        }
        finalize_report(dataset, detector.name(), samples)
    }

    /// Run a [`FutureRelevanceDetector`] against a future-relevance dataset.
    pub fn run_future_relevance<D: FutureRelevanceDetector>(
        detector: &D,
        dataset: &Dataset,
    ) -> EvalReport {
        let mut samples: Vec<Sample> = Vec::with_capacity(dataset.len());
        for ex in dataset.examples {
            let result = detector.detect(ex.text_a);
            samples.push(Sample {
                id: ex.id,
                language: ex.language,
                rationale: ex.rationale,
                predicted: result.positive,
                confidence: result.confidence,
                truth: ex.label.is_positive(),
            });
        }
        finalize_report(dataset, detector.name(), samples)
    }
}

struct Sample {
    id: &'static str,
    language: Language,
    rationale: &'static str,
    predicted: bool,
    confidence: f32,
    truth: bool,
}

fn finalize_report(
    dataset: &Dataset,
    detector_name: &'static str,
    samples: Vec<Sample>,
) -> EvalReport {
    let pre_calibration: Vec<(bool, f32, bool)> = samples
        .iter()
        .map(|s| (s.predicted, s.confidence, s.truth))
        .collect();

    let (tp, fp, tn, fneg) = confusion_matrix(&pre_calibration);
    let total = samples.len();
    let p = precision(tp, fp);
    let r = recall(tp, fneg);
    let f1 = f1_score(p, r);
    let acc = accuracy(tp, tn, total);

    // ECE convention: confidence is the model's probability for the
    // *predicted* class. Our detectors report `confidence` as the strength
    // of the positive signal — for a negative prediction, P(predicted) =
    // 1 - P(positive). We invert so ECE measures calibration of the
    // confidence the detector *actually committed to*. Without this the
    // (negative, 0.0) samples land in the [0.0, 0.1) bucket with
    // accuracy ~1.0, blowing up ECE on every well-balanced dataset.
    let calibration_input: Vec<(bool, f32, bool)> = samples
        .iter()
        .map(|s| {
            let committed_confidence = if s.predicted {
                s.confidence
            } else {
                // Negative prediction: detector implicitly claimed
                // P(negative) = 1 - confidence. Our heuristic reports
                // confidence=0.0 on negative (no signal), which translates
                // to P(negative)=1.0.
                1.0 - s.confidence
            };
            (s.predicted, committed_confidence, s.truth)
        })
        .collect();
    let (ece, calibration_buckets) =
        expected_calibration_error(&calibration_input, CALIBRATION_BUCKETS);

    // Per-language breakdown.
    let mut by_lang: HashMap<Language, Vec<(bool, f32, bool)>> = HashMap::new();
    for s in &samples {
        by_lang
            .entry(s.language)
            .or_default()
            .push((s.predicted, s.confidence, s.truth));
    }
    let mut per_language = HashMap::new();
    for (lang, subset) in by_lang {
        let (lang_tp, lang_fp, lang_tn, lang_fneg) = confusion_matrix(&subset);
        let lang_total = subset.len();
        let lang_precision = precision(lang_tp, lang_fp);
        let lang_recall = recall(lang_tp, lang_fneg);
        per_language.insert(
            lang,
            LanguageMetrics {
                language: lang,
                total: lang_total,
                accuracy: accuracy(lang_tp, lang_tn, lang_total),
                precision: lang_precision,
                recall: lang_recall,
                f1: f1_score(lang_precision, lang_recall),
            },
        );
    }

    // Failure records — for diagnostic output when an assertion fails.
    let failures: Vec<FailureRecord> = samples
        .iter()
        .filter(|s| s.predicted != s.truth)
        .map(|s| FailureRecord {
            example_id: s.id,
            language: s.language,
            was_false_positive: s.predicted && !s.truth,
            confidence: s.confidence,
            rationale: s.rationale,
        })
        .collect();

    EvalReport {
        dataset_name: dataset.name,
        detector_name,
        total,
        true_positives: tp,
        false_positives: fp,
        true_negatives: tn,
        false_negatives: fneg,
        accuracy: acc,
        precision: p,
        recall: r,
        f1,
        per_language,
        ece,
        calibration_buckets,
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nlp::HeuristicContradictionDetector;
    use crate::nlp::HeuristicFutureRelevanceDetector;
    use crate::nlp::HeuristicOpinionDetector;
    use crate::nlp::eval::data;

    #[test]
    fn runner_executes_contradiction_dataset_without_panic() {
        let det = HeuristicContradictionDetector::new();
        let report = EvalRunner::run_contradiction(&det, data::contradiction_dataset());
        assert_eq!(report.total, data::contradiction_dataset().len());
        assert!(report.precision >= 0.0 && report.precision <= 1.0);
        assert!(report.recall >= 0.0 && report.recall <= 1.0);
        assert!(report.f1 >= 0.0 && report.f1 <= 1.0);
        assert!(report.ece >= 0.0 && report.ece <= 1.0);
    }

    #[test]
    fn runner_executes_opinion_dataset_without_panic() {
        let det = HeuristicOpinionDetector::new();
        let report = EvalRunner::run_opinion(&det, data::opinion_dataset());
        assert_eq!(report.total, data::opinion_dataset().len());
        assert!(report.f1 >= 0.0 && report.f1 <= 1.0);
    }

    #[test]
    fn runner_executes_future_relevance_dataset_without_panic() {
        let det = HeuristicFutureRelevanceDetector::new();
        let report = EvalRunner::run_future_relevance(&det, data::future_relevance_dataset());
        assert_eq!(report.total, data::future_relevance_dataset().len());
        assert!(report.f1 >= 0.0 && report.f1 <= 1.0);
    }

    #[test]
    fn report_includes_failure_records_for_misclassifications() {
        let det = HeuristicContradictionDetector::new();
        let report = EvalRunner::run_contradiction(&det, data::contradiction_dataset());
        let expected_failures = report.false_positives + report.false_negatives;
        assert_eq!(report.failures.len(), expected_failures);
    }

    #[test]
    fn per_language_metrics_sum_to_overall() {
        let det = HeuristicContradictionDetector::new();
        let report = EvalRunner::run_contradiction(&det, data::contradiction_dataset());
        let per_lang_total: usize = report.per_language.values().map(|m| m.total).sum();
        assert_eq!(per_lang_total, report.total);
    }
}

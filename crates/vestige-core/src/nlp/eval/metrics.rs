//! Metrics for the NLP eval runner.
//!
//! All computations are pure (no I/O, no global state) so they can be
//! tested with hand-built confusion matrices. The runner builds the
//! confusion matrix from a (detector, dataset) pair and feeds it here.

use std::collections::HashMap;

use crate::nlp::language::Language;

/// A confusion-matrix-derived report for a single detector run.
#[derive(Debug, Clone)]
pub struct EvalReport {
    /// Dataset name the report was computed on.
    pub dataset_name: &'static str,
    /// Detector name (from [`crate::nlp::ContradictionDetector::name`] etc.).
    pub detector_name: &'static str,
    /// Total examples evaluated.
    pub total: usize,
    /// True positives: detector said positive, ground truth positive.
    pub true_positives: usize,
    /// False positives: detector said positive, ground truth negative.
    pub false_positives: usize,
    /// True negatives: detector said negative, ground truth negative.
    pub true_negatives: usize,
    /// False negatives: detector said negative, ground truth positive.
    pub false_negatives: usize,
    /// (TP + TN) / total.
    pub accuracy: f32,
    /// TP / (TP + FP). NaN when no positive predictions.
    pub precision: f32,
    /// TP / (TP + FN). NaN when no positive ground truth.
    pub recall: f32,
    /// Harmonic mean of precision and recall.
    pub f1: f32,
    /// Per-language metric breakdown (Language → LanguageMetrics).
    pub per_language: HashMap<Language, LanguageMetrics>,
    /// Expected Calibration Error in 10 equal-width buckets.
    pub ece: f32,
    /// Calibration buckets for plotting / debugging.
    pub calibration_buckets: Vec<CalibrationBucket>,
    /// Identifiers of examples the detector got *wrong*, in dataset order.
    pub failures: Vec<FailureRecord>,
}

/// Metrics scoped to a single language.
#[derive(Debug, Clone)]
pub struct LanguageMetrics {
    /// Language these metrics belong to.
    pub language: Language,
    /// Number of examples for this language.
    pub total: usize,
    /// Accuracy on the language subset.
    pub accuracy: f32,
    /// Precision on the language subset.
    pub precision: f32,
    /// Recall on the language subset.
    pub recall: f32,
    /// F1 score on the language subset.
    pub f1: f32,
}

/// A single bucket of the calibration curve.
#[derive(Debug, Clone)]
pub struct CalibrationBucket {
    /// Lower bound of the confidence bucket, inclusive.
    pub lower: f32,
    /// Upper bound of the confidence bucket, exclusive (except for the last).
    pub upper: f32,
    /// Number of predictions whose confidence fell in this bucket.
    pub count: usize,
    /// Mean confidence of predictions in this bucket.
    pub mean_confidence: f32,
    /// Mean accuracy of predictions in this bucket.
    pub mean_accuracy: f32,
}

/// A single misclassified example with diagnostic context.
#[derive(Debug, Clone)]
pub struct FailureRecord {
    /// Stable example id.
    pub example_id: &'static str,
    /// Language of the example.
    pub language: Language,
    /// Whether the prediction was a false positive (`true`) or false
    /// negative (`false`).
    pub was_false_positive: bool,
    /// Detector confidence at the time of failure.
    pub confidence: f32,
    /// Author's rationale for the ground-truth label.
    pub rationale: &'static str,
}

/// Build a confusion matrix from a list of `(predicted, predicted_confidence, truth)`.
///
/// `predicted` and `truth` are `bool` (positive class indicator).
pub fn confusion_matrix(samples: &[(bool, f32, bool)]) -> (usize, usize, usize, usize) {
    let mut tp = 0;
    let mut fp = 0;
    let mut tn = 0;
    let mut fneg = 0;
    for &(pred, _, truth) in samples {
        match (pred, truth) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => fneg += 1,
            (false, false) => tn += 1,
        }
    }
    (tp, fp, tn, fneg)
}

/// Compute precision: `TP / (TP + FP)`. Returns 0.0 (sentinel) if no positive predictions.
pub fn precision(tp: usize, fp: usize) -> f32 {
    let denom = tp + fp;
    if denom == 0 {
        0.0
    } else {
        tp as f32 / denom as f32
    }
}

/// Compute recall: `TP / (TP + FN)`. Returns 0.0 if no positive ground truth.
pub fn recall(tp: usize, fneg: usize) -> f32 {
    let denom = tp + fneg;
    if denom == 0 {
        0.0
    } else {
        tp as f32 / denom as f32
    }
}

/// Compute F1: harmonic mean of precision and recall.
pub fn f1_score(precision: f32, recall: f32) -> f32 {
    let denom = precision + recall;
    if denom == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / denom
    }
}

/// Compute accuracy: `(TP + TN) / total`.
pub fn accuracy(tp: usize, tn: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        (tp + tn) as f32 / total as f32
    }
}

/// Compute Expected Calibration Error (ECE) over `n_buckets` equal-width
/// confidence buckets, plus the per-bucket records for plotting.
///
/// Samples are `(predicted, confidence, truth)` where `confidence` is the
/// model's **probability of the predicted class** (not always the positive
/// class). `was_correct` is `predicted == truth`.
///
/// Why the predicted-class convention: the classical ECE definition asks
/// "how well does the confidence on the class the model committed to match
/// its accuracy?". A negative prediction with `confidence = 0.0` would land
/// in the `[0.0, 0.1)` bucket with `accuracy = 1.0`, blowing up ECE on every
/// well-balanced dataset. The caller (`runner::finalize_report`) handles
/// the transform by passing `1 - p_positive` for negative predictions.
///
/// Returns `(ece, buckets)`.
pub fn expected_calibration_error(
    samples: &[(bool, f32, bool)],
    n_buckets: usize,
) -> (f32, Vec<CalibrationBucket>) {
    if samples.is_empty() || n_buckets == 0 {
        return (0.0, Vec::new());
    }
    let bucket_width = 1.0 / n_buckets as f32;
    let mut buckets: Vec<Vec<(f32, bool)>> = (0..n_buckets).map(|_| Vec::new()).collect();
    for &(pred, conf, truth) in samples {
        let was_correct = pred == truth;
        let bucket_idx = ((conf / bucket_width).floor() as usize).min(n_buckets - 1);
        buckets[bucket_idx].push((conf, was_correct));
    }

    let total = samples.len() as f32;
    let mut ece = 0.0_f32;
    let mut out = Vec::with_capacity(n_buckets);

    for (i, bucket) in buckets.iter().enumerate() {
        let count = bucket.len();
        let lower = i as f32 * bucket_width;
        let upper = if i + 1 == n_buckets {
            1.0
        } else {
            (i + 1) as f32 * bucket_width
        };
        let (mean_conf, mean_acc) = if count == 0 {
            (0.0, 0.0)
        } else {
            let mc = bucket.iter().map(|(c, _)| *c).sum::<f32>() / count as f32;
            let ma = bucket.iter().filter(|(_, ok)| *ok).count() as f32 / count as f32;
            (mc, ma)
        };
        ece += (count as f32 / total) * (mean_conf - mean_acc).abs();
        out.push(CalibrationBucket {
            lower,
            upper,
            count,
            mean_confidence: mean_conf,
            mean_accuracy: mean_acc,
        });
    }

    (ece, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn precision_basic_cases() {
        assert!(approx(precision(80, 20), 0.8));
        assert_eq!(precision(0, 0), 0.0);
        assert_eq!(precision(0, 5), 0.0);
        assert!(approx(precision(10, 0), 1.0));
    }

    #[test]
    fn recall_basic_cases() {
        assert!(approx(recall(80, 20), 0.8));
        assert_eq!(recall(0, 0), 0.0);
        assert_eq!(recall(0, 5), 0.0);
        assert!(approx(recall(10, 0), 1.0));
    }

    #[test]
    fn f1_basic_cases() {
        assert!(approx(f1_score(1.0, 1.0), 1.0));
        assert!(approx(f1_score(0.5, 0.5), 0.5));
        assert!(approx(f1_score(0.8, 0.4), 0.5333));
        assert_eq!(f1_score(0.0, 0.0), 0.0);
    }

    #[test]
    fn accuracy_basic_cases() {
        assert!(approx(accuracy(90, 5, 100), 0.95));
        assert_eq!(accuracy(0, 0, 0), 0.0);
        assert!(approx(accuracy(50, 50, 100), 1.0));
    }

    #[test]
    fn confusion_matrix_all_quadrants() {
        let samples = vec![
            (true, 0.9, true),   // TP
            (true, 0.6, false),  // FP
            (false, 0.0, false), // TN
            (false, 0.0, true),  // FN
        ];
        let (tp, fp, tn, fneg) = confusion_matrix(&samples);
        assert_eq!(tp, 1);
        assert_eq!(fp, 1);
        assert_eq!(tn, 1);
        assert_eq!(fneg, 1);
    }

    #[test]
    fn confusion_matrix_empty() {
        let (tp, fp, tn, fneg) = confusion_matrix(&[]);
        assert_eq!((tp, fp, tn, fneg), (0, 0, 0, 0));
    }

    #[test]
    fn ece_perfectly_calibrated() {
        // All predictions have confidence equal to accuracy → ECE = 0.
        let samples = vec![
            (true, 1.0, true), // confidence 1.0, correct → bucket [0.9, 1.0]
            (true, 1.0, true),
            (true, 0.5, true),  // confidence 0.5, correct
            (true, 0.5, false), // confidence 0.5, incorrect — 50% accuracy in bucket [0.5, 0.6)
        ];
        let (ece, _) = expected_calibration_error(&samples, 10);
        // Buckets:
        //  [0.5,0.6): 2 samples, mean_conf 0.5, mean_acc 0.5 → 0 contribution.
        //  [0.9,1.0]: 2 samples, mean_conf 1.0, mean_acc 1.0 → 0 contribution.
        assert!(ece < 1e-4, "expected near-zero ECE, got {ece}");
    }

    #[test]
    fn ece_overconfident_model() {
        // All confidences are 1.0 but only 50% are correct → ECE = 0.5.
        let samples = vec![
            (true, 1.0, true),
            (true, 1.0, false),
            (true, 1.0, true),
            (true, 1.0, false),
        ];
        let (ece, _) = expected_calibration_error(&samples, 10);
        assert!((ece - 0.5).abs() < 0.01, "expected ECE ≈ 0.5, got {ece}");
    }

    #[test]
    fn ece_buckets_have_correct_ranges() {
        let samples = vec![(true, 0.05, true), (true, 0.15, true), (true, 0.95, true)];
        let (_, buckets) = expected_calibration_error(&samples, 10);
        assert_eq!(buckets.len(), 10);
        assert!(approx(buckets[0].lower, 0.0));
        assert!(approx(buckets[0].upper, 0.1));
        assert!(approx(buckets[9].lower, 0.9));
        assert!(approx(buckets[9].upper, 1.0));
        // Predictions correctly bucketed:
        assert_eq!(buckets[0].count, 1); // 0.05 → bucket 0
        assert_eq!(buckets[1].count, 1); // 0.15 → bucket 1
        assert_eq!(buckets[9].count, 1); // 0.95 → bucket 9
    }

    #[test]
    fn ece_zero_buckets_returns_zero() {
        let samples = vec![(true, 0.5, true)];
        let (ece, buckets) = expected_calibration_error(&samples, 0);
        assert_eq!(ece, 0.0);
        assert!(buckets.is_empty());
    }

    #[test]
    fn ece_empty_samples_returns_zero() {
        let (ece, buckets) = expected_calibration_error(&[], 10);
        assert_eq!(ece, 0.0);
        assert!(buckets.is_empty());
    }
}

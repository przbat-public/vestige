//! Baseline measurements + deep robustness tests for the NLP detectors.
//!
//! This is the integration counterpart to `crates/vestige-core/src/nlp/`.
//! It exists for two reasons:
//!
//! 1. **Baseline gating.** Each detector is evaluated against its bundled
//!    hand-curated dataset. The minimum precision / recall / F1 / ECE
//!    thresholds asserted here are the contract: any future change to the
//!    heuristic (or a future swap-in of an ONNX model) must clear these
//!    bars, otherwise CI catches the regression. The exact thresholds were
//!    calibrated against the v1 heuristic implementation and chosen with
//!    some slack so that small lexicon tweaks don't flake the suite. The
//!    bar for any ML upgrade is "strictly better than these numbers across
//!    every metric and every language slice".
//!
//! 2. **Deep robustness.** The dataset alone tests "does the detector
//!    classify a sentence correctly?". This file additionally tests
//!    behaviour under adversarial input, determinism, cross-detector
//!    consistency, and end-to-end semantics on realistic memories pulled
//!    from production audits. Without these, lexicon edits could pass
//!    F1 but introduce panics, UTF-8 corruption, or contradictions with
//!    other detectors that the dataset never exercises.
//!
//! Run `cargo test -p vestige-core --test nlp_baseline -- --nocapture` to
//! see the human-readable report.

use vestige_core::nlp::eval::{EvalReport, EvalRunner, data};
use vestige_core::nlp::language::Language;
use vestige_core::nlp::{
    ContradictionDetector, FutureRelevanceDetector, HeuristicContradictionDetector,
    HeuristicFutureRelevanceDetector, HeuristicOpinionDetector, OpinionDetector,
};

// ====================================================================
// Threshold constants. These are the *gate* — never weaken them silently.
// If you need to relax a bar, document why in the commit and update the
// rationale here.
// ====================================================================

/// Overall F1 the heuristic contradiction detector must clear. The v1
/// heuristic measured at ~0.78 on the bundled dataset; we set the bar at
/// 0.70 to allow for small lexicon tweaks without breaking CI.
const CONTRADICTION_MIN_F1: f32 = 0.70;
/// Minimum precision — we lean toward precision over recall here because
/// false-positive contradictions block legitimate memory saves.
const CONTRADICTION_MIN_PRECISION: f32 = 0.70;
/// Minimum recall — we still need to catch real contradictions to keep
/// reconsolidation honest.
const CONTRADICTION_MIN_RECALL: f32 = 0.60;

/// Overall F1 the heuristic opinion detector must clear.
const OPINION_MIN_F1: f32 = 0.70;
const OPINION_MIN_PRECISION: f32 = 0.75; // Opinions misclassified as facts
// get over-promoted by FSRS-6.
const OPINION_MIN_RECALL: f32 = 0.55;

/// Overall F1 the heuristic future-relevance detector must clear.
const FUTURE_MIN_F1: f32 = 0.75;
const FUTURE_MIN_PRECISION: f32 = 0.70;
const FUTURE_MIN_RECALL: f32 = 0.70;

/// Maximum allowed F1 gap between EN and PL slices. The whole point of
/// keeping a balanced bilingual dataset is to detect language-specific
/// degradation early — a one-language regression in a multilingual user
/// base is invisible in the overall number.
const PER_LANGUAGE_F1_GAP: f32 = 0.20;

/// Maximum allowed Expected Calibration Error. A heuristic with stacked
/// confidence ~0.7-0.95 should land below ~0.30; if it drifts higher,
/// the fusion formula or the per-signal confidences need re-tuning.
const MAX_ECE: f32 = 0.35;

// ====================================================================
// Helpers
// ====================================================================

fn print_report(report: &EvalReport) {
    eprintln!(
        "\n=== {} on {} ===",
        report.detector_name, report.dataset_name
    );
    eprintln!(
        "  total: {}  TP={}  FP={}  TN={}  FN={}",
        report.total,
        report.true_positives,
        report.false_positives,
        report.true_negatives,
        report.false_negatives,
    );
    eprintln!(
        "  accuracy={:.3}  precision={:.3}  recall={:.3}  F1={:.3}  ECE={:.3}",
        report.accuracy, report.precision, report.recall, report.f1, report.ece,
    );
    for (lang, m) in &report.per_language {
        eprintln!(
            "  [{:?}] n={}  P={:.3}  R={:.3}  F1={:.3}  acc={:.3}",
            lang, m.total, m.precision, m.recall, m.f1, m.accuracy,
        );
    }
    if !report.failures.is_empty() {
        eprintln!("  failures ({}):", report.failures.len());
        for f in &report.failures {
            eprintln!(
                "    - id={} lang={:?} {} conf={:.2} :: {}",
                f.example_id,
                f.language,
                if f.was_false_positive { "FP" } else { "FN" },
                f.confidence,
                f.rationale,
            );
        }
    }
}

fn assert_thresholds(report: &EvalReport, min_f1: f32, min_precision: f32, min_recall: f32) {
    assert!(
        report.precision >= min_precision,
        "{} precision {:.3} < min {:.3}",
        report.detector_name,
        report.precision,
        min_precision,
    );
    assert!(
        report.recall >= min_recall,
        "{} recall {:.3} < min {:.3}",
        report.detector_name,
        report.recall,
        min_recall,
    );
    assert!(
        report.f1 >= min_f1,
        "{} F1 {:.3} < min {:.3}",
        report.detector_name,
        report.f1,
        min_f1,
    );
    assert!(
        report.ece <= MAX_ECE,
        "{} ECE {:.3} > max {:.3}",
        report.detector_name,
        report.ece,
        MAX_ECE,
    );
}

fn assert_per_language_balance(report: &EvalReport) {
    let en = report.per_language.get(&Language::English);
    let pl = report.per_language.get(&Language::Polish);
    if let (Some(en), Some(pl)) = (en, pl) {
        let gap = (en.f1 - pl.f1).abs();
        assert!(
            gap <= PER_LANGUAGE_F1_GAP,
            "{}: per-language F1 gap {:.3} > {:.3} (EN F1 {:.3}, PL F1 {:.3})",
            report.detector_name,
            gap,
            PER_LANGUAGE_F1_GAP,
            en.f1,
            pl.f1,
        );
    }
}

// ====================================================================
// Baseline measurements
// ====================================================================

#[test]
fn baseline_contradiction_meets_threshold() {
    let detector = HeuristicContradictionDetector::new();
    let dataset = data::contradiction_dataset();
    let report = EvalRunner::run_contradiction(&detector, dataset);
    print_report(&report);
    assert_thresholds(
        &report,
        CONTRADICTION_MIN_F1,
        CONTRADICTION_MIN_PRECISION,
        CONTRADICTION_MIN_RECALL,
    );
    assert_per_language_balance(&report);
}

#[test]
fn baseline_opinion_meets_threshold() {
    let detector = HeuristicOpinionDetector::new();
    let dataset = data::opinion_dataset();
    let report = EvalRunner::run_opinion(&detector, dataset);
    print_report(&report);
    assert_thresholds(
        &report,
        OPINION_MIN_F1,
        OPINION_MIN_PRECISION,
        OPINION_MIN_RECALL,
    );
    assert_per_language_balance(&report);
}

#[test]
fn baseline_future_relevance_meets_threshold() {
    let detector = HeuristicFutureRelevanceDetector::new();
    let dataset = data::future_relevance_dataset();
    let report = EvalRunner::run_future_relevance(&detector, dataset);
    print_report(&report);
    assert_thresholds(
        &report,
        FUTURE_MIN_F1,
        FUTURE_MIN_PRECISION,
        FUTURE_MIN_RECALL,
    );
    assert_per_language_balance(&report);
}

// ====================================================================
// Determinism — same input must always produce the same output
// ====================================================================

#[test]
fn contradiction_detector_is_deterministic_across_runs() {
    let detector = HeuristicContradictionDetector::new();
    let new_content = "I prefer dark mode in all editors.";
    let old_content = "I prefer light mode in all editors.";

    let first = detector.detect(new_content, old_content);
    for _ in 0..200 {
        let r = detector.detect(new_content, old_content);
        assert_eq!(r.positive, first.positive);
        assert!(
            (r.confidence - first.confidence).abs() < f32::EPSILON,
            "non-deterministic confidence: {} vs {}",
            r.confidence,
            first.confidence,
        );
        assert_eq!(r.evidence, first.evidence);
    }
}

#[test]
fn opinion_detector_is_deterministic_across_runs() {
    let detector = HeuristicOpinionDetector::new();
    let content = "I think the new layout is probably better, but I'm not sure.";

    let first = detector.detect(content);
    for _ in 0..200 {
        let r = detector.detect(content);
        assert_eq!(r.positive, first.positive);
        assert!((r.confidence - first.confidence).abs() < f32::EPSILON);
        assert_eq!(r.evidence, first.evidence);
    }
}

#[test]
fn future_relevance_detector_is_deterministic_across_runs() {
    let detector = HeuristicFutureRelevanceDetector::new();
    let content = "Remind me to deploy the search reranker next Monday.";

    let first = detector.detect(content);
    for _ in 0..200 {
        let r = detector.detect(content);
        assert_eq!(r.positive, first.positive);
        assert!((r.confidence - first.confidence).abs() < f32::EPSILON);
        assert_eq!(r.evidence, first.evidence);
    }
}

// ====================================================================
// Adversarial input — should never panic, should produce a sane result
// ====================================================================

/// Build the adversarial test corpus. Built dynamically because the
/// long-input case (`"a ".repeat(5000)`) can't live in a const slice.
fn adversarial_inputs() -> Vec<String> {
    let mut v: Vec<String> = vec![
        "",
        " ",
        "\n\n\n",
        "\t  \t  ",
        "                                              ",
        "💯💯💯💯💯",
        "!!!???.,;:-_",
        "0123456789",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "ąęłńóśźżĄĘŁŃÓŚŹŻ",
        "The quick brown fox jumps over the lazy dog.",
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
        "fn main() { println!(\"hello\"); let x: Result<(), Box<dyn Error>> = Ok(()); }",
        "https://example.com/path?query=value&other=thing",
        "user@example.com",
        // Bidirectional / RTL embedded — make sure we don't slice into
        // the middle of a multibyte char.
        "Mixed עברית and العربية with English",
        // CJK characters.
        "今日は良い天気です。",
        // Zalgo / combining marks.
        "ḽ̸̳͒̃̌̎͜ǫ̴͖̃͆̃̒́͠l̶̥̮̱̰̊̅̄͂̕",
        // Whitespace + diacritic only.
        "ą ę ł ó",
        // Single character.
        "x",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    // Extremely long input — must not OOM or take seconds.
    v.push("a ".repeat(5000));
    v
}

#[test]
fn contradiction_detector_handles_adversarial_inputs() {
    let detector = HeuristicContradictionDetector::new();
    let inputs = adversarial_inputs();
    for a in &inputs {
        for b in &inputs {
            let r = detector.detect(a, b);
            assert!(
                (0.0..=1.0).contains(&r.confidence),
                "confidence out of [0,1] for ({a:?}, {b:?}): {}",
                r.confidence
            );
            if r.positive {
                // Positive results must carry at least one evidence span.
                assert!(
                    !r.evidence.is_empty(),
                    "positive without evidence for ({a:?}, {b:?})"
                );
                for ev in &r.evidence {
                    assert!(
                        ev.span_end >= ev.span_start,
                        "inverted span for ({a:?}, {b:?}): {:?}",
                        ev,
                    );
                }
            } else {
                assert!(
                    r.confidence == 0.0,
                    "negative result must have 0 confidence, got {} for ({a:?}, {b:?})",
                    r.confidence,
                );
                assert!(r.evidence.is_empty());
            }
        }
    }
}

#[test]
fn opinion_detector_handles_adversarial_inputs() {
    let detector = HeuristicOpinionDetector::new();
    for input in adversarial_inputs() {
        let r = detector.detect(&input);
        assert!(
            (0.0..=1.0).contains(&r.confidence),
            "confidence out of [0,1] for {input:?}: {}",
            r.confidence,
        );
        if r.positive {
            assert!(!r.evidence.is_empty());
        } else {
            assert!(r.confidence == 0.0);
            assert!(r.evidence.is_empty());
        }
    }
}

#[test]
fn future_relevance_detector_handles_adversarial_inputs() {
    let detector = HeuristicFutureRelevanceDetector::new();
    for input in adversarial_inputs() {
        let r = detector.detect(&input);
        assert!(
            (0.0..=1.0).contains(&r.confidence),
            "confidence out of [0,1] for {input:?}: {}",
            r.confidence,
        );
        if r.positive {
            assert!(!r.evidence.is_empty());
        } else {
            assert!(r.confidence == 0.0);
            assert!(r.evidence.is_empty());
        }
    }
}

// ====================================================================
// Cross-detector consistency
//
// A purely factual statement shouldn't fire as an opinion *or* a future
// reminder. If both detectors light up on the same factual sentence, one
// of them is over-firing and the heuristic needs tightening.
// ====================================================================

#[test]
fn factual_statements_are_clean_across_detectors() {
    let opinion = HeuristicOpinionDetector::new();
    let future = HeuristicFutureRelevanceDetector::new();

    let facts: &[&str] = &[
        "The HTTP standard is defined in RFC 9110.",
        "Standard Pub/Sub deduplicates messages within a 10-minute window.",
        "PostgreSQL 18 was released on September 25, 2025.",
        "Cloud Run Direct VPC egress reduces cold-start time.",
        "Stała Boltzmanna wynosi 1,38 × 10⁻²³ J/K.",
        "Wisła ma 1047 kilometrów długości.",
        "Kompilator Rust 1.95 wprowadza nowe lintery clippy.",
        "Klient backend-bridge komunikuje się przez postMessage.",
    ];

    for fact in facts {
        let o = opinion.detect(fact);
        let f = future.detect(fact);
        assert!(
            !o.positive,
            "opinion fired on factual: {fact:?} (evidence={:?})",
            o.evidence
        );
        assert!(
            !f.positive,
            "future-relevance fired on factual: {fact:?} (evidence={:?})",
            f.evidence
        );
    }
}

#[test]
fn opinions_are_not_also_future_relevant() {
    let opinion = HeuristicOpinionDetector::new();
    let future = HeuristicFutureRelevanceDetector::new();

    // Pure opinion (no future markers) — should fire opinion but NOT future.
    let opinions_only: &[&str] = &[
        "I think this design is probably overengineered.",
        "Moim zdaniem ten interfejs jest mylący.",
        "I believe the migration was the right call.",
        "Wydaje mi się, że ten algorytm jest za wolny.",
    ];

    for content in opinions_only {
        let o = opinion.detect(content);
        let f = future.detect(content);
        assert!(o.positive, "expected opinion to fire on: {content:?}");
        assert!(
            !f.positive,
            "future-relevance fired on pure opinion: {content:?} (evidence={:?})",
            f.evidence,
        );
    }
}

#[test]
fn future_tasks_are_not_misclassified_as_opinions() {
    let opinion = HeuristicOpinionDetector::new();
    let future = HeuristicFutureRelevanceDetector::new();

    let todos: &[&str] = &[
        "TODO: investigate slow query on the orders table tomorrow.",
        "Remind me to publish the new SDK version on Friday.",
        "Muszę zaktualizować dokumentację przed piątkiem.",
        "Trzeba dodać monitoring do nowego endpointa.",
    ];

    for content in todos {
        let f = future.detect(content);
        let o = opinion.detect(content);
        assert!(f.positive, "expected future-relevance on: {content:?}");
        assert!(
            !o.positive,
            "opinion fired on todo: {content:?} (evidence={:?})",
            o.evidence,
        );
    }
}

// ====================================================================
// Regression — real-world examples from production audits
// ====================================================================

#[test]
fn regression_compound_memory_is_not_contradiction_with_itself() {
    // Pulled from the 2026-05-07 audit: long compound memories with
    // multiple [Updated] sections should NOT trip the contradiction
    // detector when compared against their own earlier revision (i.e.
    // edits to a single memory must not look like a fact reversal).
    let detector = HeuristicContradictionDetector::new();
    let v1 = "Biological age tracking: I use Bryan Johnson's protocol. \
              The blueprint includes specific supplements.";
    let v2 = "Biological age tracking: I use Bryan Johnson's protocol. \
              The blueprint includes specific supplements. \
              [Updated 2026-04-15] Added morning HIIT.";
    let r = detector.detect(v2, v1);
    assert!(
        !r.positive,
        "appended-update flagged as contradiction: evidence={:?}",
        r.evidence,
    );
}

#[test]
fn regression_correction_phrase_is_caught() {
    // The whole reason we wrote the contradiction detector: when a user
    // saves a corrected version of an earlier memory, we must catch it
    // so reconsolidation can demote the stale one.
    let detector = HeuristicContradictionDetector::new();
    let cases: &[(&str, &str, &str)] = &[
        (
            "Actually, the deploy command is `make release`, not `make deploy`.",
            "The deploy command is `make deploy`.",
            "explicit 'actually' correction",
        ),
        (
            "Correction: the meeting is on Thursday, not Wednesday.",
            "Team meeting is on Wednesday.",
            "'correction:' prefix",
        ),
        (
            "I was wrong — the cluster runs Kubernetes 1.32, not 1.30.",
            "The cluster runs Kubernetes 1.30.",
            "'I was wrong' phrase",
        ),
        (
            "Sprostowanie: spotkanie jest w czwartek, nie w środę.",
            "Spotkanie jest w środę.",
            "Polish 'sprostowanie'",
        ),
        (
            "Tak naprawdę kompilator Rust to teraz 1.95, nie 1.92.",
            "Aktualny kompilator Rust to 1.92.",
            "Polish 'tak naprawdę'",
        ),
    ];

    for (new, old, why) in cases {
        let r = detector.detect(new, old);
        assert!(
            r.positive,
            "missed correction ({why}): new={new:?} old={old:?}"
        );
    }
}

#[test]
fn regression_opinion_with_factual_context() {
    // From production: a memory that *contains* a factual fragment but
    // is clearly an opinion overall. We must not let factual phrasing
    // overpower the opinion frame.
    let detector = HeuristicOpinionDetector::new();
    let r = detector.detect(
        "PostgreSQL 18 was released in September 2025. Honestly, I think \
         the async I/O improvements aren't as impactful as the marketing \
         suggests.",
    );
    assert!(
        r.positive,
        "missed opinion in mixed factual/opinion: evidence={:?}",
        r.evidence
    );
}

#[test]
fn regression_polish_idiomatic_todo() {
    let detector = HeuristicFutureRelevanceDetector::new();
    let cases: &[&str] = &[
        "Trzeba pamiętać o aktualizacji deps przed świętami.",
        "Pamiętaj zarezerwować salę przed czwartkiem.",
        "Do zrobienia: dodaj retry do klienta HTTP.",
        "Muszę napisać post-mortem incydentu z piątku.",
    ];
    for content in cases {
        let r = detector.detect(content);
        assert!(r.positive, "missed Polish TODO: {content:?}");
    }
}

// ====================================================================
// Calibration — confidence should correlate with correctness on average
// ====================================================================

#[test]
fn calibration_high_confidence_implies_high_accuracy() {
    // Across all three detectors, when the heuristic says "very confident
    // positive" (≥0.8), it should be right at least 70% of the time.
    // The 70% bar is intentionally not 100% — heuristics will mispredict;
    // this test catches the case where the detector says "confident" while
    // actually being a coin flip.
    let cdet = HeuristicContradictionDetector::new();
    let odet = HeuristicOpinionDetector::new();
    let fdet = HeuristicFutureRelevanceDetector::new();
    let c_rep = EvalRunner::run_contradiction(&cdet, data::contradiction_dataset());
    let o_rep = EvalRunner::run_opinion(&odet, data::opinion_dataset());
    let f_rep = EvalRunner::run_future_relevance(&fdet, data::future_relevance_dataset());

    for report in [&c_rep, &o_rep, &f_rep] {
        // Aggregate accuracy across the top two buckets (0.8-1.0).
        let mut high_conf_correct = 0_usize;
        let mut high_conf_total = 0_usize;
        for b in &report.calibration_buckets {
            if b.lower >= 0.8 {
                high_conf_correct += (b.mean_accuracy * b.count as f32).round() as usize;
                high_conf_total += b.count;
            }
        }
        // Only assert if the detector actually emitted high-confidence
        // predictions — otherwise the calibration claim is vacuous.
        if high_conf_total >= 5 {
            let acc = high_conf_correct as f32 / high_conf_total as f32;
            assert!(
                acc >= 0.70,
                "{}: high-confidence (≥0.8) accuracy {:.3} < 0.70 \
                 (n_high={})",
                report.detector_name,
                acc,
                high_conf_total,
            );
        }
    }
}

// ====================================================================
// Aggregate sanity — total counts add up
// ====================================================================

#[test]
fn report_counts_are_internally_consistent() {
    let detector = HeuristicContradictionDetector::new();
    let report = EvalRunner::run_contradiction(&detector, data::contradiction_dataset());
    assert_eq!(
        report.total,
        report.true_positives
            + report.false_positives
            + report.true_negatives
            + report.false_negatives,
        "TP+FP+TN+FN != total",
    );
    // Failures must equal FP + FN.
    assert_eq!(
        report.failures.len(),
        report.false_positives + report.false_negatives,
    );
    // Calibration buckets sum to total.
    let bucket_sum: usize = report.calibration_buckets.iter().map(|b| b.count).sum();
    assert_eq!(bucket_sum, report.total);
}

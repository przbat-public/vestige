//! # Arm-vs-baseline behaviour tests (Phase 7.6)
//!
//! Each test here runs one arm on real product code and, where it compares,
//! contrasts it with a **baseline implemented in this file**. What that means
//! for reading a green run: these are *behaviour* tests - every assertion is
//! about the numbers two arms produce on one input, none about throughput or
//! wall-clock speed.
//!
//! 1. **FSRS-6 vs SM-2** — the FSRS arm is the production
//!    `vestige_core::fsrs::FSRSScheduler` (weights, rating paths, lapse path,
//!    interval formula, retrievability). The SM-2 arm is a local
//!    implementation ([`sm2_review`], [`sm2_retention`]) and is the baseline.
//!    A test comparing them therefore pins the *behaviour of two deterministic
//!    algorithms*, not product superiority over a real SM-2 deployment; the
//!    published accuracy of FSRS over SM-2 comes from the open
//!    srs-benchmark, not from this file.
//! 2. **Spreading activation vs similarity** — the activation arm is the
//!    production `ActivationNetwork` (also used by the search pipeline's
//!    finalize stage); the similarity arm is the local [`SimilaritySearch`]
//!    mock over hand-written vectors, chosen so that the 1-hop/2-hop/3-hop
//!    contrast is exactly constructible. It is not the product's HNSW path.
//! 3. **Retroactive importance** — production `SynapticTaggingSystem`. There
//!    is no baseline arm in these tests; they pin what the capture window
//!    captures.
//! 4. **Hippocampal indexing** — production `HippocampalIndex`. "Two-phase"
//!    names the compressed-index design; no flat-search arm is timed here.
//!
//! An earlier revision of this file reimplemented FSRS-6 in the test
//! (`fsrs6_review`/`fsrs6_interval`/`FSRS6_WEIGHTS`) and asserted
//! `fsrs6_retention >= 0.85` on the local copy, so a regression in the
//! production scheduler could not have failed any test in it. The FSRS arm is
//! now the scheduler; assertions are directional comparisons between the two
//! arms' actual outputs on identical inputs, with the numbers printed.
//!
//! Reference papers:
//! - FSRS: https://github.com/open-spaced-repetition/fsrs4anki
//! - SM-2: Pimsleur, P. (1967) / Wozniak & Gorzelanczyk (1994)
//! - Spreading Activation: Collins & Loftus (1975)
//! - Synaptic Tagging: Frey & Morris (1997), Redondo & Morris (2011)
//! - Hippocampal Indexing: Teyler & Rudy (2007)

use chrono::{Duration, Utc};
use std::collections::{HashMap, HashSet};

// Production FSRS-6 scheduler: the arm under test in every `fsrs6_*` test.
use vestige_core::fsrs::{
    FSRSParameters, FSRSScheduler, FSRSState, Rating, next_interval_with_decay,
    retrievability_with_decay,
};
use vestige_core::neuroscience::hippocampal_index::{
    BarcodeGenerator, ContentPointer, ContentType, HippocampalIndex, HippocampalIndexConfig,
    INDEX_EMBEDDING_DIM, IndexQuery, MemoryBarcode,
};
use vestige_core::neuroscience::spreading_activation::{
    ActivationConfig, ActivationNetwork, LinkType,
};
use vestige_core::neuroscience::synaptic_tagging::{
    CaptureWindow, ImportanceEvent, ImportanceEventType, SynapticTaggingConfig,
    SynapticTaggingSystem,
};

/// Build the production scheduler used by the FSRS arm.
///
/// `enable_fuzz: false` is what makes the arm deterministic: with fuzzing on,
/// the next interval depends on `state.last_review`, which `FSRSScheduler::review`
/// stamps with `Utc::now()`, so the same input sequence would produce different
/// intervals on different runs and the arm-vs-baseline comparisons below would
/// not be reproducible. `desired_retention` stays at the product default of 0.9.
fn production_scheduler() -> FSRSScheduler {
    FSRSScheduler::new(FSRSParameters {
        enable_fuzz: false,
        ..FSRSParameters::default()
    })
}

// ============================================================================
// BASELINE ARM: SM-2 (implemented in this file, NOT product code)
// ============================================================================
//
// The baseline arm of every `*_vs_local_sm2_*` test is this implementation:
// the 1987 SM-2 interval rule with the original EF update. It is written here
// on purpose — there is no SM-2 in the product to call.
//
// `sm2_retention` below is *not* part of SM-2: the algorithm defines intervals,
// never a recall curve. It is a local approximation (linear ramp before the due
// date, exponential decay after it) whose only role is to put a number on the
// baseline arm's implied retention so it can be printed next to the production
// scheduler's own curve. A comparison between the two curves says that two
// formulas disagree; it is not evidence about either one's real-world recall.

/// SM-2 state for a card
#[derive(Debug, Clone)]
struct SM2State {
    easiness_factor: f64, // EF, starts at 2.5
    interval: i32,        // Days until next review
    repetitions: i32,     // Number of successful reviews
}

impl Default for SM2State {
    fn default() -> Self {
        Self {
            easiness_factor: 2.5,
            interval: 0,
            repetitions: 0,
        }
    }
}

/// SM-2 grade (0-5)
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum SM2Grade {
    CompleteBlackout = 0,
    Incorrect = 1,
    IncorrectRemembered = 2,
    CorrectDifficult = 3,
    CorrectHesitation = 4,
    Perfect = 5,
}

impl SM2Grade {
    fn as_i32(&self) -> i32 {
        *self as i32
    }
}

/// Classic SM-2 algorithm implementation
fn sm2_review(state: &SM2State, grade: SM2Grade) -> SM2State {
    let q = grade.as_i32();

    // Update easiness factor
    let mut new_ef =
        state.easiness_factor + (0.1 - (5 - q) as f64 * (0.08 + (5 - q) as f64 * 0.02));
    new_ef = new_ef.max(1.3); // EF never goes below 1.3

    if q < 3 {
        // Failed - restart learning
        SM2State {
            easiness_factor: new_ef,
            interval: 1,
            repetitions: 0,
        }
    } else {
        // Success
        let new_interval = match state.repetitions {
            0 => 1,
            1 => 6,
            _ => (state.interval as f64 * state.easiness_factor).round() as i32,
        };

        SM2State {
            easiness_factor: new_ef,
            interval: new_interval,
            repetitions: state.repetitions + 1,
        }
    }
}

/// Calculate the baseline's implied retention after elapsed time (local model,
/// not part of SM-2 - see the section header).
fn sm2_retention(interval: i32, elapsed_days: i32) -> f64 {
    if elapsed_days <= interval {
        // Not yet due - assume high retention
        0.9 + 0.1 * (1.0 - elapsed_days as f64 / interval as f64)
    } else {
        // Overdue - exponential decay
        let overdue_ratio = elapsed_days as f64 / interval as f64;
        0.9 * (-0.5 * (overdue_ratio - 1.0)).exp()
    }
}

// ============================================================================
// BASELINE ARM: Leitner boxes (this file only)
// ============================================================================

/// Leitner box state
#[derive(Debug, Clone)]
struct LeitnerState {
    box_number: i32, // 1-5
}

impl Default for LeitnerState {
    fn default() -> Self {
        Self { box_number: 1 }
    }
}

/// Leitner box intervals
fn leitner_interval(box_number: i32) -> i32 {
    match box_number {
        1 => 1,
        2 => 2,
        3 => 5,
        4 => 8,
        5 => 14,
        _ => 14,
    }
}

/// Leitner review
fn leitner_review(state: &LeitnerState, correct: bool) -> LeitnerState {
    if correct {
        LeitnerState {
            box_number: (state.box_number + 1).min(5),
        }
    } else {
        LeitnerState { box_number: 1 }
    }
}

// ============================================================================
// BASELINE ARMS: fixed 7-day cadence (this file only)
// ============================================================================

/// Fixed interval baseline - always reviews at the same interval, ignoring
/// whether the last recall succeeded.
fn fixed_interval_schedule(_correct: bool) -> i32 {
    7 // Always 7 days
}

// ============================================================================
// BASELINE ARM: cosine-similarity search mock (this file only)
// ============================================================================
//
// Hand-written vectors and a plain cosine ranking. The product's retrieval
// path is FTS5 + HNSW + RRF inside `Storage`; this mock exists so the hop-count
// contrast with `ActivationNetwork` is exactly constructible (e.g. two vectors
// can be made orthogonal), which the real embedder of hand-written memories
// cannot guarantee.

/// Mock similarity search that only uses direct embedding similarity
struct SimilaritySearch {
    embeddings: HashMap<String, Vec<f32>>,
}

impl SimilaritySearch {
    fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
        }
    }

    fn add(&mut self, id: &str, embedding: Vec<f32>) {
        self.embeddings.insert(id.to_string(), embedding);
    }

    fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<(String, f64)> {
        let mut results: Vec<(String, f64)> = self
            .embeddings
            .iter()
            .map(|(id, emb)| {
                let sim = cosine_similarity(query_embedding, emb);
                (id.clone(), sim)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        (dot / (norm_a * norm_b)) as f64
    } else {
        0.0
    }
}

// ============================================================================
// FSRS-6 (production scheduler) vs SM-2 (local baseline) - 7 tests
//
// Every test in this section drives `FSRSScheduler` - the same type `Storage`
// holds for its review path, so a change to the product's scheduler moves these
// numbers. The SM-2 arm is always the local baseline defined above, and every
// comparison prints both arms' numbers. Nothing here measures recall in humans.
// ============================================================================

/// Same review sequence through both arms: 10 successful recalls, always
/// "recalled with effort" (production `Rating::Good`, baseline q = 4 - the
/// matching grade on SM-2's 0-5 scale).
///
/// Pinned: the production scheduler reviews at its own 0.9 design target (the
/// `retrievability(S, t = interval) = 0.9` calibration), its interval grows
/// monotonically on a lapse-free sequence, it reaches a longer interval than
/// the baseline at the same review count, and at the baseline's own horizon the
/// production curve still predicts more retention than the baseline's local
/// curve. The last one compares two formulas, not two measurements of recall -
/// the baseline's curve is this file's invention (SM-2 has no recall model).
#[test]
fn test_fsrs6_scheduler_vs_local_sm2_baseline_identical_successful_reviews() {
    const REVIEWS: usize = 10;

    let scheduler = production_scheduler();
    let w20 = scheduler.params().weights[20];

    let mut fsrs_state = scheduler.new_card();
    let mut elapsed_days = 0.0_f64;
    let mut fsrs_intervals = Vec::with_capacity(REVIEWS);

    for round in 0..REVIEWS {
        let outcome = scheduler.review(&fsrs_state, Rating::Good, elapsed_days, None);
        // Round 0 is the first review of a New card: no interval to be due at
        // yet, so retrievability is reported as 1.0. Every later review is due
        // at the previous scheduled interval, i.e. at t = S, where the FSRS-6
        // curve is 0.9 by construction; whole-day rounding of the interval is
        // what allows the small deviation.
        if round > 0 {
            assert!(
                (outcome.retrievability - 0.9).abs() < 0.02,
                "round {round}: the production scheduler must review at its 0.9 target, got {:.4}",
                outcome.retrievability
            );
        }
        fsrs_intervals.push(outcome.interval);
        fsrs_state = outcome.state;
        elapsed_days = outcome.interval as f64;
    }

    let mut sm2_state = SM2State::default();
    let mut sm2_intervals = Vec::with_capacity(REVIEWS);
    for _ in 0..REVIEWS {
        sm2_state = sm2_review(&sm2_state, SM2Grade::CorrectHesitation);
        sm2_intervals.push(sm2_state.interval);
    }

    let fsrs_final_interval = *fsrs_intervals.last().unwrap();
    let sm2_final_interval = *sm2_intervals.last().unwrap();
    let fsrs_retention_at = |days: f64| retrievability_with_decay(fsrs_state.stability, days, w20);

    println!(
        "FSRS (production) after {REVIEWS} successful recalls: stability {:.1}d, intervals {fsrs_intervals:?}",
        fsrs_state.stability
    );
    println!("SM-2 (local baseline) intervals: {sm2_intervals:?}");
    println!(
        "implied retention at the baseline horizon ({sm2_final_interval}d): production FSRS-6 {:.4} vs local SM-2 curve {:.4}",
        fsrs_retention_at(sm2_final_interval as f64),
        sm2_retention(sm2_final_interval, sm2_final_interval)
    );
    println!(
        "implied retention at the production horizon ({fsrs_final_interval}d): production FSRS-6 {:.4} vs local SM-2 curve {:.4}",
        fsrs_retention_at(fsrs_final_interval as f64),
        sm2_retention(sm2_final_interval, fsrs_final_interval)
    );

    assert!(
        fsrs_intervals.windows(2).all(|pair| pair[1] > pair[0]),
        "the production scheduler's interval must grow on a lapse-free sequence: {fsrs_intervals:?}"
    );
    assert!(
        fsrs_final_interval > sm2_final_interval,
        "after {REVIEWS} identical successful recalls the production scheduler should schedule a \
         longer interval than the local SM-2 baseline: {fsrs_final_interval}d vs {sm2_final_interval}d"
    );
    assert!(
        fsrs_retention_at(sm2_final_interval as f64)
            > sm2_retention(sm2_final_interval, sm2_final_interval),
        "at the baseline's own scheduled interval the production curve should still predict more \
         retention than the baseline's local curve (model-implied, not empirical): {:.4} vs {:.4}",
        fsrs_retention_at(sm2_final_interval as f64),
        sm2_retention(sm2_final_interval, sm2_final_interval)
    );
}

/// 100 cards over 30 days, reviewed whenever due, always successful. Each arm's
/// own scheduled interval decides the next due day, so this is the original
/// "FSRS-6 needs fewer reviews than SM-2 for the same learning window" claim,
/// measured on the production scheduler instead of a local copy of FSRS-6.
///
/// Pinned: the production arm does not need *more* reviews than the baseline for
/// the same window. Both counts are printed; with the current weights the arms
/// tie at 4 reviews per card, so read this as "no worse", not "better".
#[test]
fn test_fsrs6_scheduler_vs_local_sm2_review_count_over_30_days() {
    const CARDS: usize = 100;
    const DAYS: i32 = 30;

    let scheduler = production_scheduler();

    let mut sm2_reviews = 0_usize;
    let mut sm2_cards: Vec<(SM2State, i32)> =
        (0..CARDS).map(|_| (SM2State::default(), 0)).collect();
    for day in 1..=DAYS {
        for (state, next_due) in sm2_cards.iter_mut() {
            if *next_due <= day {
                sm2_reviews += 1;
                *state = sm2_review(state, SM2Grade::CorrectHesitation);
                *next_due = day + state.interval;
            }
        }
    }

    let mut fsrs_reviews = 0_usize;
    let mut fsrs_cards: Vec<(FSRSState, i32)> =
        (0..CARDS).map(|_| (scheduler.new_card(), 0)).collect();
    for day in 1..=DAYS {
        for (state, next_due) in fsrs_cards.iter_mut() {
            if *next_due <= day {
                fsrs_reviews += 1;
                // Reviewed on the due day: one day since the last review, never
                // the same-day branch.
                let outcome = scheduler.review(state, Rating::Good, 1.0, None);
                *state = outcome.state;
                *next_due = day + outcome.interval.max(1);
            }
        }
    }

    println!(
        "{CARDS} cards over {DAYS} days: production FSRS-6 {fsrs_reviews} reviews, local SM-2 baseline {sm2_reviews} reviews"
    );

    assert!(
        fsrs_reviews > 0,
        "the production arm must have scheduled at least one review"
    );
    assert!(
        fsrs_reviews <= sm2_reviews,
        "the production scheduler must not need more reviews than the local SM-2 baseline for the \
         same {DAYS}-day window: {fsrs_reviews} vs {sm2_reviews}"
    );
}

/// One card, 30 days, lapse-free, production scheduler vs the two fixed-cadence
/// local baselines in this file: a constant 7-day interval and Leitner boxes,
/// both driven over the same window.
///
/// Pinned: the production arm's interval grows with each successful recall and
/// ends up longer than both fixed cadences - which is what "adaptive beats
/// fixed" means once the fixed arms are the ones written down here.
#[test]
fn test_fsrs6_scheduler_expands_intervals_past_local_fixed_cadences() {
    const SIMULATION_DAYS: i32 = 30;

    let scheduler = production_scheduler();
    let mut state = scheduler.new_card();
    let mut next_due = 1;
    let mut intervals = Vec::new();

    for day in 1..=SIMULATION_DAYS {
        if day >= next_due {
            let outcome = scheduler.review(&state, Rating::Good, 1.0, None);
            intervals.push(outcome.interval);
            state = outcome.state;
            next_due = day + outcome.interval.max(1);
        }
    }
    let production_reviews = intervals.len();
    let final_interval = *intervals.last().unwrap_or(&0);

    // Leitner baseline over the same window: every recall succeeds, so the card
    // climbs one box per review until box 5, whose interval is the ceiling.
    let mut leitner_state = LeitnerState::default();
    let mut leitner_next = 1;
    let mut leitner_reviews = 0_usize;
    for day in 1..=SIMULATION_DAYS {
        if day >= leitner_next {
            leitner_reviews += 1;
            leitner_state = leitner_review(&leitner_state, true);
            leitner_next = day + leitner_interval(leitner_state.box_number);
        }
    }
    let leitner_ceiling = leitner_interval(leitner_state.box_number);
    let fixed_interval = fixed_interval_schedule(true);

    println!(
        "production intervals over {SIMULATION_DAYS} days: {intervals:?} ({production_reviews} reviews); \
         Leitner reached box {} with a {leitner_ceiling}d ceiling ({leitner_reviews} reviews); fixed cadence {fixed_interval}d",
        leitner_state.box_number
    );

    assert!(
        intervals.len() >= 2,
        "the production arm must have scheduled more than one review: {intervals:?}"
    );
    assert!(
        intervals.windows(2).all(|pair| pair[1] > pair[0]),
        "each successful recall must lengthen the production interval: {intervals:?}"
    );
    assert!(
        final_interval > fixed_interval,
        "the production interval must pass the fixed {fixed_interval}-day cadence: {final_interval}d"
    );
    assert!(
        final_interval > leitner_ceiling,
        "the production interval must pass the Leitner ceiling of box {} ({leitner_ceiling}d): {final_interval}d",
        leitner_state.box_number
    );
}

/// From one identical card state, the three recall ratings must order stability
/// and difficulty the way the FSRS-6 update prescribes: Hard < Good < Easy in
/// stability, Hard > Good > Easy in difficulty.
#[test]
fn test_fsrs6_scheduler_recall_ratings_order_stability() {
    let scheduler = production_scheduler();
    let first = scheduler.review(&scheduler.new_card(), Rating::Good, 0.0, None);
    let elapsed = first.interval as f64;

    let hard = scheduler.review(&first.state, Rating::Hard, elapsed, None);
    let good = scheduler.review(&first.state, Rating::Good, elapsed, None);
    let easy = scheduler.review(&first.state, Rating::Easy, elapsed, None);

    println!(
        "same card (stability {:.2}d, difficulty {:.3}), elapsed {elapsed}d: hard {:.2}d/{:.3}, good {:.2}d/{:.3}, easy {:.2}d/{:.3}",
        first.state.stability,
        first.state.difficulty,
        hard.state.stability,
        hard.state.difficulty,
        good.state.stability,
        good.state.difficulty,
        easy.state.stability,
        easy.state.difficulty
    );

    assert!(
        hard.state.stability < good.state.stability,
        "Hard must grow stability less than Good: {:.2} vs {:.2}",
        hard.state.stability,
        good.state.stability
    );
    assert!(
        good.state.stability < easy.state.stability,
        "Good must grow stability less than Easy: {:.2} vs {:.2}",
        good.state.stability,
        easy.state.stability
    );
    assert!(
        hard.state.difficulty > good.state.difficulty,
        "Hard must raise difficulty above Good: {:.3} vs {:.3}",
        hard.state.difficulty,
        good.state.difficulty
    );
    assert!(
        good.state.difficulty > easy.state.difficulty,
        "Good must leave difficulty above Easy: {:.3} vs {:.3}",
        good.state.difficulty,
        easy.state.difficulty
    );
}

/// The same-day branch (`elapsed < 1`) is separate from the multi-day recall
/// path in `FSRSScheduler::review`.
///
/// Pinned: a same-day Easy grows stability, a same-day Again shrinks it, and
/// same-day Good moves it less than Easy does. A passing same-day repeat is
/// clamped to `sinc >= 1` by the production formula, so Good cannot shorten the
/// schedule; the assertion that Good moves stability less than Easy is what
/// would catch that clamp being dropped.
#[test]
fn test_fsrs6_scheduler_same_day_review_path() {
    let scheduler = production_scheduler();
    let mut state = scheduler.new_card();
    let mut elapsed = 0.0_f64;

    // Three normal recalls leave the card in the Review state with a mid-range
    // stability, which is when a same-day repeat is possible at all.
    for _ in 0..3 {
        let outcome = scheduler.review(&state, Rating::Good, elapsed, None);
        state = outcome.state;
        elapsed = outcome.interval as f64;
    }
    let base_stability = state.stability;

    let same_day = |rating: Rating| scheduler.review(&state, rating, 0.5, None);
    let again = same_day(Rating::Again);
    let good = same_day(Rating::Good);
    let easy = same_day(Rating::Easy);

    println!(
        "same-day review (elapsed 0.5d) from stability {base_stability:.2}d: again {:.2}d, good {:.2}d, easy {:.2}d",
        again.state.stability, good.state.stability, easy.state.stability
    );

    assert!(
        easy.state.stability > base_stability,
        "a same-day Easy must grow stability: {:.2} vs {:.2}",
        easy.state.stability,
        base_stability
    );
    assert!(
        again.state.stability < base_stability,
        "a same-day Again must shrink stability: {:.2} vs {:.2}",
        again.state.stability,
        base_stability
    );
    assert!(
        (good.state.stability - base_stability).abs()
            < (easy.state.stability - base_stability).abs(),
        "a same-day Good must move stability less than Easy: |{:.2} - {:.2}| vs |{:.2} - {:.2}|",
        good.state.stability,
        base_stability,
        easy.state.stability,
        base_stability
    );
}

/// A single `Again` after a long successful run. The production lapse path
/// multiplies stability down through the FSRS-6 forgetting formula but keeps a
/// non-trivial interval; the local SM-2 baseline resets repetitions to 0 and
/// the interval to 1 day.
///
/// Pinned: both behaviours, side by side, with the numbers printed. This is a
/// statement about the two schedulers' lapse rules, not about which one is
/// right for a given user.
#[test]
fn test_fsrs6_scheduler_lapse_vs_local_sm2_reset() {
    const REVIEWS: usize = 10;

    let scheduler = production_scheduler();
    let mut fsrs_state = scheduler.new_card();
    let mut elapsed_days = 0.0_f64;
    for _ in 0..REVIEWS {
        let outcome = scheduler.review(&fsrs_state, Rating::Good, elapsed_days, None);
        fsrs_state = outcome.state;
        elapsed_days = outcome.interval as f64;
    }
    let stability_before = fsrs_state.stability;
    let difficulty_before = fsrs_state.difficulty;

    let lapse = scheduler.review(&fsrs_state, Rating::Again, elapsed_days, None);

    let mut sm2_state = SM2State::default();
    for _ in 0..REVIEWS {
        sm2_state = sm2_review(&sm2_state, SM2Grade::CorrectHesitation);
    }
    let sm2_lapse = sm2_review(&sm2_state, SM2Grade::Incorrect);

    println!(
        "lapse after {REVIEWS} successful recalls: production FSRS-6 stability {stability_before:.1}d -> {:.1}d, interval {}d, difficulty {difficulty_before:.3} -> {:.3}",
        lapse.state.stability, lapse.interval, lapse.state.difficulty
    );
    println!(
        "local SM-2 baseline: interval {}d, repetitions {}, EF {:.3}",
        sm2_lapse.interval, sm2_lapse.repetitions, sm2_lapse.easiness_factor
    );

    assert!(
        lapse.is_lapse,
        "a failed review of a card in the Review state must be reported as a lapse"
    );
    assert!(
        lapse.state.stability < stability_before,
        "a lapse must reduce stability: {:.1} -> {:.1}",
        stability_before,
        lapse.state.stability
    );
    assert!(
        lapse.state.stability > 0.0,
        "the lapse path must not zero out stability: {:.3}",
        lapse.state.stability
    );
    assert!(
        lapse.interval > sm2_lapse.interval,
        "the production lapse path keeps some of the learned stability, unlike the baseline's \
         reset to 1 day: {}d vs {}d",
        lapse.interval,
        sm2_lapse.interval
    );
}

/// Pure formula property of the production FSRS-6 curve: the personalizable
/// decay weight w20 reshapes the forgetting function while the curve stays
/// pinned at R = 0.9 when t = S. A flatter curve (lower w20) must imply both
/// higher retention past the scheduled interval and a longer interval for the
/// same target retention.
///
/// This is the mathematical content behind "personalization". Whether fitting
/// w20 to a real review log improves a real user's recall needs that log and a
/// ground-truth recall measurement; neither exists in this test.
#[test]
fn test_fsrs6_decay_weight_reshapes_the_forgetting_curve() {
    const STABILITY: f64 = 10.0;
    const PAST_OPTIMAL: f64 = 15.0; // 1.5 x stability
    const TARGET_RETENTION: f64 = 0.85;

    let default_w20 = production_scheduler().params().weights[20];
    let flat_w20 = 0.08; // slower decay
    let steep_w20 = 0.35; // faster decay

    for w20 in [flat_w20, default_w20, steep_w20] {
        let at_optimal = retrievability_with_decay(STABILITY, STABILITY, w20);
        assert!(
            (at_optimal - 0.9).abs() < 1e-9,
            "every decay weight must keep the curve pinned at 0.9 when t = S, got {at_optimal} for w20 = {w20}"
        );
    }

    let flat_retention = retrievability_with_decay(STABILITY, PAST_OPTIMAL, flat_w20);
    let default_retention = retrievability_with_decay(STABILITY, PAST_OPTIMAL, default_w20);
    let steep_retention = retrievability_with_decay(STABILITY, PAST_OPTIMAL, steep_w20);

    let flat_interval = next_interval_with_decay(STABILITY, TARGET_RETENTION, flat_w20);
    let default_interval = next_interval_with_decay(STABILITY, TARGET_RETENTION, default_w20);
    let steep_interval = next_interval_with_decay(STABILITY, TARGET_RETENTION, steep_w20);

    println!(
        "stability {STABILITY}d at t = {PAST_OPTIMAL}d: retention flat(w20={flat_w20}) {flat_retention:.4}, default({default_w20}) {default_retention:.4}, steep(w20={steep_w20}) {steep_retention:.4}"
    );
    println!(
        "interval for {TARGET_RETENTION} retention: flat {flat_interval}d, default {default_interval}d, steep {steep_interval}d"
    );

    assert!(
        flat_retention > default_retention && default_retention > steep_retention,
        "past the scheduled interval a flatter curve must retain more: {flat_retention:.4} > {default_retention:.4} > {steep_retention:.4}"
    );
    assert!(
        flat_interval > default_interval && default_interval > steep_interval,
        "a flatter curve must be allowed a longer interval for the same target retention: {flat_interval} > {default_interval} > {steep_interval}"
    );
}

// ============================================================================
// SPREADING ACTIVATION (production) vs COSINE MOCK (this file) - 8 tests
//
// The activation arm is `ActivationNetwork`, the same type the product's
// search finalize stage calls. The similarity arm is the `SimilaritySearch`
// mock above over hand-written vectors - it exists to make the hop-count
// contrast exact, not to stand in for the product's FTS5 + HNSW + RRF path.
// ============================================================================

/// 1-hop: production activation and the local cosine mock both reach the two
/// direct neighbours of the seed.
#[test]
fn test_activation_network_vs_local_cosine_mock_1_hop() {
    // Setup spreading activation network
    let mut network = ActivationNetwork::new();
    network.add_edge(
        "rust".to_string(),
        "cargo".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "rust".to_string(),
        "ownership".to_string(),
        LinkType::Semantic,
        0.85,
    );

    // Setup similarity search with similar embeddings
    let mut sim_search = SimilaritySearch::new();
    sim_search.add("rust", vec![1.0, 0.0, 0.0]);
    sim_search.add("cargo", vec![0.9, 0.1, 0.0]); // Similar to rust
    sim_search.add("ownership", vec![0.85, 0.15, 0.0]); // Similar to rust
    sim_search.add("python", vec![0.0, 1.0, 0.0]); // Unrelated

    // Spreading activation
    let spreading_results = network.activate("rust", 1.0);
    let spreading_found: HashSet<_> = spreading_results
        .iter()
        .map(|r| r.memory_id.as_str())
        .collect();

    // Similarity search
    let sim_results = sim_search.search(&[1.0, 0.0, 0.0], 3);
    let sim_found: HashSet<_> = sim_results
        .iter()
        .filter(|(_, score)| *score > 0.8)
        .map(|(id, _)| id.as_str())
        .collect();

    // At 1-hop, both should find the direct connections
    assert!(
        spreading_found.contains("cargo"),
        "Spreading should find cargo"
    );
    assert!(
        spreading_found.contains("ownership"),
        "Spreading should find ownership"
    );
    assert!(sim_found.contains("cargo"), "Similarity should find cargo");
    assert!(
        sim_found.contains("ownership"),
        "Similarity should find ownership"
    );
}

/// 2-hop: production activation reaches through the bridge; the local cosine
/// mock finds the bridge but not what lies behind it.
#[test]
fn test_activation_network_vs_local_cosine_mock_2_hop() {
    let config = ActivationConfig {
        decay_factor: 0.8,
        max_hops: 3,
        min_threshold: 0.1,
        allow_cycles: false,
    };
    let mut network = ActivationNetwork::with_config(config);

    // Create a chain: rust -> tokio -> async_runtime
    // rust and async_runtime have NO direct similarity
    network.add_edge(
        "rust".to_string(),
        "tokio".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "tokio".to_string(),
        "async_runtime".to_string(),
        LinkType::Semantic,
        0.85,
    );

    // Similarity search - embeddings show NO similarity between rust and async_runtime
    let mut sim_search = SimilaritySearch::new();
    sim_search.add("rust", vec![1.0, 0.0, 0.0, 0.0]);
    sim_search.add("tokio", vec![0.7, 0.7, 0.0, 0.0]); // Bridge
    sim_search.add("async_runtime", vec![0.0, 1.0, 0.0, 0.0]); // No similarity to rust

    // Spreading finds async_runtime through the chain
    let spreading_results = network.activate("rust", 1.0);
    let spreading_found_async = spreading_results
        .iter()
        .any(|r| r.memory_id == "async_runtime");

    // Similarity from "rust" does NOT find async_runtime
    let sim_results = sim_search.search(&[1.0, 0.0, 0.0, 0.0], 5);
    let sim_found_async = sim_results
        .iter()
        .any(|(id, score)| id == "async_runtime" && *score > 0.5);

    assert!(
        spreading_found_async,
        "Spreading activation SHOULD find async_runtime through tokio"
    );
    assert!(
        !sim_found_async,
        "Similarity search should NOT find async_runtime (no direct similarity)"
    );
}

/// 3-hop: production activation reaches the far end of a pure chain whose
/// vector is orthogonal to the seed, so the local cosine mock cannot.
#[test]
fn test_activation_network_vs_local_cosine_mock_3_hop() {
    let config = ActivationConfig {
        decay_factor: 0.8,
        max_hops: 4,
        min_threshold: 0.05,
        allow_cycles: false,
    };
    let mut network = ActivationNetwork::with_config(config);

    // Create 3-hop chain: A -> B -> C -> D
    // Each step has semantic connection, but A and D have ZERO direct similarity
    network.add_edge(
        "concept_a".to_string(),
        "concept_b".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "concept_b".to_string(),
        "concept_c".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "concept_c".to_string(),
        "concept_d".to_string(),
        LinkType::Semantic,
        0.9,
    );

    // Embeddings: A and D are orthogonal (zero similarity)
    let mut sim_search = SimilaritySearch::new();
    sim_search.add("concept_a", vec![1.0, 0.0, 0.0, 0.0]);
    sim_search.add("concept_b", vec![0.7, 0.7, 0.0, 0.0]);
    sim_search.add("concept_c", vec![0.0, 0.7, 0.7, 0.0]);
    sim_search.add("concept_d", vec![0.0, 0.0, 0.0, 1.0]); // Orthogonal to A

    // Spreading finds D
    let spreading_results = network.activate("concept_a", 1.0);
    let d_result = spreading_results
        .iter()
        .find(|r| r.memory_id == "concept_d");

    assert!(
        d_result.is_some(),
        "Spreading MUST find concept_d at 3 hops"
    );
    assert_eq!(
        d_result.unwrap().distance,
        3,
        "Should be exactly 3 hops away"
    );

    // Similarity CANNOT find D from A
    let sim_results = sim_search.search(&[1.0, 0.0, 0.0, 0.0], 10);
    let sim_d_score = sim_results
        .iter()
        .find(|(id, _)| id == "concept_d")
        .map(|(_, score)| *score)
        .unwrap_or(0.0);

    assert!(
        sim_d_score < 0.1,
        "Similarity should NOT find concept_d (orthogonal embedding): score = {:.4}",
        sim_d_score
    );
}

/// A debugging chain where the answer shares no vocabulary with the symptom:
/// production activation walks the chain, the local cosine mock cannot.
#[test]
fn test_activation_network_finds_chains_the_local_cosine_mock_misses() {
    let mut network = ActivationNetwork::new();

    // Real-world scenario: User debugging a memory leak
    // Chain: "memory_leak" -> "reference_counting" -> "Arc_Weak" -> "cyclic_references"
    // The solution (cyclic_references) is NOT semantically similar to "memory_leak"

    network.add_edge(
        "memory_leak".to_string(),
        "reference_counting".to_string(),
        LinkType::Causal,
        0.9,
    );
    network.add_edge(
        "reference_counting".to_string(),
        "arc_weak".to_string(),
        LinkType::Semantic,
        0.85,
    );
    network.add_edge(
        "arc_weak".to_string(),
        "cyclic_references".to_string(),
        LinkType::Semantic,
        0.9,
    );

    // The problem: "cyclic_references" has zero direct similarity to "memory_leak"
    // (they use completely different vocabulary)
    let mut sim_search = SimilaritySearch::new();
    sim_search.add("memory_leak", vec![1.0, 0.0, 0.0, 0.0]);
    sim_search.add("reference_counting", vec![0.5, 0.5, 0.0, 0.0]);
    sim_search.add("arc_weak", vec![0.0, 0.7, 0.3, 0.0]);
    sim_search.add("cyclic_references", vec![0.0, 0.0, 0.0, 1.0]); // Totally different!

    // Spreading activation finds the solution
    let spreading_results = network.activate("memory_leak", 1.0);
    let found_solution = spreading_results
        .iter()
        .any(|r| r.memory_id == "cyclic_references");

    // Similarity search cannot find it
    let sim_results = sim_search.search(&[1.0, 0.0, 0.0, 0.0], 10);
    let sim_found = sim_results
        .iter()
        .any(|(id, score)| id == "cyclic_references" && *score > 0.3);

    assert!(
        found_solution,
        "Spreading activation finds the solution (cyclic_references) through association"
    );
    assert!(
        !sim_found,
        "Similarity search CANNOT find cyclic_references from memory_leak"
    );
}

/// Production activation must report the concrete path it walked, with
/// activation decaying along it.
#[test]
fn test_spreading_path_quality() {
    let mut network = ActivationNetwork::new();

    // Create a knowledge graph about Rust error handling
    network.add_edge(
        "error_handling".to_string(),
        "result_type".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "result_type".to_string(),
        "question_mark_operator".to_string(),
        LinkType::Semantic,
        0.85,
    );
    network.add_edge(
        "question_mark_operator".to_string(),
        "early_return".to_string(),
        LinkType::Semantic,
        0.8,
    );

    let results = network.activate("error_handling", 1.0);

    // Find the path to early_return
    let early_return_result = results
        .iter()
        .find(|r| r.memory_id == "early_return")
        .expect("Should find early_return");

    // Verify the path makes sense
    assert_eq!(
        early_return_result.path.len(),
        4,
        "Path should have 4 nodes"
    );
    assert_eq!(early_return_result.path[0], "error_handling");
    assert_eq!(early_return_result.path[1], "result_type");
    assert_eq!(early_return_result.path[2], "question_mark_operator");
    assert_eq!(early_return_result.path[3], "early_return");

    // Activation should decay along the path
    let result_type_activation = results
        .iter()
        .find(|r| r.memory_id == "result_type")
        .map(|r| r.activation)
        .unwrap_or(0.0);

    assert!(
        early_return_result.activation < result_type_activation,
        "Activation should decay: early_return ({:.3}) < result_type ({:.3})",
        early_return_result.activation,
        result_type_activation
    );
}

/// Production activation at 1000 nodes / ~3000 edges: the reachable set must
/// be found, and fast enough to sit in the search pipeline.
#[test]
fn test_spreading_scale_performance() {
    let config = ActivationConfig {
        decay_factor: 0.7,
        max_hops: 3,
        min_threshold: 0.1,
        allow_cycles: false,
    };
    let mut network = ActivationNetwork::with_config(config);

    // Create a larger network (1000 nodes, ~3000 edges)
    const NUM_NODES: usize = 1000;
    const EDGES_PER_NODE: usize = 3;

    for i in 0..NUM_NODES {
        for j in 1..=EDGES_PER_NODE {
            let target = (i + j * 7) % NUM_NODES;
            if i != target {
                network.add_edge(
                    format!("node_{}", i),
                    format!("node_{}", target),
                    LinkType::Semantic,
                    0.8,
                );
            }
        }
    }

    // Measure activation time
    let start = std::time::Instant::now();
    let results = network.activate("node_0", 1.0);
    let duration = start.elapsed();

    // Should complete in reasonable time (< 100ms)
    assert!(
        duration.as_millis() < 100,
        "Spreading activation should be fast: {:?}",
        duration
    );

    // Reachable set, derived from the construction above instead of guessed:
    // every node has out-edges to i+7, i+14 and i+21, so from node_0 three hops
    // reach exactly the 3+3+3 distinct nodes below (each hop multiplies
    // activation by strength 0.8 and decay 0.7, i.e. 0.56 per hop, so the third
    // hop arrives at 0.1756 - above the 0.1 threshold).
    //
    // The previous assertion here was `results.len() > 10`, a number nothing in
    // this graph or in `activate` implies: the real count is 9, so the test
    // asserted something false about the production code and failed.
    let found: Vec<&str> = results.iter().map(|r| r.memory_id.as_str()).collect();
    for expected in [
        "node_7", "node_14", "node_21", "node_28", "node_35", "node_42", "node_49", "node_56",
        "node_63",
    ] {
        assert!(
            found.contains(&expected),
            "{expected} is within 3 hops of node_0 in this graph and must be activated; got {found:?}"
        );
    }
}

/// Production activation on dense vs sparse graphs: a denser neighbourhood
/// reaches more nodes, each with less activation.
#[test]
fn test_spreading_dense_vs_sparse() {
    // Dense network: Many connections per node
    let mut dense_network = ActivationNetwork::new();
    for i in 0..20 {
        for j in 0..20 {
            if i != j {
                dense_network.add_edge(
                    format!("dense_{}", i),
                    format!("dense_{}", j),
                    LinkType::Semantic,
                    0.5,
                );
            }
        }
    }

    // Sparse network: Few connections per node
    let mut sparse_network = ActivationNetwork::new();
    for i in 0..20 {
        let next = (i + 1) % 20;
        sparse_network.add_edge(
            format!("sparse_{}", i),
            format!("sparse_{}", next),
            LinkType::Semantic,
            0.9,
        );
    }

    // Dense network should spread widely but with lower individual activations
    let dense_results = dense_network.activate("dense_0", 1.0);
    let dense_activations: Vec<f64> = dense_results.iter().map(|r| r.activation).collect();

    // Sparse network should spread linearly with higher individual activations
    let sparse_results = sparse_network.activate("sparse_0", 1.0);
    let sparse_activations: Vec<f64> = sparse_results.iter().map(|r| r.activation).collect();

    // Dense should find more nodes
    assert!(
        dense_results.len() > sparse_results.len(),
        "Dense network should activate more nodes: {} vs {}",
        dense_results.len(),
        sparse_results.len()
    );

    // Sparse should have higher max activation (less dilution)
    let dense_max = dense_activations.iter().cloned().fold(0.0_f64, f64::max);
    let sparse_max = sparse_activations.iter().cloned().fold(0.0_f64, f64::max);

    assert!(
        sparse_max >= dense_max,
        "Sparse network should have higher peak activation: {:.3} vs {:.3}",
        sparse_max,
        dense_max
    );
}

/// Production activation must traverse every `LinkType` and report the type
/// that brought the activation in.
#[test]
fn test_spreading_mixed_link_types() {
    let mut network = ActivationNetwork::new();

    // Create edges with different link types
    network.add_edge(
        "event".to_string(),
        "semantic_relation".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "event".to_string(),
        "temporal_relation".to_string(),
        LinkType::Temporal,
        0.9,
    );
    network.add_edge(
        "event".to_string(),
        "causal_relation".to_string(),
        LinkType::Causal,
        0.9,
    );
    network.add_edge(
        "event".to_string(),
        "spatial_relation".to_string(),
        LinkType::Spatial,
        0.9,
    );

    let results = network.activate("event", 1.0);

    // Should find all related nodes
    let found_ids: HashSet<_> = results.iter().map(|r| r.memory_id.as_str()).collect();

    assert!(
        found_ids.contains("semantic_relation"),
        "Should find semantic relation"
    );
    assert!(
        found_ids.contains("temporal_relation"),
        "Should find temporal relation"
    );
    assert!(
        found_ids.contains("causal_relation"),
        "Should find causal relation"
    );
    assert!(
        found_ids.contains("spatial_relation"),
        "Should find spatial relation"
    );

    // Verify link types are preserved
    for result in &results {
        match result.memory_id.as_str() {
            "semantic_relation" => assert_eq!(result.link_type, LinkType::Semantic),
            "temporal_relation" => assert_eq!(result.link_type, LinkType::Temporal),
            "causal_relation" => assert_eq!(result.link_type, LinkType::Causal),
            "spatial_relation" => assert_eq!(result.link_type, LinkType::Spatial),
            _ => {}
        }
    }
}

// ============================================================================
// SYNAPTIC TAGGING AND CAPTURE (production) - 5 tests
//
// All five drive the production `SynapticTaggingSystem` / `CaptureWindow`.
// There is no baseline arm in this section: nothing here is compared against
// another algorithm, and no claim is made about systems that are not in this
// repository. What is pinned is what the capture window and the PRP trigger do
// with the inputs each test constructs.
// ============================================================================

/// Memories tagged just before a user flag are captured by the PRP trigger.
#[test]
fn test_prp_captures_tagged_memories_on_user_flag() {
    let config = SynapticTaggingConfig {
        capture_window: CaptureWindow::new(9.0, 2.0), // 9 hours back, 2 hours forward
        prp_threshold: 0.7,
        tag_lifetime_hours: 12.0,
        min_tag_strength: 0.3,
        max_cluster_size: 50,
        enable_clustering: true,
        auto_decay: true,
        cleanup_interval_hours: 1.0,
    };

    let mut stc = SynapticTaggingSystem::with_config(config);

    // Tag memories as they are encoded (simulating normal operation)
    stc.tag_memory("ordinary_memory_1");
    stc.tag_memory("ordinary_memory_2");
    stc.tag_memory("ordinary_memory_3");

    // Simulate importance event happening LATER (or at same time in test)
    let event = ImportanceEvent::user_flag("important_trigger", Some("Remember this!"));
    let result = stc.trigger_prp(event);

    // STC should capture the earlier memories
    assert!(
        result.has_captures(),
        "Retroactive importance SHOULD capture earlier memories"
    );
    assert!(
        result.captured_count() >= 3,
        "Should capture all tagged memories: captured {}",
        result.captured_count()
    );

    // Verify captured memories were encoded BEFORE OR AT the importance event time
    // (In tests, tag_memory() uses Utc::now(), so temporal_distance ~= 0)
    for captured in &result.captured_memories {
        assert!(
            captured.temporal_distance_hours >= 0.0
                || captured.temporal_distance_hours.abs() < 0.01,
            "Captured memory {} should be encoded at or before event (distance: {:.4}h)",
            captured.memory_id,
            captured.temporal_distance_hours
        );
    }
}

/// An emotional event forms a cluster that holds the memories tagged before it.
#[test]
fn test_prp_cluster_contains_the_tagged_memories() {
    let mut stc = SynapticTaggingSystem::new();

    // Encode several memories
    stc.tag_memory("context_memory_1");
    stc.tag_memory("context_memory_2");
    stc.tag_memory("context_memory_3");

    // Trigger with emotional content (high importance)
    let event = ImportanceEvent::emotional("trigger_memory", 0.95);
    let result = stc.trigger_prp(event);

    // Should create a cluster of related memories
    assert!(result.cluster.is_some(), "Should create importance cluster");

    let cluster = result.cluster.unwrap();
    assert!(
        cluster.size() >= 3,
        "Cluster should contain the context memories: size = {}",
        cluster.size()
    );

    // Verify cluster properties
    assert!(
        cluster.average_importance > 0.0,
        "Cluster should have positive importance"
    );
    assert_eq!(
        cluster.trigger_event_type,
        ImportanceEventType::EmotionalContent
    );
}

/// The capture window's boundaries (9 hours back, 2 hours forward): inside the
/// window a capture probability exists, outside it the memory is not captured.
#[test]
fn test_capture_window_boundaries() {
    let window = CaptureWindow::new(9.0, 2.0);
    let event_time = Utc::now();

    // Test memories at various distances from event
    let test_cases = vec![
        (Duration::hours(1), true, "1 hour before"),
        (Duration::hours(4), true, "4 hours before"),
        (Duration::hours(8), true, "8 hours before"),
        (
            Duration::hours(10),
            false,
            "10 hours before (outside window)",
        ),
        (Duration::minutes(-30), true, "30 minutes after"),
        (Duration::hours(-3), false, "3 hours after (outside window)"),
    ];

    for (offset, should_capture, description) in test_cases {
        let memory_time = event_time - offset;
        let in_window = window.is_in_window(memory_time, event_time);

        assert_eq!(
            in_window, should_capture,
            "{}: in_window={}, expected={}",
            description, in_window, should_capture
        );

        if should_capture {
            let prob = window.capture_probability(memory_time, event_time);
            assert!(
                prob.is_some(),
                "{} should have capture probability",
                description
            );
            assert!(
                prob.unwrap() > 0.0,
                "{} should have positive capture probability",
                description
            );
        }
    }
}

/// Tag strength - not semantic similarity, which this system never sees at this
/// layer - decides which tagged memories are captured and how strongly.
#[test]
fn test_capture_probability_ranks_by_tag_strength() {
    let config = SynapticTaggingConfig {
        capture_window: CaptureWindow::new(9.0, 2.0),
        prp_threshold: 0.7,
        tag_lifetime_hours: 12.0,
        min_tag_strength: 0.1, // Low threshold to test strength effects
        max_cluster_size: 100,
        enable_clustering: true,
        auto_decay: true,
        cleanup_interval_hours: 1.0,
    };

    let mut stc = SynapticTaggingSystem::with_config(config);

    // Tag memories with different initial strengths
    // (simulating semantic relevance)
    stc.tag_memory_with_strength("highly_relevant", 0.95);
    stc.tag_memory_with_strength("moderately_relevant", 0.6);
    stc.tag_memory_with_strength("barely_relevant", 0.35);
    stc.tag_memory_with_strength("irrelevant", 0.05); // Below threshold

    // Trigger importance event
    let event = ImportanceEvent::user_flag("trigger", None);
    let result = stc.trigger_prp(event);

    // Higher strength memories should be captured with higher consolidated importance
    let captured_ids: HashSet<_> = result
        .captured_memories
        .iter()
        .map(|c| c.memory_id.as_str())
        .collect();

    assert!(
        captured_ids.contains("highly_relevant"),
        "Highly relevant memory should be captured"
    );
    assert!(
        captured_ids.contains("moderately_relevant"),
        "Moderately relevant memory should be captured"
    );

    // Find consolidated importance values
    let highly_relevant_importance = result
        .captured_memories
        .iter()
        .find(|c| c.memory_id == "highly_relevant")
        .map(|c| c.consolidated_importance)
        .unwrap_or(0.0);

    let moderately_relevant_importance = result
        .captured_memories
        .iter()
        .find(|c| c.memory_id == "moderately_relevant")
        .map(|c| c.consolidated_importance)
        .unwrap_or(0.0);

    assert!(
        highly_relevant_importance >= moderately_relevant_importance,
        "Highly relevant should have >= importance: {:.2} vs {:.2}",
        highly_relevant_importance,
        moderately_relevant_importance
    );
}

/// A memory tagged before a later user flag is captured retroactively: the
/// vacation note becomes part of the important cluster only after the
/// departure announcement arrives.
///
/// This pins the ordering the production system implements (tag first, flag
/// later). It is deliberately not a claim that no other system can do this -
/// nothing in this repository measures another system.
#[test]
fn test_prp_captures_a_prior_memory_when_a_later_user_flag_arrives() {
    // Scenario: AI assistant conversation
    // 1. User mentions "Bob is taking a vacation next week"
    // 2. Hours later, user says "Bob is leaving the company"
    // 3. The vacation memory becomes retroactively important as context!

    let mut stc = SynapticTaggingSystem::new();

    // Memory 1: Ordinary conversation about vacation (time T)
    let _vacation_memory =
        stc.tag_memory_with_context("vacation_mention", "User mentioned Bob's vacation plans");

    // Memory 2: Some other ordinary memories
    stc.tag_memory("unrelated_memory_1");
    stc.tag_memory("unrelated_memory_2");

    // Hours later (time T + N hours): Important revelation
    // "Bob is leaving the company" - triggers importance
    let event = ImportanceEvent {
        event_type: ImportanceEventType::UserFlag,
        memory_id: Some("departure_announcement".to_string()),
        timestamp: Utc::now(),
        strength: 1.0, // Maximum importance
        context: Some("Bob is leaving - this makes prior context important".to_string()),
    };

    let result = stc.trigger_prp(event);

    // The vacation memory should be captured!
    let vacation_captured = result
        .captured_memories
        .iter()
        .any(|c| c.memory_id == "vacation_mention");

    assert!(
        vacation_captured,
        "the vacation memory must be captured retroactively by the later user flag"
    );

    // Verify the capture details
    let vacation_capture = result
        .captured_memories
        .iter()
        .find(|c| c.memory_id == "vacation_mention")
        .unwrap();

    // In test context, memories are tagged at ~same time as event,
    // so temporal_distance is ~0 (but conceptually it's a "backward" capture
    // since the memory existed BEFORE it became important)
    assert!(
        vacation_capture.temporal_distance_hours >= 0.0
            || vacation_capture.temporal_distance_hours.abs() < 0.01,
        "Memory should be encoded at or before the importance event (distance: {:.4}h)",
        vacation_capture.temporal_distance_hours
    );

    assert!(
        vacation_capture.consolidated_importance > 0.5,
        "Vacation memory should have high consolidated importance: {:.2}",
        vacation_capture.consolidated_importance
    );

    // The measured contrast: the memory carried no importance of its own at
    // tagging time; its consolidated importance is a function of the later
    // event, which is what `trigger_prp` implements.
}

// ============================================================================
// HIPPOCAMPAL INDEXING (production) - 4 tests
//
// All four drive the production `HippocampalIndex` / `ContentPointer`. "Two
// phase" describes the design (compressed barcode index, then the content
// pointer); no flat-search arm is built or timed here, so nothing in this
// section compares retrieval strategies.
// ============================================================================

/// Index search returns matches inside its time budget and the index reports
/// the compressed embedding dimension it stores instead of the full vector.
#[test]
fn test_index_search_returns_matches_with_compressed_dimensions() {
    let index = HippocampalIndex::new();
    let now = Utc::now();

    // Create test data with embeddings
    const NUM_MEMORIES: usize = 100;

    for i in 0..NUM_MEMORIES {
        let embedding: Vec<f32> = (0..384)
            .map(|j| ((i * 17 + j) as f32 / 1000.0).sin())
            .collect();

        let _ = index.index_memory(
            &format!("memory_{}", i),
            &format!("Content for memory {} with some text", i),
            "fact",
            now,
            Some(embedding),
        );
    }

    // Search the compressed index (no content fetch in this phase).
    let query = IndexQuery::from_text("memory").with_limit(10);

    let start = std::time::Instant::now();
    let results = index.search_indices(&query).unwrap();
    let index_search_time = start.elapsed();

    // Should complete quickly
    assert!(
        index_search_time.as_millis() < 50,
        "Index search should be fast: {:?}",
        index_search_time
    );

    // Should find results
    assert!(!results.is_empty(), "Should find matching memories");

    // The index stores its own compressed vectors, not the caller's 384-dim
    // ones; that is the design claim this assertion can actually check.
    let stats = index.stats();
    assert_eq!(
        stats.index_dimensions, INDEX_EMBEDDING_DIM,
        "Index should use compressed embeddings ({}D)",
        INDEX_EMBEDDING_DIM
    );
}

/// The index's summary dimension is a real compression of the product's
/// embedding dimension, and the default config is the documented 128.
#[test]
fn test_index_compression_ratio() {
    let config = HippocampalIndexConfig::default();

    // The product's embedding width, not a literal copied into the test.
    let full_embedding_dim = vestige_core::embeddings::EMBEDDING_DIMENSIONS;

    // Index embedding size
    let index_embedding_dim = config.summary_dimensions; // 128 by default

    // Compression ratio
    let compression_ratio = full_embedding_dim as f64 / index_embedding_dim as f64;

    assert!(
        compression_ratio >= 2.0,
        "Index should compress embeddings by at least 2x: {:.1}x",
        compression_ratio
    );

    // Default should be 3x compression (384 -> 128)
    assert_eq!(
        index_embedding_dim, INDEX_EMBEDDING_DIM,
        "Default index dimension should be {}",
        INDEX_EMBEDDING_DIM
    );

    // Memory savings per memory
    let full_size_bytes = full_embedding_dim * 4; // f32 = 4 bytes
    let index_size_bytes = index_embedding_dim * 4;
    let savings_per_memory = full_size_bytes - index_size_bytes;

    assert!(
        savings_per_memory > 0,
        "Should save {} bytes per memory",
        savings_per_memory
    );
}

/// Barcodes are unique per memory and their content fingerprints collide only
/// for identical content (the index's dedup signal).
#[test]
fn test_barcode_uniqueness_and_content_fingerprints() {
    let mut generator = BarcodeGenerator::new();
    let now = Utc::now();

    // Generate many barcodes
    let mut barcodes: Vec<MemoryBarcode> = Vec::new();
    let mut barcode_strings: HashSet<String> = HashSet::new();

    for i in 0..1000 {
        let content = format!("Content {}", i);
        let timestamp = now + Duration::milliseconds(i);
        let barcode = generator.generate(&content, timestamp);

        // Check uniqueness
        let barcode_str = barcode.to_compact_string();
        assert!(
            !barcode_strings.contains(&barcode_str),
            "Barcode {} should be unique",
            barcode_str
        );
        barcode_strings.insert(barcode_str);

        barcodes.push(barcode);
    }

    // Verify all IDs are sequential and unique
    for i in 0..barcodes.len() - 1 {
        assert_eq!(
            barcodes[i + 1].id,
            barcodes[i].id + 1,
            "IDs should be sequential"
        );
    }

    // Verify content fingerprints differ for different content
    let fingerprints: HashSet<u32> = barcodes.iter().map(|b| b.content_fingerprint).collect();
    assert_eq!(
        fingerprints.len(),
        barcodes.len(),
        "Content fingerprints should be unique for different content"
    );

    // Test same_content detection
    let barcode1 = generator.generate_with_id(9999, "same content", now);
    let barcode2 = generator.generate_with_id(9998, "same content", now + Duration::hours(1));

    assert!(
        barcode1.same_content(&barcode2),
        "Same content should produce same fingerprint"
    );
    assert_ne!(
        barcode1.id, barcode2.id,
        "But IDs should still be different"
    );
}

/// Content pointers keep table, row, type, chunk range and hash, and survive an
/// index round-trip.
#[test]
fn test_content_pointer_accuracy() {
    // Test SQLite pointer
    let sqlite_ptr = ContentPointer::sqlite("knowledge_nodes", 42, ContentType::Text);
    assert!(!sqlite_ptr.is_inline());
    assert!(matches!(sqlite_ptr.content_type, ContentType::Text));

    // Test inline pointer
    let data = vec![1u8, 2, 3, 4, 5];
    let inline_ptr = ContentPointer::inline(data.clone(), ContentType::Binary);
    assert!(inline_ptr.is_inline());
    assert_eq!(inline_ptr.size_bytes, Some(5));

    // Test vector store pointer
    let vector_ptr = ContentPointer::vector_store("embeddings", 123);
    assert!(!vector_ptr.is_inline());
    assert!(matches!(vector_ptr.content_type, ContentType::Embedding));

    // Test with chunk range
    let chunked_ptr = ContentPointer::sqlite("chunks", 99, ContentType::Text)
        .with_chunk_range(100, 200)
        .with_size(100);

    assert_eq!(chunked_ptr.chunk_range, Some((100, 200)));
    assert_eq!(chunked_ptr.size_bytes, Some(100));

    // Test with hash
    let hashed_ptr = ContentPointer::sqlite("data", 1, ContentType::Text).with_hash(0xDEADBEEF);

    assert_eq!(hashed_ptr.content_hash, Some(0xDEADBEEF));

    // Create full memory index and verify pointers work
    let index = HippocampalIndex::new();
    let now = Utc::now();

    let barcode = index
        .index_memory(
            "test_memory",
            "Test content for pointer verification",
            "fact",
            now,
            None,
        )
        .unwrap();

    // Retrieve and verify
    let retrieved = index.get_index("test_memory").unwrap().unwrap();

    assert_eq!(retrieved.barcode, barcode);
    assert!(
        !retrieved.content_pointers.is_empty(),
        "Should have content pointer"
    );

    // Verify the default pointer is SQLite
    let default_ptr = &retrieved.content_pointers[0];
    assert!(matches!(default_ptr.content_type, ContentType::Text));
}

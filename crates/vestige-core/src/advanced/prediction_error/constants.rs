//! Tunable thresholds for the prediction-error gate.

/// Default similarity threshold for considering memories as "similar".
/// Above this = potential update candidate.
pub(super) const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.75;

/// Threshold for considering content as "nearly identical".
/// Above this = definitely update, not create.
pub(super) const NEAR_IDENTICAL_THRESHOLD: f32 = 0.92;

/// Threshold for "correction" detection: new content contradicts existing
/// with high similarity.
pub(super) const CORRECTION_THRESHOLD: f32 = 0.70;

/// Minimum contradiction confidence required before a memory may be retired.
///
/// Supersession is the only destructive decision the gate makes: it sets
/// `valid_until`, so every later reader is told the old memory stopped being
/// true at that instant. A single mid-confidence signal is not grounds for that
/// claim — the detector reports a lone correction phrase at 0.60 and a lone
/// asymmetric negation at 0.45 — while two independent signals fuse to 0.94 and
/// clear the bar. Observed failure this prevents: a Polish adverbial
/// ("w rzeczywistości") inside a new memory's own explanation retired an
/// unrelated decision from the same project, because the two shared domain
/// vocabulary and the similarity threshold was met.
pub(super) const CORRECTION_MIN_CONFIDENCE: f32 = 0.80;

/// Maximum candidates to consider for update.
pub(super) const MAX_UPDATE_CANDIDATES: usize = 5;

//! `PredictionErrorGate`, its config, and explicit `EvaluationIntent`.

use serde::{Deserialize, Serialize};

use crate::memory::freshness::compare_memory_ids;

use super::candidate::{CandidateMemory, SimilarityResult};
use super::constants::{
    CORRECTION_MIN_CONFIDENCE, CORRECTION_THRESHOLD, DEFAULT_SIMILARITY_THRESHOLD,
    MAX_UPDATE_CANDIDATES, NEAR_IDENTICAL_THRESHOLD,
};
use super::decision::{
    CreateReason, GateDecision, GateFinding, MergeStrategy, SupersedeReason, UpdateType,
};
use super::similarity::cosine_similarity;
use super::stats::GateStats;

/// Serde default for [`PredictionErrorConfig::correction_min_confidence`], so a
/// config serialized before the field existed still deserializes.
fn default_correction_min_confidence() -> f32 {
    CORRECTION_MIN_CONFIDENCE
}

/// Turn the detector's evidence into the shape a response carries.
///
/// The hint is the part the writer needs and the detector cannot know: what to
/// do about a signal that is deliberately not acted on. It says "check", not
/// "retire", because the retirement decision belongs to the caller.
fn contradiction_findings(result: &crate::nlp::DetectionResult) -> Vec<GateFinding> {
    result
        .evidence
        .iter()
        .map(|evidence| GateFinding {
            kind: evidence.kind.as_str().to_string(),
            span: evidence.snippet.clone(),
            hint: match evidence.kind {
                crate::nlp::EvidenceKind::CorrectionPhrase => {
                    "the new text reads as a correction of this memory; if it replaces it, retire \
                     this memory explicitly instead of leaving both to be read as current"
                }
                _ => {
                    "the new text negates a phrase this memory also contains; check whether it \
                     denies the same claim before retiring anything"
                }
            }
            .to_string(),
        })
        .collect()
}

/// Configuration for the prediction error gate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionErrorConfig {
    /// Similarity threshold for update consideration
    pub similarity_threshold: f32,
    /// Threshold for near-identical detection
    pub near_identical_threshold: f32,
    /// Threshold for correction detection
    pub correction_threshold: f32,
    /// Minimum contradiction confidence before a memory may be retired.
    ///
    /// Guards the destructive path only: `Update`, `Merge` and `Create` are
    /// non-destructive and still act on weaker evidence.
    #[serde(default = "default_correction_min_confidence")]
    pub correction_min_confidence: f32,
    /// Maximum candidates to consider
    pub max_candidates: usize,
    /// Whether to auto-supersede demoted memories
    pub auto_supersede_demoted: bool,
}

impl Default for PredictionErrorConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            near_identical_threshold: NEAR_IDENTICAL_THRESHOLD,
            correction_threshold: CORRECTION_THRESHOLD,
            correction_min_confidence: default_correction_min_confidence(),
            max_candidates: MAX_UPDATE_CANDIDATES,
            auto_supersede_demoted: true,
        }
    }
}

/// The Prediction Error Gate
///
/// Evaluates new content against existing memories to determine
/// whether to create, update, or supersede.
#[derive(Debug)]
pub struct PredictionErrorGate {
    /// Configuration
    config: PredictionErrorConfig,
    /// Statistics
    stats: GateStats,
}

impl Default for PredictionErrorGate {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictionErrorGate {
    /// Create a new prediction error gate with default config
    pub fn new() -> Self {
        Self {
            config: PredictionErrorConfig::default(),
            stats: GateStats::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: PredictionErrorConfig) -> Self {
        Self {
            config,
            stats: GateStats::default(),
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &PredictionErrorConfig {
        &self.config
    }

    /// Get mutable configuration
    pub fn config_mut(&mut self) -> &mut PredictionErrorConfig {
        &mut self.config
    }

    /// Evaluate new content against candidates
    ///
    /// Returns a decision on whether to create, update, or supersede.
    pub fn evaluate(
        &mut self,
        new_content: &str,
        new_embedding: &[f32],
        candidates: &[CandidateMemory],
    ) -> GateDecision {
        self.stats.total_evaluations += 1;

        // No candidates = definitely create
        if candidates.is_empty() {
            self.stats.creates += 1;
            return GateDecision::Create {
                reason: CreateReason::FirstMemory,
                prediction_error: 1.0,
                related_memory_ids: vec![],
            };
        }

        // Calculate similarities
        let mut similarities: Vec<SimilarityResult> = candidates
            .iter()
            .map(|c| {
                let similarity = cosine_similarity(new_embedding, &c.embedding);
                let contradiction = self.detect_contradiction(new_content, &c.content);

                SimilarityResult {
                    memory_id: c.id.clone(),
                    similarity,
                    prediction_error: 1.0 - similarity,
                    semantic_overlap: similarity, // Simplified; could use more sophisticated measure
                    appears_contradictory: contradiction.positive,
                    contradiction_confidence: contradiction.confidence,
                    contradiction_evidence: contradiction_findings(&contradiction),
                }
            })
            .collect();

        // Sort by similarity (highest first). Equal similarities — and NaN
        // similarities, which used to leave the comparison `Equal` — are broken
        // by the shared identity tiebreak in `memory::freshness`, so the memory
        // named in an Update/Supersede/Merge decision is a property of the
        // candidate set rather than of the storage layer's row order.
        similarities.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| compare_memory_ids(&b.memory_id, &a.memory_id))
        });

        // Take top candidates
        let top_candidates: Vec<_> = similarities
            .iter()
            .take(self.config.max_candidates)
            .collect();

        // Check the best candidate. Order matters: contradiction is tested *before*
        // the near-identical short-circuit below. A correction is by definition very
        // similar to the memory it corrects ("deploy is Friday" → "deploy moved to
        // Monday"), so testing similarity first swallowed every correction into
        // `Update { Reinforce }` and left `correction_threshold` unreachable.
        //
        // What the contradiction branch may do is bounded: it *reports*. Retiring a
        // memory is a claim about the past that every later reader inherits, and no
        // lexical rule here is strong enough to make it — see
        // `GateDecision::Contradiction`. The confidence floor is the bar for even
        // reporting, so a lone mid-confidence signal stays out of the response.
        if let Some(best) = top_candidates.first() {
            if best.appears_contradictory
                && best.contradiction_confidence >= self.config.correction_min_confidence
                && best.similarity >= self.config.correction_threshold
            {
                self.stats.contradictions += 1;
                return GateDecision::Contradiction {
                    existing_id: best.memory_id.clone(),
                    similarity: best.similarity,
                    confidence: best.contradiction_confidence,
                    evidence: best.contradiction_evidence.clone(),
                };
            }

            if best.similarity >= self.config.near_identical_threshold {
                // Nearly identical and not contradictory — reinforce existing
                self.stats.updates += 1;
                return GateDecision::Update {
                    target_id: best.memory_id.clone(),
                    similarity: best.similarity,
                    update_type: UpdateType::Reinforce,
                    prediction_error: best.prediction_error,
                };
            }

            // Check for potential supersede
            let candidate = candidates.iter().find(|c| c.id == best.memory_id);
            if let Some(c) = candidate {
                // If similar and the existing memory was demoted, supersede it
                if best.similarity >= self.config.similarity_threshold
                    && c.was_demoted
                    && self.config.auto_supersede_demoted
                {
                    self.stats.supersedes += 1;
                    return GateDecision::Supersede {
                        old_memory_id: c.id.clone(),
                        similarity: best.similarity,
                        supersede_reason: SupersedeReason::Improvement,
                        prediction_error: best.prediction_error,
                    };
                }

                // Similar vocabulary, different claim. This band used to return
                // `UpdateType::Merge`, which the storage layer implemented by
                // rewriting the existing memory's text as
                // `"{old}\n\n[Updated <date>]\n{new}"`: a compound memory under
                // the wrong heading, and a second copy of the new text next to
                // its own memory. Sharing the vocabulary of a subject is not
                // agreeing with a claim, so the new content becomes its own
                // memory and the neighbours it resembles are named in
                // `related_memory_ids` — the `Create` path already forges those
                // edges and reports them.
                if best.similarity >= self.config.similarity_threshold {
                    self.stats.creates += 1;
                    return GateDecision::Create {
                        reason: CreateReason::DifferentDomain,
                        prediction_error: best.prediction_error,
                        related_memory_ids: top_candidates
                            .iter()
                            .map(|s| s.memory_id.clone())
                            .collect(),
                    };
                }
            }
        }

        // Check for merge opportunity (multiple similar memories)
        let merge_candidates: Vec<_> = top_candidates
            .iter()
            .filter(|s| s.similarity >= self.config.similarity_threshold * 0.9)
            .collect();

        if merge_candidates.len() >= 2 {
            let avg_similarity = merge_candidates.iter().map(|s| s.similarity).sum::<f32>()
                / merge_candidates.len() as f32;

            self.stats.merges += 1;
            return GateDecision::Merge {
                memory_ids: merge_candidates
                    .iter()
                    .map(|s| s.memory_id.clone())
                    .collect(),
                avg_similarity,
                strategy: MergeStrategy::Combine,
            };
        }

        // Default: create new (high prediction error)
        let best_pe = top_candidates
            .first()
            .map(|s| s.prediction_error)
            .unwrap_or(1.0);

        self.stats.creates += 1;
        GateDecision::Create {
            reason: if candidates.is_empty() {
                CreateReason::NoSimilarMemories
            } else {
                CreateReason::HighPredictionError
            },
            prediction_error: best_pe,
            related_memory_ids: top_candidates.iter().map(|s| s.memory_id.clone()).collect(),
        }
    }

    /// Evaluate with explicit intent
    ///
    /// Use when the user has indicated intent (e.g., "update X" or "this is better than Y")
    pub fn evaluate_with_intent(
        &mut self,
        new_content: &str,
        new_embedding: &[f32],
        candidates: &[CandidateMemory],
        intent: EvaluationIntent,
    ) -> GateDecision {
        match intent {
            EvaluationIntent::ForceCreate => {
                self.stats.creates += 1;
                GateDecision::Create {
                    reason: CreateReason::ExplicitCreate,
                    prediction_error: 1.0,
                    related_memory_ids: vec![],
                }
            }
            EvaluationIntent::ForceUpdate { target_id } => {
                // Find the target candidate
                if let Some(c) = candidates.iter().find(|c| c.id == target_id) {
                    let similarity = cosine_similarity(new_embedding, &c.embedding);
                    self.stats.updates += 1;
                    GateDecision::Update {
                        target_id: target_id.clone(),
                        similarity,
                        update_type: UpdateType::Replace,
                        prediction_error: 1.0 - similarity,
                    }
                } else {
                    // Target not found, evaluate normally
                    self.evaluate(new_content, new_embedding, candidates)
                }
            }
            EvaluationIntent::Supersede {
                old_memory_id,
                reason,
            } => {
                if let Some(c) = candidates.iter().find(|c| c.id == old_memory_id) {
                    let similarity = cosine_similarity(new_embedding, &c.embedding);
                    self.stats.supersedes += 1;
                    GateDecision::Supersede {
                        old_memory_id,
                        similarity,
                        supersede_reason: reason,
                        prediction_error: 1.0 - similarity,
                    }
                } else {
                    self.evaluate(new_content, new_embedding, candidates)
                }
            }
            EvaluationIntent::Auto => self.evaluate(new_content, new_embedding, candidates),
        }
    }

    /// Detect if two pieces of content appear contradictory.
    ///
    /// Delegates to [`crate::nlp::default_contradiction_detector`] — today
    /// a heuristic (NegEx scope + correction phrases + asymmetric negation,
    /// EN + PL). Going through the factory means a future ONNX / LLM
    /// backend swaps in via one file, not three call sites.
    ///
    /// Every Vestige release runs an eval suite against the hand-curated
    /// datasets in `nlp::eval::data` to detect regressions in this signal.
    pub(super) fn detect_contradiction(
        &self,
        new_content: &str,
        old_content: &str,
    ) -> crate::nlp::DetectionResult {
        crate::nlp::default_contradiction_detector().detect(new_content, old_content)
    }

    /// Get statistics
    pub fn stats(&self) -> &GateStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = GateStats::default();
    }
}

/// Explicit intent for evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvaluationIntent {
    /// Automatically determine best action
    Auto,
    /// Force creation of new memory
    ForceCreate,
    /// Force update of specific memory
    ForceUpdate { target_id: String },
    /// Force supersede of specific memory
    Supersede {
        old_memory_id: String,
        reason: SupersedeReason,
    },
}

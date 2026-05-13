//! [`PredictiveMemory`] engine — configuration, scoring, learning loop.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

use super::cache::PredictionCache;
use super::error::{PredictiveMemoryError, Result};
use super::types::{PredictedMemory, PredictionOutcome, PredictionReason, SessionContext};
use super::user_model::UserModel;
use super::{DEFAULT_MIN_CONFIDENCE, MAX_CACHE_SIZE, MAX_PREDICTIONS};

// ============================================================================
// PREDICTIVE MEMORY ENGINE
// ============================================================================

/// Configuration for the predictive memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictiveMemoryConfig {
    /// Minimum confidence threshold for predictions
    pub min_confidence: f64,
    /// Maximum predictions to return
    pub max_predictions: usize,
    /// Cache size for predictions
    pub cache_size: usize,
    /// Enable temporal pattern learning
    pub enable_temporal_patterns: bool,
    /// Enable co-access pattern learning
    pub enable_co_access_patterns: bool,
    /// Weight for interest-based predictions
    pub interest_weight: f64,
    /// Weight for temporal predictions
    pub temporal_weight: f64,
    /// Weight for co-access predictions
    pub co_access_weight: f64,
    /// Weight for session context predictions
    pub session_weight: f64,
}

impl Default for PredictiveMemoryConfig {
    fn default() -> Self {
        Self {
            min_confidence: DEFAULT_MIN_CONFIDENCE,
            max_predictions: MAX_PREDICTIONS,
            cache_size: MAX_CACHE_SIZE,
            enable_temporal_patterns: true,
            enable_co_access_patterns: true,
            interest_weight: 0.3,
            temporal_weight: 0.2,
            co_access_weight: 0.3,
            session_weight: 0.2,
        }
    }
}

/// The main predictive memory engine
#[allow(clippy::type_complexity)]
pub struct PredictiveMemory {
    /// User behavior model
    user_model: Arc<RwLock<UserModel>>,
    /// Prediction cache
    prediction_cache: Arc<RwLock<PredictionCache>>,
    /// History of prediction outcomes for learning
    prediction_history: Arc<RwLock<Vec<PredictionOutcome>>>,
    /// Pending predictions awaiting outcome
    pending_predictions: Arc<RwLock<HashMap<String, PredictedMemory>>>,
    /// Configuration
    config: PredictiveMemoryConfig,
    /// Memory metadata cache (memory_id -> (content_preview, tags))
    memory_metadata: Arc<RwLock<HashMap<String, (String, Vec<String>)>>>,
}

impl PredictiveMemory {
    /// Create a new predictive memory engine with default configuration
    pub fn new() -> Self {
        Self::with_config(PredictiveMemoryConfig::default())
    }

    /// Create a new predictive memory engine with custom configuration
    pub fn with_config(config: PredictiveMemoryConfig) -> Self {
        Self {
            user_model: Arc::new(RwLock::new(UserModel::new())),
            prediction_cache: Arc::new(RwLock::new(PredictionCache::new(config.cache_size))),
            prediction_history: Arc::new(RwLock::new(Vec::new())),
            pending_predictions: Arc::new(RwLock::new(HashMap::new())),
            config,
            memory_metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &PredictiveMemoryConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: PredictiveMemoryConfig) {
        self.config = config;
    }

    /// Record a user query
    pub fn record_query(&self, query: &str, tags: &[&str]) -> Result<()> {
        let mut model = self
            .user_model
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        model.record_query(query, tags);

        // Invalidate cache for changed interests
        let mut cache = self
            .prediction_cache
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        for tag in tags {
            cache.invalidate(tag);
        }

        Ok(())
    }

    /// Record an interest with weight
    pub fn record_interest(&self, topic: &str, weight: f64) -> Result<()> {
        let mut model = self
            .user_model
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        model.update_interest(topic, weight);

        // Invalidate related cache entries
        let mut cache = self
            .prediction_cache
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;
        cache.invalidate(topic);

        Ok(())
    }

    /// Record that a memory was accessed
    pub fn record_memory_access(
        &self,
        memory_id: &str,
        content_preview: &str,
        tags: &[String],
    ) -> Result<()> {
        // Update user model
        let mut model = self
            .user_model
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        model.record_memory_access(memory_id, tags);

        // Store metadata for future predictions
        let mut metadata = self
            .memory_metadata
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        metadata.insert(
            memory_id.to_string(),
            (content_preview.to_string(), tags.to_vec()),
        );

        // Check if this was a predicted memory
        self.record_prediction_outcome(memory_id, true)?;

        Ok(())
    }

    /// Update session context
    pub fn update_session_context(
        &self,
        update_fn: impl FnOnce(&mut SessionContext),
    ) -> Result<()> {
        let mut model = self
            .user_model
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        // Reset session if stale
        if model.should_reset_session() {
            model.reset_session();
        }

        update_fn(&mut model.session_context);

        Ok(())
    }

    /// Predict memories that will be needed based on current context
    pub fn predict_needed_memories(
        &self,
        context: &SessionContext,
    ) -> Result<Vec<PredictedMemory>> {
        let model = self
            .user_model
            .read()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        let now = Utc::now();
        let mut predictions: Vec<PredictedMemory> = Vec::new();

        // 1. Interest-based predictions
        if self.config.interest_weight > 0.0 {
            predictions.extend(self.predict_from_interests(&model, now));
        }

        // 2. Temporal pattern predictions
        if self.config.enable_temporal_patterns && self.config.temporal_weight > 0.0 {
            predictions.extend(self.predict_from_temporal(&model, now));
        }

        // 3. Co-access pattern predictions
        if self.config.enable_co_access_patterns && self.config.co_access_weight > 0.0 {
            predictions.extend(self.predict_from_co_access(&model, context, now));
        }

        // 4. Session context predictions
        if self.config.session_weight > 0.0 {
            predictions.extend(self.predict_from_session(context, now));
        }

        // Deduplicate and combine scores
        predictions = self.merge_predictions(predictions);

        // Filter by minimum confidence
        predictions.retain(|p| p.confidence >= self.config.min_confidence);

        // Sort by confidence
        predictions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to max
        predictions.truncate(self.config.max_predictions);

        // Store as pending for outcome tracking
        self.store_pending_predictions(&predictions)?;

        Ok(predictions)
    }

    /// Get proactive suggestions ("You might also need...")
    pub fn get_proactive_suggestions(&self, min_confidence: f64) -> Result<Vec<PredictedMemory>> {
        let model = self
            .user_model
            .read()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        let predictions = self.predict_needed_memories(&model.session_context)?;

        Ok(predictions
            .into_iter()
            .filter(|p| p.confidence >= min_confidence)
            .collect())
    }

    /// Pre-fetch likely-needed memories into cache
    pub async fn prefetch(&self, context: &SessionContext) -> Result<usize> {
        let predictions = self.predict_needed_memories(context)?;
        let count = predictions.len();

        // Generate cache key from context
        let cache_key = self.generate_cache_key(context);

        // Store in cache
        let mut cache = self
            .prediction_cache
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        cache.insert(cache_key, predictions);

        Ok(count)
    }

    /// Get cached predictions for a context
    pub fn get_cached_predictions(
        &self,
        context: &SessionContext,
    ) -> Result<Option<Vec<PredictedMemory>>> {
        let cache_key = self.generate_cache_key(context);

        let mut cache = self
            .prediction_cache
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        Ok(cache.get(&cache_key).cloned())
    }

    /// Record the outcome of a prediction (for learning)
    pub fn record_prediction_outcome(&self, memory_id: &str, was_useful: bool) -> Result<()> {
        let mut pending = self
            .pending_predictions
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        if let Some(prediction) = pending.remove(memory_id) {
            let outcome = PredictionOutcome {
                memory_id: memory_id.to_string(),
                confidence: prediction.confidence,
                was_useful,
                time_to_use: Some(Utc::now() - prediction.predicted_at),
                recorded_at: Utc::now(),
            };

            let mut history = self
                .prediction_history
                .write()
                .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

            history.push(outcome);

            // Keep history manageable
            if history.len() > 10_000 {
                history.drain(0..5000);
            }
        }

        Ok(())
    }

    /// Calculate prediction accuracy based on history
    pub fn prediction_accuracy(&self) -> Result<f64> {
        let history = self
            .prediction_history
            .read()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        if history.is_empty() {
            return Ok(0.0);
        }

        let useful_count = history.iter().filter(|o| o.was_useful).count();
        Ok(useful_count as f64 / history.len() as f64)
    }

    /// Apply decay to learned patterns (call periodically, e.g., daily)
    pub fn apply_decay(&self) -> Result<()> {
        let mut model = self
            .user_model
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        model.apply_decay();

        // Clear old cache entries
        let mut cache = self
            .prediction_cache
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        cache.clear();

        Ok(())
    }

    /// Get the current user model (read-only)
    pub fn get_user_model(&self) -> Result<UserModel> {
        let model = self
            .user_model
            .read()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        Ok(model.clone())
    }

    /// Get top interests from the user model
    pub fn get_top_interests(&self, limit: usize) -> Result<Vec<(String, f64)>> {
        let model = self
            .user_model
            .read()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        Ok(model.top_interests(limit))
    }

    /// Signal novelty (prediction error) for enhanced encoding
    pub fn signal_novelty(&self, _memory_id: &str, tags: &[String]) -> Result<f64> {
        // Calculate novelty based on how unexpected this is
        let model = self
            .user_model
            .read()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        // Novelty is higher when tags don't match current interests
        let mut interest_match = 0.0;
        for tag in tags {
            if let Some(weight) = model.interests.get(&tag.to_lowercase()) {
                interest_match += weight;
            }
        }

        // Normalize and invert (high match = low novelty)
        let avg_match = if tags.is_empty() {
            0.0
        } else {
            interest_match / tags.len() as f64
        };
        let novelty = 1.0 - avg_match;

        // Novelty signals should boost encoding of this memory
        // The caller can use this to adjust retention strength

        Ok(novelty)
    }

    // ========================================================================
    // Private prediction methods
    // ========================================================================

    fn predict_from_interests(
        &self,
        model: &UserModel,
        now: DateTime<Utc>,
    ) -> Vec<PredictedMemory> {
        let metadata = self.memory_metadata.read().ok();
        let mut predictions = Vec::new();

        if let Some(meta) = metadata {
            let top_interests = model.top_interests(10);

            for (topic, interest_weight) in top_interests {
                // Find memories with matching tags
                for (memory_id, (content_preview, tags)) in meta.iter() {
                    if tags.iter().any(|t| t.to_lowercase() == topic) {
                        let confidence = interest_weight * self.config.interest_weight;

                        predictions.push(PredictedMemory {
                            memory_id: memory_id.clone(),
                            content_preview: content_preview.clone(),
                            confidence,
                            reasoning: PredictionReason::InterestBased {
                                topic: topic.clone(),
                                weight: interest_weight,
                            },
                            predicted_at: now,
                            tags: tags.clone(),
                        });
                    }
                }
            }
        }

        predictions
    }

    fn predict_from_temporal(&self, model: &UserModel, now: DateTime<Utc>) -> Vec<PredictedMemory> {
        let metadata = self.memory_metadata.read().ok();
        let mut predictions = Vec::new();

        let temporal_topics = model.temporal_patterns.topics_for_time(now);

        if let Some(meta) = metadata {
            for (topic, temporal_weight) in temporal_topics {
                for (memory_id, (content_preview, tags)) in meta.iter() {
                    if tags
                        .iter()
                        .any(|t| t.to_lowercase() == topic.to_lowercase())
                    {
                        let confidence = temporal_weight * self.config.temporal_weight;

                        predictions.push(PredictedMemory {
                            memory_id: memory_id.clone(),
                            content_preview: content_preview.clone(),
                            confidence,
                            reasoning: PredictionReason::TemporalPattern {
                                pattern_description: format!(
                                    "You often work on {} at this time",
                                    topic
                                ),
                                historical_accuracy: temporal_weight,
                            },
                            predicted_at: now,
                            tags: tags.clone(),
                        });
                    }
                }
            }
        }

        predictions
    }

    fn predict_from_co_access(
        &self,
        model: &UserModel,
        context: &SessionContext,
        now: DateTime<Utc>,
    ) -> Vec<PredictedMemory> {
        let metadata = self.memory_metadata.read().ok();
        let mut predictions = Vec::new();

        // For each recently accessed memory, find co-access candidates
        for accessed_id in &context.accessed_memories {
            let candidates = model.get_co_access_candidates(accessed_id);

            for (candidate_id, co_occurrence_rate) in candidates {
                // Skip if already accessed
                if context.accessed_memories.contains(&candidate_id) {
                    continue;
                }

                let confidence = co_occurrence_rate * self.config.co_access_weight;

                let (content_preview, tags) = metadata
                    .as_ref()
                    .and_then(|m| m.get(&candidate_id))
                    .cloned()
                    .unwrap_or_default();

                predictions.push(PredictedMemory {
                    memory_id: candidate_id.clone(),
                    content_preview,
                    confidence,
                    reasoning: PredictionReason::CoAccess {
                        trigger_memory: accessed_id.clone(),
                        co_occurrence_rate,
                    },
                    predicted_at: now,
                    tags,
                });
            }
        }

        predictions
    }

    fn predict_from_session(
        &self,
        context: &SessionContext,
        now: DateTime<Utc>,
    ) -> Vec<PredictedMemory> {
        let metadata = self.memory_metadata.read().ok();
        let mut predictions = Vec::new();

        // Use session focus and recent queries to find relevant memories
        if let Some(meta) = metadata {
            // Match against current focus
            if let Some(focus) = &context.current_focus {
                for (memory_id, (content_preview, tags)) in meta.iter() {
                    if tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&focus.to_lowercase()))
                        || content_preview
                            .to_lowercase()
                            .contains(&focus.to_lowercase())
                    {
                        let confidence = 0.6 * self.config.session_weight;

                        predictions.push(PredictedMemory {
                            memory_id: memory_id.clone(),
                            content_preview: content_preview.clone(),
                            confidence,
                            reasoning: PredictionReason::SessionContext {
                                trigger: format!("Current focus: {}", focus),
                                similarity: 0.6,
                            },
                            predicted_at: now,
                            tags: tags.clone(),
                        });
                    }
                }
            }

            // Match against recent queries
            for query in &context.recent_queries {
                for (memory_id, (content_preview, tags)) in meta.iter() {
                    let query_lower = query.to_lowercase();
                    if tags.iter().any(|t| query_lower.contains(&t.to_lowercase()))
                        || content_preview.to_lowercase().contains(&query_lower)
                    {
                        let confidence = 0.5 * self.config.session_weight;

                        predictions.push(PredictedMemory {
                            memory_id: memory_id.clone(),
                            content_preview: content_preview.clone(),
                            confidence,
                            reasoning: PredictionReason::SessionContext {
                                trigger: format!("Recent query: {}", query),
                                similarity: 0.5,
                            },
                            predicted_at: now,
                            tags: tags.clone(),
                        });
                    }
                }
            }
        }

        predictions
    }

    fn merge_predictions(&self, predictions: Vec<PredictedMemory>) -> Vec<PredictedMemory> {
        let mut merged: HashMap<String, PredictedMemory> = HashMap::new();

        for pred in predictions {
            merged
                .entry(pred.memory_id.clone())
                .and_modify(|existing| {
                    // Combine confidence scores (taking max, with a small boost for multiple signals)
                    existing.confidence = (existing.confidence.max(pred.confidence) * 1.1).min(1.0);
                })
                .or_insert(pred);
        }

        merged.into_values().collect()
    }

    fn store_pending_predictions(&self, predictions: &[PredictedMemory]) -> Result<()> {
        let mut pending = self
            .pending_predictions
            .write()
            .map_err(|e| PredictiveMemoryError::LockPoisoned(e.to_string()))?;

        pending.clear();
        for pred in predictions {
            pending.insert(pred.memory_id.clone(), pred.clone());
        }

        Ok(())
    }

    fn generate_cache_key(&self, context: &SessionContext) -> String {
        let mut key = String::new();

        if let Some(focus) = &context.current_focus {
            key.push_str(focus);
        }

        for query in context.recent_queries.iter().take(3) {
            key.push_str(query);
        }

        // Include time bucket (hourly)
        key.push_str(&format!("_h{}", Utc::now().hour()));

        key
    }
}

impl Default for PredictiveMemory {
    fn default() -> Self {
        Self::new()
    }
}

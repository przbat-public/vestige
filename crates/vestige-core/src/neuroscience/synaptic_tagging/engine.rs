//! [`SynapticTaggingSystem`] orchestrator: configuration, statistics and
//! the actual tag/capture/decay loop.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::events::{CaptureResult, CapturedMemory, ImportanceCluster, ImportanceEvent};
use super::tag::{CaptureWindow, SynapticTag};
use super::{
    DEFAULT_MAX_CLUSTER_SIZE, DEFAULT_MIN_TAG_STRENGTH, DEFAULT_PRP_THRESHOLD,
    DEFAULT_TAG_LIFETIME_HOURS,
};

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Configuration for the Synaptic Tagging System
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynapticTaggingConfig {
    /// Capture window configuration
    pub capture_window: CaptureWindow,
    /// Minimum event strength to trigger PRP production
    pub prp_threshold: f64,
    /// Tag lifetime before complete decay (hours)
    pub tag_lifetime_hours: f64,
    /// Minimum tag strength for capture eligibility
    pub min_tag_strength: f64,
    /// Maximum memories in a single cluster
    pub max_cluster_size: usize,
    /// Whether to create importance clusters
    pub enable_clustering: bool,
    /// Whether to auto-decay tags
    pub auto_decay: bool,
    /// Interval for automatic tag cleanup (hours)
    pub cleanup_interval_hours: f64,
}

impl Default for SynapticTaggingConfig {
    fn default() -> Self {
        Self {
            capture_window: CaptureWindow::default(),
            prp_threshold: DEFAULT_PRP_THRESHOLD,
            tag_lifetime_hours: DEFAULT_TAG_LIFETIME_HOURS,
            min_tag_strength: DEFAULT_MIN_TAG_STRENGTH,
            max_cluster_size: DEFAULT_MAX_CLUSTER_SIZE,
            enable_clustering: true,
            auto_decay: true,
            cleanup_interval_hours: 1.0,
        }
    }
}

// ============================================================================
// STATISTICS
// ============================================================================

/// Statistics about the synaptic tagging system
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggingStats {
    /// Total tags created
    pub total_tags_created: u64,
    /// Currently active tags
    pub active_tags: usize,
    /// Total memories captured
    pub total_captures: u64,
    /// Total importance events processed
    pub total_events: u64,
    /// Total clusters created
    pub total_clusters: u64,
    /// Average capture rate
    pub average_capture_rate: f64,
    /// Average captures per event
    pub average_captures_per_event: f64,
    /// Tags expired without capture
    pub tags_expired: u64,
    /// Last cleanup time
    pub last_cleanup: Option<DateTime<Utc>>,
}

// ============================================================================
// SYNAPTIC TAGGING SYSTEM
// ============================================================================

/// The Synaptic Tagging and Capture (STC) system
///
/// This is the main entry point for retroactive importance assignment.
/// It manages synaptic tags, processes importance events, and captures
/// memories for consolidation.
///
/// ## Thread Safety
///
/// The system is thread-safe and can be shared across threads using Arc.
/// All internal state is protected by RwLock.
///
/// ## Usage
///
/// ```rust,ignore
/// let mut stc = SynapticTaggingSystem::new();
///
/// // Tag memories as they are encoded
/// stc.tag_memory("mem-123");
///
/// // Later, process importance events
/// let result = stc.trigger_prp(ImportanceEvent::user_flag("mem-456", None));
/// for captured in result.captured_memories {
///     // Promote to long-term storage
///     storage.promote_memory(&captured.memory_id, captured.consolidated_importance)?;
/// }
/// ```
pub struct SynapticTaggingSystem {
    /// Active synaptic tags
    tags: Arc<RwLock<HashMap<String, SynapticTag>>>,
    /// Importance clusters
    clusters: Arc<RwLock<Vec<ImportanceCluster>>>,
    /// Configuration
    config: SynapticTaggingConfig,
    /// Statistics
    stats: Arc<RwLock<TaggingStats>>,
}

impl Default for SynapticTaggingSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapticTaggingSystem {
    /// Create a new STC system with default configuration
    pub fn new() -> Self {
        Self::with_config(SynapticTaggingConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: SynapticTaggingConfig) -> Self {
        Self {
            tags: Arc::new(RwLock::new(HashMap::new())),
            clusters: Arc::new(RwLock::new(Vec::new())),
            config,
            stats: Arc::new(RwLock::new(TaggingStats::default())),
        }
    }

    /// Get current configuration
    pub fn config(&self) -> &SynapticTaggingConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: SynapticTaggingConfig) {
        self.config = config;
    }

    /// Tag a memory for potential capture
    ///
    /// This should be called when a memory is encoded. The tag will remain
    /// active for the configured lifetime, eligible for capture if an
    /// importance event occurs nearby.
    ///
    /// # Arguments
    /// * `memory_id` - The ID of the memory to tag
    ///
    /// # Returns
    /// The created synaptic tag
    pub fn tag_memory(&mut self, memory_id: &str) -> SynapticTag {
        let tag = SynapticTag::new(memory_id);

        if let Ok(mut tags) = self.tags.write() {
            tags.insert(memory_id.to_string(), tag.clone());
        }

        if let Ok(mut stats) = self.stats.write() {
            stats.total_tags_created += 1;
            stats.active_tags = self.tags.read().map(|t| t.len()).unwrap_or(0);
        }

        tag
    }

    /// Tag a memory with custom strength
    ///
    /// Use this for memories that have initial importance signals (e.g., emotional content)
    /// but haven't crossed the threshold for full importance yet.
    pub fn tag_memory_with_strength(&mut self, memory_id: &str, strength: f64) -> SynapticTag {
        let tag = SynapticTag::with_strength(memory_id, strength);

        if let Ok(mut tags) = self.tags.write() {
            tags.insert(memory_id.to_string(), tag.clone());
        }

        if let Ok(mut stats) = self.stats.write() {
            stats.total_tags_created += 1;
            stats.active_tags = self.tags.read().map(|t| t.len()).unwrap_or(0);
        }

        tag
    }

    /// Tag a memory with encoding context
    pub fn tag_memory_with_context(&mut self, memory_id: &str, context: &str) -> SynapticTag {
        let tag = SynapticTag::with_context(memory_id, context);

        if let Ok(mut tags) = self.tags.write() {
            tags.insert(memory_id.to_string(), tag.clone());
        }

        if let Ok(mut stats) = self.stats.write() {
            stats.total_tags_created += 1;
            stats.active_tags = self.tags.read().map(|t| t.len()).unwrap_or(0);
        }

        tag
    }

    /// Trigger PRP production from an importance event
    ///
    /// This is the core STC mechanism. When an importance event occurs:
    /// 1. PRPs are produced (if event strength >= threshold)
    /// 2. System sweeps for eligible tagged memories
    /// 3. Eligible memories are captured (consolidated)
    /// 4. Optionally, an importance cluster is created
    ///
    /// # Arguments
    /// * `event` - The importance event
    ///
    /// # Returns
    /// Result containing captured memories and cluster info
    pub fn trigger_prp(&mut self, event: ImportanceEvent) -> CaptureResult {
        let start = std::time::Instant::now();

        // Check if event is strong enough to trigger PRPs
        if event.strength < self.config.prp_threshold {
            return CaptureResult {
                event,
                captured_memories: vec![],
                considered_count: 0,
                cluster: None,
                processing_time_us: start.elapsed().as_micros() as u64,
            };
        }

        // Sweep for eligible tags
        let (captured, considered_count) = self.sweep_for_capture_internal(&event);

        // Update stats
        if let Ok(mut stats) = self.stats.write() {
            stats.total_events += 1;
            stats.total_captures += captured.len() as u64;

            // Update rolling average
            let total = stats.total_events as f64;
            stats.average_captures_per_event =
                (stats.average_captures_per_event * (total - 1.0) + captured.len() as f64) / total;

            if considered_count > 0 {
                let rate = captured.len() as f64 / considered_count as f64;
                stats.average_capture_rate =
                    (stats.average_capture_rate * (total - 1.0) + rate) / total;
            }
        }

        // Create cluster if enabled and we have captures
        let cluster = if self.config.enable_clustering && !captured.is_empty() {
            let cluster = ImportanceCluster::new(&event, &captured);

            if let Ok(mut clusters) = self.clusters.write() {
                clusters.push(cluster.clone());
            }

            if let Ok(mut stats) = self.stats.write() {
                stats.total_clusters += 1;
            }

            Some(cluster)
        } else {
            None
        };

        CaptureResult {
            event,
            captured_memories: captured,
            considered_count,
            cluster,
            processing_time_us: start.elapsed().as_micros() as u64,
        }
    }

    /// Internal sweep implementation
    fn sweep_for_capture_internal(
        &mut self,
        event: &ImportanceEvent,
    ) -> (Vec<CapturedMemory>, usize) {
        let mut captured = Vec::new();
        let mut considered = 0;

        // Calculate capture window with event-type-specific multiplier
        let multiplier = event.event_type.capture_radius_multiplier();
        let effective_backward = self.config.capture_window.backward_hours * multiplier;
        let effective_forward = self.config.capture_window.forward_hours * multiplier;

        let effective_window = CaptureWindow::new(effective_backward, effective_forward);

        if let Ok(mut tags) = self.tags.write() {
            let event_id = event.event_id();

            for tag in tags.values_mut() {
                // Skip already captured tags
                if tag.captured {
                    continue;
                }

                // Check if in temporal window
                if !effective_window.is_in_window(tag.created_at, event.timestamp) {
                    continue;
                }

                considered += 1;

                // Calculate current tag strength
                let current_strength = tag.current_strength(
                    self.config.capture_window.decay_function,
                    self.config.tag_lifetime_hours,
                );

                // Check if tag is strong enough
                if current_strength < self.config.min_tag_strength {
                    continue;
                }

                // Calculate capture probability
                let capture_prob = effective_window
                    .capture_probability(tag.created_at, event.timestamp)
                    .unwrap_or(0.0);

                // Check if we should capture (probabilistic based on strength and proximity)
                let capture_score = current_strength * capture_prob * event.strength;

                if capture_score >= self.config.min_tag_strength {
                    // Calculate temporal distance
                    let temporal_distance =
                        (event.timestamp - tag.created_at).num_minutes() as f64 / 60.0;

                    // Calculate consolidated importance
                    let consolidated_importance =
                        (capture_score * 0.6 + event.strength * 0.4).min(1.0);

                    // Mark tag as captured
                    tag.capture(&event_id);

                    captured.push(CapturedMemory {
                        memory_id: tag.memory_id.clone(),
                        encoded_at: tag.created_at,
                        capture_event_id: event_id.clone(),
                        capture_event_type: event.event_type,
                        captured_at: Utc::now(),
                        capture_probability: capture_prob,
                        tag_strength_at_capture: current_strength,
                        consolidated_importance,
                        temporal_distance_hours: temporal_distance,
                    });

                    // Limit cluster size
                    if captured.len() >= self.config.max_cluster_size {
                        break;
                    }
                }
            }
        }

        (captured, considered)
    }

    /// Sweep for capture around a specific time
    ///
    /// Use this when you want to retroactively check for captures without
    /// a specific importance event (e.g., during periodic consolidation).
    ///
    /// # Arguments
    /// * `center_time` - The center time to sweep around
    ///
    /// # Returns
    /// List of memory IDs that could be captured
    pub fn sweep_for_capture(&mut self, center_time: DateTime<Utc>) -> Vec<String> {
        let mut eligible = Vec::new();

        if let Ok(tags) = self.tags.read() {
            for tag in tags.values() {
                if tag.captured {
                    continue;
                }

                if !self
                    .config
                    .capture_window
                    .is_in_window(tag.created_at, center_time)
                {
                    continue;
                }

                let current_strength = tag.current_strength(
                    self.config.capture_window.decay_function,
                    self.config.tag_lifetime_hours,
                );

                if current_strength >= self.config.min_tag_strength {
                    eligible.push(tag.memory_id.clone());
                }
            }
        }

        eligible
    }

    /// Decay all tags and clean up expired ones
    ///
    /// Should be called periodically (e.g., every hour) to:
    /// 1. Update tag strengths based on decay
    /// 2. Remove tags that have decayed below threshold
    /// 3. Remove captured tags that are no longer needed
    pub fn decay_tags(&mut self) {
        let mut expired_count = 0;

        if let Ok(mut tags) = self.tags.write() {
            tags.retain(|_, tag| {
                // Keep captured tags for a while (for reference)
                if tag.captured {
                    // Keep for 24 hours after capture
                    if let Some(captured_at) = tag.captured_at {
                        return (Utc::now() - captured_at).num_hours() < 24;
                    }
                    return false;
                }

                // Check if tag has decayed
                let current_strength = tag.current_strength(
                    self.config.capture_window.decay_function,
                    self.config.tag_lifetime_hours,
                );

                if current_strength < self.config.min_tag_strength * 0.1 {
                    expired_count += 1;
                    return false;
                }

                // Update stored strength
                tag.tag_strength = current_strength;
                true
            });
        }

        if let Ok(mut stats) = self.stats.write() {
            stats.tags_expired += expired_count;
            stats.active_tags = self.tags.read().map(|t| t.len()).unwrap_or(0);
            stats.last_cleanup = Some(Utc::now());
        }
    }

    /// Get a specific tag
    pub fn get_tag(&self, memory_id: &str) -> Option<SynapticTag> {
        self.tags.read().ok()?.get(memory_id).cloned()
    }

    /// Check if a memory has an active tag
    pub fn has_active_tag(&self, memory_id: &str) -> bool {
        self.tags
            .read()
            .ok()
            .and_then(|tags| tags.get(memory_id).cloned())
            .map(|tag| {
                tag.is_active(
                    self.config.capture_window.decay_function,
                    self.config.tag_lifetime_hours,
                    self.config.min_tag_strength,
                )
            })
            .unwrap_or(false)
    }

    /// Check if a memory was captured
    pub fn is_captured(&self, memory_id: &str) -> bool {
        self.tags
            .read()
            .ok()
            .and_then(|tags| tags.get(memory_id).map(|t| t.captured))
            .unwrap_or(false)
    }

    /// Get all active tags
    pub fn get_active_tags(&self) -> Vec<SynapticTag> {
        self.tags
            .read()
            .ok()
            .map(|tags| {
                tags.values()
                    .filter(|tag| {
                        tag.is_active(
                            self.config.capture_window.decay_function,
                            self.config.tag_lifetime_hours,
                            self.config.min_tag_strength,
                        )
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all captured tags
    pub fn get_captured_tags(&self) -> Vec<SynapticTag> {
        self.tags
            .read()
            .ok()
            .map(|tags| tags.values().filter(|tag| tag.captured).cloned().collect())
            .unwrap_or_default()
    }

    /// Get clusters containing a memory
    pub fn get_clusters_for_memory(&self, memory_id: &str) -> Vec<ImportanceCluster> {
        self.clusters
            .read()
            .ok()
            .map(|clusters| {
                clusters
                    .iter()
                    .filter(|c| c.contains(memory_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all clusters
    pub fn get_all_clusters(&self) -> Vec<ImportanceCluster> {
        self.clusters
            .read()
            .ok()
            .map(|clusters| clusters.clone())
            .unwrap_or_default()
    }

    /// Get statistics
    pub fn stats(&self) -> TaggingStats {
        self.stats
            .read()
            .ok()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Clear all state (for testing)
    pub fn clear(&mut self) {
        if let Ok(mut tags) = self.tags.write() {
            tags.clear();
        }
        if let Ok(mut clusters) = self.clusters.write() {
            clusters.clear();
        }
        if let Ok(mut stats) = self.stats.write() {
            *stats = TaggingStats::default();
        }
    }

    /// Get memory IDs that are candidates for capture within a time range
    ///
    /// This is useful for batch processing - you can get candidates first,
    /// then process importance events for them.
    pub fn get_capture_candidates(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<String> {
        self.tags
            .read()
            .ok()
            .map(|tags| {
                tags.values()
                    .filter(|tag| {
                        !tag.captured
                            && tag.created_at >= start
                            && tag.created_at <= end
                            && tag.is_active(
                                self.config.capture_window.decay_function,
                                self.config.tag_lifetime_hours,
                                self.config.min_tag_strength,
                            )
                    })
                    .map(|tag| tag.memory_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Bulk tag multiple memories
    pub fn tag_memories(&mut self, memory_ids: &[&str]) -> Vec<SynapticTag> {
        memory_ids.iter().map(|id| self.tag_memory(id)).collect()
    }

    /// Process multiple importance events
    pub fn trigger_prp_batch(&mut self, events: Vec<ImportanceEvent>) -> Vec<CaptureResult> {
        events.into_iter().map(|e| self.trigger_prp(e)).collect()
    }
}

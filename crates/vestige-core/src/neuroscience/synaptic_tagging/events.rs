//! Importance events that trigger PRP production, plus the captured-memory
//! and clustering structures we hand back to callers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// IMPORTANCE EVENTS
// ============================================================================

/// Types of events that trigger PRP production
///
/// Each type has different characteristics:
/// - **UserFlag**: Highest priority, explicit user action
/// - **EmotionalContent**: Detected via sentiment analysis
/// - **NoveltySpike**: High prediction error indicates something unexpected
/// - **RepeatedAccess**: Pattern of repeated retrieval
/// - **CrossReference**: Important memory references this one
/// - **TemporalProximity**: Close in time to confirmed important memory
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImportanceEventType {
    /// Explicit user flag ("remember this", "important")
    UserFlag,
    /// Detected emotional content via sentiment analysis
    EmotionalContent,
    /// High prediction error (novelty detection)
    NoveltySpike,
    /// Memory accessed multiple times in short period
    RepeatedAccess,
    /// Referenced by other important memories
    CrossReference,
    /// Temporally close to confirmed important memory
    TemporalProximity,
}

impl ImportanceEventType {
    /// Get the base PRP strength for this event type
    ///
    /// Different event types have different inherent importance:
    /// - UserFlag has highest strength (explicit user intent)
    /// - NoveltySpike has high strength (surprising = memorable)
    /// - EmotionalContent and RepeatedAccess have medium strength
    /// - CrossReference and TemporalProximity have lower strength (indirect)
    pub fn base_strength(&self) -> f64 {
        match self {
            ImportanceEventType::UserFlag => 1.0,
            ImportanceEventType::NoveltySpike => 0.9,
            ImportanceEventType::EmotionalContent => 0.8,
            ImportanceEventType::RepeatedAccess => 0.75,
            ImportanceEventType::CrossReference => 0.6,
            ImportanceEventType::TemporalProximity => 0.5,
        }
    }

    /// Get the capture radius multiplier
    ///
    /// Some event types should have wider capture windows:
    /// - UserFlag: Standard window (1.0x)
    /// - EmotionalContent: Wider window (1.5x) - emotions spread context
    /// - NoveltySpike: Narrower window (0.7x) - novelty is specific
    pub fn capture_radius_multiplier(&self) -> f64 {
        match self {
            ImportanceEventType::EmotionalContent => 1.5,
            ImportanceEventType::UserFlag => 1.0,
            ImportanceEventType::RepeatedAccess => 1.2,
            ImportanceEventType::CrossReference => 1.0,
            ImportanceEventType::TemporalProximity => 0.8,
            ImportanceEventType::NoveltySpike => 0.7,
        }
    }
}

impl std::fmt::Display for ImportanceEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportanceEventType::UserFlag => write!(f, "user_flag"),
            ImportanceEventType::EmotionalContent => write!(f, "emotional"),
            ImportanceEventType::NoveltySpike => write!(f, "novelty"),
            ImportanceEventType::RepeatedAccess => write!(f, "repeated"),
            ImportanceEventType::CrossReference => write!(f, "cross_ref"),
            ImportanceEventType::TemporalProximity => write!(f, "temporal"),
        }
    }
}

/// An event that triggers PRP production
///
/// When an importance event occurs, the system produces Plasticity-Related
/// Products that can be captured by nearby tagged memories, consolidating
/// them retroactively.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportanceEvent {
    /// Type of importance event
    pub event_type: ImportanceEventType,
    /// Memory that triggered the event (if any)
    pub memory_id: Option<String>,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Event strength (0.0 to 1.0)
    pub strength: f64,
    /// Additional context about the event
    pub context: Option<String>,
}

impl ImportanceEvent {
    /// Create a new importance event
    pub fn new(event_type: ImportanceEventType) -> Self {
        Self {
            event_type,
            memory_id: None,
            timestamp: Utc::now(),
            strength: event_type.base_strength(),
            context: None,
        }
    }

    /// Create with memory ID
    pub fn for_memory(memory_id: &str, event_type: ImportanceEventType) -> Self {
        Self {
            event_type,
            memory_id: Some(memory_id.to_string()),
            timestamp: Utc::now(),
            strength: event_type.base_strength(),
            context: None,
        }
    }

    /// Create with custom strength
    pub fn with_strength(event_type: ImportanceEventType, strength: f64) -> Self {
        Self {
            event_type,
            memory_id: None,
            timestamp: Utc::now(),
            strength: strength.clamp(0.0, 1.0),
            context: None,
        }
    }

    /// Create a user flag event
    pub fn user_flag(memory_id: &str, context: Option<&str>) -> Self {
        Self {
            event_type: ImportanceEventType::UserFlag,
            memory_id: Some(memory_id.to_string()),
            timestamp: Utc::now(),
            strength: 1.0,
            context: context.map(|s| s.to_string()),
        }
    }

    /// Create an emotional content event
    pub fn emotional(memory_id: &str, sentiment_magnitude: f64) -> Self {
        Self {
            event_type: ImportanceEventType::EmotionalContent,
            memory_id: Some(memory_id.to_string()),
            timestamp: Utc::now(),
            strength: sentiment_magnitude.clamp(0.0, 1.0),
            context: None,
        }
    }

    /// Create a novelty spike event
    pub fn novelty(memory_id: &str, prediction_error: f64) -> Self {
        Self {
            event_type: ImportanceEventType::NoveltySpike,
            memory_id: Some(memory_id.to_string()),
            timestamp: Utc::now(),
            strength: prediction_error.clamp(0.0, 1.0),
            context: None,
        }
    }

    /// Create a repeated access event
    pub fn repeated_access(memory_id: &str, access_count: u32) -> Self {
        // Strength scales with access count but caps at 1.0
        let strength = (access_count as f64 / 5.0).min(1.0);
        Self {
            event_type: ImportanceEventType::RepeatedAccess,
            memory_id: Some(memory_id.to_string()),
            timestamp: Utc::now(),
            strength,
            context: Some(format!("{} accesses", access_count)),
        }
    }

    /// Generate unique event ID
    pub fn event_id(&self) -> String {
        format!(
            "{}-{}-{}",
            self.event_type,
            self.timestamp.timestamp_millis(),
            self.memory_id.as_deref().unwrap_or("none")
        )
    }
}

// ============================================================================
// CAPTURED MEMORY
// ============================================================================

/// A memory that was captured (retroactively consolidated)
///
/// This represents the result of successful STC - a previously ordinary memory
/// that has been promoted to long-term storage due to a subsequent importance
/// event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedMemory {
    /// The memory that was captured
    pub memory_id: String,
    /// When the memory was originally encoded
    pub encoded_at: DateTime<Utc>,
    /// The event that caused capture
    pub capture_event_id: String,
    /// The type of event that caused capture
    pub capture_event_type: ImportanceEventType,
    /// When the memory was captured
    pub captured_at: DateTime<Utc>,
    /// Capture probability at time of capture
    pub capture_probability: f64,
    /// Tag strength at time of capture
    pub tag_strength_at_capture: f64,
    /// Final consolidated importance score
    pub consolidated_importance: f64,
    /// Temporal distance from trigger event (hours)
    pub temporal_distance_hours: f64,
}

impl CapturedMemory {
    /// Check if this was a backward capture (memory before event)
    pub fn is_backward_capture(&self) -> bool {
        self.temporal_distance_hours > 0.0
    }

    /// Check if this was a forward capture (memory after event)
    pub fn is_forward_capture(&self) -> bool {
        self.temporal_distance_hours < 0.0
    }
}

// ============================================================================
// IMPORTANCE CLUSTER
// ============================================================================

/// A cluster of important memories around a significant moment
///
/// When an importance event occurs, it often captures multiple related memories.
/// These form an "importance cluster" - a group of memories that collectively
/// provide context around a significant moment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportanceCluster {
    /// Unique cluster ID
    pub cluster_id: String,
    /// The triggering importance event
    pub trigger_event_id: String,
    /// Type of the triggering event
    pub trigger_event_type: ImportanceEventType,
    /// Center time of the cluster
    pub center_time: DateTime<Utc>,
    /// Memory IDs in this cluster
    pub memory_ids: Vec<String>,
    /// Average importance of memories in cluster
    pub average_importance: f64,
    /// When this cluster was created
    pub created_at: DateTime<Utc>,
    /// Temporal span of the cluster (hours)
    pub temporal_span_hours: f64,
}

impl ImportanceCluster {
    /// Create a new cluster
    pub fn new(trigger_event: &ImportanceEvent, captured: &[CapturedMemory]) -> Self {
        let memory_ids: Vec<String> = captured.iter().map(|c| c.memory_id.clone()).collect();

        let average_importance = if captured.is_empty() {
            0.0
        } else {
            captured
                .iter()
                .map(|c| c.consolidated_importance)
                .sum::<f64>()
                / captured.len() as f64
        };

        let temporal_span = if captured.len() < 2 {
            0.0
        } else {
            // Safe: captured.len() >= 2 guarantees non-empty iterator
            match (
                captured.iter().map(|c| c.encoded_at).min(),
                captured.iter().map(|c| c.encoded_at).max(),
            ) {
                (Some(min_time), Some(max_time)) => {
                    (max_time - min_time).num_minutes() as f64 / 60.0
                }
                _ => 0.0,
            }
        };

        Self {
            cluster_id: uuid::Uuid::new_v4().to_string(),
            trigger_event_id: trigger_event.event_id(),
            trigger_event_type: trigger_event.event_type,
            center_time: trigger_event.timestamp,
            memory_ids,
            average_importance,
            created_at: Utc::now(),
            temporal_span_hours: temporal_span,
        }
    }

    /// Get the number of memories in this cluster
    pub fn size(&self) -> usize {
        self.memory_ids.len()
    }

    /// Check if a memory is in this cluster
    pub fn contains(&self, memory_id: &str) -> bool {
        self.memory_ids.iter().any(|id| id == memory_id)
    }
}

// ============================================================================
// CAPTURE RESULT
// ============================================================================

/// Result of a PRP trigger operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    /// The event that triggered capture
    pub event: ImportanceEvent,
    /// Memories that were captured
    pub captured_memories: Vec<CapturedMemory>,
    /// Tags that were considered but not captured
    pub considered_count: usize,
    /// The importance cluster created (if any)
    pub cluster: Option<ImportanceCluster>,
    /// Processing time in microseconds
    pub processing_time_us: u64,
}

impl CaptureResult {
    /// Get the number of captured memories
    pub fn captured_count(&self) -> usize {
        self.captured_memories.len()
    }

    /// Check if any memories were captured
    pub fn has_captures(&self) -> bool {
        !self.captured_memories.is_empty()
    }

    /// Get the capture rate (captured / considered)
    pub fn capture_rate(&self) -> f64 {
        if self.considered_count == 0 {
            return 0.0;
        }
        self.captured_memories.len() as f64 / self.considered_count as f64
    }
}

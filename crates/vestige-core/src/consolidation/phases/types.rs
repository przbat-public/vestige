//! Data types for the 4-phase dream cycle (results, triage, insights, connections).

use chrono::{DateTime, Utc};

/// Which dream phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreamPhase {
    /// Light sleep — triage and scoring
    Nrem1,
    /// Deep sleep — consolidation and replay
    Nrem3,
    /// REM sleep — creative connections and emotional processing
    Rem,
    /// Pre-wake — validate and integrate
    Integration,
}

impl DreamPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            DreamPhase::Nrem1 => "NREM1_Triage",
            DreamPhase::Nrem3 => "NREM3_Consolidation",
            DreamPhase::Rem => "REM_Creative",
            DreamPhase::Integration => "Integration",
        }
    }
}

impl std::fmt::Display for DreamPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result from a single dream phase
#[derive(Debug, Clone)]
pub struct PhaseResult {
    pub phase: DreamPhase,
    pub duration_ms: u64,
    pub memories_processed: usize,
    pub actions: Vec<String>,
}

/// Memory categorized during NREM1 triage
#[derive(Debug, Clone)]
pub struct TriagedMemory {
    pub id: String,
    pub content: String,
    pub importance: f64,
    pub category: TriageCategory,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub retention_strength: f64,
    pub emotional_valence: f64,
    pub is_flashbulb: bool,
}

/// Categories assigned during NREM1 triage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageCategory {
    /// High emotional content (bug fixes, breakthroughs, frustrations)
    Emotional,
    /// Future-relevant (intentions, plans, TODOs)
    FutureRelevant,
    /// User-promoted or high-reward memories
    Rewarded,
    /// High prediction error / novel content
    Novel,
    /// Standard memory, no special category
    Standard,
}

/// A creative connection discovered during REM
#[derive(Debug, Clone)]
pub struct CreativeConnection {
    pub memory_a_id: String,
    pub memory_b_id: String,
    pub insight: String,
    pub confidence: f64,
    pub connection_type: CreativeConnectionType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreativeConnectionType {
    /// Memories from different domains share an abstract pattern
    CrossDomain,
    /// Memories together suggest a causal relationship
    Causal,
    /// Memories complement each other (fill knowledge gaps)
    Complementary,
    /// Memories contradict — needs resolution
    Contradictory,
}

/// A validated insight from the Integration phase
#[derive(Debug, Clone)]
pub struct DreamInsight {
    pub insight: String,
    pub source_memory_ids: Vec<String>,
    pub confidence: f64,
    pub novelty: f64,
    pub insight_type: String,
}

/// Complete result from the 4-phase dream cycle
#[derive(Debug, Clone)]
pub struct FourPhaseDreamResult {
    pub phases: Vec<PhaseResult>,
    pub total_duration_ms: u64,
    pub memories_replayed: usize,
    pub insights: Vec<DreamInsight>,
    pub creative_connections: Vec<CreativeConnection>,
    pub memories_strengthened: usize,
    pub memories_downscaled: usize,
    pub emotional_processed: usize,
    pub replay_queue_size: usize,
}

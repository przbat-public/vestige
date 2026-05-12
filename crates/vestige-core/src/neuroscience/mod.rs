//! # Neuroscience-Inspired Memory Mechanisms
//!
//! This module implements cutting-edge neuroscience findings for memory systems.
//! Unlike traditional AI memory systems that treat importance as static, these
//! mechanisms capture the dynamic nature of biological memory.
//!
//! ## Key Insight: Retroactive Importance
//!
//! In biological systems, memories can become important AFTER encoding based on
//! subsequent events. This is fundamentally different from how AI systems typically
//! work, where importance is determined at encoding time.
//!
//! ## Implemented Mechanisms
//!
//! - **Memory States**: Memories exist on a continuum of accessibility (Active, Dormant,
//!   Silent, Unavailable) rather than simply "remembered" or "forgotten". Implements
//!   retrieval-induced forgetting where retrieving one memory can suppress similar ones.
//!
//! - **Synaptic Tagging and Capture (STC)**: Memories can be consolidated retroactively
//!   when related important events occur within a temporal window (up to 9 hours in
//!   biological systems, configurable here).
//!
//! - **Context-Dependent Memory**: Encoding Specificity Principle (Tulving & Thomson, 1973)
//!   Memory retrieval is most effective when the retrieval context matches the encoding context.
//!
//! - **Spreading Activation**: Associative Memory Network (Collins & Loftus, 1975)
//!   Based on Hebbian learning: "Neurons that fire together wire together"
//!
//! ## Scientific Foundations
//!
//! ### Encoding Specificity Principle
//!
//! Tulving's research showed that memory recall is significantly enhanced when the
//! retrieval environment matches the learning environment. This includes:
//!
//! - **Physical Context**: Where you were when you learned something
//! - **Temporal Context**: When you learned it (time of day, day of week)
//! - **Emotional Context**: Your emotional state during encoding
//! - **Cognitive Context**: What you were thinking about (active topics)
//!
//! ### Spreading Activation Theory
//!
//! Collins and Loftus proposed that memory is organized as a semantic network where:
//!
//! - Concepts are represented as **nodes**
//! - Related concepts are connected by **associative links**
//! - Activating one concept spreads activation to related concepts
//! - Stronger/more recently used links spread more activation
//!
//! ## References
//!
//! - Frey, U., & Morris, R. G. (1997). Synaptic tagging and long-term potentiation. Nature.
//! - Redondo, R. L., & Morris, R. G. (2011). Making memories last: the synaptic tagging
//!   and capture hypothesis. Nature Reviews Neuroscience.
//! - Tulving, E., & Thomson, D. M. (1973). Encoding specificity and retrieval processes
//!   in episodic memory. Psychological Review.
//! - Collins, A. M., & Loftus, E. F. (1975). A spreading-activation theory of semantic
//!   processing. Psychological Review.

// ============================================================================
// Submodule layout — grouped by cognitive role
// ----------------------------------------------------------------------------
// All submodules stay at this level for backward-compatible `use` paths.
// The grouping below documents *why* each file exists, so future readers can
// find what they need without re-running grep:
//
//   ENCODING (write-time signals that shape what gets stored and how strongly)
//     - synaptic_tagging      : Synaptic Tagging & Capture (retroactive importance)
//     - importance_signals    : 4-channel encoding (novelty / arousal / reward / attention)
//     - emotional_memory      : Emotion-modulated encoding & slower decay
//     - context_memory        : Encoding Specificity Principle (also used at recall)
//
//   RETRIEVAL (read-time mechanisms that surface memories on demand)
//     - hippocampal_index     : Two-phase recall via compact index pointers
//     - spreading_activation  : Associative semantic-network activation
//     - predictive_retrieval  : Predict needed memories before they're asked for
//     - prospective_memory    : Future-oriented intentions and trigger checking
//
//   STATE (lifecycle of an individual memory across time)
//     - memory_states         : Active/Dormant/Silent/Unavailable + competition
//
//   META (the system's awareness of its own memory quality)
//     - metacognition         : Confidence / contradictions / known-unknowns
// ============================================================================

pub mod context_memory;
pub mod emotional_memory;
pub mod hippocampal_index;
pub mod importance_signals;
pub mod memory_states;
pub mod metacognition;
pub mod predictive_retrieval;
pub mod prospective_memory;
pub mod spreading_activation;
pub mod synaptic_tagging;

// Re-exports for convenient access
pub use synaptic_tagging::{
    // Results
    CaptureResult,
    CaptureWindow,
    CapturedMemory,
    DecayFunction,
    ImportanceCluster,
    // Importance events
    ImportanceEvent,
    ImportanceEventType,
    // Core types
    SynapticTag,
    // Configuration
    SynapticTaggingConfig,
    SynapticTaggingSystem,
    TaggingStats,
};

// Context-dependent memory (Encoding Specificity Principle)
pub use context_memory::{
    ContextMatcher, ContextReinstatement, ContextWeights, EmotionalContext, EncodingContext,
    RecencyBucket, ScoredMemory, SessionContext, TemporalContext, TimeOfDay, TopicalContext,
};

// Memory states (accessibility continuum)
pub use memory_states::{
    // Constants
    ACCESSIBILITY_ACTIVE,
    ACCESSIBILITY_DORMANT,
    ACCESSIBILITY_SILENT,
    ACCESSIBILITY_UNAVAILABLE,
    // Accessibility scoring
    AccessibilityCalculator,
    BatchUpdateResult,
    COMPETITION_SIMILARITY_THRESHOLD,
    CompetitionCandidate,
    CompetitionConfig,
    CompetitionEvent,
    // Competition system (Retrieval-Induced Forgetting)
    CompetitionManager,
    CompetitionResult,
    DEFAULT_ACTIVE_DECAY_HOURS,
    DEFAULT_DORMANT_DECAY_DAYS,
    LifecycleSummary,
    MemoryLifecycle,
    // Core types
    MemoryState,
    MemoryStateInfo,
    StateDecayConfig,
    StatePercentages,
    // Analytics and info
    StateTimeAccumulator,
    StateTransition,
    StateTransitionReason,
    // State management
    StateUpdateService,
};

// Multi-channel importance signaling (Neuromodulator-inspired)
pub use importance_signals::{
    AccessPattern,
    ArousalExplanation,
    ArousalSignal,
    AttentionExplanation,
    AttentionSignal,
    CompositeWeights,
    ConsolidationPriority,
    Context,
    EmotionalMarker,
    ImportanceConsolidationConfig,
    // Configuration types
    ImportanceEncodingConfig,
    ImportanceRetrievalConfig,
    ImportanceScore,
    // Core types
    ImportanceSignals,
    MarkerType,
    // Explanation types
    NoveltyExplanation,
    // Individual signals
    NoveltySignal,
    Outcome,
    OutcomeType,
    RewardExplanation,
    RewardSignal,
    // Supporting types
    SentimentAnalyzer,
    SentimentResult,
    Session,
};

// Hippocampal indexing (Teyler & Rudy, 2007)
pub use hippocampal_index::{
    // Link types
    AssociationLinkType,
    // Barcode generation
    BarcodeGenerator,
    ContentPointer,
    ContentStore,
    // Storage types
    ContentType,
    FullMemory,
    // Core types
    HippocampalIndex,
    HippocampalIndexConfig,
    HippocampalIndexError,
    // Constants
    INDEX_EMBEDDING_DIM,
    ImportanceFlags,
    IndexLink,
    IndexMatch,
    // Query types
    IndexQuery,
    MemoryBarcode,
    // Index structures
    MemoryIndex,
    MigrationNode,
    // Migration
    MigrationResult,
    StorageLocation,
    TemporalMarker,
};

// Predictive memory retrieval (Free Energy Principle - Friston, 2010)
pub use predictive_retrieval::{
    // Backward-compatible aliases
    ContextualPredictor,
    // Enhanced types (Friston's Active Inference)
    PredictedMemory,
    Prediction,
    PredictionConfidence,
    PredictionOutcome,
    PredictionReason,
    PredictiveConfig,
    PredictiveMemory,
    PredictiveMemoryConfig,
    PredictiveMemoryError,
    PredictiveRetriever,
    ProjectContext as PredictiveProjectContext,
    QueryPattern,
    SequencePredictor,
    SessionContext as PredictiveSessionContext,
    TemporalPatterns,
    TemporalPredictor,
    UserModel,
};

// Prospective memory (Einstein & McDaniel, 1990)
pub use prospective_memory::{
    // Context monitoring
    Context as ProspectiveContext,
    ContextMonitor,
    // Triggers and patterns
    ContextPattern,
    // Intentions
    Intention,
    IntentionParser,
    IntentionSource,
    IntentionStats,
    IntentionStatus,
    IntentionTrigger,
    Priority,
    // Core engine
    ProspectiveMemory,
    ProspectiveMemoryConfig,
    ProspectiveMemoryError,
    RecurrencePattern,
    TriggerPattern,
};

// Spreading activation (Associative Memory Network - Collins & Loftus, 1975)
pub use spreading_activation::{
    ActivatedMemory, ActivationConfig, ActivationNetwork, ActivationNode, AssociatedMemory,
    AssociationEdge, LinkType,
};

// Emotional memory (Brown & Kulik 1977, Bower 1981, LaBar & Cabeza 2006)
pub use emotional_memory::{
    EmotionCategory, EmotionalEvaluation, EmotionalMemory, EmotionalMemoryStats,
};

// Metacognition (Nelson & Narens 1990)
pub use metacognition::{MetacognitionMonitor, MetacognitionReport};

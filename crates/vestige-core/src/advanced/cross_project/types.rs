//! Data model for the cross-project learner.
//!
//! Splitting types out of the main [`super`] module keeps the API surface
//! easy to scan: anyone reading the learner only sees behaviour, not 200
//! lines of `Serialize`-derived structs scrolling past.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Minimum projects a pattern must appear in to be considered universal.
pub(super) const MIN_PROJECTS_FOR_UNIVERSAL: usize = 2;

/// Minimum success rate for pattern recommendations.
pub(super) const MIN_SUCCESS_RATE: f64 = 0.6;

// ---------------------------------------------------------------------------
// Public types — re-exported via `pub use` in `mod.rs`.
// ---------------------------------------------------------------------------

/// A universal pattern found across multiple projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalPattern {
    /// Unique pattern ID.
    pub id: String,
    /// The pattern itself.
    pub pattern: CodePattern,
    /// Projects where this pattern was observed.
    pub projects_seen_in: Vec<String>,
    /// Success rate (how often it helped).
    pub success_rate: f64,
    /// Description of when this pattern is applicable.
    pub applicability: String,
    /// Confidence in this pattern (based on evidence).
    pub confidence: f64,
    /// When this pattern was first observed.
    pub first_seen: DateTime<Utc>,
    /// When this pattern was last observed.
    pub last_seen: DateTime<Utc>,
    /// How many times this pattern was applied.
    pub application_count: u32,
}

/// A code pattern that can be learned and applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePattern {
    /// Pattern name/identifier.
    pub name: String,
    /// Pattern category.
    pub category: PatternCategory,
    /// Description of the pattern.
    pub description: String,
    /// Example code or usage.
    pub example: Option<String>,
    /// Conditions that suggest this pattern applies.
    pub triggers: Vec<PatternTrigger>,
    /// What the pattern helps with.
    pub benefits: Vec<String>,
    /// Potential drawbacks or considerations.
    pub considerations: Vec<String>,
}

/// Categories of patterns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PatternCategory {
    /// Error handling patterns.
    ErrorHandling,
    /// Async/concurrent code patterns.
    AsyncConcurrency,
    /// Testing strategies.
    Testing,
    /// Code organization/architecture.
    Architecture,
    /// Performance optimization.
    Performance,
    /// Security practices.
    Security,
    /// Debugging approaches.
    Debugging,
    /// Refactoring techniques.
    Refactoring,
    /// Documentation practices.
    Documentation,
    /// Build/tooling patterns.
    Tooling,
    /// Custom category.
    Custom(String),
}

/// Conditions that trigger pattern applicability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternTrigger {
    /// Type of trigger.
    pub trigger_type: TriggerType,
    /// Value/pattern to match.
    pub value: String,
    /// Confidence that this trigger indicates pattern applies.
    pub confidence: f64,
}

/// Types of triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerType {
    /// File name or extension.
    FileName,
    /// Code construct or keyword.
    CodeConstruct,
    /// Error message pattern.
    ErrorMessage,
    /// Directory structure.
    DirectoryStructure,
    /// Dependency/import.
    Dependency,
    /// Intent detected.
    Intent,
    /// Topic being discussed.
    Topic,
}

/// Knowledge that might apply to current context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicableKnowledge {
    /// The pattern that might apply.
    pub pattern: UniversalPattern,
    /// Why we think it applies.
    pub match_reason: String,
    /// Confidence that it applies here.
    pub applicability_confidence: f64,
    /// Specific suggestions for applying it.
    pub suggestions: Vec<String>,
    /// Memories that support this application.
    pub supporting_memories: Vec<String>,
}

/// A suggestion for applying patterns to a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// What we suggest.
    pub suggestion: String,
    /// Pattern this is based on.
    pub based_on: String,
    /// Confidence level.
    pub confidence: f64,
    /// Supporting evidence (memory IDs).
    pub evidence: Vec<String>,
    /// Priority (higher = more important).
    pub priority: u32,
}

/// Context about the current project.
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    /// Project root path.
    pub path: Option<PathBuf>,
    /// Project name.
    pub name: Option<String>,
    /// Languages used.
    pub languages: Vec<String>,
    /// Frameworks detected.
    pub frameworks: Vec<String>,
    /// File types present.
    pub file_types: HashSet<String>,
    /// Dependencies.
    pub dependencies: Vec<String>,
    /// Project structure (key directories).
    pub structure: Vec<String>,
}

impl ProjectContext {
    /// Create context from a project path (would scan project in production).
    pub fn from_path(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            name: path.file_name().map(|n| n.to_string_lossy().to_string()),
            ..Default::default()
        }
    }

    /// Add detected language.
    pub fn with_language(mut self, lang: &str) -> Self {
        self.languages.push(lang.to_string());
        self
    }

    /// Add detected framework.
    pub fn with_framework(mut self, framework: &str) -> Self {
        self.frameworks.push(framework.to_string());
        self
    }
}

/// Memory input for learning.
#[derive(Debug, Clone)]
pub struct MemoryForLearning {
    /// Memory ID.
    pub id: String,
    /// Memory content.
    pub content: String,
    /// Project name.
    pub project_name: String,
    /// Category.
    pub category: Option<PatternCategory>,
}

// ---------------------------------------------------------------------------
// Internal types — never leak from the module.
// ---------------------------------------------------------------------------

/// Project memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Internal data carrier shared between mod.rs (writes) and internals.rs \
              (reads); both live in the same `cross_project` module and the struct \
              never escapes it."
)]
pub(super) struct ProjectMemory {
    pub(super) memory_id: String,
    pub(super) project_name: String,
    pub(super) category: Option<PatternCategory>,
    pub(super) was_helpful: Option<bool>,
    pub(super) timestamp: DateTime<Utc>,
}

/// Outcome of applying a pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Internal data carrier shared between mod.rs (writes) and internals.rs \
              (reads); both live in the same `cross_project` module and the struct \
              never escapes it."
)]
pub(super) struct PatternOutcome {
    pub(super) pattern_id: String,
    pub(super) project_name: String,
    pub(super) was_successful: bool,
    pub(super) timestamp: DateTime<Utc>,
}

/// Stable string key for a [`PatternCategory`] used when minting auto-generated
/// pattern IDs (`auto-<category>-<keyword>`).
pub(super) fn category_to_string(cat: &PatternCategory) -> &'static str {
    match cat {
        PatternCategory::ErrorHandling => "error-handling",
        PatternCategory::AsyncConcurrency => "async",
        PatternCategory::Testing => "testing",
        PatternCategory::Architecture => "architecture",
        PatternCategory::Performance => "performance",
        PatternCategory::Security => "security",
        PatternCategory::Debugging => "debugging",
        PatternCategory::Refactoring => "refactoring",
        PatternCategory::Documentation => "docs",
        PatternCategory::Tooling => "tooling",
        PatternCategory::Custom(_) => "custom",
    }
}

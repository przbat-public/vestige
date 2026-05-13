//! # Cross-Project Learning
//!
//! Learn patterns that apply across ALL projects. Vestige doesn't just
//! remember project-specific knowledge — it identifies universal patterns
//! that make you more effective everywhere.
//!
//! ## Pattern Types
//!
//! - **Code Patterns**: Error handling, async patterns, testing strategies
//! - **Architecture Patterns**: Project structures, module organization
//! - **Process Patterns**: Debug workflows, refactoring approaches
//! - **Domain Patterns**: Industry-specific knowledge that transfers
//!
//! ## How It Works
//!
//! 1. **Pattern Extraction**: Analyzes memories across projects for commonalities.
//! 2. **Success Tracking**: Monitors which patterns led to successful outcomes.
//! 3. **Applicability Detection**: Recognizes when current context matches a pattern.
//! 4. **Suggestion Generation**: Provides actionable suggestions based on patterns.
//!
//! ## Example
//!
//! ```rust,ignore
//! let learner = CrossProjectLearner::new();
//!
//! // Find patterns that worked across multiple projects.
//! let patterns = learner.find_universal_patterns();
//!
//! // Apply to a new project.
//! let suggestions = learner.apply_to_project(Path::new("/new/project"));
//! ```
//!
//! ## Module Layout
//!
//! - `types.rs` — public data model + private bookkeeping records.
//! - `internals.rs` — private learner methods (extraction, applicability
//!   scoring). Skipped from rustdoc because the helpers are crate-private;
//!   pass `--document-private-items` to inspect them.
//! - This file — [`CrossProjectLearner`] struct + public API.

mod internals;
mod types;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use chrono::Utc;

pub use types::{
    ApplicableKnowledge, CodePattern, MemoryForLearning, PatternCategory, PatternTrigger,
    ProjectContext, Suggestion, TriggerType, UniversalPattern,
};

use types::{MIN_PROJECTS_FOR_UNIVERSAL, MIN_SUCCESS_RATE, PatternOutcome, ProjectMemory};

/// Cross-project learning engine.
//
// Field-level visibility is restricted to the `cross_project` module so the
// private `ProjectMemory` / `PatternOutcome` types stay encapsulated even
// though `CrossProjectLearner` itself is `pub`. We document the conscious
// scoped access for the lint that `field_scoped_visibility_modifiers`
// produces.
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "internals.rs is a sibling submodule that needs raw access to the locks; \
              wrapping every read/write through the parent would multiply boilerplate \
              without changing the trust window."
)]
pub struct CrossProjectLearner {
    /// Patterns discovered.
    pub(in crate::advanced::cross_project) patterns: Arc<RwLock<HashMap<String, UniversalPattern>>>,
    /// Project-memory associations.
    pub(in crate::advanced::cross_project) project_memories: Arc<RwLock<Vec<ProjectMemory>>>,
    /// Pattern application outcomes.
    pub(in crate::advanced::cross_project) outcomes: Arc<RwLock<Vec<PatternOutcome>>>,
}

impl CrossProjectLearner {
    /// Create a new cross-project learner.
    pub fn new() -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
            project_memories: Arc::new(RwLock::new(Vec::new())),
            outcomes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Find patterns that appear in multiple projects.
    pub fn find_universal_patterns(&self) -> Vec<UniversalPattern> {
        let patterns = self
            .patterns
            .read()
            .map(|p| p.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        patterns
            .into_iter()
            .filter(|p| {
                p.projects_seen_in.len() >= MIN_PROJECTS_FOR_UNIVERSAL
                    && p.success_rate >= MIN_SUCCESS_RATE
            })
            .collect()
    }

    /// Apply learned patterns to a new project.
    pub fn apply_to_project(&self, project: &Path) -> Vec<Suggestion> {
        let context = ProjectContext::from_path(project);
        self.generate_suggestions(&context)
    }

    /// Apply with full context.
    pub fn apply_to_context(&self, context: &ProjectContext) -> Vec<Suggestion> {
        self.generate_suggestions(context)
    }

    /// Detect when current situation matches cross-project knowledge.
    pub fn detect_applicable(&self, context: &ProjectContext) -> Vec<ApplicableKnowledge> {
        let mut applicable = Vec::new();

        let patterns = self
            .patterns
            .read()
            .map(|p| p.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        for pattern in patterns {
            if let Some(knowledge) = self.check_pattern_applicability(&pattern, context) {
                applicable.push(knowledge);
            }
        }

        // Sort by applicability confidence (NaN-safe).
        applicable.sort_by(|a, b| {
            b.applicability_confidence
                .partial_cmp(&a.applicability_confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        applicable
    }

    /// Record that a memory was associated with a project.
    pub fn record_project_memory(
        &self,
        memory_id: &str,
        project_name: &str,
        category: Option<PatternCategory>,
    ) {
        if let Ok(mut memories) = self.project_memories.write() {
            memories.push(ProjectMemory {
                memory_id: memory_id.to_string(),
                project_name: project_name.to_string(),
                category,
                was_helpful: None,
                timestamp: Utc::now(),
            });
        }
    }

    /// Record outcome of applying a pattern.
    pub fn record_pattern_outcome(
        &self,
        pattern_id: &str,
        project_name: &str,
        was_successful: bool,
    ) {
        if let Ok(mut outcomes) = self.outcomes.write() {
            outcomes.push(PatternOutcome {
                pattern_id: pattern_id.to_string(),
                project_name: project_name.to_string(),
                was_successful,
                timestamp: Utc::now(),
            });
        }

        self.update_pattern_success_rate(pattern_id);
    }

    /// Add or update a pattern.
    pub fn add_pattern(&self, pattern: UniversalPattern) {
        if let Ok(mut patterns) = self.patterns.write() {
            patterns.insert(pattern.id.clone(), pattern);
        }
    }

    /// Learn patterns from existing memories.
    pub fn learn_from_memories(&self, memories: &[MemoryForLearning]) {
        // Group memories by category.
        let mut by_category: HashMap<PatternCategory, Vec<&MemoryForLearning>> = HashMap::new();
        for memory in memories {
            if let Some(cat) = &memory.category {
                by_category.entry(cat.clone()).or_default().push(memory);
            }
        }

        for (category, cat_memories) in by_category {
            self.extract_patterns_from_category(category, &cat_memories);
        }
    }

    /// Get all discovered patterns.
    pub fn get_all_patterns(&self) -> Vec<UniversalPattern> {
        self.patterns
            .read()
            .map(|p| p.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get patterns by category.
    pub fn get_patterns_by_category(&self, category: &PatternCategory) -> Vec<UniversalPattern> {
        self.patterns
            .read()
            .map(|p| {
                p.values()
                    .filter(|pat| &pat.pattern.category == category)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for CrossProjectLearner {
    fn default() -> Self {
        Self::new()
    }
}

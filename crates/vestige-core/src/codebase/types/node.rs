//! `CodebaseNode` enum and dispatcher impls.

use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::architectural_decision::ArchitecturalDecision;
use super::bug_fix::BugFix;
use super::code_entity::CodeEntity;
use super::code_pattern::CodePattern;
use super::coding_preference::CodingPreference;
use super::file_relationship::FileRelationship;
use super::work_context::WorkContext;

/// Types of memories specific to codebases.
///
/// Each variant captures a different kind of knowledge that developers accumulate
/// but typically lose over time or when context-switching between projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodebaseNode {
    /// "We use X pattern because Y"
    ///
    /// Captures architectural decisions with their rationale. This is critical
    /// for maintaining consistency and understanding why the codebase evolved
    /// the way it did.
    ArchitecturalDecision(ArchitecturalDecision),

    /// "This bug was caused by X, fixed by Y"
    ///
    /// Records bug fixes with root cause analysis. Invaluable for preventing
    /// regression and understanding historical issues.
    BugFix(BugFix),

    /// "Use this pattern for X"
    ///
    /// Codifies recurring patterns with examples and guidance on when to use them.
    CodePattern(CodePattern),

    /// "These files always change together"
    ///
    /// Tracks file relationships discovered through git history analysis or
    /// explicit user teaching.
    FileRelationship(FileRelationship),

    /// "User prefers X over Y"
    ///
    /// Captures coding preferences and style decisions for consistent suggestions.
    CodingPreference(CodingPreference),

    /// "This function does X and is called by Y"
    ///
    /// Stores knowledge about specific code entities - functions, types, modules.
    CodeEntity(CodeEntity),

    /// "The current task is implementing X"
    ///
    /// Tracks ongoing work context for continuity across sessions.
    WorkContext(WorkContext),
}

impl CodebaseNode {
    /// Get the unique identifier for this node
    pub fn id(&self) -> &str {
        match self {
            Self::ArchitecturalDecision(n) => &n.id,
            Self::BugFix(n) => &n.id,
            Self::CodePattern(n) => &n.id,
            Self::FileRelationship(n) => &n.id,
            Self::CodingPreference(n) => &n.id,
            Self::CodeEntity(n) => &n.id,
            Self::WorkContext(n) => &n.id,
        }
    }

    /// Get the node type as a string
    pub fn node_type(&self) -> &'static str {
        match self {
            Self::ArchitecturalDecision(_) => "architectural_decision",
            Self::BugFix(_) => "bug_fix",
            Self::CodePattern(_) => "code_pattern",
            Self::FileRelationship(_) => "file_relationship",
            Self::CodingPreference(_) => "coding_preference",
            Self::CodeEntity(_) => "code_entity",
            Self::WorkContext(_) => "work_context",
        }
    }

    /// Get the creation timestamp
    pub fn created_at(&self) -> DateTime<Utc> {
        match self {
            Self::ArchitecturalDecision(n) => n.created_at,
            Self::BugFix(n) => n.created_at,
            Self::CodePattern(n) => n.created_at,
            Self::FileRelationship(n) => n.created_at,
            Self::CodingPreference(n) => n.created_at,
            Self::CodeEntity(n) => n.created_at,
            Self::WorkContext(n) => n.created_at,
        }
    }

    /// Get all file paths associated with this node
    pub fn associated_files(&self) -> Vec<&PathBuf> {
        match self {
            Self::ArchitecturalDecision(n) => n.files_affected.iter().collect(),
            Self::BugFix(n) => n.files_changed.iter().collect(),
            Self::CodePattern(n) => n.example_files.iter().collect(),
            Self::FileRelationship(n) => n.files.iter().collect(),
            Self::CodingPreference(_) => vec![],
            Self::CodeEntity(n) => n.file_path.as_ref().map(|p| vec![p]).unwrap_or_default(),
            Self::WorkContext(n) => n.active_files.iter().collect(),
        }
    }

    /// Convert to a searchable text representation
    pub fn to_searchable_text(&self) -> String {
        match self {
            Self::ArchitecturalDecision(n) => {
                format!(
                    "Architectural Decision: {} - Rationale: {} - Context: {}",
                    n.decision,
                    n.rationale,
                    n.context.as_deref().unwrap_or("")
                )
            }
            Self::BugFix(n) => {
                format!(
                    "Bug Fix: {} - Root Cause: {} - Solution: {}",
                    n.symptom, n.root_cause, n.solution
                )
            }
            Self::CodePattern(n) => {
                format!(
                    "Code Pattern: {} - {} - When to use: {}",
                    n.name, n.description, n.when_to_use
                )
            }
            Self::FileRelationship(n) => {
                format!(
                    "File Relationship: {:?} - Type: {:?} - {}",
                    n.files, n.relationship_type, n.description
                )
            }
            Self::CodingPreference(n) => {
                format!(
                    "Coding Preference ({}): {} vs {:?}",
                    n.context, n.preference, n.counter_preference
                )
            }
            Self::CodeEntity(n) => {
                format!(
                    "Code Entity: {} ({:?}) - {}",
                    n.name, n.entity_type, n.description
                )
            }
            Self::WorkContext(n) => {
                format!(
                    "Work Context: {} - {} - Active files: {:?}",
                    n.task_description,
                    n.status.as_str(),
                    n.active_files
                )
            }
        }
    }
}

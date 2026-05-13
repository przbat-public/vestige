//! Codebase-specific memory types for Vestige.
//!
//! Specialized node types that capture the contextual knowledge developers
//! accumulate but traditionally lose — architectural decisions, bug fixes,
//! coding patterns, and file relationships.
//!
//! ## Module Layout
//!
//! - `node` — `CodebaseNode` enum and dispatcher impl
//! - `architectural_decision` — `ArchitecturalDecision`, `DecisionStatus`
//! - `bug_fix` — `BugFix`, `BugSeverity`
//! - `code_pattern` — `CodePattern`
//! - `file_relationship` — `FileRelationship`, `RelationType`, `RelationshipSource`
//! - `coding_preference` — `CodingPreference`, `PreferenceSource`
//! - `code_entity` — `CodeEntity`, `EntityType`
//! - `work_context` — `WorkContext`, `WorkStatus`

mod architectural_decision;
mod bug_fix;
mod code_entity;
mod code_pattern;
mod coding_preference;
mod file_relationship;
mod node;
mod work_context;

#[cfg(test)]
mod tests;

pub use architectural_decision::{ArchitecturalDecision, DecisionStatus};
pub use bug_fix::{BugFix, BugSeverity};
pub use code_entity::{CodeEntity, EntityType};
pub use code_pattern::CodePattern;
pub use coding_preference::{CodingPreference, PreferenceSource};
pub use file_relationship::{FileRelationship, RelationType, RelationshipSource};
pub use node::CodebaseNode;
pub use work_context::{WorkContext, WorkStatus};

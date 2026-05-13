//! `FileRelationship`, `RelationType`, `RelationshipSource`.

use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Tracks relationships between files in the codebase.
///
/// Relationships can be:
/// - Discovered from imports/dependencies
/// - Detected from git co-change patterns
/// - Explicitly taught by the user
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRelationship {
    pub id: String,
    /// The files involved in this relationship
    pub files: Vec<PathBuf>,
    /// Type of relationship
    pub relationship_type: RelationType,
    /// Strength of the relationship (0.0 - 1.0)
    /// For co-change relationships, this is the frequency they change together
    pub strength: f64,
    /// Human-readable description
    pub description: String,
    /// When this relationship was first detected
    pub created_at: DateTime<Utc>,
    /// When this relationship was last confirmed
    pub last_confirmed: Option<DateTime<Utc>>,
    /// How this relationship was discovered
    pub source: RelationshipSource,
    /// Number of times this relationship has been observed
    pub observation_count: u32,
}

/// Types of relationships between files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// A imports/depends on B
    ImportsDependency,
    /// A tests implementation in B
    TestsImplementation,
    /// A configures service B
    ConfiguresService,
    /// Files are in the same domain/feature area
    SharedDomain,
    /// Files frequently change together in commits
    FrequentCochange,
    /// A extends/implements B
    ExtendsImplements,
    /// A is the interface, B is the implementation
    InterfaceImplementation,
    /// A and B are related through documentation
    DocumentationReference,
}

/// How a relationship was discovered
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipSource {
    /// Detected from git history co-change analysis
    GitCochange,
    /// Detected from import/dependency analysis
    ImportAnalysis,
    /// Detected from AST analysis
    AstAnalysis,
    /// Explicitly taught by user
    UserDefined,
    /// Inferred from file naming conventions
    NamingConvention,
}

impl FileRelationship {
    pub fn new(
        id: String,
        files: Vec<PathBuf>,
        relationship_type: RelationType,
        description: String,
    ) -> Self {
        Self {
            id,
            files,
            relationship_type,
            strength: 0.5,
            description,
            created_at: Utc::now(),
            last_confirmed: None,
            source: RelationshipSource::UserDefined,
            observation_count: 1,
        }
    }

    pub fn from_git_cochange(id: String, files: Vec<PathBuf>, strength: f64, count: u32) -> Self {
        Self {
            id,
            files: files.clone(),
            relationship_type: RelationType::FrequentCochange,
            strength,
            description: format!(
                "Files frequently change together ({} co-occurrences)",
                count
            ),
            created_at: Utc::now(),
            last_confirmed: Some(Utc::now()),
            source: RelationshipSource::GitCochange,
            observation_count: count,
        }
    }
}

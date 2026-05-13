//! `CodeEntity` and `EntityType`.

use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Knowledge about a specific code entity (function, type, module, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeEntity {
    pub id: String,
    /// Name of the entity
    pub name: String,
    /// Type of entity
    pub entity_type: EntityType,
    /// Description of what this entity does
    pub description: String,
    /// File where this entity is defined
    pub file_path: Option<PathBuf>,
    /// Line number where entity starts
    pub line_number: Option<u32>,
    /// Entities that this one depends on
    pub dependencies: Vec<String>,
    /// Entities that depend on this one
    pub dependents: Vec<String>,
    /// When this was recorded
    pub created_at: DateTime<Utc>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Usage notes or gotchas
    pub notes: Option<String>,
}

/// Type of code entity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Interface,
    Class,
    Module,
    Constant,
    Variable,
    Type,
}

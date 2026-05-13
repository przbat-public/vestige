//! `DetectedIntent` and its sibling enums (maintenance/learning/review/optimization).

use serde::{Deserialize, Serialize};

/// Detected intent from user actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectedIntent {
    /// User is debugging an issue
    Debugging {
        /// Suspected area of the bug
        suspected_area: String,
        /// Error messages or symptoms observed
        symptoms: Vec<String>,
    },

    /// User is refactoring code
    Refactoring {
        /// What is being refactored
        target: String,
        /// Goal of the refactoring
        goal: String,
    },

    /// User is building a new feature
    NewFeature {
        /// Description of the feature
        feature_description: String,
        /// Related existing components
        related_components: Vec<String>,
    },

    /// User is trying to learn/understand something
    Learning {
        /// Topic being learned
        topic: String,
        /// Current understanding level (estimated)
        level: LearningLevel,
    },

    /// User is doing maintenance work
    Maintenance {
        /// Type of maintenance
        maintenance_type: MaintenanceType,
        /// Target of maintenance
        target: Option<String>,
    },

    /// User is reviewing/understanding code
    CodeReview {
        /// Files being reviewed
        files: Vec<String>,
        /// Depth of review
        depth: ReviewDepth,
    },

    /// User is writing documentation
    Documentation {
        /// What is being documented
        subject: String,
    },

    /// User is optimizing performance
    Optimization {
        /// Target of optimization
        target: String,
        /// Type of optimization
        optimization_type: OptimizationType,
    },

    /// User is integrating with external systems
    Integration {
        /// System being integrated
        system: String,
    },

    /// Intent could not be determined
    Unknown,
}

impl DetectedIntent {
    /// Get a short description of the intent
    pub fn description(&self) -> String {
        match self {
            Self::Debugging { suspected_area, .. } => {
                format!("Debugging issue in {}", suspected_area)
            }
            Self::Refactoring { target, goal } => format!("Refactoring {} to {}", target, goal),
            Self::NewFeature {
                feature_description,
                ..
            } => format!("Building: {}", feature_description),
            Self::Learning { topic, .. } => format!("Learning about {}", topic),
            Self::Maintenance {
                maintenance_type, ..
            } => format!("{:?} maintenance", maintenance_type),
            Self::CodeReview { files, .. } => format!("Reviewing {} files", files.len()),
            Self::Documentation { subject } => format!("Documenting {}", subject),
            Self::Optimization { target, .. } => format!("Optimizing {}", target),
            Self::Integration { system } => format!("Integrating with {}", system),
            Self::Unknown => "Unknown intent".to_string(),
        }
    }

    /// Get relevant tags for memory search
    pub fn relevant_tags(&self) -> Vec<String> {
        match self {
            Self::Debugging { .. } => vec![
                "debugging".to_string(),
                "error".to_string(),
                "troubleshooting".to_string(),
                "fix".to_string(),
            ],
            Self::Refactoring { .. } => vec![
                "refactoring".to_string(),
                "architecture".to_string(),
                "patterns".to_string(),
                "clean-code".to_string(),
            ],
            Self::NewFeature { .. } => vec![
                "feature".to_string(),
                "implementation".to_string(),
                "design".to_string(),
            ],
            Self::Learning { topic, .. } => vec![
                "learning".to_string(),
                "tutorial".to_string(),
                topic.to_lowercase(),
            ],
            Self::Maintenance {
                maintenance_type, ..
            } => {
                let mut tags = vec!["maintenance".to_string()];
                match maintenance_type {
                    MaintenanceType::DependencyUpdate => tags.push("dependencies".to_string()),
                    MaintenanceType::SecurityPatch => tags.push("security".to_string()),
                    MaintenanceType::Cleanup => tags.push("cleanup".to_string()),
                    MaintenanceType::Configuration => tags.push("config".to_string()),
                    MaintenanceType::Migration => tags.push("migration".to_string()),
                }
                tags
            }
            Self::CodeReview { .. } => vec!["review".to_string(), "code-quality".to_string()],
            Self::Documentation { .. } => vec!["documentation".to_string(), "docs".to_string()],
            Self::Optimization {
                optimization_type, ..
            } => {
                let mut tags = vec!["optimization".to_string(), "performance".to_string()];
                match optimization_type {
                    OptimizationType::Speed => tags.push("speed".to_string()),
                    OptimizationType::Memory => tags.push("memory".to_string()),
                    OptimizationType::Size => tags.push("bundle-size".to_string()),
                    OptimizationType::Startup => tags.push("startup".to_string()),
                }
                tags
            }
            Self::Integration { system } => vec![
                "integration".to_string(),
                "api".to_string(),
                system.to_lowercase(),
            ],
            Self::Unknown => vec![],
        }
    }
}

/// Types of maintenance activities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaintenanceType {
    /// Updating dependencies
    DependencyUpdate,
    /// Applying security patches
    SecurityPatch,
    /// Code cleanup
    Cleanup,
    /// Configuration changes
    Configuration,
    /// Data/schema migration
    Migration,
}

/// Learning level estimation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LearningLevel {
    /// Just starting to learn
    Beginner,
    /// Has some understanding
    Intermediate,
    /// Deep dive into specifics
    Advanced,
}

/// Depth of code review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewDepth {
    /// Quick scan
    Shallow,
    /// Normal review
    Standard,
    /// Deep analysis
    Deep,
}

/// Type of optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizationType {
    /// Speed/latency optimization
    Speed,
    /// Memory usage optimization
    Memory,
    /// Bundle/binary size
    Size,
    /// Startup time
    Startup,
}

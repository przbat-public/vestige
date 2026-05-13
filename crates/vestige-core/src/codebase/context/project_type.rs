//! Detected language/project family for the working tree.

use serde::{Deserialize, Serialize};

/// Detected project type based on files present
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    Kotlin,
    Swift,
    CSharp,
    Cpp,
    Ruby,
    Php,
    Mixed(Vec<String>), // Multiple languages detected
    Unknown,
}

impl ProjectType {
    /// Get the file extensions associated with this project type
    pub fn extensions(&self) -> Vec<&'static str> {
        match self {
            Self::Rust => vec!["rs"],
            Self::TypeScript => vec!["ts", "tsx"],
            Self::JavaScript => vec!["js", "jsx"],
            Self::Python => vec!["py"],
            Self::Go => vec!["go"],
            Self::Java => vec!["java"],
            Self::Kotlin => vec!["kt", "kts"],
            Self::Swift => vec!["swift"],
            Self::CSharp => vec!["cs"],
            Self::Cpp => vec!["cpp", "cc", "cxx", "c", "h", "hpp"],
            Self::Ruby => vec!["rb"],
            Self::Php => vec!["php"],
            Self::Mixed(_) => vec![],
            Self::Unknown => vec![],
        }
    }

    /// Get the language name as a string
    pub fn language_name(&self) -> &str {
        match self {
            Self::Rust => "Rust",
            Self::TypeScript => "TypeScript",
            Self::JavaScript => "JavaScript",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::Swift => "Swift",
            Self::CSharp => "C#",
            Self::Cpp => "C++",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Mixed(_) => "Mixed",
            Self::Unknown => "Unknown",
        }
    }
}

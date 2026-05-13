//! `ContextCapture` — collects working/file context from the project tree.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::codebase::git::GitAnalyzer;

use super::errors::Result;
use super::file_context::FileContext;
use super::framework::Framework;
use super::project_type::ProjectType;
use super::working_context::{GitContextInfo, WorkingContext};

/// Captures and manages working context
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Fields are accessed by sibling submodules of the cohesive `context` component (split-by-responsibility refactor: framework, working_context, file_context, project_type); siblings have the same trust level as the parent module."
)]
pub struct ContextCapture {
    /// Git analyzer for the repository
    pub(super) git: Option<GitAnalyzer>,
    /// Currently active files
    pub(super) active_files: Vec<PathBuf>,
    /// Project root directory
    pub(super) project_root: PathBuf,
}

impl ContextCapture {
    /// Create a new context capture for a project directory
    pub fn new(project_root: PathBuf) -> Result<Self> {
        // Try to create git analyzer (may fail if not a git repo)
        let git = GitAnalyzer::new(project_root.clone()).ok();

        Ok(Self {
            git,
            active_files: vec![],
            project_root,
        })
    }

    /// Set the currently active file(s)
    pub fn set_active_files(&mut self, files: Vec<PathBuf>) {
        self.active_files = files;
    }

    /// Add an active file
    pub fn add_active_file(&mut self, file: PathBuf) {
        if !self.active_files.contains(&file) {
            self.active_files.push(file);
        }
    }

    /// Remove an active file
    pub fn remove_active_file(&mut self, file: &Path) {
        self.active_files.retain(|f| f != file);
    }

    /// Capture the full working context
    pub fn capture(&self) -> Result<WorkingContext> {
        let git = self
            .git
            .as_ref()
            .and_then(|g| g.get_current_context().ok().map(GitContextInfo::from));

        let project_type = self.detect_project_type()?;
        let frameworks = self.detect_frameworks()?;
        let project_name = self.detect_project_name()?;
        let config_files = self.find_config_files()?;

        Ok(WorkingContext {
            git,
            active_file: self.active_files.first().cloned(),
            project_type,
            frameworks,
            project_name,
            project_root: self.project_root.clone(),
            captured_at: Utc::now(),
            recent_files: self.active_files.clone(),
            config_files,
        })
    }

    /// Get context specific to a file
    pub fn context_for_file(&self, path: &Path) -> Result<FileContext> {
        let extension = path.extension().map(|e| e.to_string_lossy().to_string());

        let language = extension
            .as_ref()
            .and_then(|ext| match ext.as_str() {
                "rs" => Some("rust"),
                "ts" | "tsx" => Some("typescript"),
                "js" | "jsx" => Some("javascript"),
                "py" => Some("python"),
                "go" => Some("go"),
                "java" => Some("java"),
                "kt" | "kts" => Some("kotlin"),
                "swift" => Some("swift"),
                "cs" => Some("csharp"),
                "cpp" | "cc" | "cxx" | "c" => Some("cpp"),
                "h" | "hpp" => Some("cpp"),
                "rb" => Some("ruby"),
                "php" => Some("php"),
                "sql" => Some("sql"),
                "json" => Some("json"),
                "yaml" | "yml" => Some("yaml"),
                "toml" => Some("toml"),
                "md" => Some("markdown"),
                _ => None,
            })
            .map(|s| s.to_string());

        let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();

        // Detect related files
        let related_files = self.find_related_files(path)?;

        // Check git status
        let has_changes = self
            .git
            .as_ref()
            .map(|g| {
                g.get_current_context()
                    .ok()
                    .map(|ctx| {
                        ctx.uncommitted_changes.contains(&path.to_path_buf())
                            || ctx.staged_changes.contains(&path.to_path_buf())
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        // Check if test file
        let is_test_file = self.is_test_file(path);

        // Get last modified time
        let last_modified = fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok().map(DateTime::<Utc>::from));

        // Detect module
        let module = self.detect_module(path);

        Ok(FileContext {
            path: path.to_path_buf(),
            language,
            extension,
            directory,
            related_files,
            has_changes,
            last_modified,
            is_test_file,
            module,
        })
    }

    /// Detect the project type based on files present
    pub(super) fn detect_project_type(&self) -> Result<ProjectType> {
        let mut detected = Vec::new();

        // Check for Rust
        if self.file_exists("Cargo.toml") {
            detected.push("Rust".to_string());
        }

        // Check for JavaScript/TypeScript
        if self.file_exists("package.json") {
            // Check for TypeScript
            if self.file_exists("tsconfig.json") || self.file_exists("tsconfig.base.json") {
                detected.push("TypeScript".to_string());
            } else {
                detected.push("JavaScript".to_string());
            }
        }

        // Check for Python
        if self.file_exists("pyproject.toml")
            || self.file_exists("setup.py")
            || self.file_exists("requirements.txt")
        {
            detected.push("Python".to_string());
        }

        // Check for Go
        if self.file_exists("go.mod") {
            detected.push("Go".to_string());
        }

        // Check for Java/Kotlin
        if self.file_exists("pom.xml") || self.file_exists("build.gradle") {
            if self.dir_exists("src/main/kotlin") || self.file_exists("build.gradle.kts") {
                detected.push("Kotlin".to_string());
            } else {
                detected.push("Java".to_string());
            }
        }

        // Check for Swift
        if self.file_exists("Package.swift") {
            detected.push("Swift".to_string());
        }

        // Check for C#
        if self.glob_exists("*.csproj") || self.glob_exists("*.sln") {
            detected.push("CSharp".to_string());
        }

        // Check for Ruby
        if self.file_exists("Gemfile") {
            detected.push("Ruby".to_string());
        }

        // Check for PHP
        if self.file_exists("composer.json") {
            detected.push("PHP".to_string());
        }

        match detected.len() {
            0 => Ok(ProjectType::Unknown),
            1 => Ok(match detected[0].as_str() {
                "Rust" => ProjectType::Rust,
                "TypeScript" => ProjectType::TypeScript,
                "JavaScript" => ProjectType::JavaScript,
                "Python" => ProjectType::Python,
                "Go" => ProjectType::Go,
                "Java" => ProjectType::Java,
                "Kotlin" => ProjectType::Kotlin,
                "Swift" => ProjectType::Swift,
                "CSharp" => ProjectType::CSharp,
                "Ruby" => ProjectType::Ruby,
                "PHP" => ProjectType::Php,
                _ => ProjectType::Unknown,
            }),
            _ => Ok(ProjectType::Mixed(detected)),
        }
    }

    /// Detect frameworks used in the project
    pub(super) fn detect_frameworks(&self) -> Result<Vec<Framework>> {
        let mut frameworks = Vec::new();

        // Rust frameworks
        if let Ok(content) = fs::read_to_string(self.project_root.join("Cargo.toml")) {
            if content.contains("tauri") {
                frameworks.push(Framework::Tauri);
            }
            if content.contains("actix-web") {
                frameworks.push(Framework::Actix);
            }
            if content.contains("axum") {
                frameworks.push(Framework::Axum);
            }
            if content.contains("rocket") {
                frameworks.push(Framework::Rocket);
            }
            if content.contains("tokio") {
                frameworks.push(Framework::Tokio);
            }
            if content.contains("diesel") {
                frameworks.push(Framework::Diesel);
            }
            if content.contains("sea-orm") {
                frameworks.push(Framework::SeaOrm);
            }
        }

        // JavaScript/TypeScript frameworks
        if let Ok(content) = fs::read_to_string(self.project_root.join("package.json")) {
            if content.contains("\"react\"") || content.contains("\"react\":") {
                frameworks.push(Framework::React);
            }
            if content.contains("\"vue\"") || content.contains("\"vue\":") {
                frameworks.push(Framework::Vue);
            }
            if content.contains("\"@angular/") {
                frameworks.push(Framework::Angular);
            }
            if content.contains("\"svelte\"") {
                frameworks.push(Framework::Svelte);
            }
            if content.contains("\"next\"") || content.contains("\"next\":") {
                frameworks.push(Framework::NextJs);
            }
            if content.contains("\"nuxt\"") || content.contains("\"nuxt\":") {
                frameworks.push(Framework::NuxtJs);
            }
            if content.contains("\"express\"") {
                frameworks.push(Framework::Express);
            }
            if content.contains("\"@nestjs/") {
                frameworks.push(Framework::NestJs);
            }
        }

        // Deno
        if self.file_exists("deno.json") || self.file_exists("deno.jsonc") {
            frameworks.push(Framework::Deno);
        }

        // Bun
        if self.file_exists("bun.lockb") || self.file_exists("bunfig.toml") {
            frameworks.push(Framework::Bun);
        }

        // Python frameworks
        if let Ok(content) = fs::read_to_string(self.project_root.join("pyproject.toml")) {
            if content.contains("django") {
                frameworks.push(Framework::Django);
            }
            if content.contains("flask") {
                frameworks.push(Framework::Flask);
            }
            if content.contains("fastapi") {
                frameworks.push(Framework::FastApi);
            }
            if content.contains("pytest") {
                frameworks.push(Framework::Pytest);
            }
            if content.contains("[tool.poetry]") {
                frameworks.push(Framework::Poetry);
            }
        }

        // Check requirements.txt too
        if let Ok(content) = fs::read_to_string(self.project_root.join("requirements.txt")) {
            if content.contains("django") && !frameworks.contains(&Framework::Django) {
                frameworks.push(Framework::Django);
            }
            if content.contains("flask") && !frameworks.contains(&Framework::Flask) {
                frameworks.push(Framework::Flask);
            }
            if content.contains("fastapi") && !frameworks.contains(&Framework::FastApi) {
                frameworks.push(Framework::FastApi);
            }
        }

        // Java Spring
        if let Ok(content) = fs::read_to_string(self.project_root.join("pom.xml"))
            && content.contains("spring")
        {
            frameworks.push(Framework::Spring);
        }

        // Ruby Rails
        if self.file_exists("config/routes.rb") {
            frameworks.push(Framework::Rails);
        }

        // PHP Laravel
        if self.file_exists("artisan") && self.dir_exists("app/Http") {
            frameworks.push(Framework::Laravel);
        }

        // .NET
        if self.glob_exists("*.csproj") {
            frameworks.push(Framework::DotNet);
        }

        Ok(frameworks)
    }

    /// Detect the project name from config files
    pub(super) fn detect_project_name(&self) -> Result<Option<String>> {
        // Try Cargo.toml
        if let Ok(content) = fs::read_to_string(self.project_root.join("Cargo.toml"))
            && let Some(name) = self.extract_toml_value(&content, "name")
        {
            return Ok(Some(name));
        }

        // Try package.json
        if let Ok(content) = fs::read_to_string(self.project_root.join("package.json"))
            && let Some(name) = self.extract_json_value(&content, "name")
        {
            return Ok(Some(name));
        }

        // Try pyproject.toml
        if let Ok(content) = fs::read_to_string(self.project_root.join("pyproject.toml"))
            && let Some(name) = self.extract_toml_value(&content, "name")
        {
            return Ok(Some(name));
        }

        // Try go.mod
        if let Ok(content) = fs::read_to_string(self.project_root.join("go.mod"))
            && let Some(line) = content.lines().next()
            && line.starts_with("module ")
        {
            let name = line
                .trim_start_matches("module ")
                .split('/')
                .next_back()
                .unwrap_or("")
                .to_string();
            if !name.is_empty() {
                return Ok(Some(name));
            }
        }

        // Fall back to directory name
        Ok(self
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string()))
    }

    /// Find configuration files in the project
    fn find_config_files(&self) -> Result<Vec<PathBuf>> {
        let config_names = [
            "Cargo.toml",
            "package.json",
            "tsconfig.json",
            "pyproject.toml",
            "go.mod",
            ".gitignore",
            ".env",
            ".env.local",
            "docker-compose.yml",
            "docker-compose.yaml",
            "Dockerfile",
            "Makefile",
            "justfile",
            ".editorconfig",
            ".prettierrc",
            ".eslintrc.json",
            "rustfmt.toml",
            ".rustfmt.toml",
            "clippy.toml",
            ".clippy.toml",
            "tauri.conf.json",
        ];

        let mut found = Vec::new();

        for name in config_names {
            let path = self.project_root.join(name);
            if path.exists() {
                found.push(path);
            }
        }

        Ok(found)
    }

    /// Find files related to a given file
    fn find_related_files(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut related = Vec::new();

        let file_stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
        let extension = path.extension().map(|s| s.to_string_lossy().to_string());
        let parent = path.parent();

        if let (Some(stem), Some(parent)) = (file_stem, parent) {
            // Look for test files
            let test_patterns = [
                format!("{}.test", stem),
                format!("{}_test", stem),
                format!("{}.spec", stem),
                format!("test_{}", stem),
            ];

            // Common test directories
            let test_dirs = ["tests", "test", "__tests__", "spec"];

            // Check same directory for test files
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let entry_path = entry.path();
                    if let Some(entry_stem) = entry_path.file_stem() {
                        let entry_stem = entry_stem.to_string_lossy();
                        for pattern in &test_patterns {
                            if entry_stem.eq_ignore_ascii_case(pattern) {
                                related.push(entry_path.clone());
                                break;
                            }
                        }
                    }
                }
            }

            // Check test directories
            for test_dir in test_dirs {
                let test_path = self.project_root.join(test_dir);
                if test_path.exists()
                    && let Ok(entries) = fs::read_dir(&test_path)
                {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let entry_path = entry.path();
                        if let Some(entry_stem) = entry_path.file_stem() {
                            let entry_stem = entry_stem.to_string_lossy();
                            if entry_stem.contains(&stem) {
                                related.push(entry_path);
                            }
                        }
                    }
                }
            }

            // For Rust, look for mod.rs in same directory
            if extension.as_deref() == Some("rs") {
                let mod_path = parent.join("mod.rs");
                if mod_path.exists() && mod_path != path {
                    related.push(mod_path);
                }

                // Look for lib.rs or main.rs at project root
                let lib_path = self.project_root.join("src/lib.rs");
                let main_path = self.project_root.join("src/main.rs");

                if lib_path.exists() && lib_path != path {
                    related.push(lib_path);
                }
                if main_path.exists() && main_path != path {
                    related.push(main_path);
                }
            }
        }

        // Remove duplicates
        let related: HashSet<_> = related.into_iter().collect();
        Ok(related.into_iter().collect())
    }

    /// Check if a file is a test file
    pub(super) fn is_test_file(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();

        path_str.contains("test")
            || path_str.contains("spec")
            || path_str.contains("__tests__")
            || path
                .file_name()
                .map(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with("test_")
                        || n.ends_with("_test.rs")
                        || n.ends_with(".test.ts")
                        || n.ends_with(".test.tsx")
                        || n.ends_with(".test.js")
                        || n.ends_with(".spec.ts")
                        || n.ends_with(".spec.js")
                })
                .unwrap_or(false)
    }

    /// Detect the module a file belongs to
    fn detect_module(&self, path: &Path) -> Option<String> {
        // For Rust, use the parent directory name relative to src/
        if path.extension().map(|e| e == "rs").unwrap_or(false)
            && let Ok(relative) = path.strip_prefix(&self.project_root)
            && let Ok(src_relative) = relative.strip_prefix("src")
        {
            // Get the module path
            let components: Vec<_> = src_relative
                .parent()?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();

            if !components.is_empty() {
                return Some(components.join("::"));
            }
        }

        // For TypeScript/JavaScript, use the parent directory
        if path
            .extension()
            .map(|e| e == "ts" || e == "tsx" || e == "js" || e == "jsx")
            .unwrap_or(false)
            && let Ok(relative) = path.strip_prefix(&self.project_root)
        {
            // Skip src/ or lib/ prefix
            let relative = relative
                .strip_prefix("src")
                .or_else(|_| relative.strip_prefix("lib"))
                .unwrap_or(relative);

            if let Some(parent) = relative.parent() {
                let module = parent.to_string_lossy().replace('/', ".");
                if !module.is_empty() {
                    return Some(module);
                }
            }
        }

        None
    }

    /// Check if a file exists relative to project root
    fn file_exists(&self, name: &str) -> bool {
        self.project_root.join(name).exists()
    }

    /// Check if a directory exists relative to project root
    fn dir_exists(&self, name: &str) -> bool {
        let path = self.project_root.join(name);
        path.exists() && path.is_dir()
    }

    /// Check if any file matching a glob pattern exists
    fn glob_exists(&self, pattern: &str) -> bool {
        if let Ok(entries) = fs::read_dir(&self.project_root) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Some(name) = entry.file_name().to_str() {
                    // Simple glob matching for patterns like "*.ext"
                    if pattern.starts_with("*.") {
                        let ext = &pattern[1..];
                        if name.ends_with(ext) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Simple TOML value extraction (basic, no full parser)
    fn extract_toml_value(&self, content: &str, key: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            if (trimmed.starts_with(&format!("{} ", key))
                || trimmed.starts_with(&format!("{}=", key)))
                && let Some(value) = trimmed.split('=').nth(1)
            {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                return Some(value.to_string());
            }
        }
        None
    }

    /// Simple JSON value extraction (basic, no full parser)
    fn extract_json_value(&self, content: &str, key: &str) -> Option<String> {
        let pattern = format!("\"{}\"", key);
        for line in content.lines() {
            if line.contains(&pattern) {
                // Try to extract the value after the colon
                if let Some(colon_pos) = line.find(':') {
                    let value = line[colon_pos + 1..].trim();
                    let value = value.trim_start_matches('"');
                    if let Some(end) = value.find('"') {
                        return Some(value[..end].to_string());
                    }
                }
            }
        }
        None
    }
}

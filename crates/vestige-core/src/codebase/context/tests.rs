//! Tests for the context-capture pipeline.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::capture::ContextCapture;
use super::framework::Framework;
use super::project_type::ProjectType;

fn create_test_project() -> TempDir {
    let dir = TempDir::new().unwrap();

    // Create Cargo.toml
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
tokio = "1.0"
axum = "0.7"
"#,
    )
    .unwrap();

    // Create src directory
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    dir
}

#[test]
fn test_detect_project_type() {
    let dir = create_test_project();
    let capture = ContextCapture::new(dir.path().to_path_buf()).unwrap();

    let project_type = capture.detect_project_type().unwrap();
    assert_eq!(project_type, ProjectType::Rust);
}

#[test]
fn test_detect_frameworks() {
    let dir = create_test_project();
    let capture = ContextCapture::new(dir.path().to_path_buf()).unwrap();

    let frameworks = capture.detect_frameworks().unwrap();
    assert!(frameworks.contains(&Framework::Tokio));
    assert!(frameworks.contains(&Framework::Axum));
}

#[test]
fn test_detect_project_name() {
    let dir = create_test_project();
    let capture = ContextCapture::new(dir.path().to_path_buf()).unwrap();

    let name = capture.detect_project_name().unwrap();
    assert_eq!(name, Some("test-project".to_string()));
}

#[test]
fn test_is_test_file() {
    let capture = ContextCapture {
        git: None,
        active_files: vec![],
        project_root: PathBuf::from("."),
    };

    assert!(capture.is_test_file(Path::new("src/utils_test.rs")));
    assert!(capture.is_test_file(Path::new("tests/integration.rs")));
    assert!(capture.is_test_file(Path::new("src/utils.test.ts")));
    assert!(!capture.is_test_file(Path::new("src/utils.rs")));
    assert!(!capture.is_test_file(Path::new("src/main.ts")));
}

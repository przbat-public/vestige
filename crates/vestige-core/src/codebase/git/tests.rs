//! Tests for the git-analysis pipeline.

use std::path::{Path, PathBuf};

use git2::Repository;
use tempfile::TempDir;

use super::analyzer::GitAnalyzer;

fn create_test_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();

    // Configure signature
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();

    // Create initial commit
    {
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();
    }

    (dir, repo)
}

#[test]
fn test_git_analyzer_creation() {
    let (dir, _repo) = create_test_repo();
    let analyzer = GitAnalyzer::new(dir.path().to_path_buf());
    assert!(analyzer.is_ok());
}

#[test]
fn test_get_current_context() {
    let (dir, _repo) = create_test_repo();
    let analyzer = GitAnalyzer::new(dir.path().to_path_buf()).unwrap();

    let context = analyzer.get_current_context().unwrap();
    assert!(context.has_commits);
    assert!(!context.head_commit.is_empty());
}

#[test]
fn test_is_relevant_file() {
    let analyzer = GitAnalyzer {
        repo_path: PathBuf::from("."),
    };

    assert!(analyzer.is_relevant_file(Path::new("src/main.rs")));
    assert!(analyzer.is_relevant_file(Path::new("lib/utils.ts")));
    assert!(!analyzer.is_relevant_file(Path::new("Cargo.lock")));
    assert!(!analyzer.is_relevant_file(Path::new("node_modules/foo.js")));
    assert!(!analyzer.is_relevant_file(Path::new("target/debug/main")));
}

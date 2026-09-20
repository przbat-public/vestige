//! Storage-level tests for code anchors: the write, the batched read, the
//! report-only audit, and the deletion paths.
//!
//! Erasure gets its own case here on purpose. An anchor is a record of *which
//! files a subject's memories touched*, so a "right to erasure" that removes the
//! text and leaves the anchors has removed the sentence and kept the map — which
//! is why `code_refs` is named in `gdpr.rs` and why that is checked against the
//! table rather than against a query that might filter.

use std::path::{Path, PathBuf};

use git2::{Repository, Signature};
use tempfile::tempdir;

use crate::code_refs::{AnchorVerdict, CodeAnchor, IngestAnchor};
use crate::memory::IngestInput;

use super::Storage;

fn test_storage() -> Storage {
    let dir = tempdir().unwrap();
    Storage::new(Some(dir.path().join("anchors.db"))).unwrap()
}

fn ingest_with_anchors(storage: &Storage, content: &str, anchors: Vec<IngestAnchor>) -> String {
    storage
        .ingest(IngestInput {
            content: content.to_string(),
            anchors,
            ..Default::default()
        })
        .unwrap()
        .id
}

fn unchecked(anchor: CodeAnchor) -> IngestAnchor {
    IngestAnchor::unchecked(anchor)
}

#[test]
fn anchors_are_stored_with_the_memory_they_belong_to() {
    let storage = test_storage();
    let mut anchor = CodeAnchor::new("src/lib.rs");
    anchor.commit_sha = Some("1cfdf45aa".to_string());
    anchor.symbol = Some("Storage::new".to_string());
    anchor.hint_line = Some(112);

    let node_id = ingest_with_anchors(
        &storage,
        "the merge dropped a sub-query",
        vec![unchecked(anchor)],
    );

    let stored = storage.code_refs_for(&node_id).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].node_id, node_id);
    assert_eq!(stored[0].anchor.path, "src/lib.rs");
    assert_eq!(stored[0].anchor.symbol.as_deref(), Some("Storage::new"));
    assert_eq!(stored[0].anchor.hint_line, Some(112));
    assert_eq!(stored[0].verdict, AnchorVerdict::Unchecked);
    assert_eq!(
        stored[0].resolved_at, None,
        "a caller that did not check the anchor must not look like one that did"
    );
}

#[test]
fn a_memory_with_no_anchors_stores_none() {
    let storage = test_storage();
    let node_id = ingest_with_anchors(&storage, "no code here", vec![]);
    assert!(storage.code_refs_for(&node_id).unwrap().is_empty());
    assert_eq!(storage.code_ref_count().unwrap(), 0);
}

#[test]
fn anchors_for_a_page_of_results_come_back_in_one_lookup() {
    let storage = test_storage();
    let first = ingest_with_anchors(
        &storage,
        "first",
        vec![unchecked(CodeAnchor::new("src/a.rs"))],
    );
    let second = ingest_with_anchors(
        &storage,
        "second",
        vec![unchecked(CodeAnchor::new("src/b.rs"))],
    );
    let third = ingest_with_anchors(&storage, "third", vec![]);

    let page = storage
        .code_refs_for_nodes(&[first.clone(), second.clone(), third.clone()])
        .unwrap();

    assert_eq!(page.len(), 2, "a node with no anchors is absent, not empty");
    assert_eq!(page[&first][0].anchor.path, "src/a.rs");
    assert_eq!(page[&second][0].anchor.path, "src/b.rs");
    assert!(!page.contains_key(&third));
}

/// Erasure has to remove the anchors: they name the files the subject's memories
/// touched, so leaving them behind is a record of the subject's work.
#[test]
fn erasure_removes_the_anchors_along_with_the_memory() {
    let storage = test_storage();
    let node_id = ingest_with_anchors(
        &storage,
        "gdańsk prefers the migration to run before the deploy",
        vec![unchecked(CodeAnchor::new("src/private/path.rs"))],
    );
    assert_eq!(
        storage
            .count_code_refs_with_path("src/private/path.rs")
            .unwrap(),
        1
    );

    storage.right_to_erasure(&node_id).unwrap();

    assert_eq!(
        storage.code_ref_count().unwrap(),
        0,
        "an anchor outlived the memory it belonged to"
    );
    assert!(
        storage.get_node(&node_id).unwrap().is_none(),
        "the memory itself must be gone too"
    );
}

#[test]
fn erasure_by_tag_removes_the_anchors_too() {
    let storage = test_storage();
    let node_id = storage
        .ingest(IngestInput {
            content: "tagged for erasure".to_string(),
            tags: vec!["gdpr-subject".to_string()],
            anchors: vec![unchecked(CodeAnchor::new("src/tagged.rs"))],
            ..Default::default()
        })
        .unwrap()
        .id;

    let (memories, _) = storage.erase_by_tag("gdpr-subject").unwrap();

    assert_eq!(memories, 1);
    assert_eq!(storage.code_ref_count().unwrap(), 0);
    assert!(storage.get_node(&node_id).unwrap().is_none());
}

#[test]
fn deleting_a_node_removes_its_anchors() {
    let storage = test_storage();
    let node_id = ingest_with_anchors(
        &storage,
        "about to be deleted",
        vec![unchecked(CodeAnchor::new("src/deleted.rs"))],
    );

    assert!(storage.delete_node(&node_id).unwrap());

    assert_eq!(storage.code_ref_count().unwrap(), 0);
}

// ============================================================================
// The audit
// ============================================================================

/// A repository to audit against, built per test so nothing here depends on
/// this checkout's history.
struct TestRepo {
    /// Owns the temporary directory; dropping it would delete the repository
    /// from under a test that still holds its path.
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl TestRepo {
    fn new(source: &str) -> Self {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();
        Repository::init(&path).unwrap();
        let repo = Self { _dir: dir, path };
        repo.commit(source, "initial");
        repo
    }

    fn repository(&self) -> Repository {
        Repository::open(&self.path).unwrap()
    }

    fn commit(&self, source: &str, message: &str) -> String {
        let repo = self.repository();
        let file = self.path.join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, source).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("src/lib.rs")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("vestige test", "test@example.invalid").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
            .to_string()
    }

    /// Store an anchor for a symbol at the current revision, checked the way the
    /// write path checks it.
    fn store(&self, storage: &Storage, symbol: &str) -> (String, i64) {
        let mut anchor = CodeAnchor::new("src/lib.rs");
        anchor.symbol = Some(symbol.to_string());
        anchor.repo_remote = Some(self.path.to_string_lossy().into_owned());
        let roots = vec![self.path.clone()];
        let prepared = crate::code_refs::resolve_for_write(anchor, &roots);
        assert_eq!(
            prepared.verdict,
            AnchorVerdict::Fresh,
            "the fixture must start from a verifiable anchor"
        );

        let node_id = ingest_with_anchors(storage, "a memory about code", vec![prepared]);
        let stored = storage.code_refs_for(&node_id).unwrap();
        (node_id, stored[0].id)
    }
}

#[test]
fn the_audit_reports_rot_and_records_the_verdict_without_repairing_anything() {
    let repo = TestRepo::new("pub fn a() -> u32 {\n    1\n}\n");
    let storage = test_storage();
    let (node_id, id) = repo.store(&storage, "a");
    let before = storage.code_refs_for(&node_id).unwrap().remove(0);
    assert_eq!(before.verdict, AnchorVerdict::Fresh, "{}", before.id);

    // The code moves on: the symbol still exists, its body changed.
    repo.commit("pub fn a() -> u32 {\n    2\n}\n", "change a");

    let audit = storage.audit_code_anchors(100).unwrap();

    assert_eq!(audit.checked, 1);
    assert_eq!(audit.stale, 1, "{audit:?}");
    assert_eq!(audit.needing_review(), 1);

    let after = storage.code_refs_for(&node_id).unwrap().remove(0);
    assert_eq!(after.id, id);
    assert_eq!(after.verdict, AnchorVerdict::Stale);
    assert!(after.resolved_at.is_some(), "the check must be timestamped");
    assert_eq!(
        after.anchor.commit_sha, before.anchor.commit_sha,
        "the audit must not rewrite the revision"
    );
    assert_eq!(
        after.anchor.symbol, before.anchor.symbol,
        "the audit must not rewrite the symbol"
    );
    assert_eq!(
        after.anchor.content_hash, before.anchor.content_hash,
        "adopting the new text would report `fresh` on the next pass, which is \
         the silent repair this feature refuses to make"
    );
}

#[test]
fn a_second_audit_does_not_turn_a_stale_anchor_fresh() {
    let repo = TestRepo::new("pub fn a() -> u32 {\n    1\n}\n");
    let storage = test_storage();
    let (node_id, _) = repo.store(&storage, "a");

    repo.commit("pub fn a() -> u32 {\n    99\n}\n", "change a");

    let first = storage.audit_code_anchors(100).unwrap();
    let second = storage.audit_code_anchors(100).unwrap();

    assert_eq!(first.stale, 1);
    assert_eq!(
        second.stale, 1,
        "rot that heals itself on the second look is rot that is never reported"
    );
    assert_eq!(
        storage.code_refs_for(&node_id).unwrap()[0].verdict,
        AnchorVerdict::Stale
    );
}

#[test]
fn the_audit_no_longer_calls_an_anchor_fresh_after_its_file_is_deleted() {
    let repo = TestRepo::new("pub fn a() -> u32 {\n    1\n}\n");
    let storage = test_storage();
    let (node_id, _) = repo.store(&storage, "a");

    // Delete the file at a new revision, index and working tree together.
    let repo_handle = repo.repository();
    let mut index = repo_handle.index().unwrap();
    index.remove_path(Path::new("src/lib.rs")).unwrap();
    index.write().unwrap();
    let tree = repo_handle.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("vestige test", "test@example.invalid").unwrap();
    let parent = repo_handle
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo_handle
        .commit(Some("HEAD"), &sig, &sig, "delete lib.rs", &tree, &parents)
        .unwrap();
    std::fs::remove_file(repo.path.join("src/lib.rs")).unwrap();

    let audit = storage.audit_code_anchors(100).unwrap();
    assert_eq!(audit.orphaned, 1, "{audit:?}");
    assert_eq!(
        storage.code_refs_for(&node_id).unwrap()[0].verdict,
        AnchorVerdict::Orphaned
    );
}

#[test]
fn the_audit_never_checks_more_than_its_batch() {
    let storage = test_storage();
    for i in 0..5 {
        ingest_with_anchors(
            &storage,
            &format!("memory {i}"),
            vec![unchecked(CodeAnchor::new(format!("src/{i}.rs")))],
        );
    }

    let audit = storage.audit_code_anchors(2).unwrap();

    assert_eq!(audit.checked, 2, "the batch bound must hold");
}

/// An update that absorbs new text keeps the citations that came with it.
///
/// The update path rewrites the node's content instead of creating a row, so
/// without this the anchors a caller passed would be dropped on the floor —
/// silently, which is the failure the whole table exists to end.
#[test]
fn anchors_ride_along_with_an_update_to_an_existing_memory() {
    let storage = test_storage();
    let node_id = ingest_with_anchors(&storage, "the first wording", vec![]);
    assert_eq!(storage.code_ref_count().unwrap(), 0);

    storage
        .attach_code_anchors(&node_id, &[unchecked(CodeAnchor::new("src/updated.rs"))])
        .unwrap();

    let stored = storage.code_refs_for(&node_id).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].anchor.path, "src/updated.rs");
    assert_eq!(stored[0].node_id, node_id);

    // Attaching to a node with no anchors is a no-op, not an error.
    assert_eq!(storage.attach_code_anchors(&node_id, &[]).unwrap(), 0);
}

/// The rot audit runs inside the existing consolidation cycle and its counts
/// come back with the cycle's result.
///
/// Report-only is the point: the cycle re-resolves and records, and the numbers
/// are what an operator acts on. A cycle that silently absorbed the rot would be
/// indistinguishable from one that found nothing.
#[test]
fn consolidation_runs_the_audit_and_reports_its_counts() {
    let repo = TestRepo::new("pub fn a() -> u32 {\n    1\n}\n");
    let storage = test_storage();
    let (node_id, _) = repo.store(&storage, "a");

    repo.commit("pub fn a() -> u32 {\n    7\n}\n", "change a");

    let result = storage.run_consolidation().unwrap();

    assert_eq!(
        result.code_anchor_audit.checked, 1,
        "{:?}",
        result.code_anchor_audit
    );
    assert_eq!(
        result.code_anchor_audit.stale, 1,
        "{:?}",
        result.code_anchor_audit
    );
    assert_eq!(result.code_anchor_audit.needing_review(), 1);
    assert_eq!(
        storage.code_refs_for(&node_id).unwrap()[0].verdict,
        AnchorVerdict::Stale
    );
}

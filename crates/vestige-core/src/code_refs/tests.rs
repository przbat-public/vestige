//! Tests for the code-anchor scanner, the symbol resolver and the verdicts.
//!
//! The moved / changed / deleted cases each build a **throwaway repository**
//! rather than reading this one: the verdicts are about what a revision says,
//! and a repository whose history changes under the test would turn a pinned
//! expectation into a flake. Commits are made through `git2` directly, so the
//! tests never shell out and never write to the checkout under test.

use std::path::{Path, PathBuf};

use git2::{Repository, Signature};
use tempfile::TempDir;

use super::*;

/// A throwaway repository with a known history.
///
/// `_dir` owns the temporary directory: dropping it would delete the repository
/// out from under a test that still holds its path.
struct TestRepo {
    _dir: TempDir,
    path: PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().to_path_buf();
        Repository::init(&path).expect("init repository");
        let repo = Self { _dir: dir, path };
        repo.commit(&[("src/lib.rs", "pub fn a() {}\n")], "initial");
        repo
    }

    fn repo(&self) -> Repository {
        Repository::open(&self.path).expect("open repository")
    }

    /// Replace the tracked files with `files` and commit.
    fn commit(&self, files: &[(&str, &str)], message: &str) -> String {
        let repo = self.repo();
        let mut index = repo.index().expect("index");
        for (path, body) in files {
            let full = self.path.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("create dirs");
            }
            std::fs::write(&full, body).expect("write file");
            index.add_path(Path::new(path)).expect("add path");
        }
        // Files removed between commits have to leave the index too, or the
        // "deleted" case would still resolve.
        let mut to_remove = Vec::new();
        {
            let entries = index.iter();
            let tracked: Vec<String> = entries
                .filter_map(|entry| String::from_utf8(entry.path).ok())
                .collect();
            for tracked_path in tracked {
                if !files.iter().any(|(path, _)| *path == tracked_path) {
                    to_remove.push(tracked_path);
                }
            }
        }
        for path in to_remove {
            index.remove_path(Path::new(&path)).expect("remove path");
            let _ = std::fs::remove_file(self.path.join(&path));
        }
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = Signature::now("vestige test", "test@example.invalid").expect("signature");
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .expect("commit")
            .to_string()
    }

    /// A `CodeAnchor` rooted at this repository, with an observed revision.
    fn anchor(&self, path: &str, symbol: &str) -> CodeAnchor {
        let mut anchor = CodeAnchor::new(path);
        anchor.symbol = Some(symbol.to_string());
        anchor.repo_remote = Some(self.path.to_string_lossy().into_owned());
        anchor
    }

    fn roots(&self) -> Vec<PathBuf> {
        vec![self.path.clone()]
    }
}

/// A body that a human would call "the same function", with the whitespace
/// reflowed.
const A_BODY: &str = "pub fn a() -> u32 {\n    1\n}\n";

#[test]
fn a_symbol_that_moved_within_the_file_stays_fresh() {
    let repo = TestRepo::new();
    let mut anchor = repo.anchor("src/lib.rs", "a");
    anchor.commit_sha = Some(repo.commit(
        &[(
            "src/lib.rs",
            "pub fn a() -> u32 {\n    1\n}\n\npub fn b() {}\n",
        )],
        "add a and b",
    ));
    let written = anchor.resolve_with(&repo.roots());
    assert_eq!(written.verdict, AnchorVerdict::Fresh, "{}", written.note);
    anchor.content_hash = written.content_hash;

    // The symbol is pushed down by three comment lines and nothing else about it
    // changes. A file hash would report `stale` here; hashing the body is why
    // the anchor survives a refactor that only moved it.
    repo.commit(
        &[(
            "src/lib.rs",
            "// a much longer comment\n// that pushes everything down\n// by three lines\n\
             pub fn a() -> u32 {\n    1\n}\n\npub fn b() {}\n",
        )],
        "move a down without touching it",
    );

    let after = anchor.resolve_with(&repo.roots());
    assert_eq!(
        after.verdict,
        AnchorVerdict::Fresh,
        "a body hash is not a file hash: {}",
        after.note
    );
}

#[test]
fn a_symbol_whose_body_changed_is_stale() {
    let repo = TestRepo::new();
    let mut anchor = repo.anchor("src/lib.rs", "a");
    anchor.commit_sha = Some(repo.commit(&[("src/lib.rs", A_BODY)], "add a"));
    let written = anchor.resolve_with(&repo.roots());
    assert_eq!(written.verdict, AnchorVerdict::Fresh, "{}", written.note);
    anchor.content_hash = written.content_hash;

    repo.commit(
        &[("src/lib.rs", "pub fn a() -> u32 {\n    2\n}\n")],
        "change what a returns",
    );

    let after = anchor.resolve_with(&repo.roots());

    assert_eq!(after.verdict, AnchorVerdict::Stale, "{}", after.note);
    assert!(
        after.note.contains("text has changed"),
        "the note has to say what changed: {}",
        after.note
    );
}

#[test]
fn a_symbol_that_no_longer_exists_is_orphaned() {
    let repo = TestRepo::new();
    let mut anchor = repo.anchor("src/lib.rs", "a");
    anchor.commit_sha = Some(repo.commit(&[("src/lib.rs", A_BODY)], "add a"));
    anchor.content_hash = anchor.resolve_with(&repo.roots()).content_hash;

    repo.commit(
        &[("src/lib.rs", "pub fn renamed() -> u32 {\n    1\n}\n")],
        "rename a",
    );

    let after = anchor.resolve_with(&repo.roots());

    assert_eq!(after.verdict, AnchorVerdict::Orphaned, "{}", after.note);
    assert!(after.note.contains("no longer in"), "{}", after.note);
}

#[test]
fn a_file_that_no_longer_exists_is_orphaned() {
    let repo = TestRepo::new();
    let mut anchor = repo.anchor("src/gone.rs", "a");
    anchor.commit_sha = Some(repo.commit(&[("src/gone.rs", A_BODY)], "add gone.rs"));
    anchor.content_hash = anchor.resolve_with(&repo.roots()).content_hash;

    repo.commit(&[("src/lib.rs", A_BODY)], "delete gone.rs");

    let after = anchor.resolve_with(&repo.roots());

    assert_eq!(after.verdict, AnchorVerdict::Orphaned, "{}", after.note);
    assert!(after.note.contains("no longer exists"), "{}", after.note);
}

/// A file the *recorded* revision does not have is orphaned there, whatever the
/// checkout looks like: the memory cited something that was not in the tree it
/// named.
#[test]
fn a_path_absent_from_the_recorded_revision_is_orphaned_there() {
    let repo = TestRepo::new();
    let revision = repo.commit(&[("src/lib.rs", A_BODY)], "only lib.rs");

    let mut anchor = CodeAnchor::new("src/never_existed.rs");
    anchor.repo_remote = Some(repo.path.to_string_lossy().into_owned());
    anchor.commit_sha = Some(revision);

    let after = anchor.resolve_with(&repo.roots());
    assert_eq!(after.verdict, AnchorVerdict::Orphaned);
    assert!(
        after.note.contains("no longer exists at revision"),
        "{}",
        after.note
    );
}

/// The honest state. A verdict nobody could earn must never be reported as
/// `fresh`: a reader who sees `fresh` stops checking.
#[test]
fn no_repository_is_unchecked_and_never_fresh() {
    let anchor = CodeAnchor {
        repo_remote: Some("/nonexistent/repository/for/vestige/tests".to_string()),
        commit_sha: Some("1cfdf45aa".to_string()),
        path: "src/lib.rs".to_string(),
        symbol: Some("a".to_string()),
        hint_line: None,
        content_hash: None,
    };

    let resolution = anchor.resolve_with(&[]);
    assert_eq!(resolution.verdict, AnchorVerdict::Unchecked);
    assert_ne!(resolution.verdict, AnchorVerdict::Fresh);
    assert!(
        resolution.note.contains("not verified"),
        "{}",
        resolution.note
    );
}

#[test]
fn an_anchor_with_no_recorded_revision_is_unchecked() {
    let repo = TestRepo::new();
    let anchor = CodeAnchor {
        repo_remote: Some(repo.path.to_string_lossy().into_owned()),
        commit_sha: None,
        path: "src/lib.rs".to_string(),
        symbol: None,
        hint_line: Some(3),
        content_hash: None,
    };

    let resolution = anchor.resolve_with(&repo.roots());
    assert_eq!(
        resolution.verdict,
        AnchorVerdict::Unchecked,
        "{}",
        resolution.note
    );
    assert!(
        resolution.note.contains("no revision"),
        "{}",
        resolution.note
    );
}

#[test]
fn a_revision_the_checkout_does_not_have_is_unchecked() {
    let repo = TestRepo::new();
    let anchor = CodeAnchor {
        repo_remote: Some(repo.path.to_string_lossy().into_owned()),
        // A well-formed SHA that is not in this repository: a shallow clone or a
        // rebased branch looks exactly like this, and fetching is not something
        // a read-path check may decide to do.
        commit_sha: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
        path: "src/lib.rs".to_string(),
        symbol: None,
        hint_line: None,
        content_hash: None,
    };

    let resolution = anchor.resolve_with(&repo.roots());
    assert_eq!(
        resolution.verdict,
        AnchorVerdict::Unchecked,
        "{}",
        resolution.note
    );
    assert!(
        resolution.note.contains("not present"),
        "{}",
        resolution.note
    );
}

/// A revision is caller input. It may be any string, and formatting a note
/// about a bad one must not panic: a byte slice at 8 would.
#[test]
fn a_non_ascii_revision_is_unchecked_rather_than_a_panic() {
    let repo = TestRepo::new();
    let anchor = CodeAnchor {
        repo_remote: Some(repo.path.to_string_lossy().into_owned()),
        commit_sha: Some("1234567łabrak".to_string()),
        path: "src/lib.rs".to_string(),
        symbol: None,
        hint_line: None,
        content_hash: None,
    };

    let resolution = anchor.resolve_with(&repo.roots());
    assert_eq!(resolution.verdict, AnchorVerdict::Unchecked);
    assert!(
        resolution.note.contains("not a revision id"),
        "{}",
        resolution.note
    );
}

/// A path with no symbol can only ever be `fresh` or `orphaned`, and the note
/// has to say so rather than implying the file's contents were verified.
#[test]
fn a_path_only_anchor_checks_existence_and_says_so() {
    let repo = TestRepo::new();
    let mut anchor = CodeAnchor::new("src/lib.rs");
    anchor.repo_remote = Some(repo.path.to_string_lossy().into_owned());
    record_revision(&mut anchor, &repo);

    let resolution = anchor.resolve_with(&repo.roots());
    assert_eq!(
        resolution.verdict,
        AnchorVerdict::Fresh,
        "{}",
        resolution.note
    );
    assert!(
        resolution.note.contains("only the file is checked"),
        "{}",
        resolution.note
    );
    assert_eq!(resolution.content_hash, None, "there is nothing to hash");

    // The file goes away and the path-only anchor becomes what it always could
    // become: orphaned. It can never be `stale`, because there is no symbol
    // whose text could have changed.
    repo.commit(&[("src/other.rs", "pub fn other() {}\n")], "delete lib.rs");
    let after = anchor.resolve_with(&repo.roots());
    assert_eq!(after.verdict, AnchorVerdict::Orphaned, "{}", after.note);
}

/// The write path records the revision it can see — and only when the path
/// exists there, because a path that does not is evidence of the wrong
/// repository, and a confident wrong revision is worse than `unchecked`.
#[test]
fn the_write_path_observes_head_only_for_a_path_that_is_in_it() {
    let repo = TestRepo::new();
    let head = repo.commit(&[("src/lib.rs", A_BODY)], "add a");

    let mut present = CodeAnchor::new("src/lib.rs");
    present.repo_remote = Some(repo.path.to_string_lossy().into_owned());
    assert!(present.observe_revision(&repo.roots()));
    assert_eq!(present.commit_sha.as_deref(), Some(head.as_str()));

    let mut absent = CodeAnchor::new("src/elsewhere.rs");
    absent.repo_remote = Some(repo.path.to_string_lossy().into_owned());
    assert!(!absent.observe_revision(&repo.roots()));
    assert_eq!(
        absent.commit_sha, None,
        "a path not in HEAD is not evidence"
    );

    // A caller that named a revision keeps it: observing never overwrites.
    let mut explicit = repo.anchor("src/lib.rs", "a");
    explicit.commit_sha = Some("deadbeef".to_string());
    assert!(!explicit.observe_revision(&repo.roots()));
    assert_eq!(explicit.commit_sha.as_deref(), Some("deadbeef"));
}

/// Whitespace is not a change: a formatter run must not mark every memory
/// about the file stale, or the verdict becomes noise and readers ignore it.
#[test]
fn reformatting_alone_does_not_make_an_anchor_stale() {
    let repo = TestRepo::new();
    let mut anchor = repo.anchor("src/lib.rs", "a");
    anchor.commit_sha = Some(repo.commit(
        &[("src/lib.rs", "pub fn a() -> u32 {\n    1\n}\n")],
        "add a",
    ));
    anchor.content_hash = anchor.resolve_with(&repo.roots()).content_hash;

    repo.commit(
        &[("src/lib.rs", "pub  fn   a() -> u32 {\n\n        1\n}\n")],
        "rustfmt",
    );

    let after = anchor.resolve_with(&repo.roots());
    assert_eq!(after.verdict, AnchorVerdict::Fresh, "{}", after.note);
}

/// A symbol is looked up inside the container the memory named, so two
/// same-named methods in one file do not resolve to each other.
#[test]
fn a_qualified_symbol_resolves_inside_its_own_container() {
    let source = "\
impl First {
    pub fn shared(&self) -> u32 { 1 }
}

impl Second {
    pub fn shared(&self) -> u32 { 2 }
}
";
    let repo = TestRepo::new();
    let head = repo.commit(&[("src/lib.rs", source)], "two impls");
    let mut anchor = repo.anchor("src/lib.rs", "Second::shared");
    anchor.commit_sha = Some(head);
    let resolution = anchor.resolve_with(&repo.roots());
    assert_eq!(
        resolution.verdict,
        AnchorVerdict::Fresh,
        "{}",
        resolution.note
    );
    anchor.content_hash = resolution.content_hash;

    // Only `First::shared` changes; the anchor on `Second::shared` must not.
    repo.commit(
        &[("src/lib.rs", &source.replace("{ 1 }", "{ 11 }"))],
        "change First::shared",
    );
    let after = anchor.resolve_with(&repo.roots());
    assert_eq!(
        after.verdict,
        AnchorVerdict::Fresh,
        "the wrong container was hashed: {}",
        after.note
    );
}

/// A brace inside a string literal must not end the body early — otherwise a
/// change *after* the symbol would look like a change *to* it.
#[test]
fn braces_inside_literals_and_comments_do_not_end_a_body() {
    let repo = TestRepo::new();
    let source =
        "pub fn a() -> &'static str {\n    let s = \"}\";\n    // }\n    s\n}\n\npub fn b() {}\n";
    let head = repo.commit(&[("src/lib.rs", source)], "add a with braces in literals");

    let body = find_symbol_body(source, "a").expect("a must resolve");
    assert!(
        !body.contains("pub fn b"),
        "the body ran past its own closing brace: {body:?}"
    );
    assert!(body.trim_end().ends_with('}'), "{body:?}");

    let mut anchor = repo.anchor("src/lib.rs", "a");
    anchor.commit_sha = Some(head);
    anchor.content_hash = anchor.resolve_with(&repo.roots()).content_hash;

    repo.commit(
        &[(
            "src/lib.rs",
            &source.replace("pub fn b() {}", "pub fn b() { let _ = 1; }"),
        )],
        "change b, not a",
    );
    let after = anchor.resolve_with(&repo.roots());
    assert_eq!(after.verdict, AnchorVerdict::Fresh, "{}", after.note);
}

/// A lifetime is not an unterminated char literal: mistaking one for the other
/// swallows the rest of the file into the body.
#[test]
fn a_lifetime_does_not_swallow_the_body() {
    let source = "pub fn a<'x>(s: &'x str) -> &'x str {\n    s\n}\n\npub fn b() {}\n";
    let body = find_symbol_body(source, "a").expect("a must resolve");
    assert!(
        !body.contains("pub fn b"),
        "body ran past the symbol: {body:?}"
    );
    assert!(body.trim_end().ends_with('}'), "{body:?}");
}

/// Give the anchor the revision the fixture's `HEAD` currently names.
fn record_revision(anchor: &mut CodeAnchor, repo: &TestRepo) {
    anchor.observe_revision(&repo.roots());
    assert!(
        anchor.commit_sha.is_some(),
        "the fixture must record a revision"
    );
}

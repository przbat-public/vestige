//! Resolving an anchor against the revision it names.
//!
//! The check is entirely offline: a repository is opened from the anchor's own
//! `repo_remote` when that names something on this machine, or from the
//! repositories the process can reach. Nothing is fetched. A remote that is not
//! checked out locally is not an error to work around — it is the `unchecked`
//! state, which is what a reader needs to see instead of a `fresh` that was
//! never earned.
//!
//! Everything is read from the recorded commit's tree, never from the working
//! tree: the working tree is a moving target, and a verdict computed against it
//! would say "this matches" about code the memory was never about.

use std::path::{Path, PathBuf};

use chrono::Utc;
use git2::{Oid, Repository};

use super::anchor::{AnchorResolution, AnchorVerdict, CodeAnchor, short_revision};
use super::symbol::{find_symbol_body, hash_body};

/// Repositories this process can reach without a network.
///
/// The current directory, and `VESTIGE_REPO_ROOT` when it is set (the MCP server
/// is usually started from the checkout it serves, but a supervised deployment
/// may not be).
pub fn default_repo_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(root) = std::env::var("VESTIGE_REPO_ROOT")
        && !root.trim().is_empty()
    {
        roots.push(PathBuf::from(root));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    roots
}

impl CodeAnchor {
    /// Resolve against [`default_repo_roots`].
    pub fn resolve(&self) -> AnchorResolution {
        self.resolve_with(&default_repo_roots())
    }

    /// Resolve against an explicit set of repository roots.
    ///
    /// An empty `roots` slice is legal and means "only look at a repository the
    /// anchor names by path" — a caller that wants no ambient repository to be
    /// consulted can say so.
    ///
    /// # What is compared, and why both revisions are needed
    ///
    /// The check reads **two** trees: the revision the memory recorded, and the
    /// revision the repository is on now.
    ///
    /// The recorded revision is what the memory is *about* — it is where the
    /// recorded content hash came from, and a file or symbol that is missing
    /// there is `orphaned` outright. But a commit's tree is immutable, so a check
    /// that read only that tree could never change its answer, and an audit that
    /// reports the same thing forever is not an audit. The rot the design exists
    /// to catch — "the code moved on and the memory did not" — is only visible
    /// against the current checkout, so the symbol is located there too and the
    /// two bodies are compared:
    ///
    /// - same body in both trees → `fresh` (it does not matter that the symbol
    ///   changed line, or that unrelated code above it was rewritten);
    /// - symbol or file gone at `HEAD` → `orphaned`;
    /// - body differs → `stale`.
    ///
    /// The recorded content hash is checked against the recorded revision's own
    /// text as well, which is what catches an anchor whose revision was rewritten
    /// underneath it (an amend or a force-push): the hash no longer describes the
    /// tree it names, and that is `stale`, not `fresh`.
    pub fn resolve_with(&self, roots: &[PathBuf]) -> AnchorResolution {
        let Some(commit_sha) = self.commit_sha.as_deref() else {
            return AnchorResolution::unchecked(format!(
                "no revision was recorded for {}, so there was nothing to check it against",
                self.path
            ));
        };

        let Some(repo) = open_repository(self.repo_remote.as_deref(), roots) else {
            return AnchorResolution::unchecked(match self.repo_remote.as_deref() {
                Some(remote) => format!(
                    "no local checkout of {remote} is reachable, so {} was not verified",
                    self.path
                ),
                None => format!(
                    "no repository is reachable from this process, so {} was not verified",
                    self.path
                ),
            });
        };

        let where_ = self
            .repo_remote
            .as_deref()
            .unwrap_or("the local repository");
        // `get`-safe: `commit_sha` is caller input and may be any string.
        let short = short_revision(commit_sha);

        let Ok(oid) = Oid::from_str(commit_sha) else {
            return AnchorResolution::unchecked(format!(
                "'{commit_sha}' is not a revision id, so {} was not verified",
                self.path
            ));
        };
        let Ok(commit) = repo.find_commit(oid) else {
            // A shallow clone, a rebase or a garbage collection can lose a
            // revision. Checking against a different lineage would be a guess,
            // and fetching is the caller's decision, not a read-path check's.
            return AnchorResolution::unchecked(format!(
                "revision {short} is not present in {where_}, so {} was not verified",
                self.path
            ));
        };
        let Ok(recorded_tree) = commit.tree() else {
            return AnchorResolution::unchecked(format!(
                "revision {short} in {where_} has no tree, so {} was not verified",
                self.path
            ));
        };

        // The recorded revision first: if the file was not there, the anchor was
        // wrong when it was written and is orphaned now.
        let recorded_body = match self.body_at(&repo, &recorded_tree) {
            BodyLookup::Found(body) => body,
            BodyLookup::Missing => return self.orphaned_at(&repo, &recorded_tree, short),
            BodyLookup::NotText => {
                return AnchorResolution::unchecked(format!(
                    "{} is not UTF-8 text at revision {short}, so it was not checked",
                    self.path
                ));
            }
        };

        if let Some(recorded_hash) = self.content_hash.as_deref()
            && let Some(body) = recorded_body.as_deref()
            && hash_body(body) != recorded_hash
        {
            return AnchorResolution {
                verdict: AnchorVerdict::Stale,
                note: format!(
                    "the text of {commit_sha} no longer matches the hash this anchor recorded — \
                     the revision was rewritten, or the anchor never described it"
                ),
                content_hash: Some(hash_body(body)),
                resolved_at: Some(Utc::now()),
            };
        }

        // Then the checkout as it stands. `HEAD` unresolvable (an empty or bare
        // repository) is not a failure: the recorded revision answered already,
        // and claiming more than was checked is the one thing not to do.
        let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) else {
            return AnchorResolution {
                verdict: AnchorVerdict::Fresh,
                note: format!(
                    "{} resolves at revision {short}; this checkout has no HEAD to compare it \
                     against",
                    self.symbol.as_deref().unwrap_or(&self.path)
                ),
                content_hash: recorded_body.as_deref().map(hash_body),
                resolved_at: Some(Utc::now()),
            };
        };

        if head.id() == commit.id() {
            return self.fresh_at(short, recorded_body.as_deref());
        }

        let head_id = head.id().to_string();
        let head_short = short_revision(&head_id);
        let Ok(head_tree) = head.tree() else {
            return self.fresh_at(short, recorded_body.as_deref());
        };
        let head_body = match self.body_at(&repo, &head_tree) {
            BodyLookup::Found(body) => body,
            BodyLookup::Missing => return self.orphaned_at(&repo, &head_tree, head_short),
            BodyLookup::NotText => {
                return AnchorResolution::unchecked(format!(
                    "{} is not UTF-8 text at {head_short}, so it was not checked",
                    self.path
                ));
            }
        };

        match (recorded_body.as_deref(), head_body.as_deref()) {
            // No symbol: only the file's presence can be compared, which is the
            // honest ceiling of a path-only anchor.
            (None, None) => AnchorResolution {
                verdict: AnchorVerdict::Fresh,
                note: format!(
                    "{} exists at revision {short} and at {head_short}; no symbol was recorded, \
                     so only the file is checked",
                    self.path
                ),
                content_hash: None,
                resolved_at: Some(Utc::now()),
            },
            (Some(recorded), Some(current)) if hash_body(recorded) == hash_body(current) => {
                AnchorResolution {
                    verdict: AnchorVerdict::Fresh,
                    note: format!(
                        "{} in {} is unchanged since revision {short}",
                        self.symbol.as_deref().unwrap_or(&self.path),
                        self.path
                    ),
                    content_hash: Some(hash_body(recorded)),
                    resolved_at: Some(Utc::now()),
                }
            }
            (Some(_recorded), Some(current)) => AnchorResolution {
                verdict: AnchorVerdict::Stale,
                note: format!(
                    "{} still resolves in {}, but its text has changed since revision {short} \
                     (checked at {head_short})",
                    self.symbol.as_deref().unwrap_or(&self.path),
                    self.path
                ),
                // The *current* hash is reported but never written back by the
                // audit: an anchor that adopted the new text would report `fresh`
                // on the next pass, which is the silent repair this refuses.
                content_hash: Some(hash_body(current)),
                resolved_at: Some(Utc::now()),
            },
            _ => self.orphaned_at(&repo, &head_tree, head_short),
        }
    }

    /// The text of the symbol (or `None` for a path-only anchor) in `tree`.
    ///
    /// Three outcomes rather than two, because "there is no such file" and "the
    /// file is not text" are different answers: the first is an orphan, the
    /// second is something no anchor can be checked against, and collapsing them
    /// would report a binary asset as a deleted file. Keeping the symbol lookup
    /// in one function is also what makes "resolve the symbol first" true in
    /// every branch rather than only in the one that was written last.
    fn body_at(&self, repo: &Repository, tree: &git2::Tree<'_>) -> BodyLookup {
        let Ok(entry) = tree.get_path(Path::new(&self.path)) else {
            return BodyLookup::Missing;
        };
        let Ok(blob) = repo.find_blob(entry.id()) else {
            return BodyLookup::Missing;
        };
        let Ok(text) = std::str::from_utf8(blob.content()) else {
            return BodyLookup::NotText;
        };
        match self.symbol.as_deref() {
            Some(symbol) => match find_symbol_body(text, symbol) {
                Some(body) => BodyLookup::Found(Some(body.to_string())),
                None => BodyLookup::Missing,
            },
            None => BodyLookup::Found(None),
        }
    }

    /// `fresh` for the case where the checkout could not be compared.
    fn fresh_at(&self, short: &str, recorded_body: Option<&str>) -> AnchorResolution {
        AnchorResolution {
            verdict: AnchorVerdict::Fresh,
            note: format!(
                "{} resolves at revision {short}{}",
                self.symbol.as_deref().unwrap_or(&self.path),
                if self.symbol.is_some() {
                    ""
                } else {
                    "; no symbol was recorded, so only the file is checked"
                }
            ),
            content_hash: recorded_body.map(hash_body),
            resolved_at: Some(Utc::now()),
        }
    }

    /// `orphaned`, naming what is missing and where it was looked for.
    fn orphaned_at(
        &self,
        repo: &Repository,
        tree: &git2::Tree<'_>,
        revision: &str,
    ) -> AnchorResolution {
        let file_present = tree
            .get_path(Path::new(&self.path))
            .ok()
            .and_then(|entry| repo.find_blob(entry.id()).ok())
            .is_some();
        let note = match (&self.symbol, file_present) {
            (Some(symbol), true) => format!(
                "{symbol} is no longer in {} (it was there at revision {revision})",
                self.path
            ),
            _ => format!("{} no longer exists at revision {revision}", self.path),
        };
        AnchorResolution {
            verdict: AnchorVerdict::Orphaned,
            note,
            content_hash: None,
            resolved_at: Some(Utc::now()),
        }
    }

    /// Record the revision this anchor was written against, when the caller did
    /// not name one.
    ///
    /// A path derived from prose carries no revision, and an anchor with no
    /// revision can only ever be `unchecked`. Reading the revision we can see is
    /// an **observation**, not a guess: `HEAD` of the checkout the path resolves
    /// in, and only when the path exists there. The existence test is the guard
    /// that matters — a path that is absent from `HEAD` is evidence we are
    /// looking at the wrong repository, and an honest `unchecked` beats a
    /// confident revision that was never the memory's.
    ///
    /// Returns true when a revision was recorded.
    pub fn observe_revision(&mut self, roots: &[PathBuf]) -> bool {
        if self.commit_sha.is_some() {
            return false;
        }
        let Some(repo) = open_repository(self.repo_remote.as_deref(), roots) else {
            return false;
        };
        let Some(head) = repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
            .filter(|commit| {
                commit
                    .tree()
                    .ok()
                    .and_then(|tree| tree.get_path(Path::new(&self.path)).ok())
                    .is_some()
            })
        else {
            return false;
        };
        self.commit_sha = Some(head.id().to_string());
        if self.repo_remote.is_none() {
            self.repo_remote = repository_identity(&repo);
        }
        true
    }
}

/// Prepare an anchor for storage: observe the revision when the caller did not
/// name one, then check the anchor against it.
///
/// One function so a writer cannot record a revision without checking it, or
/// check one it did not record.
pub fn resolve_for_write(mut anchor: CodeAnchor, roots: &[PathBuf]) -> super::anchor::IngestAnchor {
    anchor.observe_revision(roots);
    let resolution = anchor.resolve_with(roots);
    super::anchor::IngestAnchor::from_resolution(anchor, resolution)
}

/// What a symbol lookup in one tree found.
enum BodyLookup {
    /// The body (or `None` when the anchor names no symbol).
    Found(Option<String>),
    /// No such file, or no such symbol in it.
    Missing,
    /// The file exists but is not UTF-8 text.
    NotText,
}

/// How a repository is named in `code_refs.repo_remote`: its `origin` URL when
/// it has one, otherwise the checkout path.
///
/// The path fallback is what makes a repository with no remote re-checkable at
/// all, and it is written as a path so [`open_repository`] recognises it on the
/// way back in.
fn repository_identity(repo: &Repository) -> Option<String> {
    if let Ok(remote) = repo.find_remote("origin")
        && let Some(url) = remote.url()
    {
        return Some(url.to_string());
    }
    repo.workdir()
        .or_else(|| repo.path().parent())
        .map(|path| path.to_string_lossy().into_owned())
}

/// Open the repository an anchor names, without touching the network.
///
/// Two shapes are accepted, in this order:
///
/// 1. `repo_remote` is itself a directory on this machine — the shape a caller
///    uses for a repository with no remote, and the shape a test uses.
/// 2. `repo_remote` is a URL (or `None`, meaning "the repository this process is
///    running in") and one of `roots` is inside a checkout whose remote matches
///    it. Matching is by remote URL, so a memory about project A is not verified
///    against project B that happens to be the working directory.
pub fn open_repository(repo_remote: Option<&str>, roots: &[PathBuf]) -> Option<Repository> {
    if let Some(remote) = repo_remote.map(str::trim).filter(|r| !r.is_empty()) {
        let path = Path::new(remote);
        if path.is_dir()
            && let Ok(repo) = Repository::open(path).or_else(|_| Repository::discover(path))
        {
            return Some(repo);
        }
    }

    for root in roots {
        let Ok(repo) = Repository::discover(root) else {
            continue;
        };
        match repo_remote.map(str::trim).filter(|r| !r.is_empty()) {
            Some(expected) if !remote_matches(&repo, expected) => continue,
            _ => return Some(repo),
        }
    }
    None
}

/// Does this checkout come from `expected`?
fn remote_matches(repo: &Repository, expected: &str) -> bool {
    let expected = normalize_remote(expected);
    let Ok(remotes) = repo.remotes() else {
        return false;
    };
    remotes.iter().flatten().any(|name| {
        repo.find_remote(name)
            .ok()
            .and_then(|remote| remote.url().map(normalize_remote))
            .is_some_and(|url| url == expected)
    })
}

/// Reduce a remote URL to `host/path` so `git@github.com:me/x.git` and
/// `https://github.com/me/x` compare equal.
///
/// A port is not stripped: `ssh://host:2222/a/b` and `https://host/a/b` are
/// different remotes, and silently equating them would verify a memory against
/// the wrong checkout.
fn normalize_remote(url: &str) -> String {
    let trimmed = url.trim();
    let without_scheme = trimmed.split_once("://").map_or(trimmed, |(_, rest)| rest);
    let without_user = without_scheme
        .rsplit_once('@')
        .map_or(without_scheme, |(_, rest)| rest);
    let without_git = without_user.strip_suffix(".git").unwrap_or(without_user);
    // `host:owner/repo` (scp syntax) and `host/owner/repo` are the same remote.
    let slashed = match without_git.split_once(':') {
        Some((host, rest))
            if host.contains('.') && !rest.starts_with(|c: char| c.is_ascii_digit()) =>
        {
            format!("{host}/{rest}")
        }
        _ => without_git.to_string(),
    };
    slashed
        .trim_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_urls_from_the_same_repository_normalise_together() {
        for (a, b) in [
            ("git@github.com:me/x.git", "https://github.com/me/x"),
            ("https://github.com/me/x.git", "https://github.com/me/x"),
            ("ssh://git@github.com/me/x.git", "git@github.com:me/x"),
        ] {
            assert_eq!(normalize_remote(a), normalize_remote(b), "{a} vs {b}");
        }
    }

    #[test]
    fn different_hosts_or_owners_do_not_normalise_together() {
        assert_ne!(
            normalize_remote("https://github.com/me/x"),
            normalize_remote("https://github.com/other/x")
        );
        assert_ne!(
            normalize_remote("https://github.com/me/x"),
            normalize_remote("https://gitlab.com/me/x")
        );
    }
}

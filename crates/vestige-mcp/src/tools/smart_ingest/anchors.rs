//! Collecting the code anchors a write carries.
//!
//! Two sources, in order of authority:
//!
//! 1. **The caller's explicit reference.** A caller that knows the revision and
//!    the symbol says so, and nothing derived from prose may override it.
//! 2. **Paths named in the content.** A memory that says "the merge bug was in
//!    `crates/.../decompose.rs`" carries a real reference whether or not the
//!    caller thought to pass one. Deriving it is what turns the self-containedness
//!    gate's `bare_code_reference` warning into something checkable: the gate
//!    fires on a path with no anchor, so the write path makes the anchor.
//!
//! What derivation deliberately does **not** do: invent a symbol. A path with no
//! symbol is stored with `symbol = NULL`, and the strongest claim it supports is
//! "this file existed at this revision". Guessing `merge_results` from prose
//! would produce a confident verdict about a function the memory never named.

use std::collections::HashSet;

use vestige_core::code_refs::{
    CodeAnchor, IngestAnchor, default_repo_roots, parse_anchor_text, path_candidates,
    resolve_for_write,
};

use super::args::AnchorArg;

/// Cap on the anchors one memory may carry.
///
/// A pasted stack trace can name dozens of files; storing all of them would make
/// one memory's citation list longer than the store's audit budget and would
/// dilute the anchors that actually matter. The explicit references are kept
/// whatever their number — the cap applies to what was *derived*.
pub(super) const MAX_DERIVED_ANCHORS: usize = 8;

/// The anchors this write should carry, explicit first.
pub(super) fn collect(explicit: &[AnchorArg], content: &str) -> Vec<CodeAnchor> {
    let mut anchors: Vec<CodeAnchor> = Vec::new();
    let mut seen: HashSet<(String, Option<String>)> = HashSet::new();

    for arg in explicit {
        if let Some(anchor) = arg.to_anchor() {
            let key = (anchor.path.clone(), anchor.symbol.clone());
            if seen.insert(key) {
                anchors.push(anchor);
            }
        }
    }

    // A path the caller anchored explicitly is not re-derived: the explicit form
    // is the better one (it may carry a revision and a symbol the prose does
    // not), and two rows for one path would make the search result list the same
    // file twice with potentially different verdicts.
    let explicit_paths: HashSet<String> = anchors.iter().map(|a| a.path.clone()).collect();
    let mut derived = 0usize;
    for candidate in path_candidates(content) {
        if derived >= MAX_DERIVED_ANCHORS {
            break;
        }
        if explicit_paths.contains(candidate.path.as_str()) {
            continue;
        }
        let anchor = candidate.into_anchor();
        let key = (anchor.path.clone(), anchor.symbol.clone());
        if seen.insert(key) {
            anchors.push(anchor);
            derived += 1;
        }
    }

    anchors
}

/// Check each anchor at write time and fold the outcome into what gets stored.
///
/// The check runs here rather than in the storage layer for two reasons: it
/// reads the filesystem and opens a repository (neither belongs inside a SQLite
/// write transaction), and the hash it produces is what gives the *next* check
/// something to compare — an anchor stored without one could never be `stale`.
pub(super) fn resolve(anchors: Vec<CodeAnchor>) -> Vec<IngestAnchor> {
    if anchors.is_empty() {
        return Vec::new();
    }
    let roots = default_repo_roots();
    anchors
        .into_iter()
        .map(|anchor| resolve_for_write(anchor, &roots))
        .collect()
}

/// The paths that carry an anchor, for the self-containedness gate.
///
/// Deduplicated and owned because the gate runs after the anchors are built and
/// must see exactly what was stored — not what the content scan found, which
/// includes paths an explicit anchor already covers.
pub(super) fn anchored_paths(anchors: &[IngestAnchor]) -> Vec<String> {
    let mut paths: Vec<String> = anchors
        .iter()
        .map(|a| a.anchor.path.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    paths.sort();
    paths
}

/// The anchors as the write response reports them.
///
/// Present so a caller learns immediately that the reference it just stored is
/// already stale or orphaned, instead of finding out from a reader six months
/// later.
pub(super) fn response_anchors(anchors: &[IngestAnchor]) -> serde_json::Value {
    serde_json::Value::Array(
        anchors
            .iter()
            .map(|a| {
                serde_json::json!({
                    "reference": a.anchor.reference(),
                    "path": a.anchor.path,
                    "symbol": a.anchor.symbol,
                    "commit": a.anchor.commit_sha,
                    "verdict": a.verdict,
                })
            })
            .collect(),
    )
}

/// The two ways a caller's explicit reference becomes an anchor.
impl AnchorArg {
    /// A bare path from a caller that has no revision and no symbol to offer.
    ///
    /// The short form on purpose: the parse is the same one a caller's
    /// `codeRefs` string goes through, so `files: ["src/lib.rs"]` and
    /// `codeRefs: ["src/lib.rs"]` cannot come to mean two different things.
    /// Nothing is filled in — resolution observes the revision later, and a
    /// symbol is never invented.
    pub(super) fn from_path(path: impl Into<String>) -> Self {
        Self::Text(path.into())
    }

    /// Parse the caller's explicit reference into an anchor.
    fn to_anchor(&self) -> Option<CodeAnchor> {
        match self {
            AnchorArg::Text(text) => parse_anchor_text(text),
            AnchorArg::Full {
                path,
                commit,
                symbol,
                line,
                repo,
            } => {
                let mut anchor = CodeAnchor::new(path.trim());
                anchor.commit_sha = commit
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(str::to_string);
                anchor.symbol = symbol
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                anchor.hint_line = *line;
                anchor.repo_remote = repo
                    .as_deref()
                    .map(str::trim)
                    .filter(|r| !r.is_empty())
                    .map(str::to_string);
                Some(anchor)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(json: serde_json::Value) -> AnchorArg {
        serde_json::from_value(json).expect("explicit anchor object")
    }

    #[test]
    fn an_explicit_anchor_wins_over_a_path_in_the_content() {
        let explicit = vec![object(serde_json::json!({
            "path": "crates/a/b.rs",
            "commit": "1cfdf45aa",
            "symbol": "Storage::new"
        }))];
        let anchors = collect(&explicit, "the bug was in crates/a/b.rs and nowhere else");

        assert_eq!(anchors.len(), 1, "{anchors:?}");
        assert_eq!(anchors[0].commit_sha.as_deref(), Some("1cfdf45aa"));
        assert_eq!(anchors[0].symbol.as_deref(), Some("Storage::new"));
    }

    #[test]
    fn a_path_in_the_content_becomes_an_anchor_with_no_symbol() {
        let anchors = collect(
            &[],
            "BUG FIX: the merge dropped a sub-query in src/search.rs:112",
        );
        assert_eq!(anchors.len(), 1, "{anchors:?}");
        assert_eq!(anchors[0].path, "src/search.rs");
        assert_eq!(anchors[0].hint_line, Some(112));
        assert_eq!(
            anchors[0].symbol, None,
            "a symbol is never invented from prose"
        );
        assert_eq!(anchors[0].commit_sha, None);
    }

    #[test]
    fn the_explicit_text_form_is_accepted() {
        let explicit = vec![
            serde_json::from_value::<AnchorArg>(serde_json::json!(
                "src/search.rs@1cfdf45#merge_results"
            ))
            .unwrap(),
        ];
        let anchors = collect(&explicit, "");
        assert_eq!(anchors[0].path, "src/search.rs");
        assert_eq!(anchors[0].symbol.as_deref(), Some("merge_results"));
    }

    #[test]
    fn derived_anchors_are_capped_but_explicit_ones_are_not() {
        let content = (0..20)
            .map(|i| format!("src/file{i}.rs"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(collect(&[], &content).len(), MAX_DERIVED_ANCHORS);

        let explicit: Vec<AnchorArg> = (0..20)
            .map(|i| {
                object(serde_json::json!({
                    "path": format!("src/file{i}.rs"),
                    "commit": "1cfdf45aa"
                }))
            })
            .collect();
        assert_eq!(collect(&explicit, "").len(), 20);
    }
}

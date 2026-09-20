//! Finding code references in memory text.
//!
//! One scanner, two consumers: `smart_ingest` derives anchors from it, and the
//! self-containedness gate's `bare_code_reference` rule fires on it. They share
//! the scanner on purpose — if the two disagreed about what a "path" is, the
//! gate would warn about a reference that was anchored, or stay silent about one
//! that was not, and the pair would be worse than either alone.
//!
//! The scanner is deliberately shape-based rather than semantic: it recognises
//! what a path *looks like* (a known prefix or a known extension, optionally
//! with `:line`, `@sha` and `#symbol`), because a memory's text is prose and no
//! parser can know which noun is a file. It is deterministic and allocation-
//! bounded; it never consults a repository, which is why it can run before the
//! write and inside the gate.

use super::anchor::CodeAnchor;

/// Extensions that mark a token as a code reference. Shared with the write gate
/// so a token that is anchored and a token that is flagged are the same tokens.
pub const CODE_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".md", ".json", ".toml", ".yml", ".yaml", ".sql",
    ".sh", ".go", ".java", ".rb", ".c", ".h", ".cpp",
];

/// Path prefixes that only make sense inside a checkout.
pub const CODE_PREFIXES: &[&str] = &[
    "src/",
    "crates/",
    "apps/",
    "tests/",
    "docs/",
    "packages/",
    "scripts/",
    "./src/",
];

/// One path-shaped token found in memory text, with whatever else was written
/// attached to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCandidate {
    /// The token exactly as it appeared, which is what the gate reports as the
    /// span that fired the rule.
    pub raw: String,
    /// The path with `:line`, `@sha` and `#symbol` stripped.
    pub path: String,
    pub hint_line: Option<u32>,
    pub commit_sha: Option<String>,
    pub symbol: Option<String>,
}

impl PathCandidate {
    /// True when the text already carried a revision *and* a symbol — the
    /// anchored form `path@sha#symbol`, which is the fix rather than the problem.
    ///
    /// Only the fully anchored form counts. `path@sha` still says nothing about
    /// *what* inside the file the memory is about, so the gate keeps warning.
    pub fn is_anchored(&self) -> bool {
        self.commit_sha.is_some() && self.symbol.is_some()
    }

    /// The anchor this token describes. `repo_remote` is left empty: prose can
    /// name a path, but only a caller (or the write-time observation) can name
    /// the repository it belongs to.
    pub fn into_anchor(self) -> CodeAnchor {
        let mut anchor = CodeAnchor::new(self.path);
        anchor.commit_sha = self.commit_sha;
        anchor.symbol = self.symbol;
        anchor.hint_line = self.hint_line;
        anchor
    }
}

/// Every path-shaped token in `content`, in order of appearance.
///
/// A token that carries fragment-looking text after `#` but no path shape is not
/// returned, and neither is a URL: `https://…/file.rs#L10` is a link, and the
/// entity extractor already tags it as one.
pub fn path_candidates(content: &str) -> Vec<PathCandidate> {
    let mut found = Vec::new();
    for raw in content.split_whitespace() {
        let token = trim_outer(raw);
        if token.is_empty() || token.contains("://") {
            continue;
        }
        if let Some(candidate) = parse_token(token) {
            found.push(candidate);
        }
    }
    found
}

/// Parse a single token as `path[:line][@sha][#symbol]`.
///
/// Public because a caller may hand an anchor in directly; the explicit form is
/// preferred over anything derived from prose, and both end up here.
pub fn parse_anchor_text(text: &str) -> Option<CodeAnchor> {
    let token = trim_outer(text.trim());
    if token.is_empty() || token.contains("://") {
        return None;
    }
    parse_token(token).map(PathCandidate::into_anchor)
}

/// Strip the punctuation prose puts around a reference.
///
/// `@` and `#` are deliberately *not* in the keep set: they are trimmed from the
/// ends (so a Markdown heading `#src/lib.rs` still scans) and kept in the middle
/// (so `path@sha#symbol` survives whole).
fn trim_outer(raw: &str) -> &str {
    raw.trim_matches(|c: char| !c.is_alphanumeric() && !matches!(c, '/' | '.' | '_' | '-' | ':'))
}

fn parse_token(token: &str) -> Option<PathCandidate> {
    // `#symbol` first, then `@sha`, then `:line`: the symbol itself may contain
    // `::`, so splitting it before anything else is what keeps `Type::method`
    // intact.
    let (head, symbol) = match token.split_once('#') {
        Some((head, symbol)) if !symbol.is_empty() => (head, Some(symbol.to_string())),
        Some((head, _)) => (head, None),
        None => (token, None),
    };
    let (head, commit_sha) = match head.rsplit_once('@') {
        Some((head, sha)) if is_sha_like(sha) => (head, Some(sha.to_string())),
        _ => (head, None),
    };
    let (path, hint_line) = match head.rsplit_once(':') {
        Some((path, line)) if line.chars().all(|c| c.is_ascii_digit()) && !line.is_empty() => {
            (path, line.parse::<u32>().ok())
        }
        _ => (head, None),
    };

    // A sentence-final period is punctuation, not part of the path: without
    // this, "see crates/a/b.rs." stores a path with a trailing dot that no
    // revision contains, and the anchor reads as orphaned for a typographic
    // reason.
    let path = path.trim_end_matches('.');
    if !looks_like_code_path(path) {
        return None;
    }

    Some(PathCandidate {
        raw: token.to_string(),
        path: path.to_string(),
        hint_line,
        commit_sha,
        symbol,
    })
}

/// A path-shaped token: a known prefix, a known extension, or a known extension
/// followed by a line number.
pub fn looks_like_code_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    CODE_PREFIXES.iter().any(|p| lower.starts_with(p))
        || CODE_EXTENSIONS.iter().any(|e| lower.ends_with(e))
}

/// Abbreviated or full hex SHA. Anything else after `@` is part of the token
/// (`user@host`), not a revision — and a wrong revision is worse than none,
/// because it makes the anchor look checkable.
fn is_sha_like(value: &str) -> bool {
    (7..=40).contains(&value.len()) && value.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_path_is_a_candidate_with_no_symbol() {
        let found =
            path_candidates("the fix landed in crates/vestige-core/src/search/mmr.rs today");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "crates/vestige-core/src/search/mmr.rs");
        assert_eq!(
            found[0].symbol, None,
            "a symbol is never invented from prose"
        );
        assert_eq!(found[0].commit_sha, None);
        assert!(!found[0].is_anchored());
    }

    #[test]
    fn a_path_with_a_line_number_keeps_the_line_as_a_hint() {
        let found = path_candidates("see src/search.rs:112 for the constant");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "src/search.rs");
        assert_eq!(found[0].hint_line, Some(112));
        assert_eq!(found[0].symbol, None);
    }

    #[test]
    fn the_anchored_form_is_parsed_into_its_three_parts() {
        let found = path_candidates("crates/a/b.rs@1cfdf45#Storage::embeddings_fingerprint moved");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "crates/a/b.rs");
        assert_eq!(found[0].commit_sha.as_deref(), Some("1cfdf45"));
        assert_eq!(
            found[0].symbol.as_deref(),
            Some("Storage::embeddings_fingerprint")
        );
        assert!(found[0].is_anchored());
    }

    /// A URL is a link the entity extractor owns, not a checkout reference. If
    /// the scanner took it, every memory quoting a GitHub permalink would grow
    /// an anchor pointing at a path no repository has.
    #[test]
    fn a_url_is_not_a_path_candidate() {
        assert!(path_candidates("see https://github.com/x/y/blob/main/src/lib.rs#L4").is_empty());
    }

    /// `@` without a revision is how email addresses and handles are written;
    /// treating the tail as a SHA would make the anchor claim a revision that
    /// does not exist, which reads as checkable and is not.
    #[test]
    fn an_at_sign_without_a_revision_is_not_a_revision() {
        let found = path_candidates("ping marek@example.com about src/lib.rs");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "src/lib.rs");
        assert_eq!(found[0].commit_sha, None);
    }

    #[test]
    fn a_sentence_final_period_is_not_part_of_the_path() {
        let found = path_candidates("the constant moved to crates/vestige-core/src/search/mmr.rs.");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "crates/vestige-core/src/search/mmr.rs");
        assert_eq!(
            found[0].raw, "crates/vestige-core/src/search/mmr.rs.",
            "the gate still reports the span it found"
        );
    }

    #[test]
    fn every_candidate_keeps_the_span_the_gate_reports() {
        let found = path_candidates("Files: src/search.rs:112, and crates/core/src/lib.rs");
        assert_eq!(found[0].raw, "src/search.rs:112");
        assert_eq!(found[1].raw, "crates/core/src/lib.rs");
    }
}

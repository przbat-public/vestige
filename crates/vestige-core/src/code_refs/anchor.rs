//! The anchor value, its verdict, and the result of checking it.

use chrono::{DateTime, Utc};

/// What re-checking an anchor against the revision it names found.
///
/// The four states are the whole point of the table: a reader has to be able to
/// tell "this still says what it said" from "this now says something else" from
/// "the thing it pointed at is gone" from "nobody could check". Collapsing the
/// last into `fresh` is the failure mode the design exists to prevent — an
/// unverifiable reference returned as fact is exactly the silently-wrong path a
/// stored path already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorVerdict {
    /// The symbol is where it was and its text hashes to the recorded value.
    Fresh,
    /// The symbol still resolves at the recorded revision, but its text changed.
    Stale,
    /// The file or the symbol no longer exists at the recorded revision.
    Orphaned,
    /// No repository could be opened, or no revision was recorded. Never `fresh`.
    Unchecked,
}

impl AnchorVerdict {
    /// The stored form. Matches the `CHECK` constraint on `code_refs.verdict`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Orphaned => "orphaned",
            Self::Unchecked => "unchecked",
        }
    }

    /// Parse a stored verdict. An unrecognised value yields `None` rather than a
    /// default: a row written by a newer build must surface as "cannot place
    /// this", not silently become `unchecked` (which is a claim).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "fresh" => Some(Self::Fresh),
            "stale" => Some(Self::Stale),
            "orphaned" => Some(Self::Orphaned),
            "unchecked" => Some(Self::Unchecked),
            _ => None,
        }
    }
}

impl std::fmt::Display for AnchorVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A reference to code that can be re-checked: repository, revision, path,
/// symbol, and the hash of the symbol's own text.
///
/// `symbol` is optional and is **not** guessed. When only a path is available,
/// the anchor stores the path with `symbol = NULL`, and the strongest claim it
/// can support is "the file existed at this revision" — inventing a symbol from
/// prose would produce a confident answer about the wrong function.
///
/// `hint_line` is a reader aid only. Nothing resolves through it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAnchor {
    /// The repository the anchor belongs to: a remote URL, or a local path.
    /// `None` means "the repository this process is running in".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_remote: Option<String>,
    /// The revision the memory was written against, as a full or abbreviated
    /// SHA. `None` when the caller did not name one: resolution is then
    /// impossible and the verdict is `unchecked`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Repository-relative path, with `/` separators.
    pub path: String,
    /// Fully-qualified symbol name (`Storage::embeddings_fingerprint`), when the
    /// caller named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Where the symbol sat when the memory was written. A hint, never a locator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint_line: Option<u32>,
    /// Hash of the symbol's body (not of the file), recorded so a later check
    /// can tell "the symbol changed" from "the file changed around it".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

impl CodeAnchor {
    /// An anchor for `path` with everything else unknown.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            repo_remote: None,
            commit_sha: None,
            path: path.into(),
            symbol: None,
            hint_line: None,
            content_hash: None,
        }
    }

    /// The canonical short form a caller can paste back: `path@sha#symbol`.
    ///
    /// Degrades to the longest prefix it can honestly write — `path@sha` when no
    /// symbol was recorded, `path` when no revision was either. It is never
    /// padded with a placeholder, because a placeholder reads as a reference.
    pub fn reference(&self) -> String {
        let mut out = self.path.clone();
        if let Some(sha) = &self.commit_sha {
            out.push('@');
            out.push_str(sha);
        }
        if let Some(symbol) = &self.symbol {
            out.push('#');
            out.push_str(symbol);
        }
        out
    }

    /// True when the anchor names a revision *and* a symbol — the only shape
    /// that can support a `stale` verdict.
    pub fn is_fully_anchored(&self) -> bool {
        self.commit_sha.is_some() && self.symbol.is_some()
    }
}

/// The outcome of checking one anchor.
///
/// `note` is the sentence a reader sees in the search result. It is written for
/// a person who does not know what a `code_refs` row is, because the whole point
/// is that they stop trusting the text and start trusting the verdict.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorResolution {
    pub verdict: AnchorVerdict,
    /// Short human-readable reason, naming the revision that was checked.
    pub note: String,
    /// The hash observed at the revision. Equals the recorded hash on `fresh`;
    /// holds the newly computed body hash on `stale` (never written back, so a
    /// stale anchor stays stale until a human acts on it); `None` when nothing
    /// could be hashed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// When the check ran. `None` for an anchor that was never checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl AnchorResolution {
    /// An unchecked outcome with a reason. Used for every path that could not
    /// verify anything — never for a failure that a `fresh` would hide.
    pub fn unchecked(note: impl Into<String>) -> Self {
        Self {
            verdict: AnchorVerdict::Unchecked,
            note: note.into(),
            content_hash: None,
            resolved_at: Some(Utc::now()),
        }
    }
}

/// Counts from one code-anchor rot audit.
///
/// Report-only by construction: there is no field here for "repaired", because
/// the audit never repairs. A verdict is what the store can say honestly about
/// an anchor; a new pointer is a claim only a human — or the writer of the
/// memory — can make.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAnchorAudit {
    pub checked: i64,
    pub fresh: i64,
    pub stale: i64,
    pub orphaned: i64,
    pub unchecked: i64,
}

impl CodeAnchorAudit {
    /// Record one verdict in the tally.
    pub fn count(&mut self, verdict: AnchorVerdict) {
        self.checked += 1;
        match verdict {
            AnchorVerdict::Fresh => self.fresh += 1,
            AnchorVerdict::Stale => self.stale += 1,
            AnchorVerdict::Orphaned => self.orphaned += 1,
            AnchorVerdict::Unchecked => self.unchecked += 1,
        }
    }

    /// Anchors whose claim no longer holds — the queue a human reviews.
    pub fn needing_review(&self) -> i64 {
        self.stale + self.orphaned
    }
}

/// The abbreviated form of a revision id used in notes.
///
/// `get` rather than a byte slice: a revision arrives from a caller and may be
/// any string, and slicing one that is not ASCII at byte 8 would panic while
/// *formatting a note about a bad revision* — the one place that must never take
/// the process down.
pub(crate) fn short_revision(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

/// One short sentence a reader can act on, derived from a stored verdict.
///
/// The note is *derived* at read time rather than stored: it is a pure function
/// of the verdict, the anchor and when the check ran, and storing prose that can
/// be recomputed is how a row's text and its columns drift apart. What is stored
/// is the verdict, which is the part that required looking at a repository.
pub fn verdict_note(
    verdict: AnchorVerdict,
    anchor: &CodeAnchor,
    resolved_at: Option<DateTime<Utc>>,
) -> String {
    let revision = anchor
        .commit_sha
        .as_deref()
        .map(|sha| format!("revision {}", short_revision(sha)));
    let when = resolved_at.map(|at| format!(" on {}", at.format("%Y-%m-%d")));
    let checked = match (&revision, &when) {
        (Some(rev), Some(when)) => format!("checked against {rev}{when}"),
        (Some(rev), None) => format!("checked against {rev}"),
        _ => "unchecked".to_string(),
    };

    match verdict {
        AnchorVerdict::Fresh => match &anchor.symbol {
            Some(symbol) => format!("{checked}: {symbol} is unchanged"),
            None => format!(
                "{checked}: the file exists (no symbol was recorded, so only the file is checked)"
            ),
        },
        AnchorVerdict::Stale => format!(
            "{checked}: {} still resolves but its text has changed since this memory was written",
            anchor.symbol.as_deref().unwrap_or(&anchor.path)
        ),
        AnchorVerdict::Orphaned => match &anchor.symbol {
            Some(symbol) => format!("{checked}: {symbol} is gone from {}", anchor.path),
            None => format!("{checked}: {} no longer exists", anchor.path),
        },
        AnchorVerdict::Unchecked => {
            "not verified: no local checkout of the recorded revision could be read".to_string()
        }
    }
}

/// An anchor on its way into the store: the reference plus the verdict the
/// write-time check reached.
///
/// The verdict travels with the anchor because the write path already had to
/// resolve it (that is where the content hash comes from), and making the
/// storage layer resolve again would put a filesystem walk and a repository
/// open inside the SQLite write transaction.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestAnchor {
    pub anchor: CodeAnchor,
    pub verdict: AnchorVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl IngestAnchor {
    /// An anchor nobody has checked. The honest default: a caller that supplies
    /// a reference without a revision gets `unchecked`, not a free pass.
    pub fn unchecked(anchor: CodeAnchor) -> Self {
        Self {
            anchor,
            verdict: AnchorVerdict::Unchecked,
            resolved_at: None,
        }
    }

    /// Fold a check's outcome back into the anchor, so what is stored is what
    /// was observed: the freshly computed hash is kept, which is what gives the
    /// *next* check something to compare against.
    pub fn from_resolution(mut anchor: CodeAnchor, resolution: AnchorResolution) -> Self {
        if resolution.content_hash.is_some() {
            anchor.content_hash = resolution.content_hash.clone();
        }
        Self {
            anchor,
            verdict: resolution.verdict,
            resolved_at: resolution.resolved_at,
        }
    }
}

//! What every write path must do to content before it reaches storage.
//!
//! Two steps, and they are one step because their order is not a preference:
//!
//! 1. **Anchors, from the stored text and from the caller's explicit paths.**
//!    An anchor is what lets a reader tell "still true" from "moved on" instead
//!    of trusting a path that rots silently in both directions.
//! 2. **The self-containedness gate, told which of those paths now carry an
//!    anchor.** The gate's `bare_code_reference` rule exists to *demand* an
//!    anchor, so it has to see the ones this write is about to store, or it
//!    would warn a caller to do what the write had just done.
//!
//! Reversing them is the bug this module exists to make impossible: a gate run
//! before the anchors were collected flags every path in the content, and a gate
//! run on text that differs from what is stored grades a memory nobody will
//! read.
//!
//! It is shared rather than copied because a second copy is how one write path
//! comes to check what another one skips — which is exactly how the `codebase`
//! tool came to store six memories carrying no verdict at all while
//! `smart_ingest` marked every one of its own.

use vestige_core::IngestAnchor;

use super::anchors;
use super::args::AnchorArg;
use super::self_contained::{Report, detect_with_anchors, reject_response};

/// Where the caller's explicit references come from.
///
/// `Paths` is the shape a tool takes from its own schema — `files: ["src/lib.rs"]`
/// — and `Anchors` the shape a caller uses when it knows the revision and the
/// symbol. Both outrank anything derived from prose, because a caller that names
/// a reference knows things the text does not say.
#[derive(Debug, Clone, Copy)]
pub enum FileRefs<'a> {
    None,
    Paths(&'a [String]),
    Anchors(&'a [AnchorArg]),
}

impl<'a> From<Option<&'a Vec<String>>> for FileRefs<'a> {
    fn from(files: Option<&'a Vec<String>>) -> Self {
        match files {
            Some(paths) if !paths.is_empty() => Self::Paths(paths),
            _ => Self::None,
        }
    }
}

impl<'a> From<&'a [AnchorArg]> for FileRefs<'a> {
    fn from(anchors: &'a [AnchorArg]) -> Self {
        if anchors.is_empty() {
            Self::None
        } else {
            Self::Anchors(anchors)
        }
    }
}

/// A write that has been through both steps: the anchors to store, and the
/// gate's verdict on the text they belong to.
///
/// Cloneable because two halves of one write need it: the storage call consumes
/// the anchors, and the response that reports them is assembled afterwards.
#[derive(Debug, Clone)]
pub struct PreparedWrite {
    /// The anchors to store with the memory.
    pub anchors: Vec<IngestAnchor>,
    /// The refusal response, when the gate refused the content. `Some` means
    /// nothing may be written and this is what the caller returns instead.
    pub refusal: Option<serde_json::Value>,
    /// The marker to persist, and the findings behind it.
    pub marker: bool,
    pub findings: Option<serde_json::Value>,
    /// The gate's own report, private because the fields above are the whole
    /// contract a write path needs: a caller that wants one more fact about the
    /// verdict should get a method here rather than reach into the gate.
    self_contained: Report,
}

impl PreparedWrite {
    /// The marker as a write response reports it — absent unless the gate found
    /// something, because an `ok: true` on every response is noise that hides
    /// the flagged ones and the caller's question is only ever "was this
    /// flagged".
    pub fn response_marker(&self) -> Option<serde_json::Value> {
        self.self_contained
            .requires_context()
            .then(|| self.self_contained.to_json())
    }

    /// The anchors as the write response reports them, absent when there are
    /// none.
    pub fn response_anchors(&self) -> Option<serde_json::Value> {
        (!self.anchors.is_empty()).then(|| anchors::response_anchors(&self.anchors))
    }
}

/// Run both steps over `content`, which must be the text that will be stored.
///
/// The gate runs on the preprocessed text on purpose: coreference rewriting has
/// already replaced every pronoun it could resolve, so a pronoun still present
/// here is one it could not — exactly the case a write-time rewrite cannot
/// repair. `anchored_time` is the caller's knowledge that the same preprocessing
/// attached an absolute time, which is what suppresses the relative-time rule.
pub fn prepare(content: &str, explicit: FileRefs<'_>, anchored_time: bool) -> PreparedWrite {
    let explicit: Vec<AnchorArg> = match explicit {
        FileRefs::None => Vec::new(),
        FileRefs::Paths(paths) => paths.iter().map(AnchorArg::from_path).collect(),
        FileRefs::Anchors(anchors) => anchors.to_vec(),
    };
    let anchors = anchors::resolve(anchors::collect(&explicit, content));
    let anchored_paths = anchors::anchored_paths(&anchors);

    let self_contained = detect_with_anchors(content, anchored_time, &anchored_paths);

    PreparedWrite {
        refusal: reject_response(&self_contained),
        marker: self_contained.marker(),
        findings: self_contained.findings_json(),
        anchors,
        self_contained,
    }
}

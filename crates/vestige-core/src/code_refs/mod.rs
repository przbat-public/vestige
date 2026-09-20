//! Code anchors: what a memory's reference to code is, and whether it is still
//! true.
//!
//! The problem this module exists for is narrow and measured. A memory about
//! code that identifies the code by *position* — a path, a line, a file — is
//! fail-silent in both directions: after a version bump the stored path keeps
//! resolving to an old copy and nothing signals it, and when the target is
//! deleted the reference vanishes without a word. It breaks into wrongness or
//! into nothing, never into an error, so a reader has no way to tell whether
//! what the memory says is still true. Reference rot in code references is
//! measurable, not theoretical (23.0% of 356 repositories in the study the
//! design cites).
//!
//! The fix is a citation that can be re-checked, and it has three parts:
//!
//! 1. **Identity, not position.** [`CodeAnchor`] names a repository, a revision,
//!    a path and a symbol. A line number is kept only as a `hint_line`, named so
//!    that nothing downstream mistakes it for a locator.
//! 2. **Resolve the symbol, then hash its text.** Hashing the file would
//!    invalidate the memory on every unrelated edit; hashing the symbol's own
//!    body is what lets an anchor survive a refactor that moved it
//!    ([`resolve`]).
//! 3. **A verdict the reader sees.** [`AnchorVerdict`] distinguishes "checked and
//!    still true" from "checked and changed" from "gone" from "could not be
//!    checked at all" — and the last of those is never reported as the first.
//!
//! What this module deliberately does **not** do: repair a reference. Rewriting
//! an anchor to wherever the code went is inventing a pointer, and the research
//! is explicit that a reported broken reference is worth more than a confident
//! wrong one. The audit re-resolves and records; a human decides.

mod anchor;
mod parse;
mod resolve;
mod symbol;

#[cfg(test)]
mod tests;

pub use anchor::{
    AnchorResolution, AnchorVerdict, CodeAnchor, CodeAnchorAudit, IngestAnchor, verdict_note,
};
pub use parse::{
    CODE_EXTENSIONS, CODE_PREFIXES, PathCandidate, looks_like_code_path, parse_anchor_text,
    path_candidates,
};
pub use resolve::{default_repo_roots, open_repository, resolve_for_write};
pub use symbol::{find_symbol_body, hash_body};

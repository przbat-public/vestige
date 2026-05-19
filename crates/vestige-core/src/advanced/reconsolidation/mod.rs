//! # Memory Reconsolidation
//!
//! Implements Nader's reconsolidation theory: "Memories are rebuilt every time they're recalled."
//!
//! When a memory is accessed, it enters a "labile" (modifiable) state. During this window:
//! - New context can be integrated
//! - Connections can be strengthened
//! - Related information can be linked
//! - Emotional associations can be updated
//!
//! After the labile window closes, the memory is "reconsolidated" with any modifications.
//!
//! ## Module Layout
//!
//! - `labile` — `LabileState`, `MemorySnapshot`, `Modification`, `RelationshipType`
//! - `context` — `AccessContext`, `AccessTrigger`
//! - `results` — `ReconsolidatedMemory`, `AppliedModification`, `ChangeSummary`, `RetrievalRecord`
//! - `stats` — `ReconsolidationStats`
//! - `manager` — `ReconsolidationManager` (the public API)
//!
//! ## Scientific Background
//!
//! Inspired by Karim Nader's 2000 work on memory reconsolidation in the
//! amygdala — retrieved memories were shown to become temporarily unstable
//! and to require protein synthesis to be re-stored, which is how the brain
//! ends up rewriting memories at retrieval time rather than playing them
//! back unchanged.
//!
//! What Vestige implements is **not** that biochemical mechanism. It is a
//! short flag-based window per memory ("labile" for a few minutes after
//! retrieval) during which the modification API will apply changes in-place
//! and bump retrieval strength. The biology is the metaphor; the code is a
//! plain bookkeeping layer on top of SQLite.

mod constants;
mod context;
mod helpers;
mod labile;
mod manager;
mod results;
mod stats;

#[cfg(test)]
mod tests;

pub use context::{AccessContext, AccessTrigger};
pub use labile::{LabileState, MemorySnapshot, Modification, RelationshipType};
pub use manager::ReconsolidationManager;
pub use results::{AppliedModification, ChangeSummary, ReconsolidatedMemory, RetrievalRecord};
pub use stats::ReconsolidationStats;

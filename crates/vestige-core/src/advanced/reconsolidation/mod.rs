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
//! Based on Karim Nader's groundbreaking 2000 research showing that:
//! - Retrieved memories become temporarily unstable
//! - Protein synthesis is required to re-store them
//! - This window allows memories to be updated or modified
//! - Memories are not static recordings but dynamic reconstructions

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

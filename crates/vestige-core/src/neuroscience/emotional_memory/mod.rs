//! # Emotional Memory Module
//!
//! Implements emotion-cognition interaction for memory encoding, consolidation,
//! and retrieval. Based on foundational neuroscience research:
//!
//! - **Flashbulb Memory** (Brown & Kulik, 1977): Ultra-high-fidelity encoding
//!   for highly arousing + novel events. The amygdala triggers a "Now Print!"
//!   mechanism.
//! - **Mood-Congruent Memory** (Bower, 1981): Emotional content is better
//!   remembered when current mood matches the emotion of the content.
//! - **Emotional Decay Modulation** (LaBar & Cabeza, 2006): Emotional memories
//!   decay more slowly than neutral ones. FSRS stability is modulated by
//!   emotional intensity.
//! - **Tag-and-Capture** (Frey & Morris, 1997): High-emotion events
//!   retroactively strengthen temporally adjacent memories within a ±30 minute
//!   capture window.
//!
//! ## Module Layout
//!
//! - `constants` — tunable thresholds and capture-window sizing
//! - `evaluation` — `EmotionalEvaluation`, `EmotionCategory` (+ `Display`)
//! - `record` — internal `EmotionalRecord` for tag-and-capture (private)
//! - `module` — `EmotionalMemory` (the public CognitiveEngine field) +
//!   `EmotionalMemoryStats`

mod constants;
mod evaluation;
mod module;
mod record;

#[cfg(test)]
mod tests;

pub use evaluation::{EmotionCategory, EmotionalEvaluation};
pub use module::{EmotionalMemory, EmotionalMemoryStats};

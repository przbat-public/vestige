//! # Context-Dependent Memory - Encoding Specificity Principle
//!
//! Memory retrieval is best when the retrieval context MATCHES the encoding context.
//! This is one of the most robust findings in memory science, established by Tulving
//! and Thomson (1973).
//!
//! ## Scientific Background
//!
//! The Encoding Specificity Principle states that memory is most accessible when
//! the retrieval cues match the encoding conditions. This has been demonstrated
//! across multiple domains:
//!
//! - **State-Dependent Memory**: Information learned in one state (e.g., emotional,
//!   physiological) is better recalled in the same state
//! - **Context-Dependent Memory**: Environmental context during learning affects
//!   subsequent retrieval
//! - **Mood Congruence**: Emotional content is better remembered when current mood
//!   matches the emotion of the content
//!
//! ## Implementation Strategy
//!
//! We capture rich context at encoding time including:
//! - **Temporal Context**: Time of day, day of week, recency
//! - **Topical Context**: Active topics, recent queries, conversation thread
//! - **Session Context**: Session ID, activity type, project
//! - **Emotional Context**: Sentiment polarity and magnitude
//!
//! At retrieval time, we compute context similarity and use it to boost
//! relevance scores for memories encoded in similar contexts.
//!
//! ## Example
//!
//! ```rust,ignore
//! use vestige_core::neuroscience::{
//!     ContextMatcher, EncodingContext, TemporalContext, TopicalContext,
//! };
//!
//! let matcher = ContextMatcher::default();
//!
//! // Compare encoding and retrieval contexts
//! let encoding_ctx = memory.encoding_context();
//! let current_ctx = EncodingContext::capture_current();
//!
//! let similarity = matcher.match_contexts(&encoding_ctx, &current_ctx);
//! println!("Context match: {:.2}", similarity); // 0.0 to 1.0
//!
//! // Boost retrieval scores based on context match
//! let boosted = matcher.boost_retrieval(memories, &current_ctx);
//! ```
//!
//! ## References
//!
//! - Tulving, E., & Thomson, D. M. (1973). Encoding specificity and retrieval
//!   processes in episodic memory. Psychological Review, 80(5), 352-373.
//! - Godden, D. R., & Baddeley, A. D. (1975). Context-dependent memory in two
//!   natural environments: On land and underwater. British Journal of Psychology.

mod emotional;
mod encoding;
mod matcher;
mod session;
mod temporal;
mod topical;

#[cfg(test)]
mod tests;

pub use emotional::EmotionalContext;
pub use encoding::EncodingContext;
pub use matcher::{ContextMatcher, ContextReinstatement, ContextWeights, ScoredMemory};
pub use session::SessionContext;
pub use temporal::{RecencyBucket, TemporalContext, TimeOfDay};
pub use topical::TopicalContext;

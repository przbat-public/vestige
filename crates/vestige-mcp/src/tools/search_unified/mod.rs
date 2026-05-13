//! Unified Search Tool
//!
//! Merges recall, semantic_search, and hybrid_search into a single `search` tool.
//! Always uses hybrid search internally (keyword + semantic + RRF fusion).
//! Implements Testing Effect (Roediger & Karpicke 2006) by auto-strengthening memories on access.
//!
//! v2.1.0: Enhanced cognitive pipeline with retrieval quality improvements
//!   (Yuan et al. 2026: retrieval method = 20pp accuracy range vs 3-8pp for write strategy)
//!
//!   1. Reranker (over-fetch 3x, rerank down)
//!   2. Result deduplication (remove near-identical results — saves context tokens)
//!   3. Temporal boosting (recency + validity)
//!   4. Freshness-aware ranking (prefer newer when scores close + topics overlap)
//!   5. Memory state accessibility filtering
//!   6. Context matching (topic overlap)
//!   7. Spreading activation associations
//!   8. Score-adaptive pruning (drop results below dynamic threshold)
//!   9. Side effects: predictive memory recording + reconsolidation

mod args;
mod execute;
mod format;
mod helpers;
mod pipeline;
mod schema;

#[cfg(test)]
mod tests;

pub use execute::execute;
pub use format::format_node;
pub use schema::schema;

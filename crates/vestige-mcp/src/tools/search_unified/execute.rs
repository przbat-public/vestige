//! Orchestrator for the cognitive search pipeline.
//!
//! The entire 8-stage pipeline lives in the sibling `pipeline` module (split
//! into three phases). This file:
//!   1. Parses + validates `SearchArgs` once.
//!   2. Builds an immutable `PipelineConfig` used by every phase.
//!   3. Runs `retrieval → scoring → finalize` and returns the response.
//!
//! Stage order (matches `ARCHITECTURE.md`):
//!   - Stage 0..2B  (retrieval) — gate, hybrid+decompose, rerank, dedup.
//!   - Stage 3..5G  (scoring)   — every score adjustment + adaptive prune.
//!   - Stage 6..7   (finalize)  — spreading activation, side effects,
//!     formatting, token budget, metacognition.
//!
//! The `pipeline` submodules are intentionally `pub(in crate::tools::search_unified)`
//! and therefore not linked via rustdoc — pass `--document-private-items` if
//! you want to inspect them.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::Storage;

use crate::cognitive::CognitiveEngine;

use super::args::SearchArgs;
use super::pipeline::{self, PipelineConfig};

/// Execute unified search with the 8-stage cognitive pipeline.
///
/// See the sibling `pipeline` module (private, scoped to `search_unified`)
/// for the per-phase implementations.
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: SearchArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    if args.query.trim().is_empty() {
        return Err("Query cannot be empty".to_string());
    }

    let config = build_config(&args)?;

    let retrieval = match pipeline::retrieval::run(storage, cognitive, &args, &config).await? {
        Ok(out) => out,
        Err(early_response) => return Ok(early_response),
    };

    let dedup_removed = retrieval.dedup_removed;
    let scoring =
        pipeline::scoring::run(storage, cognitive, &args, &config, retrieval.results).await?;

    pipeline::finalize::run(storage, cognitive, &args, &config, scoring, dedup_removed).await
}

/// Validate user-facing parameters once and freeze them into a copyable
/// config struct. Every phase below sees the same numbers — no chance of
/// "stage N silently changed limit/min_retention/etc." regressions.
fn build_config(args: &SearchArgs) -> Result<PipelineConfig, String> {
    let detail_level = match args.detail_level.as_deref() {
        Some("brief") => "brief",
        Some("full") => "full",
        Some("summary") | None => "summary",
        Some(invalid) => {
            return Err(format!(
                "Invalid detail_level '{}'. Must be 'brief', 'summary', or 'full'.",
                invalid
            ));
        }
    };

    let retrieval_mode = match args.retrieval_mode.as_deref() {
        Some("precise") => "precise",
        Some("exhaustive") => "exhaustive",
        Some("balanced") | None => "balanced",
        Some(invalid) => {
            return Err(format!(
                "Invalid retrieval_mode '{}'. Must be 'precise', 'balanced', or 'exhaustive'.",
                invalid
            ));
        }
    };

    Ok(PipelineConfig {
        detail_level,
        retrieval_mode,
        limit: args.limit.unwrap_or(10).clamp(1, 100),
        min_retention: args.min_retention.unwrap_or(0.0).clamp(0.0, 1.0),
        min_similarity: args.min_similarity.unwrap_or(0.5).clamp(0.0, 1.0),
    })
}

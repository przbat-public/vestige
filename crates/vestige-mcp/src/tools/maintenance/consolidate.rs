//! `consolidate` tool — runs FSRS consolidation across the node table.

use std::sync::Arc;

use serde_json::Value;

use vestige_core::Storage;

pub fn consolidate_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

/// Consolidate tool
pub async fn execute_consolidate(
    storage: &Arc<Storage>,
    _args: Option<Value>,
) -> Result<Value, String> {
    // run_consolidation walks the whole node table and rewrites multiple
    // rows — easily hundreds of milliseconds on warm DBs and seconds on
    // cold ones. Offload to the blocking pool.
    let storage_clone = storage.clone();
    let result = tokio::task::spawn_blocking(move || storage_clone.run_consolidation())
        .await
        .map_err(|e| format!("Consolidation task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "tool": "consolidate",
        "nodesProcessed": result.nodes_processed,
        "nodesPromoted": result.nodes_promoted,
        "nodesPruned": result.nodes_pruned,
        "decayApplied": result.decay_applied,
        "embeddingsGenerated": result.embeddings_generated,
        "duplicatesMerged": result.duplicates_merged,
        "activationsComputed": result.activations_computed,
        "w20Optimized": result.w20_optimized,
        "durationMs": result.duration_ms,
        // Report-only, and reported: the code-anchor rot audit re-checked this
        // many anchors and found this many whose text no longer matches. A count
        // nobody can read is the same as not running the audit, and nothing here
        // was repaired — `stale` and `orphaned` are a queue for a human.
        "codeAnchorAudit": {
            "checked": result.code_anchor_audit.checked,
            "fresh": result.code_anchor_audit.fresh,
            "stale": result.code_anchor_audit.stale,
            "orphaned": result.code_anchor_audit.orphaned,
            "unchecked": result.code_anchor_audit.unchecked,
            "needingReview": result.code_anchor_audit.needing_review(),
        },
    }))
}

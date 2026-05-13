//! Result formatting helpers shared across MCP tools.

use serde_json::Value;

/// Format a search result based on the requested detail level.
pub(super) fn format_search_result(r: &vestige_core::SearchResult, detail_level: &str) -> Value {
    match detail_level {
        "brief" => serde_json::json!({
            "id": r.node.id,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "combinedScore": r.combined_score,
        }),
        "full" => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "storageStrength": r.node.storage_strength,
            "retrievalStrength": r.node.retrieval_strength,
            "source": r.node.source,
            "sentimentScore": r.node.sentiment_score,
            "sentimentMagnitude": r.node.sentiment_magnitude,
            "createdAt": r.node.created_at.to_rfc3339(),
            "updatedAt": r.node.updated_at.to_rfc3339(),
            "lastAccessed": r.node.last_accessed.to_rfc3339(),
            "nextReview": r.node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": r.node.stability,
            "difficulty": r.node.difficulty,
            "reps": r.node.reps,
            "lapses": r.node.lapses,
            "validFrom": r.node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": r.node.valid_until.map(|dt| dt.to_rfc3339()),
            "matchType": format!("{:?}", r.match_type),
            "epistemicStatus": r.node.epistemic_status().to_string(),
            "memorySystem": r.node.memory_system().to_string(),
            "provenance": r.node.provenance,
        }),
        // "summary" (default) — includes dates so AI never has to guess when a memory is from
        _ => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "createdAt": r.node.created_at.to_rfc3339(),
            "updatedAt": r.node.updated_at.to_rfc3339(),
            "epistemicStatus": r.node.epistemic_status().to_string(),
            "memorySystem": r.node.memory_system().to_string(),
        }),
    }
}

/// Format a KnowledgeNode based on the requested detail level.
/// Reusable across search, timeline, and other tools.
pub fn format_node(node: &vestige_core::KnowledgeNode, detail_level: &str) -> Value {
    match detail_level {
        "brief" => serde_json::json!({
            "id": node.id,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
        }),
        "full" => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
            "storageStrength": node.storage_strength,
            "retrievalStrength": node.retrieval_strength,
            "source": node.source,
            "sentimentScore": node.sentiment_score,
            "sentimentMagnitude": node.sentiment_magnitude,
            "createdAt": node.created_at.to_rfc3339(),
            "updatedAt": node.updated_at.to_rfc3339(),
            "lastAccessed": node.last_accessed.to_rfc3339(),
            "nextReview": node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": node.stability,
            "difficulty": node.difficulty,
            "reps": node.reps,
            "lapses": node.lapses,
            "validFrom": node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": node.valid_until.map(|dt| dt.to_rfc3339()),
            "provenance": node.provenance,
        }),
        // "summary" (default)
        _ => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
        }),
    }
}

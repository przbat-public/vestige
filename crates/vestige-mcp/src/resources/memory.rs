//! Memory Resources
//!
//! memory:// URI scheme resources for the MCP server.

use std::sync::Arc;

use vestige_core::Storage;

use super::ResourceError;

/// Read a memory:// resource
///
/// Returns [`ResourceError::NotFound`] for a path this server does not publish
/// (mapped to `-32002` on the wire) and [`ResourceError::Internal`] for a failure
/// while serving a path that does exist (mapped to `-32603`).
pub async fn read(storage: &Arc<Storage>, uri: &str) -> Result<String, ResourceError> {
    let path = uri.strip_prefix("memory://").unwrap_or("");

    // Parse query parameters if present
    let (path, query) = match path.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path, None),
    };

    let body = match path {
        "stats" => read_stats(storage).await,
        "recent" => {
            let n = parse_query_param(query, "n", 10);
            read_recent(storage, n).await
        }
        "decaying" => read_decaying(storage).await,
        "due" => read_due(storage).await,
        "intentions" => read_intentions(storage).await,
        "intentions/due" => read_triggered_intentions(storage).await,
        "insights" => read_insights(storage).await,
        "consolidation-log" => read_consolidation_log(storage).await,
        _ => return Err(ResourceError::NotFound(uri.to_string())),
    };

    body.map_err(ResourceError::Internal)
}

/// Truncate content for a summary field, counting *characters*.
///
/// The previous `&content[..200]` sliced by byte offset and panicked whenever
/// byte 200 landed inside a multi-byte UTF-8 character — which is most memories
/// containing any non-ASCII text near the cut-off, i.e. any Polish or accented
/// content longer than 200 bytes. A panic here aborts the whole `resources/read`.
fn truncate_for_display(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    let mut truncated: String = content.chars().take(max_chars).collect();
    truncated.push_str("...");
    truncated
}

fn parse_query_param(query: Option<&str>, key: &str, default: i32) -> i32 {
    query
        .and_then(|q| {
            q.split('&').find_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                if k == key { v.parse().ok() } else { None }
            })
        })
        .unwrap_or(default)
        .clamp(1, 100)
}

async fn read_stats(storage: &Arc<Storage>) -> Result<String, String> {
    let storage_stats = storage.clone();
    let storage_ready = storage.clone();
    let (stats, embedding_service_ready) = tokio::task::spawn_blocking(move || {
        let stats = storage_stats.get_stats();
        let ready = storage_ready.is_embedding_ready();
        (stats, ready)
    })
    .await
    .map_err(|e| format!("read_stats task panicked: {}", e))?;
    let stats = stats.map_err(|e| e.to_string())?;

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    let status = if stats.total_nodes == 0 {
        "empty"
    } else if stats.average_retention < 0.3 {
        "critical"
    } else if stats.average_retention < 0.5 {
        "degraded"
    } else {
        "healthy"
    };

    let result = serde_json::json!({
        "status": status,
        "totalNodes": stats.total_nodes,
        "nodesDueForReview": stats.nodes_due_for_review,
        "averageRetention": stats.average_retention,
        "averageStorageStrength": stats.average_storage_strength,
        "averageRetrievalStrength": stats.average_retrieval_strength,
        "oldestMemory": stats.oldest_memory.map(|d| d.to_rfc3339()),
        "newestMemory": stats.newest_memory.map(|d| d.to_rfc3339()),
        "nodesWithEmbeddings": stats.nodes_with_embeddings,
        "embeddingCoverage": format!("{:.1}%", embedding_coverage),
        "embeddingModel": stats.embedding_model,
        "embeddingServiceReady": embedding_service_ready,
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn read_recent(storage: &Arc<Storage>, limit: i32) -> Result<String, String> {
    let storage_clone = storage.clone();
    let nodes = tokio::task::spawn_blocking(move || storage_clone.get_all_nodes(limit, 0))
        .await
        .map_err(|e| format!("get_all_nodes task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    let items: Vec<serde_json::Value> = nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "summary": truncate_for_display(&n.content, 200),
                "nodeType": n.node_type,
                "tags": n.tags,
                "createdAt": n.created_at.to_rfc3339(),
                "retentionStrength": n.retention_strength,
            })
        })
        .collect();

    let result = serde_json::json!({
        "total": nodes.len(),
        "items": items,
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn read_decaying(storage: &Arc<Storage>) -> Result<String, String> {
    // Get nodes with low retention (below 0.5)
    let storage_clone = storage.clone();
    let all_nodes = tokio::task::spawn_blocking(move || storage_clone.get_all_nodes(100, 0))
        .await
        .map_err(|e| format!("get_all_nodes task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    let mut decaying: Vec<_> = all_nodes
        .into_iter()
        .filter(|n| n.retention_strength < 0.5)
        .collect();

    // Sort by retention strength (lowest first)
    decaying.sort_by(|a, b| {
        a.retention_strength
            .partial_cmp(&b.retention_strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let items: Vec<serde_json::Value> = decaying
        .iter()
        .take(20)
        .map(|n| {
            let days_since_access = (chrono::Utc::now() - n.last_accessed).num_days();
            serde_json::json!({
                "id": n.id,
                "summary": truncate_for_display(&n.content, 200),
                "retentionStrength": n.retention_strength,
                "daysSinceAccess": days_since_access,
                "lastAccessed": n.last_accessed.to_rfc3339(),
                "hint": if n.retention_strength < 0.2 {
                    "Critical - review immediately!"
                } else {
                    "Should be reviewed soon"
                },
            })
        })
        .collect();

    let result = serde_json::json!({
        "total": decaying.len(),
        "showing": items.len(),
        "items": items,
        "recommendation": if decaying.is_empty() {
            "All memories are healthy!"
        } else if decaying.len() > 10 {
            "Many memories are decaying. Consider reviewing the most important ones."
        } else {
            "Some memories need attention. Review to strengthen retention."
        },
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn read_due(storage: &Arc<Storage>) -> Result<String, String> {
    let storage_clone = storage.clone();
    let nodes = tokio::task::spawn_blocking(move || storage_clone.get_review_queue(20))
        .await
        .map_err(|e| format!("get_review_queue task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    let items: Vec<serde_json::Value> = nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "summary": truncate_for_display(&n.content, 200),
                "nodeType": n.node_type,
                "retentionStrength": n.retention_strength,
                "difficulty": n.difficulty,
                "reps": n.reps,
                "nextReview": n.next_review.map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    let result = serde_json::json!({
        "total": nodes.len(),
        "items": items,
        // Must name a tool that `tools/list` actually advertises — this used to say
        // `mark_reviewed`, which was dispatch-only, so hosts that validate tool
        // names against the catalog rejected the call the instruction asked for.
        "instruction": "Use memory(action=\"review\", id=<id>, rating=1-4) to complete a review",
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn read_intentions(storage: &Arc<Storage>) -> Result<String, String> {
    let storage_clone = storage.clone();
    let intentions = tokio::task::spawn_blocking(move || storage_clone.get_active_intentions())
        .await
        .map_err(|e| format!("get_active_intentions task panicked: {}", e))?
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now();

    let items: Vec<serde_json::Value> = intentions
        .iter()
        .map(|i| {
            let is_overdue = i.deadline.map(|d| d < now).unwrap_or(false);
            serde_json::json!({
                "id": i.id,
                "description": i.content,
                "status": i.status,
                "priority": match i.priority {
                    1 => "low",
                    3 => "high",
                    4 => "critical",
                    _ => "normal",
                },
                "createdAt": i.created_at.to_rfc3339(),
                "deadline": i.deadline.map(|d| d.to_rfc3339()),
                "isOverdue": is_overdue,
                "snoozedUntil": i.snoozed_until.map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    let overdue_count = items
        .iter()
        .filter(|i| i["isOverdue"].as_bool().unwrap_or(false))
        .count();

    let result = serde_json::json!({
        "total": intentions.len(),
        "overdueCount": overdue_count,
        "items": items,
        "tip": "Use set_intention to add new intentions, complete_intention to mark done",
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn read_triggered_intentions(storage: &Arc<Storage>) -> Result<String, String> {
    let storage_clone = storage.clone();
    let overdue = tokio::task::spawn_blocking(move || storage_clone.get_overdue_intentions())
        .await
        .map_err(|e| format!("get_overdue_intentions task panicked: {}", e))?
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now();

    let items: Vec<serde_json::Value> = overdue
        .iter()
        .map(|i| {
            let overdue_by = i.deadline.map(|d| {
                let duration = now - d;
                if duration.num_days() > 0 {
                    format!("{} days", duration.num_days())
                } else if duration.num_hours() > 0 {
                    format!("{} hours", duration.num_hours())
                } else {
                    format!("{} minutes", duration.num_minutes())
                }
            });
            serde_json::json!({
                "id": i.id,
                "description": i.content,
                "priority": match i.priority {
                    1 => "low",
                    3 => "high",
                    4 => "critical",
                    _ => "normal",
                },
                "deadline": i.deadline.map(|d| d.to_rfc3339()),
                "overdueBy": overdue_by,
            })
        })
        .collect();

    let result = serde_json::json!({
        "triggered": items.len(),
        "items": items,
        "message": if items.is_empty() {
            "No overdue intentions!"
        } else {
            "These intentions need attention"
        },
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn read_insights(storage: &Arc<Storage>) -> Result<String, String> {
    let storage_clone = storage.clone();
    let insights = tokio::task::spawn_blocking(move || storage_clone.get_insights(50))
        .await
        .map_err(|e| format!("get_insights task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    let pending: Vec<_> = insights.iter().filter(|i| i.feedback.is_none()).collect();
    let accepted: Vec<_> = insights
        .iter()
        .filter(|i| i.feedback.as_deref() == Some("accepted"))
        .collect();

    let items: Vec<serde_json::Value> = insights
        .iter()
        .map(|i| {
            serde_json::json!({
                "id": i.id,
                "insight": i.insight,
                "type": i.insight_type,
                "confidence": i.confidence,
                "noveltyScore": i.novelty_score,
                "sourceMemories": i.source_memories,
                "generatedAt": i.generated_at.to_rfc3339(),
                "feedback": i.feedback,
            })
        })
        .collect();

    let result = serde_json::json!({
        "total": insights.len(),
        "pendingReview": pending.len(),
        "accepted": accepted.len(),
        "items": items,
        "tip": "These insights were discovered during memory consolidation",
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn read_consolidation_log(storage: &Arc<Storage>) -> Result<String, String> {
    let storage_clone = storage.clone();
    let (history_res, last_res) = tokio::task::spawn_blocking(move || {
        let history = storage_clone.get_consolidation_history(20);
        let last = storage_clone.get_last_consolidation();
        (history, last)
    })
    .await
    .map_err(|e| format!("consolidation log task panicked: {}", e))?;
    let history = history_res.map_err(|e| e.to_string())?;
    let last_run = last_res.map_err(|e| e.to_string())?;

    let items: Vec<serde_json::Value> = history
        .iter()
        .map(|h| {
            serde_json::json!({
                "id": h.id,
                "completedAt": h.completed_at.to_rfc3339(),
                "durationMs": h.duration_ms,
                "memoriesReplayed": h.memories_replayed,
                "connectionsFound": h.connections_found,
                "connectionsStrengthened": h.connections_strengthened,
                "connectionsPruned": h.connections_pruned,
                "insightsGenerated": h.insights_generated,
            })
        })
        .collect();

    let result = serde_json::json!({
        "lastRun": last_run.map(|d| d.to_rfc3339()),
        "totalRuns": history.len(),
        "history": items,
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_for_display_counts_characters_not_bytes() {
        // U+017C is two bytes; 250 of them is 500 bytes, so a byte slice at 200
        // used to panic inside the middle of a character. Written with escapes so
        // an encoding mishap cannot silently weaken the test.
        let long: String = std::iter::repeat_n("\u{17C}", 250).collect();
        let summary = truncate_for_display(&long, 200);
        assert_eq!(summary.chars().count(), 203, "200 chars + the ellipsis");
        assert!(summary.ends_with("..."));

        // Short content is returned untouched, with no ellipsis.
        assert_eq!(truncate_for_display("krotko", 200), "krotko");
        assert_eq!(truncate_for_display(&long, 250), long);
    }
}

//! Memory Changelog Tool
//!
//! View audit trail of memory changes.
//! Per-memory mode: state transitions *and* the content timeline for a single
//! memory — what state it was in, and what it used to say.
//! System-wide mode: consolidations + recent state transitions.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use uuid::Uuid;

use vestige_core::{REVISION_CONTENT_CHAR_LIMIT, Storage};

use super::search_unified::revision_timeline_json;

/// Input schema for memory_changelog tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "memory_id": {
                "type": "string",
                "description": "Scope to a single memory's audit trail. If omitted, returns system-wide changelog. Per-memory mode also returns `revisions`: the content timeline (oldest step first) with each step's kind, record time, previous and new content, reason and actor — so 'what did this memory say on date X, and why did it change' is answerable from the output. Each revision's content is capped at `contentLimitChars` characters and marked with `oldContentTruncated`/`newContentTruncated`; `revisionsOmitted` counts older steps this page left out."
            },
            "start": {
                "type": "string",
                "description": "Start of time range (ISO 8601). Only used in system-wide mode."
            },
            "end": {
                "type": "string",
                "description": "End of time range (ISO 8601). Only used in system-wide mode."
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of entries (default: 20, max: 100)",
                "default": 20,
                "minimum": 1,
                "maximum": 100
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangelogArgs {
    #[serde(alias = "memory_id")]
    memory_id: Option<String>,
    limit: Option<i32>,
}

/// Execute memory_changelog tool
pub async fn execute(storage: &Arc<Storage>, args: Option<Value>) -> Result<Value, String> {
    let args: ChangelogArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => ChangelogArgs {
            memory_id: None,
            limit: None,
        },
    };

    let limit = args.limit.unwrap_or(20).clamp(1, 100);

    if let Some(ref memory_id) = args.memory_id {
        // Per-memory mode: state transitions for a specific memory
        execute_per_memory(storage, memory_id, limit)
    } else {
        // System-wide mode: consolidations + recent transitions
        execute_system_wide(storage, limit)
    }
}

/// Per-memory changelog: state transition audit trail *and* the content
/// timeline.
///
/// The two answer different questions and both belong here: `transitions` is
/// scheduling (what state the memory was in), while `revisions` is content
/// (what it said, when that changed, and why). Before the timeline existed, a
/// changelog could say a memory had been edited without ever showing what it
/// said before — the previous picture was unrecoverable from the output.
fn execute_per_memory(storage: &Storage, memory_id: &str, limit: i32) -> Result<Value, String> {
    // Validate UUID format
    Uuid::parse_str(memory_id)
        .map_err(|_| format!("Invalid memory_id '{}'. Must be a valid UUID.", memory_id))?;

    // Get the memory for context
    let node = storage
        .get_node(memory_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Memory '{}' not found.", memory_id))?;

    // Get state transitions
    let transitions = storage
        .get_state_transitions(memory_id, limit)
        .map_err(|e| e.to_string())?;

    let formatted_transitions: Vec<Value> = transitions
        .iter()
        .map(|t| {
            serde_json::json!({
                "fromState": t.from_state,
                "toState": t.to_state,
                "reasonType": t.reason_type,
                "reasonData": t.reason_data,
                "timestamp": t.timestamp.to_rfc3339(),
            })
        })
        .collect();

    // The content timeline. `limit` caps this page too: a caller asking for the
    // last 20 steps of the audit trail is asking for the same window of content.
    // Storage returns newest-first (the page a reader wants); the timeline is
    // rendered forward in time so it can be read as a history.
    let revisions = storage
        .get_memory_revisions(memory_id, i64::from(limit))
        .map_err(|e| e.to_string())?;
    let total_revisions = storage
        .count_revisions(memory_id)
        .map_err(|e| e.to_string())?;
    let timeline = revision_timeline_json(&revisions, REVISION_CONTENT_CHAR_LIMIT);
    let revisions_returned = timeline.len() as i64;

    Ok(serde_json::json!({
        "tool": "memory_changelog",
        "mode": "per_memory",
        "memoryId": memory_id,
        "memoryContent": node.content,
        "memoryType": node.node_type,
        "currentRetention": node.retention_strength,
        "totalTransitions": formatted_transitions.len(),
        "transitions": formatted_transitions,
        // The content history. `revisionsOmitted` says how many older steps this
        // page left out, so a page cannot pass for a complete history; the cap
        // is advertised rather than assumed, so a client renders truncation from
        // the payload instead of hard-coding the number.
        "totalRevisions": total_revisions,
        "revisionsReturned": revisions_returned,
        "revisionsOmitted": (total_revisions - revisions_returned).max(0),
        "contentLimitChars": REVISION_CONTENT_CHAR_LIMIT,
        "revisions": timeline,
    }))
}

/// System-wide changelog: consolidations + dream history + recent state transitions
fn execute_system_wide(storage: &Storage, limit: i32) -> Result<Value, String> {
    // Get consolidation history
    let consolidations = storage
        .get_consolidation_history(limit)
        .map_err(|e| e.to_string())?;

    // Get recent state transitions across all memories
    let transitions = storage
        .get_recent_state_transitions(limit)
        .map_err(|e| e.to_string())?;

    // Get dream history
    let dreams = storage.get_dream_history(limit).unwrap_or_default();

    // Build unified event list
    let mut events: Vec<(DateTime<Utc>, Value)> = Vec::new();

    for c in &consolidations {
        events.push((
            c.completed_at,
            serde_json::json!({
                "type": "consolidation",
                "timestamp": c.completed_at.to_rfc3339(),
                "durationMs": c.duration_ms,
                "memoriesReplayed": c.memories_replayed,
                "connectionFound": c.connections_found,
                "connectionsStrengthened": c.connections_strengthened,
                "connectionsPruned": c.connections_pruned,
                "insightsGenerated": c.insights_generated,
            }),
        ));
    }

    for d in &dreams {
        events.push((
            d.dreamed_at,
            serde_json::json!({
                "type": "dream",
                "timestamp": d.dreamed_at.to_rfc3339(),
                "durationMs": d.duration_ms,
                "memoriesReplayed": d.memories_replayed,
                "connectionFound": d.connections_found,
                "insightsGenerated": d.insights_generated,
                "memoriesStrengthened": d.memories_strengthened,
                "memoriesCompressed": d.memories_compressed,
            }),
        ));
    }

    for t in &transitions {
        events.push((
            t.timestamp,
            serde_json::json!({
                "type": "state_transition",
                "timestamp": t.timestamp.to_rfc3339(),
                "memoryId": t.memory_id,
                "fromState": t.from_state,
                "toState": t.to_state,
                "reasonType": t.reason_type,
                "reasonData": t.reason_data,
            }),
        ));
    }

    // Sort by timestamp descending
    events.sort_by(|a, b| b.0.cmp(&a.0));

    // Truncate to limit
    events.truncate(limit as usize);

    let formatted_events: Vec<Value> = events.into_iter().map(|(_, v)| v).collect();

    Ok(serde_json::json!({
        "tool": "memory_changelog",
        "mode": "system_wide",
        "totalEvents": formatted_events.len(),
        "events": formatted_events,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    async fn ingest_test_memory(storage: &Arc<Storage>) -> String {
        let node = storage
            .ingest(vestige_core::IngestInput {
                content: "Changelog test memory".to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec![],
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            })
            .unwrap();
        node.id
    }

    #[test]
    fn test_schema_has_properties() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["memory_id"].is_object());
        assert!(s["properties"]["start"].is_object());
        assert!(s["properties"]["end"].is_object());
        assert!(s["properties"]["limit"].is_object());
        assert_eq!(s["properties"]["limit"]["default"], 20);
        assert_eq!(s["properties"]["limit"]["minimum"], 1);
        assert_eq!(s["properties"]["limit"]["maximum"], 100);
    }

    #[tokio::test]
    async fn test_changelog_no_args_system_wide() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["tool"], "memory_changelog");
        assert_eq!(value["mode"], "system_wide");
        assert!(value["events"].is_array());
    }

    #[tokio::test]
    async fn test_changelog_system_wide_empty() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        let value = result.unwrap();
        assert_eq!(value["totalEvents"], 0);
        assert!(value["events"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_changelog_per_memory_valid_id() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_test_memory(&storage).await;
        let args = serde_json::json!({ "memory_id": id });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["tool"], "memory_changelog");
        assert_eq!(value["mode"], "per_memory");
        assert_eq!(value["memoryId"], id);
        assert!(value["memoryContent"].is_string());
        assert!(value["transitions"].is_array());
    }

    #[tokio::test]
    async fn test_changelog_per_memory_invalid_uuid() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "memory_id": "not-a-uuid" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory_id"));
    }

    #[tokio::test]
    async fn test_changelog_per_memory_nonexistent() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "memory_id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_changelog_limit_clamped() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "limit": 0 });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok()); // clamped to 1
    }

    #[tokio::test]
    async fn test_changelog_limit_high_clamped() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "limit": 999 });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok()); // clamped to 100
    }

    #[tokio::test]
    async fn test_changelog_per_memory_no_transitions() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_test_memory(&storage).await;
        let args = serde_json::json!({ "memory_id": id });
        let result = execute(&storage, Some(args)).await;
        let value = result.unwrap();
        assert_eq!(value["totalTransitions"], 0);
        assert!(value["transitions"].as_array().unwrap().is_empty());
    }

    // ====================================================================
    // The content timeline
    // ====================================================================
    //
    // The changelog used to answer "what state was this memory in", which is
    // scheduling, not content: it could not say what the memory used to say or
    // why it changed. These tests pin the timeline that closes that gap.

    /// A timeline reads forward in time, and each step carries what changed,
    /// when, and why — the three things a reader needs to reconstruct the
    /// picture at any instant inside the window.
    #[tokio::test]
    async fn test_changelog_per_memory_carries_the_content_timeline_in_order() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_test_memory(&storage).await;
        storage
            .update_node_content_with_revision(
                &id,
                "Changelog test memory, second wording",
                Some("user corrected the wording"),
            )
            .unwrap();
        storage
            .invalidate_with_revision(&id, Utc::now(), Some("superseded by the v2 API"))
            .unwrap();

        let args = serde_json::json!({ "memory_id": id });
        let value = execute(&storage, Some(args.clone())).await.unwrap();

        let revisions = value["revisions"]
            .as_array()
            .expect("the per-memory changelog must carry the content timeline");
        let kinds: Vec<&str> = revisions
            .iter()
            .map(|r| r["kind"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            kinds,
            vec!["create", "edit", "invalidate"],
            "a timeline must read forward in time: {value}"
        );

        assert_eq!(revisions[0]["newContent"], "Changelog test memory");
        assert!(
            revisions[0]["oldContent"].is_null(),
            "a create has nothing before it: {value}"
        );
        assert_eq!(revisions[1]["oldContent"], "Changelog test memory");
        assert_eq!(
            revisions[1]["newContent"],
            "Changelog test memory, second wording"
        );
        assert_eq!(revisions[1]["reason"], "user corrected the wording");
        assert_eq!(revisions[2]["kind"], "invalidate");
        assert_eq!(revisions[2]["reason"], "superseded by the v2 API");
        assert_eq!(
            revisions[2]["contentField"], "validUntil",
            "an invalidate changes the validity bound, so its old/new values are timestamps, not text: {value}"
        );

        for revision in revisions {
            assert!(
                revision["recordedAt"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()),
                "an undated step cannot be placed on a timeline: {revision}"
            );
            assert_eq!(
                revision["actor"],
                serde_json::Value::Null,
                "no actor was supplied, so none may be invented: {revision}"
            );
        }

        assert_eq!(value["totalRevisions"], 3);
        assert_eq!(value["revisionsOmitted"], 0);

        let again = execute(&storage, Some(args)).await.unwrap();
        assert_eq!(
            value["revisions"], again["revisions"],
            "the same read must produce the same timeline, in the same order"
        );
    }

    /// Old and new text in full is a lot of text, so each field is capped — and
    /// the cap is advertised in the response rather than left for a client to
    /// guess or hard-code.
    #[tokio::test]
    async fn test_changelog_timeline_marks_truncation_and_advertises_its_cap() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_test_memory(&storage).await;
        let long = "w".repeat(4000);
        storage.update_node_content(&id, &long).unwrap();

        let value = execute(&storage, Some(serde_json::json!({ "memory_id": id })))
            .await
            .unwrap();

        let cap = value["contentLimitChars"]
            .as_u64()
            .expect("the timeline must advertise the cap it applied") as usize;
        // Pinned like the dashboard's limits: the wire value is the documented
        // one (`vestige_core::REVISION_CONTENT_CHAR_LIMIT`), and a casual change
        // to it has to show up here as a deliberate act.
        assert_eq!(
            cap, 500,
            "the advertised cap is part of the wire contract: {value}"
        );
        assert!(
            cap < 4000,
            "test premise: the cap has to bite on 4000 characters, got {cap}"
        );

        let revisions = value["revisions"].as_array().unwrap();
        let edit = revisions.last().expect("the edit is the newest revision");
        assert_eq!(edit["kind"], "edit");
        assert_eq!(
            edit["newContent"].as_str().unwrap().chars().count(),
            cap,
            "the content must be cut at exactly the advertised cap: {value}"
        );
        assert_eq!(
            edit["newContentTruncated"], true,
            "a capped field has to say it was capped: {value}"
        );
        assert_eq!(
            edit["oldContentTruncated"], false,
            "the short text this edit replaced was not capped: {value}"
        );
    }

    /// The page size selects the *most recent* steps — the ones a reader asking
    /// "how did this change" wants — and the response says how many older steps
    /// the page left out, so a truncated history cannot pass for a complete one.
    #[tokio::test]
    async fn test_changelog_timeline_reports_the_steps_a_limited_page_left_out() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_test_memory(&storage).await;
        for wording in ["second", "third", "fourth"] {
            storage
                .update_node_content(&id, &format!("Changelog test memory, {wording} wording"))
                .unwrap();
        }

        let value = execute(
            &storage,
            Some(serde_json::json!({ "memory_id": id, "limit": 2 })),
        )
        .await
        .unwrap();

        let revisions = value["revisions"].as_array().unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(
            revisions[0]["newContent"], "Changelog test memory, third wording",
            "a limited page keeps the newest steps, still in forward order: {value}"
        );
        assert_eq!(
            revisions[1]["newContent"],
            "Changelog test memory, fourth wording"
        );
        assert_eq!(value["totalRevisions"], 4);
        assert_eq!(
            value["revisionsOmitted"], 2,
            "the reader must be told the timeline is a page, not the whole history: {value}"
        );
    }
}

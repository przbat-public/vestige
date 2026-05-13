//! "check" action — find triggered intentions.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::Storage;
use vestige_core::neuroscience::ProspectiveContext;

use crate::cognitive::CognitiveEngine;

use super::args::{TriggerSpec, UnifiedIntentionArgs};

/// Execute "check" action - find triggered intentions
pub(super) async fn execute_check(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &UnifiedIntentionArgs,
) -> Result<Value, String> {
    let now = Utc::now();

    // ====================================================================
    // COGNITIVE: Update prospective memory context
    // ====================================================================
    if let Some(ctx) = &args.context
        && let Ok(cog) = cognitive.try_lock()
    {
        let mut prospective_ctx = ProspectiveContext::new();
        if let Some(codebase) = &ctx.codebase {
            prospective_ctx.project_name = Some(codebase.clone());
        }
        if let Some(file) = &ctx.file {
            prospective_ctx.active_files = vec![file.clone()];
        }
        if let Some(topics) = &ctx.topics {
            prospective_ctx.active_topics = topics.clone();
        }
        // Update context on prospective memory (triggers internal monitoring)
        let _ = cog.prospective_memory.update_context(prospective_ctx);
    }

    // Get active intentions on the blocking pool — covering index on
    // status + priority but still real SQL.
    let storage_clone = storage.clone();
    let intentions = tokio::task::spawn_blocking(move || {
        storage_clone
            .get_active_intentions()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("get_active_intentions task panicked: {}", e))??;

    let mut triggered = Vec::new();
    let mut pending = Vec::new();

    for intention in intentions {
        // Parse trigger data
        let trigger: Option<TriggerSpec> = serde_json::from_str(&intention.trigger_data).ok();

        // Check if triggered
        let is_triggered = if let Some(t) = &trigger {
            match t.trigger_type.as_deref() {
                Some("time") => {
                    if let Some(at) = &t.at {
                        if let Ok(trigger_time) = DateTime::parse_from_rfc3339(at) {
                            trigger_time.with_timezone(&Utc) <= now
                        } else {
                            false
                        }
                    } else if let Some(mins) = t.in_minutes {
                        let trigger_time = intention.created_at + Duration::minutes(mins);
                        trigger_time <= now
                    } else {
                        false
                    }
                }
                Some("context") => {
                    if let Some(ctx) = &args.context {
                        // Check codebase match
                        if let (Some(trigger_codebase), Some(current_codebase)) =
                            (&t.codebase, &ctx.codebase)
                        {
                            current_codebase
                                .to_lowercase()
                                .contains(&trigger_codebase.to_lowercase())
                        // Check file pattern match
                        } else if let (Some(pattern), Some(file)) = (&t.file_pattern, &ctx.file) {
                            file.contains(pattern)
                        // Check topic match
                        } else if let (Some(topic), Some(topics)) = (&t.topic, &ctx.topics) {
                            topics
                                .iter()
                                .any(|t| t.to_lowercase().contains(&topic.to_lowercase()))
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                _ => false,
            }
        } else {
            false
        };

        // Check if overdue
        let is_overdue = intention.deadline.map(|d| d < now).unwrap_or(false);

        let item = serde_json::json!({
            "id": intention.id,
            "description": intention.content,
            "priority": match intention.priority {
                1 => "low",
                3 => "high",
                4 => "critical",
                _ => "normal",
            },
            "createdAt": intention.created_at.to_rfc3339(),
            "deadline": intention.deadline.map(|d| d.to_rfc3339()),
            "isOverdue": is_overdue,
        });

        if is_triggered || is_overdue {
            triggered.push(item);
        } else {
            pending.push(item);
        }
    }

    Ok(serde_json::json!({
        "action": "check",
        "triggered": triggered,
        "pending": pending,
        "checkedAt": now.to_rfc3339(),
    }))
}

//! "set" action — create a new intention.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use vestige_core::IntentionRecord;
use vestige_core::Storage;
use vestige_core::neuroscience::prospective_memory::IntentionTrigger as ProspectiveTrigger;

use crate::cognitive::CognitiveEngine;

use super::args::UnifiedIntentionArgs;

/// Execute "set" action - create a new intention
pub(super) async fn execute_set(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &UnifiedIntentionArgs,
) -> Result<Value, String> {
    let description = args
        .description
        .as_ref()
        .ok_or("Missing 'description' for set action")?;

    if description.trim().is_empty() {
        return Err("Description cannot be empty".to_string());
    }

    if description.len() > 100_000 {
        return Err("Description too large (max 100KB)".to_string());
    }

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();

    // ====================================================================
    // COGNITIVE: NLP parsing + intent auto-tagging
    // ====================================================================
    let mut nlp_parsed = false;
    let mut nlp_trigger_type = None;
    let mut nlp_trigger_data = None;
    let mut nlp_priority = None;
    let mut tags = Vec::new();

    if let Ok(cog) = cognitive.try_lock() {
        // 8A. Try NLP parsing when no explicit trigger is provided
        if args.trigger.is_none()
            && let Ok(parsed) = cog.intention_parser.parse(description)
        {
            nlp_parsed = true;
            // Extract trigger info from parsed intention
            let (t_type, t_data) = match &parsed.trigger {
                ProspectiveTrigger::TimeBased { .. } => (
                    "time".to_string(),
                    serde_json::json!({"type": "time"}).to_string(),
                ),
                ProspectiveTrigger::DurationBased { after, .. } => {
                    let mins = after.num_minutes();
                    (
                        "time".to_string(),
                        serde_json::json!({"type": "time", "in_minutes": mins}).to_string(),
                    )
                }
                ProspectiveTrigger::EventBased { condition, .. } => (
                    "event".to_string(),
                    serde_json::json!({"type": "event", "condition": condition}).to_string(),
                ),
                ProspectiveTrigger::ContextBased { context_match } => (
                    "context".to_string(),
                    serde_json::json!({"type": "context", "topic": format!("{:?}", context_match)})
                        .to_string(),
                ),
                ProspectiveTrigger::Recurring { .. } => (
                    "recurring".to_string(),
                    serde_json::json!({"type": "recurring"}).to_string(),
                ),
                _ => (
                    "event".to_string(),
                    serde_json::json!({"type": "event"}).to_string(),
                ),
            };
            nlp_trigger_type = Some(t_type);
            nlp_trigger_data = Some(t_data);

            // Use NLP-detected priority if user didn't specify one
            if args.priority.is_none() {
                nlp_priority = Some(parsed.priority);
            }
        }

        // Auto-tag with detected intent
        let intent_result = cog.intent_detector.detect_intent();
        if intent_result.confidence > 0.5 {
            let intent_tag = format!("intent:{:?}", intent_result.primary_intent);
            let intent_tag = if intent_tag.len() > 50 {
                format!("{}...", &intent_tag[..intent_tag.floor_char_boundary(47)])
            } else {
                intent_tag
            };
            tags.push(intent_tag);
        }
    }

    // Determine trigger type and data (explicit > NLP > manual)
    let (trigger_type, trigger_data) = if let Some(trigger) = &args.trigger {
        let t_type = trigger
            .trigger_type
            .clone()
            .unwrap_or_else(|| "time".to_string());
        let data = serde_json::to_string(trigger).unwrap_or_else(|_| "{}".to_string());
        (t_type, data)
    } else if let (Some(t_type), Some(t_data)) = (nlp_trigger_type, nlp_trigger_data) {
        (t_type, t_data)
    } else {
        ("manual".to_string(), "{}".to_string())
    };

    // Parse priority (explicit > NLP > normal)
    let priority = match args.priority.as_deref() {
        Some("low") => 1,
        Some("high") => 3,
        Some("critical") => 4,
        Some("normal") => 2,
        Some(_) => 2,
        None => {
            // Use NLP-detected priority if available
            if let Some(nlp_p) = nlp_priority {
                use vestige_core::neuroscience::prospective_memory::Priority;
                match nlp_p {
                    Priority::Low => 1,
                    Priority::Normal => 2,
                    Priority::High => 3,
                    Priority::Critical => 4,
                }
            } else {
                2 // normal default
            }
        }
    };

    // Parse deadline
    let deadline = args.deadline.as_ref().and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    // Calculate trigger time if specified
    let trigger_at = if let Some(trigger) = &args.trigger {
        if let Some(at) = &trigger.at {
            DateTime::parse_from_rfc3339(at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        } else {
            trigger.in_minutes.map(|mins| now + Duration::minutes(mins))
        }
    } else {
        None
    };

    let record = IntentionRecord {
        id: id.clone(),
        content: description.clone(),
        trigger_type,
        trigger_data,
        priority,
        status: "active".to_string(),
        created_at: now,
        deadline,
        fulfilled_at: None,
        reminder_count: 0,
        last_reminded_at: None,
        notes: None,
        tags,
        related_memories: vec![],
        snoozed_until: None,
        source_type: if nlp_parsed { "nlp" } else { "mcp" }.to_string(),
        source_data: None,
    };

    let storage_clone = storage.clone();
    let record_owned = record.clone();
    tokio::task::spawn_blocking(move || {
        storage_clone
            .save_intention(&record_owned)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("save_intention task panicked: {}", e))??;

    Ok(serde_json::json!({
        "success": true,
        "action": "set",
        "intentionId": id,
        "message": format!("Intention created: {}", description),
        "priority": priority,
        "triggerAt": trigger_at.map(|dt| dt.to_rfc3339()),
        "deadline": deadline.map(|dt| dt.to_rfc3339()),
        "nlpParsed": nlp_parsed,
    }))
}

//! First-sentence extraction and intention trigger evaluation.

use chrono::{DateTime, Duration, Utc};

use super::args::{ContextSpec, TriggerData};

/// Extract the first sentence or first line from content, capped at 150 chars.
pub(super) fn first_sentence(content: &str) -> String {
    let content = content.trim();
    let end = content
        .find(". ")
        .map(|i| i + 1)
        .or_else(|| content.find('\n'))
        .unwrap_or(content.len())
        .min(150);
    let end = content.floor_char_boundary(end);
    content[..end].to_string()
}

/// Check if an intention should be triggered based on the current context.
pub(super) fn check_intention_triggered(
    intention: &vestige_core::IntentionRecord,
    ctx: &ContextSpec,
    now: DateTime<Utc>,
) -> bool {
    let trigger: Option<TriggerData> = serde_json::from_str(&intention.trigger_data).ok();
    let Some(trigger) = trigger else {
        return false;
    };

    match trigger.trigger_type.as_deref() {
        Some("time") => {
            if let Some(ref at) = trigger.at
                && let Ok(trigger_time) = DateTime::parse_from_rfc3339(at)
            {
                return trigger_time.with_timezone(&Utc) <= now;
            }
            if let Some(mins) = trigger.in_minutes {
                let trigger_time = intention.created_at + Duration::minutes(mins);
                return trigger_time <= now;
            }
            false
        }
        Some("context") => {
            if let (Some(trigger_cb), Some(current_cb)) = (&trigger.codebase, &ctx.codebase)
                && current_cb
                    .to_lowercase()
                    .contains(&trigger_cb.to_lowercase())
            {
                return true;
            }
            if let (Some(pattern), Some(file)) = (&trigger.file_pattern, &ctx.file)
                && file.contains(pattern.as_str())
            {
                return true;
            }
            if let (Some(topic), Some(topics)) = (&trigger.topic, &ctx.topics)
                && topics
                    .iter()
                    .any(|t| t.to_lowercase().contains(&topic.to_lowercase()))
            {
                return true;
            }
            false
        }
        _ => false,
    }
}

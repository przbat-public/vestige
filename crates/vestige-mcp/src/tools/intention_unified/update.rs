//! "update" action — complete, snooze, or cancel an intention.

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::Value;

use vestige_core::Storage;

use super::args::UnifiedIntentionArgs;

/// Execute "update" action - complete, snooze, or cancel an intention
pub(super) async fn execute_update(
    storage: &Arc<Storage>,
    args: &UnifiedIntentionArgs,
) -> Result<Value, String> {
    let intention_id = args.id.as_ref().ok_or("Missing 'id' for update action")?;

    let status = args
        .status
        .as_ref()
        .ok_or("Missing 'status' for update action")?;

    let storage_update = storage.clone();
    let intention_id_owned = intention_id.clone();

    match status.as_str() {
        "complete" => {
            let updated = tokio::task::spawn_blocking(move || {
                storage_update
                    .update_intention_status(&intention_id_owned, "fulfilled")
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("update_intention_status task panicked: {}", e))??;

            if updated {
                Ok(serde_json::json!({
                    "success": true,
                    "action": "update",
                    "status": "complete",
                    "message": "Intention marked as complete",
                    "intentionId": intention_id,
                }))
            } else {
                Err(format!("Intention not found: {}", intention_id))
            }
        }
        "snooze" => {
            let minutes = args.snooze_minutes.unwrap_or(30);
            let snooze_until = Utc::now() + Duration::minutes(minutes);

            let storage_snooze = storage.clone();
            let intention_id_owned = intention_id.clone();
            let updated = tokio::task::spawn_blocking(move || {
                storage_snooze
                    .snooze_intention(&intention_id_owned, snooze_until)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("snooze_intention task panicked: {}", e))??;

            if updated {
                Ok(serde_json::json!({
                    "success": true,
                    "action": "update",
                    "status": "snooze",
                    "message": format!("Intention snoozed for {} minutes", minutes),
                    "intentionId": intention_id,
                    "snoozedUntil": snooze_until.to_rfc3339(),
                }))
            } else {
                Err(format!("Intention not found: {}", intention_id))
            }
        }
        "cancel" => {
            let storage_cancel = storage.clone();
            let intention_id_owned = intention_id.clone();
            let updated = tokio::task::spawn_blocking(move || {
                storage_cancel
                    .update_intention_status(&intention_id_owned, "cancelled")
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("update_intention_status task panicked: {}", e))??;

            if updated {
                Ok(serde_json::json!({
                    "success": true,
                    "action": "update",
                    "status": "cancel",
                    "message": "Intention cancelled",
                    "intentionId": intention_id,
                }))
            } else {
                Err(format!("Intention not found: {}", intention_id))
            }
        }
        _ => Err(format!(
            "Unknown status: '{}'. Valid statuses are: complete, snooze, cancel",
            status
        )),
    }
}

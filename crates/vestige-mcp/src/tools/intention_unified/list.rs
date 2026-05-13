//! "list" action — list intentions with optional filtering.

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use vestige_core::Storage;

use super::args::UnifiedIntentionArgs;

/// Execute "list" action - list intentions with optional filtering
pub(super) async fn execute_list(
    storage: &Arc<Storage>,
    args: &UnifiedIntentionArgs,
) -> Result<Value, String> {
    let filter_status = args.filter_status.as_deref().unwrap_or("active");

    let storage_clone = storage.clone();
    let filter_status_owned = filter_status.to_string();
    let intentions = tokio::task::spawn_blocking(
        move || -> Result<Vec<vestige_core::IntentionRecord>, String> {
            if filter_status_owned == "all" {
                let mut all = storage_clone
                    .get_active_intentions()
                    .map_err(|e| e.to_string())?;
                all.extend(
                    storage_clone
                        .get_intentions_by_status("fulfilled")
                        .map_err(|e| e.to_string())?,
                );
                all.extend(
                    storage_clone
                        .get_intentions_by_status("cancelled")
                        .map_err(|e| e.to_string())?,
                );
                all.extend(
                    storage_clone
                        .get_intentions_by_status("snoozed")
                        .map_err(|e| e.to_string())?,
                );
                Ok(all)
            } else if filter_status_owned == "active" {
                storage_clone
                    .get_active_intentions()
                    .map_err(|e| e.to_string())
            } else {
                storage_clone
                    .get_intentions_by_status(&filter_status_owned)
                    .map_err(|e| e.to_string())
            }
        },
    )
    .await
    .map_err(|e| format!("list intentions task panicked: {}", e))??;

    let limit = args.limit.unwrap_or(20) as usize;
    let now = Utc::now();

    let items: Vec<Value> = intentions
        .into_iter()
        .take(limit)
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

    Ok(serde_json::json!({
        "action": "list",
        "intentions": items,
        "total": items.len(),
        "status": filter_status,
    }))
}

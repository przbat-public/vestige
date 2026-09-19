//! Action dispatcher routing to get/get_batch/delete/state/promote/demote/edit/review.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::Storage;

use crate::cognitive::CognitiveEngine;

use super::actions::{
    execute_delete, execute_demote, execute_edit, execute_get, execute_get_batch, execute_promote,
    execute_review, execute_state,
};
use super::args::MemoryArgs;

/// Every action this tool accepts, in one place so the error message and the
/// schema cannot drift apart silently.
const ACTIONS: &str = "get, get_batch, delete, state, promote, demote, edit, review";

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    // Keep the raw object: the confirmation gate has to see the flag before
    // deserialization narrows the payload to known fields.
    let raw = args.clone().unwrap_or_else(|| serde_json::json!({}));

    let args: MemoryArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    // get_batch uses 'ids' array, all other actions use 'id'
    if args.action == "get_batch" {
        let ids = args.ids.ok_or("get_batch requires 'ids' array")?;
        if ids.is_empty() {
            return Err("ids array cannot be empty".to_string());
        }
        if ids.len() > 20 {
            return Err("get_batch supports max 20 IDs per call".to_string());
        }
        for id in &ids {
            uuid::Uuid::parse_str(id).map_err(|_| format!("Invalid memory ID format: {}", id))?;
        }
        return execute_get_batch(storage, &ids).await;
    }

    let id = args.id.ok_or("This action requires 'id' parameter")?;
    uuid::Uuid::parse_str(&id).map_err(|_| "Invalid memory ID format".to_string())?;

    match args.action.as_str() {
        "get" => execute_get(storage, &id).await,
        // The gate itself lives in `execute_delete` so the rule travels with the
        // destructor, not with this dispatcher.
        "delete" => execute_delete(storage, &id, crate::tools::common::is_confirmed(&raw)).await,
        "state" => execute_state(storage, &id).await,
        "promote" => execute_promote(storage, cognitive, &id, args.reason).await,
        "demote" => execute_demote(storage, cognitive, &id, args.reason).await,
        "edit" => execute_edit(storage, &id, args.content).await,
        "review" => execute_review(storage, &id, args.rating).await,
        _ => Err(format!(
            "Invalid action '{}'. Must be one of: {}",
            args.action, ACTIONS
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The action list in the error message must match the schema enum — a model
    /// that is told "must be one of …" without `review` will never call it.
    #[test]
    fn advertised_actions_cover_the_schema_enum() {
        let schema = super::super::schema::schema();
        for action in schema["properties"]["action"]["enum"].as_array().unwrap() {
            let action = action.as_str().unwrap();
            assert!(
                ACTIONS.contains(action),
                "schema advertises '{action}' but the dispatcher error omits it: {ACTIONS}"
            );
        }
    }
}

//! Top-level dispatcher routing actions to set/check/update/list.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use vestige_core::Storage;

use crate::cognitive::CognitiveEngine;

use super::args::UnifiedIntentionArgs;
use super::check::execute_check;
use super::list::execute_list;
use super::set::execute_set;
use super::update::execute_update;

/// Execute the unified intention tool
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: UnifiedIntentionArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    match args.action.as_str() {
        "set" => execute_set(storage, cognitive, &args).await,
        "check" => execute_check(storage, cognitive, &args).await,
        "update" => execute_update(storage, &args).await,
        "list" => execute_list(storage, &args).await,
        _ => Err(format!(
            "Unknown action: '{}'. Valid actions are: set, check, update, list",
            args.action
        )),
    }
}

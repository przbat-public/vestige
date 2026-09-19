//! Deprecated → unified tool name/argument rewriter.
//!
//! Older releases (`< v1.7`) exposed many fine-grained tools (`promote_memory`,
//! `recall`, `set_intention`, …) that have since been folded into a small set
//! of unified tools (`memory`, `search`, `intention`, …) with an `action`
//! discriminator.
//!
//! Rather than scatter the back-compat shims across `handle_tools_call`, every
//! deprecation lives here as a single match arm. The dispatcher calls
//! [`rewrite_deprecated`] before the main match: if the tool name was renamed
//! we log a warning, return the new name + transformed arguments, and the
//! dispatcher continues with the unified handler.
//!
//! Removing a deprecation is a one-line change. Adding one stays local to this
//! module — no change to the dispatch surface.

use serde_json::Value;
use tracing::warn;

/// Outcome of a deprecation lookup: either "use this new tool with these
/// rewritten args", or `None` (tool name was not a deprecated alias).
pub type Rewrite = Option<(&'static str, Option<Value>)>;

/// If `name` is a deprecated tool alias, log a warning and return the
/// `(new_name, rewritten_args)` to dispatch instead. Returns `None` for
/// non-deprecated tool names so the caller can fall through to the regular
/// dispatch.
pub fn rewrite_deprecated(name: &str, args: Option<Value>) -> Rewrite {
    match name {
        // ---- straight aliases (same args, new name) -------------------------
        "ingest" => warn_and_alias(name, "smart_ingest", args),
        "session_checkpoint" => {
            warn!(
                "Tool 'session_checkpoint' is deprecated in v1.7. Use 'smart_ingest' with 'items' parameter instead."
            );
            Some(("smart_ingest", args))
        }
        "recall" | "semantic_search" | "hybrid_search" => {
            warn!("Tool '{}' is deprecated. Use 'search' instead.", name);
            Some(("search", args))
        }
        "health_check" | "stats" => {
            warn!(
                "Tool '{}' is deprecated in v1.7. Use 'system_status' instead.",
                name
            );
            Some(("system_status", args))
        }

        // ---- insert action="..." into the existing args ---------------------
        "promote_memory" => warn_and_inject_action(name, "memory", "promote", args),
        "demote_memory" => warn_and_inject_action(name, "memory", "demote", args),
        "remember_pattern" => warn_and_inject_action(name, "codebase", "remember_pattern", args),
        "remember_decision" => warn_and_inject_action(name, "codebase", "remember_decision", args),
        "get_codebase_context" => warn_and_inject_action(name, "codebase", "get_context", args),
        "set_intention" => warn_and_inject_action(name, "intention", "set", args),
        "check_intentions" => warn_and_inject_action(name, "intention", "check", args),

        // ---- bespoke argument shapes ----------------------------------------
        "get_knowledge" => warn_and_action_with_id(name, "memory", "get", "id", args),
        "delete_knowledge" => warn_and_action_with_id(name, "memory", "delete", "id", args),
        "get_memory_state" => warn_and_action_with_id(name, "memory", "state", "memory_id", args),
        // `mark_reviewed` used to be dispatch-only: callable, but absent from both
        // `tools/list` and this table, so it mutated FSRS state as an invisible,
        // unannotated tool that `memory://due` nonetheless told the model to call.
        // It now rewrites onto the advertised `memory(action="review")` like every
        // other legacy name.
        "mark_reviewed" => warn_and_inject_action(name, "memory", "review", args),
        "complete_intention" => {
            warn!(
                "Tool 'complete_intention' is deprecated. Use 'intention' with action='update', status='complete' instead."
            );
            let id = args
                .as_ref()
                .and_then(|a| a.get("intentionId"))
                .cloned()
                .unwrap_or(Value::Null);
            Some((
                "intention",
                Some(serde_json::json!({
                    "action": "update",
                    "id": id,
                    "status": "complete"
                })),
            ))
        }
        "snooze_intention" => {
            warn!(
                "Tool 'snooze_intention' is deprecated. Use 'intention' with action='update', status='snooze' instead."
            );
            let id = args
                .as_ref()
                .and_then(|a| a.get("intentionId"))
                .cloned()
                .unwrap_or(Value::Null);
            let minutes = args
                .as_ref()
                .and_then(|a| a.get("minutes"))
                .cloned()
                .unwrap_or(serde_json::json!(30));
            Some((
                "intention",
                Some(serde_json::json!({
                    "action": "update",
                    "id": id,
                    "status": "snooze",
                    "snooze_minutes": minutes
                })),
            ))
        }
        "list_intentions" => {
            warn!(
                "Tool 'list_intentions' is deprecated. Use 'intention' with action='list' instead."
            );
            // status → filter_status (renamed field), keep the rest.
            let new_args = match args {
                Some(mut a) => {
                    if let Some(obj) = a.as_object_mut() {
                        obj.insert("action".to_string(), serde_json::json!("list"));
                        if let Some(status) = obj.remove("status") {
                            obj.insert("filter_status".to_string(), status);
                        }
                    }
                    Some(a)
                }
                None => Some(serde_json::json!({"action": "list"})),
            };
            Some(("intention", new_args))
        }

        _ => None,
    }
}

fn warn_and_alias(old: &str, new: &'static str, args: Option<Value>) -> Rewrite {
    warn!(
        "Tool '{}' is deprecated in v1.7. Use '{}' instead.",
        old, new
    );
    Some((new, args))
}

/// Inject `action: <action>` into the args object. If args were `None`, build
/// `{"action": <action>}` from scratch.
fn warn_and_inject_action(
    old: &str,
    new: &'static str,
    action: &'static str,
    args: Option<Value>,
) -> Rewrite {
    warn!(
        "Tool '{}' is deprecated. Use '{}' with action='{}' instead.",
        old, new, action
    );
    let new_args = match args {
        Some(mut a) => {
            if let Some(obj) = a.as_object_mut() {
                obj.insert("action".to_string(), serde_json::json!(action));
            }
            Some(a)
        }
        None => Some(serde_json::json!({ "action": action })),
    };
    Some((new, new_args))
}

/// Build `{"action": <action>, "id": <args[id_field]>}` for tools that took a
/// single id-shaped argument under a different field name.
fn warn_and_action_with_id(
    old: &str,
    new: &'static str,
    action: &'static str,
    id_field: &str,
    args: Option<Value>,
) -> Rewrite {
    warn!(
        "Tool '{}' is deprecated. Use '{}' with action='{}' instead.",
        old, new, action
    );
    let id = args
        .as_ref()
        .and_then(|a| a.get(id_field))
        .cloned()
        .unwrap_or(Value::Null);
    Some((
        new,
        Some(serde_json::json!({
            "action": action,
            "id": id,
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_tool_returns_none() {
        assert!(rewrite_deprecated("search", None).is_none());
        assert!(rewrite_deprecated("memory", Some(json!({"action": "get"}))).is_none());
        assert!(rewrite_deprecated("totally_made_up", None).is_none());
    }

    #[test]
    fn straight_alias_preserves_args() {
        let args = Some(json!({"foo": 42}));
        let (new_name, new_args) = rewrite_deprecated("ingest", args.clone()).unwrap();
        assert_eq!(new_name, "smart_ingest");
        assert_eq!(new_args, args);
    }

    #[test]
    fn hybrid_and_semantic_alias_to_search() {
        for old in ["recall", "semantic_search", "hybrid_search"] {
            let (new, _) = rewrite_deprecated(old, Some(json!({"q": "x"}))).unwrap();
            assert_eq!(new, "search");
        }
    }

    #[test]
    fn health_check_and_stats_alias_to_system_status() {
        for old in ["health_check", "stats"] {
            let (new, _) = rewrite_deprecated(old, None).unwrap();
            assert_eq!(new, "system_status");
        }
    }

    #[test]
    fn inject_action_into_existing_args() {
        let args = Some(json!({"id": "abc", "reason": "good"}));
        let (new_name, new_args) = rewrite_deprecated("promote_memory", args).unwrap();
        assert_eq!(new_name, "memory");
        let new_args = new_args.unwrap();
        assert_eq!(new_args["action"], "promote");
        assert_eq!(new_args["id"], "abc");
        assert_eq!(new_args["reason"], "good");
    }

    #[test]
    fn inject_action_with_no_args_builds_object() {
        let (new_name, new_args) = rewrite_deprecated("set_intention", None).unwrap();
        assert_eq!(new_name, "intention");
        assert_eq!(new_args.unwrap(), json!({"action": "set"}));
    }

    #[test]
    fn get_knowledge_extracts_id_field() {
        let args = Some(json!({"id": "node-123"}));
        let (new_name, new_args) = rewrite_deprecated("get_knowledge", args).unwrap();
        assert_eq!(new_name, "memory");
        assert_eq!(
            new_args.unwrap(),
            json!({"action": "get", "id": "node-123"})
        );
    }

    #[test]
    fn get_memory_state_renames_memory_id_to_id() {
        let args = Some(json!({"memory_id": "node-1"}));
        let (_new_name, new_args) = rewrite_deprecated("get_memory_state", args).unwrap();
        assert_eq!(
            new_args.unwrap(),
            json!({"action": "state", "id": "node-1"})
        );
    }

    #[test]
    fn complete_intention_builds_update_payload() {
        let args = Some(json!({"intentionId": "i-1", "extra": "ignored"}));
        let (new_name, new_args) = rewrite_deprecated("complete_intention", args).unwrap();
        assert_eq!(new_name, "intention");
        assert_eq!(
            new_args.unwrap(),
            json!({"action": "update", "id": "i-1", "status": "complete"})
        );
    }

    #[test]
    fn snooze_intention_uses_default_minutes_when_missing() {
        let args = Some(json!({"intentionId": "i-1"}));
        let (_, new_args) = rewrite_deprecated("snooze_intention", args).unwrap();
        let new_args = new_args.unwrap();
        assert_eq!(new_args["snooze_minutes"], 30);
        assert_eq!(new_args["status"], "snooze");
    }

    #[test]
    fn snooze_intention_preserves_explicit_minutes() {
        let args = Some(json!({"intentionId": "i-1", "minutes": 120}));
        let (_, new_args) = rewrite_deprecated("snooze_intention", args).unwrap();
        assert_eq!(new_args.unwrap()["snooze_minutes"], 120);
    }

    #[test]
    fn list_intentions_renames_status_to_filter_status() {
        let args = Some(json!({"status": "active"}));
        let (new_name, new_args) = rewrite_deprecated("list_intentions", args).unwrap();
        assert_eq!(new_name, "intention");
        let new_args = new_args.unwrap();
        assert_eq!(new_args["action"], "list");
        assert_eq!(new_args["filter_status"], "active");
        assert!(new_args.get("status").is_none());
    }

    #[test]
    fn list_intentions_with_no_args_defaults_to_action_list() {
        let (_, new_args) = rewrite_deprecated("list_intentions", None).unwrap();
        assert_eq!(new_args.unwrap(), json!({"action": "list"}));
    }
}

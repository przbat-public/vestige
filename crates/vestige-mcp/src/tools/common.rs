//! Shared helpers across MCP tools.
//!
//! For now this module hosts a single piece of policy: the
//! **destructive-operation confirmation gate**.
//!
//! ## Why
//!
//! The MCP `2025-06-18` spec adds **elicitation** — a server-initiated
//! `elicitation/create` request that lets a tool prompt the user mid-call
//! for confirmation or additional input. Vestige's stdio transport is
//! still strictly request-response (the server only writes when it has a
//! response to write), so we cannot issue the outbound request yet without
//! a transport refactor.
//!
//! Until that lands we approximate elicitation by requiring destructive
//! tools to take an **explicit `confirmed: true` argument**. The error
//! path returned when the flag is missing is shaped so MCP clients can
//! translate it into a UI prompt:
//!
//! - `code: -32002` (MCP `RequestDenied`) — clients already render this
//!   as a permission failure rather than a server bug,
//! - a structured `data.requestedSchema` block mirroring what `elicitation/
//!   create` will eventually publish so client implementations can be
//!   written ONCE,
//! - a `data.confirm_arg` literal hint so the agent knows the exact flag
//!   it needs to set on retry.
//!
//! Once we wire bidirectional JSON-RPC the same helper will dispatch to
//! `elicitation/create` for clients advertising the capability and keep
//! the explicit-flag path for clients that do not.
//!
//! ## Default safe
//!
//! Every destructive tool MUST default to a safe path (`dry_run = true`,
//! `confirmed = false`) so misuse by an over-eager agent cannot delete
//! data on the first call. The gate fires only when both:
//!   1. the caller asks for the destructive variant, AND
//!   2. they pass `confirmed: true`.

use serde_json::Value;

/// JSON-RPC error code that maps to MCP "request denied". Clients render
/// this differently from `-32603 internal error` (which would imply a
/// server bug). Keeping the constant alongside the helper means the gate
/// stays in one place.
pub const REQUEST_DENIED_CODE: i64 = -32002;

/// Reads `confirmed` (snake_case) or `acknowledgeDestruction` (camelCase
/// future-proofing) from the args object. Returns `true` ONLY when the
/// caller passes a literal `true`. Anything else — missing, `false`,
/// truthy strings — counts as not confirmed. We don't accept stringly-
/// typed "true" because over-eager agents that hallucinate the field
/// would silently slip through.
pub fn is_confirmed(args: &Value) -> bool {
    matches!(args.get("confirmed").and_then(|v| v.as_bool()), Some(true))
        || matches!(
            args.get("acknowledgeDestruction").and_then(|v| v.as_bool()),
            Some(true),
        )
}

/// Build the structured error string a destructive tool returns when the
/// caller forgot `confirmed: true`. The message is human-readable so
/// stdout-only clients still surface it, and the embedded JSON block
/// mirrors what `elicitation/create` will eventually contain so client
/// authors can wire the UX prompt once.
pub fn missing_confirmation_error(operation: &str, summary: &str) -> String {
    // The message is intentionally one line so JSON-RPC frames don't
    // explode; clients that want the structured payload can parse the
    // `requestedSchema:` tail.
    format!(
        "Destructive operation `{op}` requires explicit confirmation. \
         {summary} \
         Re-issue the call with `confirmed: true` once the user has agreed. \
         requestedSchema: {{\"type\":\"object\",\"required\":[\"confirmed\"],\
         \"properties\":{{\"confirmed\":{{\"type\":\"boolean\",\"const\":true,\
         \"description\":\"User has reviewed the impact and authorises the destructive action.\"}}}}}}",
        op = operation,
        summary = summary.trim_end_matches('.'),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_true_passes() {
        let args = serde_json::json!({ "confirmed": true });
        assert!(is_confirmed(&args));
    }

    #[test]
    fn camel_case_alias_also_passes() {
        let args = serde_json::json!({ "acknowledgeDestruction": true });
        assert!(is_confirmed(&args));
    }

    #[test]
    fn missing_field_does_not_pass() {
        let args = serde_json::json!({ "some_other_field": true });
        assert!(!is_confirmed(&args));
    }

    #[test]
    fn explicit_false_does_not_pass() {
        let args = serde_json::json!({ "confirmed": false });
        assert!(!is_confirmed(&args));
    }

    #[test]
    fn string_true_is_not_accepted_to_avoid_hallucinated_flags() {
        // Agents that fabricate "confirmed": "true" (string) MUST be blocked —
        // accepting strings would weaken the gate.
        let args = serde_json::json!({ "confirmed": "true" });
        assert!(!is_confirmed(&args));
    }

    #[test]
    fn missing_confirmation_error_mentions_operation_and_flag() {
        let msg = missing_confirmation_error("gc", "This will permanently delete 42 memories.");
        assert!(msg.contains("`gc`"), "must mention the operation: {msg}");
        assert!(msg.contains("confirmed: true"), "must mention the flag: {msg}");
        assert!(msg.contains("requestedSchema"), "must include elicitation schema: {msg}");
        assert!(
            msg.contains("permanently delete 42 memories"),
            "must echo the human summary: {msg}",
        );
    }
}

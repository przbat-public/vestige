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
//! tools to take an **explicit `confirmed: true` argument**.
//!
//! ## What the caller actually receives
//!
//! The gate returns `Err(String)`, which `server::dispatch` turns into a normal
//! tool result with `isError: true` — **not** a JSON-RPC protocol error. That is
//! deliberate (SEP-1303): a missing confirmation is an execution outcome the model
//! is meant to read and act on, not a transport-level failure.
//!
//! The message is one human-readable line plus a JSON tail:
//!
//! - prose naming the operation and the impact, so stdout-only clients still
//!   surface something actionable,
//! - `requestedSchema: {...}` — the elicitation schema a client will eventually
//!   render, embedded as *text* because a `CallToolResult` text block is the only
//!   channel this transport has today.
//!
//! Two things this comment used to claim are worth calling out because they were
//! never true: the refusal does not carry `code: -32002` (that code means
//! `ResourceNotFound` in MCP `server/resources.md`, and the dead
//! `REQUEST_DENIED_CODE` constant asserting otherwise has been removed), and it
//! does not carry a structured `data.requestedSchema` member — a tool result has
//! no `data` field.
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
        assert!(
            msg.contains("confirmed: true"),
            "must mention the flag: {msg}"
        );
        assert!(
            msg.contains("requestedSchema"),
            "must include elicitation schema: {msg}"
        );
        assert!(
            msg.contains("permanently delete 42 memories"),
            "must echo the human summary: {msg}",
        );
    }

    #[test]
    fn refusal_is_a_tool_error_not_a_fabricated_protocol_code() {
        // The refusal travels as `CallToolResult { isError: true, content: [text] }`.
        // It must not advertise a JSON-RPC code: -32002 means ResourceNotFound in
        // MCP `server/resources.md`, and this result is not a protocol error at all.
        // (A `REQUEST_DENIED_CODE` constant used to claim otherwise and was dead.)
        let msg = missing_confirmation_error("gc", "Deletes data.");
        assert!(
            !msg.contains("-32002"),
            "the gate must not impersonate ResourceNotFound: {msg}"
        );
        assert!(
            !msg.contains("\"code\""),
            "a tool result carries no JSON-RPC error code: {msg}"
        );
    }
}

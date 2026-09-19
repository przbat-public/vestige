//! JSON schema for the unified `memory` MCP tool.

use serde_json::Value;

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["get", "get_batch", "delete", "state", "promote", "demote", "edit", "review"],
                "description": "Action to perform: 'get' retrieves full memory node, 'get_batch' retrieves multiple memories by IDs (use 'ids' array), 'delete' removes memory (requires confirmed: true), 'state' returns accessibility state, 'promote' increases retrieval strength (thumbs up), 'demote' decreases retrieval strength (thumbs down), 'edit' updates content in-place (preserves FSRS state), 'review' records a spaced-repetition review of a due memory (use 'rating')"
            },
            "id": {
                "type": "string",
                "description": "The ID of the memory node (required for every action except get_batch)"
            },
            "ids": {
                "type": "array",
                "items": { "type": "string" },
                "maxItems": 20,
                "description": "Array of memory IDs (for get_batch action). Max 20 IDs per call."
            },
            "reason": {
                "type": "string",
                "description": "Why this memory is being promoted/demoted (optional, for logging). Only used with promote/demote actions."
            },
            "content": {
                "type": "string",
                "description": "New content for edit action. Replaces existing content, regenerates embedding, preserves FSRS state."
            },
            "rating": {
                "type": "integer",
                "minimum": 1,
                "maximum": 4,
                "description": "FSRS rating for the review action: 1=Again (forgot), 2=Hard, 3=Good (default), 4=Easy"
            },
            "confirmed": {
                "type": "boolean",
                "description": "Set to true to authorise the destructive 'delete' action. Without it, delete is refused and nothing is removed."
            }
        },
        "required": ["action"],
        // Per-action requirements, in the schema rather than only in the runtime
        // (`execute::execute` rejects exactly these, but only after a round-trip):
        //   * `id`      — everything except get_batch
        //   * `ids`     — get_batch
        //   * `content` — edit
        // `confirmed` is deliberately NOT required for delete: the gate answers a
        // refused call with an actionable message, which is the elicitation flow.
        "allOf": [
            {
                "if": { "properties": { "action": { "const": "get_batch" } }, "required": ["action"] },
                "then": { "required": ["ids"] },
                "else": { "required": ["id"] }
            },
            {
                "if": { "properties": { "action": { "const": "edit" } }, "required": ["action"] },
                "then": { "required": ["content"] }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_action_and_rating_bounds_are_declared() {
        // Added when `mark_reviewed` (dispatch-only, absent from tools/list) was
        // folded into this tool: `memory://due` tells the model to complete reviews,
        // so the action it names must exist in the advertised schema.
        let schema = schema();
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(
            actions.contains(&serde_json::json!("review")),
            "review must be advertised: {actions:?}"
        );

        let rating = &schema["properties"]["rating"];
        assert_eq!(rating["type"], "integer");
        assert_eq!(rating["minimum"], 1);
        assert_eq!(rating["maximum"], 4);
    }

    #[test]
    fn get_batch_ids_are_capped_in_the_schema_not_only_at_runtime() {
        // Runtime already rejects 21+ IDs; declaring the cap lets the model avoid
        // the round-trip instead of learning from an error.
        assert_eq!(schema()["properties"]["ids"]["maxItems"], 20);
    }

    #[test]
    fn per_action_requirements_are_declared() {
        // The audit's complaint: `required` was just ["action"], so the schema said
        // nothing about `id`/`ids`/`content` and the model learned by failing.
        let schema = schema();
        let all_of = schema["allOf"].as_array().expect("allOf declared");
        assert_eq!(all_of.len(), 2);

        assert_eq!(
            all_of[0]["if"]["properties"]["action"]["const"],
            serde_json::json!("get_batch")
        );
        assert_eq!(all_of[0]["then"]["required"], serde_json::json!(["ids"]));
        assert_eq!(
            all_of[0]["else"]["required"],
            serde_json::json!(["id"]),
            "every non-batch action needs an id"
        );

        assert_eq!(
            all_of[1]["if"]["properties"]["action"]["const"],
            serde_json::json!("edit")
        );
        assert_eq!(
            all_of[1]["then"]["required"],
            serde_json::json!(["content"])
        );
    }

    #[test]
    fn delete_declares_confirmed_without_requiring_it() {
        // Requiring it would turn the confirmation flow into a client-side validation
        // error, hiding the explanatory refusal the gate returns.
        let schema = schema();
        assert_eq!(schema["properties"]["confirmed"]["type"], "boolean");
        assert!(
            !schema["allOf"]
                .as_array()
                .unwrap()
                .iter()
                .any(|clause| clause["then"]["required"]
                    .as_array()
                    .is_some_and(|req| req.contains(&serde_json::json!("confirmed")))),
            "confirmed must stay optional at the schema level"
        );
    }
}

//! JSON schema for the `smart_ingest` MCP tool (single + batch modes).

use serde_json::Value;

/// Node types the store accepts (`vestige_core` node types).
///
/// Declared as an enum so a typo is rejected by the client's own validator instead
/// of being written into the database as a junk `node_type` — the runtime never
/// validated this field.
const NODE_TYPES: [&str; 8] = [
    "fact", "concept", "event", "person", "place", "note", "pattern", "decision",
];

/// Input schema for smart_ingest tool
///
/// Supports two modes:
/// - **Single mode**: provide `content` (required) + optional fields
/// - **Batch mode**: provide `items` array (max 20), each with full cognitive pipeline
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "content": {
                "type": "string",
                "description": "The content to remember. MUST be atomic: one fact, one decision, one event per call. Multi-topic content triggers compound_content_warning — split into batch items instead. (Single mode)"
            },
            "node_type": {
                "type": "string",
                "enum": NODE_TYPES,
                "description": "Type of knowledge: fact, concept, event, person, place, note, pattern, decision",
                "default": "fact"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Tags for categorization"
            },
            "source": {
                "type": "string",
                "description": "Source or reference for this knowledge"
            },
            "forceCreate": {
                "type": "boolean",
                "description": "Force creation of a new memory even if similar content exists",
                "default": false
            },
            "session_id": {
                "type": "string",
                "description": "Session/conversation identifier for provenance tracking"
            },
            "agent": {
                "type": "string",
                "description": "Agent identifier (e.g. 'cursor', 'claude') for provenance tracking"
            },
            "items": {
                "type": "array",
                "description": "Batch mode: array of items to save (max 20). Each runs through full cognitive pipeline with Prediction Error Gating. Use at session end or before context compaction.",
                "maxItems": 20,
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The content to remember"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tags for categorization"
                        },
                        "node_type": {
                            "type": "string",
                            "enum": NODE_TYPES,
                            "description": "Type: fact, concept, event, person, place, note, pattern, decision",
                            "default": "fact"
                        },
                        "source": {
                            "type": "string",
                            "description": "Source reference"
                        },
                        "forceCreate": {
                            "type": "boolean",
                            "description": "Force creation of this item even if similar content exists",
                            "default": false
                        }
                    },
                    "required": ["content"]
                }
            }
        },
        // Exactly one of the two modes, stated in the schema rather than only in
        // prose: a call with neither `content` nor `items` used to reach the handler
        // and fail there, costing the caller a round-trip.
        "anyOf": [
            { "required": ["content"] },
            { "required": ["items"] }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_type_is_an_enum_in_both_modes() {
        let schema = schema();
        let expected: Vec<Value> = NODE_TYPES.iter().map(|t| serde_json::json!(t)).collect();

        assert_eq!(
            schema["properties"]["node_type"]["enum"],
            Value::Array(expected.clone())
        );
        assert_eq!(
            schema["properties"]["items"]["items"]["properties"]["node_type"]["enum"],
            Value::Array(expected),
            "batch items must accept exactly the same types as the single mode"
        );
    }

    #[test]
    fn one_of_content_or_items_is_required() {
        let schema = schema();
        let any_of = schema["anyOf"].as_array().expect("anyOf declared");
        assert_eq!(any_of.len(), 2);
        assert_eq!(any_of[0]["required"], serde_json::json!(["content"]));
        assert_eq!(any_of[1]["required"], serde_json::json!(["items"]));
    }

    #[test]
    fn batch_size_is_capped_in_the_schema() {
        // Mirrors the runtime check (`items.len() > 20` -> error), so a model can
        // see the cap before spending a call on it.
        assert_eq!(schema()["properties"]["items"]["maxItems"], 20);
    }
}

//! JSON schema for the `smart_ingest` MCP tool (single + batch modes).

use serde_json::Value;

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
        }
    })
}

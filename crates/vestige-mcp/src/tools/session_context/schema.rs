//! JSON schema for the session_context tool.

use serde_json::Value;

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "queries": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Search queries to run (default: [\"user preferences\"])"
            },
            "token_budget": {
                "type": "integer",
                "description": "Max tokens for response (default: 1000). Server truncates content to fit budget. With 1M context models, budgets up to 100K are practical.",
                "default": 1000,
                "minimum": 100,
                "maximum": 100000
            },
            "context": {
                "type": "object",
                "description": "Current context for intention matching and predictions",
                "properties": {
                    "codebase": { "type": "string" },
                    "topics": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "file": { "type": "string" }
                }
            },
            "include_status": {
                "type": "boolean",
                "description": "Include system health info (default: true)",
                "default": true
            },
            "include_intentions": {
                "type": "boolean",
                "description": "Include triggered intentions (default: true)",
                "default": true
            },
            "include_predictions": {
                "type": "boolean",
                "description": "Include memory predictions (default: true)",
                "default": true
            }
        }
    })
}

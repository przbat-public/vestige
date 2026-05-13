//! JSON schema for the unified `search` MCP tool.

use serde_json::Value;

/// Input schema for unified search tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of results (default: 10)",
                "default": 10,
                "minimum": 1,
                "maximum": 100
            },
            "min_retention": {
                "type": "number",
                "description": "Minimum retention strength (0.0-1.0, default: 0.0)",
                "default": 0.0,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "min_similarity": {
                "type": "number",
                "description": "Minimum similarity threshold (0.0-1.0, default: 0.5)",
                "default": 0.5,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "detail_level": {
                "type": "string",
                "description": "Level of detail in results. 'brief' = id/type/tags/score only (saves tokens). 'summary' = default 8-field response. 'full' = all fields including FSRS state, timestamps, and provenance metadata.",
                "enum": ["brief", "summary", "full"],
                "default": "summary"
            },
            "context_topics": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional topics for context-dependent retrieval boosting"
            },
            "token_budget": {
                "type": "integer",
                "description": "Max tokens for response. Server truncates content to fit budget. Use memory(action='get') for full content of specific IDs. With 1M context models, budgets up to 100K are practical.",
                "minimum": 100,
                "maximum": 100000
            },
            "retrieval_mode": {
                "type": "string",
                "description": "precise: top results only (fast, token-efficient, skips activation/competition). balanced: full cognitive pipeline (default). exhaustive: maximum recall with 5x overfetch, deep graph traversal, no competition suppression.",
                "enum": ["precise", "balanced", "exhaustive"],
                "default": "balanced"
            }
        },
        "required": ["query"]
    })
}

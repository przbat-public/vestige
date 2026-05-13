//! JSON schema for the unified `memory` MCP tool.

use serde_json::Value;

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["get", "get_batch", "delete", "state", "promote", "demote", "edit"],
                "description": "Action to perform: 'get' retrieves full memory node, 'get_batch' retrieves multiple memories by IDs (use 'ids' array), 'delete' removes memory, 'state' returns accessibility state, 'promote' increases retrieval strength (thumbs up), 'demote' decreases retrieval strength (thumbs down), 'edit' updates content in-place (preserves FSRS state)"
            },
            "id": {
                "type": "string",
                "description": "The ID of the memory node (for single-memory actions)"
            },
            "ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Array of memory IDs (for get_batch action). Max 20 IDs per call."
            },
            "reason": {
                "type": "string",
                "description": "Why this memory is being promoted/demoted (optional, for logging). Only used with promote/demote actions."
            },
            "content": {
                "type": "string",
                "description": "New content for edit action. Replaces existing content, regenerates embedding, preserves FSRS state."
            }
        },
        "required": ["action"]
    })
}

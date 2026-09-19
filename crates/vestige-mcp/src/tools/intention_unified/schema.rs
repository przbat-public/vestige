//! JSON schema for the unified `intention` MCP tool.

use serde_json::Value;

/// Unified schema for the `intention` tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "description": "Unified intention management tool. Supports setting, checking, updating (complete/snooze/cancel), and listing intentions.",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["set", "check", "update", "list"],
                "description": "The action to perform: 'set' creates a new intention, 'check' finds triggered intentions, 'update' modifies status (complete/snooze/cancel), 'list' shows intentions"
            },
            // SET action parameters
            "description": {
                "type": "string",
                "description": "[set] What to remember to do"
            },
            "trigger": {
                "type": "object",
                "description": "[set] When to trigger this intention",
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["time", "context", "event"],
                        "description": "Trigger type: time-based, context-based, or event-based"
                    },
                    "at": {
                        "type": "string",
                        "description": "ISO timestamp for time-based triggers"
                    },
                    "in_minutes": {
                        "type": "integer",
                        "description": "Minutes from now for duration-based triggers"
                    },
                    "codebase": {
                        "type": "string",
                        "description": "Trigger when working in this codebase"
                    },
                    "file_pattern": {
                        "type": "string",
                        "description": "Trigger when editing files matching this pattern"
                    },
                    "topic": {
                        "type": "string",
                        "description": "Trigger when discussing this topic"
                    },
                    "condition": {
                        "type": "string",
                        "description": "Natural language condition for event triggers"
                    }
                }
            },
            "priority": {
                "type": "string",
                "enum": ["low", "normal", "high", "critical"],
                "default": "normal",
                "description": "[set] Priority level"
            },
            "deadline": {
                "type": "string",
                "description": "[set] Optional deadline (ISO timestamp)"
            },
            // UPDATE action parameters
            "id": {
                "type": "string",
                "description": "[update] ID of the intention to update"
            },
            "status": {
                "type": "string",
                "enum": ["complete", "snooze", "cancel"],
                "description": "[update] New status: 'complete' marks as fulfilled, 'snooze' delays, 'cancel' cancels"
            },
            "snooze_minutes": {
                "type": "integer",
                "default": 30,
                "description": "[update] Minutes to snooze for (when status is 'snooze')"
            },
            // CHECK action parameters
            "context": {
                "type": "object",
                "description": "[check] Current context for matching intentions",
                "properties": {
                    "current_time": {
                        "type": "string",
                        "description": "Current ISO timestamp (defaults to now)"
                    },
                    "codebase": {
                        "type": "string",
                        "description": "Current codebase/project name"
                    },
                    "file": {
                        "type": "string",
                        "description": "Current file path"
                    },
                    "topics": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Current discussion topics"
                    }
                }
            },
            "include_snoozed": {
                "type": "boolean",
                "default": false,
                "description": "[check] Include snoozed intentions"
            },
            // LIST action parameters
            "filter_status": {
                "type": "string",
                "enum": ["active", "fulfilled", "cancelled", "snoozed", "all"],
                "default": "active",
                "description": "[list] Filter by status"
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "default": 20,
                "description": "[list] Maximum number to return (1-100; values outside the range are clamped)"
            }
        },
        "required": ["action"]
    })
}

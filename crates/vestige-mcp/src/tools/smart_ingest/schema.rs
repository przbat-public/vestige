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

/// What every memory has to satisfy, stated in the schema because a model reads
/// the schema before it writes: one fact, understandable without the
/// conversation, and worth a future decision.
///
/// The last two sentences are not style advice — the write gate enforces them
/// (`self_contained` warning, `decision: "reject"`), so a caller that has not
/// read them spends a call on content that is flagged or refused.
const CONTENT_CONTRACT: &str = "The content to remember. MUST be atomic: one fact, one decision, one event per call. \
     MUST stand alone: name what and whom it is about (not \"the fix\", \"as discussed\", \"he\"), \
     and give absolute dates instead of \"yesterday\" or \"next week\". \
     State the future decision this changes; if it only describes the code, the repository is a \
     better place for it. File paths and line numbers stay out of the content — pass them in \
     `codeRefs` instead, because a stored path rots silently: it keeps resolving to an old copy, \
     or vanishes without a word. Any path left in the content is anchored automatically from it, \
     with symbol=null, so it can at least be checked. \
     Content the repository already owns (a code block, a directory tree, copied source, a version \
     number, a coverage figure) is refused with decision=\"reject\" and nothing is written; \
     everything fixable is written and flagged with a self_contained warning. \
     Multi-topic content triggers compound_content_warning — split it into batch items.";

/// The anchor contract, stated once and reused by both modes.
const CODE_REF_CONTRACT: &str = "Code this memory is about, as a checkable reference rather than a path in the text. \
     Each entry is either the string \"path@commit#symbol\" (the commit and the symbol are what \
     make it re-checkable; a bare \"path\", or \"path:line\", is accepted and stored without a \
     revision, which means it can never be verified) or an object {path, commit, symbol, line, \
     repo}. The symbol is resolved against the recorded commit and the hash of ITS body is \
     stored — not the file's — so the anchor survives a refactor that moved it and still reports \
     when the code it names changed. Verdicts (fresh/stale/orphaned/unchecked) appear in search \
     results. Pass paths you know the revision of; paths named in the content are anchored \
     automatically but can only carry what the text says.";

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
                "description": format!("{} (Single mode)", CONTENT_CONTRACT)
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
            "codeRefs": {
                "type": "array",
                "description": format!("{} (Single mode)", CODE_REF_CONTRACT),
                "items": {
                    "oneOf": [
                        { "type": "string" },
                        {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "commit": { "type": "string", "description": "Revision SHA the memory was written against" },
                                "symbol": { "type": "string", "description": "Fully-qualified symbol, e.g. Storage::embeddings_fingerprint" },
                                "line": { "type": "integer", "description": "Where the symbol sat — a hint, never a locator" },
                                "repo": { "type": "string", "description": "Remote URL or local path of the repository" }
                            },
                            "required": ["path"]
                        }
                    ]
                }
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
                            "description": format!("{} Each refused item comes back with status \"rejected\" and is not written.", CONTENT_CONTRACT)
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
                        },
                        "codeRefs": {
                            "type": "array",
                            "description": CODE_REF_CONTRACT,
                            "items": {
                                "oneOf": [
                                    { "type": "string" },
                                    {
                                        "type": "object",
                                        "properties": {
                                            "path": { "type": "string" },
                                            "commit": { "type": "string" },
                                            "symbol": { "type": "string" },
                                            "line": { "type": "integer" },
                                            "repo": { "type": "string" }
                                        },
                                        "required": ["path"]
                                    }
                                ]
                            }
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

    /// The content contract is the part of the schema a caller has to obey, and
    /// the gate enforces it. Dropping it would leave a model discovering the
    /// rules by having its memories refused, so if someone trims these
    /// descriptions back to "The content to remember", this fails.
    #[test]
    fn both_modes_state_the_content_contract() {
        let schema = schema();
        let single = schema["properties"]["content"]["description"]
            .as_str()
            .expect("single-mode content description");
        let batch = schema["properties"]["items"]["items"]["properties"]["content"]["description"]
            .as_str()
            .expect("batch item content description");

        for (mode, description) in [("single", single), ("batch", batch)] {
            for required in [
                "atomic",
                "stand alone",
                "decision",
                "reject",
                "self_contained",
            ] {
                assert!(
                    description.contains(required),
                    "{mode} mode content description must state '{required}': {description}"
                );
            }
        }
    }
}

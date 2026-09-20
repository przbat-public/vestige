//! Unified Codebase Tool
//!
//! Merges remember_pattern, remember_decision, and get_codebase_context into a single
//! `codebase` tool with action-based dispatch.
//!
//! Both write actions run the same two write-time steps every other write path
//! runs — code anchors, then the self-containedness gate over the text that will
//! be stored — through `smart_ingest::prepare`. Calling it rather than re-doing
//! it here is the point: the gate and the anchors are two halves of one check,
//! and a path that does one without the other is what `knowledge_nodes
//! .self_contained` being NULL for every memory written here used to mean.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use crate::tools::smart_ingest::{FileRefs, prepare};
use vestige_core::memory::DecisionPayload;
use vestige_core::{IngestInput, Storage};

/// Input schema for the unified codebase tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["remember_pattern", "remember_decision", "remember_decision_v2", "get_context"],
                "description": "Action to perform: 'remember_pattern' stores a code pattern, 'remember_decision' stores an architectural decision (legacy Markdown), 'remember_decision_v2' stores a structured decision matrix (question + choices + criteria + score matrix), 'get_context' retrieves patterns and decisions for a codebase"
            },
            // remember_pattern fields
            "name": {
                "type": "string",
                "description": "Name/title for the pattern (required for remember_pattern)"
            },
            "description": {
                "type": "string",
                "description": "Detailed description of the pattern (required for remember_pattern)"
            },
            // remember_decision fields
            "decision": {
                "type": "string",
                "description": "The architectural or design decision made (required for remember_decision)"
            },
            "rationale": {
                "type": "string",
                "description": "Why this decision was made (required for remember_decision)"
            },
            "alternatives": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Alternatives that were considered (optional for remember_decision)"
            },
            // remember_decision_v2 fields (structured matrix payload — see DecisionPayload)
            "question": {
                "type": "string",
                "description": "(remember_decision_v2) What was being decided? Becomes the card title."
            },
            "choices": {
                "type": "array",
                "description": "(remember_decision_v2) Alternatives considered. Exactly one entry must have chosen=true. Min 2 entries.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Stable identifier within the payload." },
                        "label": { "type": "string", "description": "Human-readable label." },
                        "summary": { "type": "string", "description": "Short distinguishing summary (optional)." },
                        "chosen": { "type": "boolean", "description": "True for the selected choice. Exactly one." }
                    },
                    "required": ["id", "label"]
                }
            },
            "criteria": {
                "type": "array",
                "description": "(remember_decision_v2) Evaluation axes. Used by the radar chart.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "label": { "type": "string" },
                        "weight": { "type": "number", "description": "Relative weight, defaults to 1.0. Must be >= 0." }
                    },
                    "required": ["id", "label"]
                }
            },
            "scoreMatrix": {
                "type": "object",
                "description": "(remember_decision_v2) Sparse 1-5 score matrix keyed by criterion id, then by choice id. Missing cells render as '—'.",
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": { "type": "integer", "minimum": 1, "maximum": 5 }
                }
            },
            "validUntil": {
                "type": "string",
                "format": "date-time",
                "description": "(remember_decision_v2) Optional RFC3339 timestamp. When the date passes the decision is auto-flagged as stale by reflect."
            },
            "supersedes": {
                "type": "array",
                "items": { "type": "string" },
                "description": "(remember_decision_v2) IDs of decisions this one replaces."
            },
            // Shared fields
            "files": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Files where this pattern is used or affected by this decision. Stored as code anchors, not as text: the path is re-checked against the revision it was written at and each result carries a verdict (fresh/stale/orphaned/unchecked), so a reader can tell that the code has moved on. Use 'path@commit#symbol' when you know the revision and the symbol. Never repeat these paths in the decision or the description — a path inside the text cannot be re-checked."
            },
            "codebase": {
                "type": "string",
                "description": "Codebase/project identifier (e.g., 'vestige-tauri')"
            },
            // get_context fields
            "limit": {
                "type": "integer",
                "description": "Maximum items per category (default: 10, for get_context)",
                "default": 10
            }
        },
        "required": ["action"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodebaseArgs {
    action: String,
    // Pattern fields
    name: Option<String>,
    description: Option<String>,
    // Decision fields (legacy remember_decision)
    decision: Option<String>,
    rationale: Option<String>,
    alternatives: Option<Vec<String>>,
    // Decision matrix fields (remember_decision_v2)
    question: Option<String>,
    choices: Option<Vec<vestige_core::memory::Choice>>,
    criteria: Option<Vec<vestige_core::memory::Criterion>>,
    #[serde(default)]
    score_matrix: Option<std::collections::HashMap<String, std::collections::HashMap<String, u8>>>,
    valid_until: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    supersedes: Option<Vec<String>>,
    // Shared fields
    files: Option<Vec<String>>,
    codebase: Option<String>,
    // Context fields
    limit: Option<i32>,
}

/// Execute the unified codebase tool
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: CodebaseArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    match args.action.as_str() {
        "remember_pattern" => execute_remember_pattern(storage, cognitive, &args).await,
        "remember_decision" => execute_remember_decision(storage, cognitive, &args).await,
        "remember_decision_v2" => execute_remember_decision_v2(storage, cognitive, &args).await,
        "get_context" => execute_get_context(storage, cognitive, &args).await,
        _ => Err(format!(
            "Invalid action '{}'. Must be one of: remember_pattern, remember_decision, remember_decision_v2, get_context",
            args.action
        )),
    }
}

/// The heading of a stored decision, derived from the decision text.
///
/// The heading is what a reader, a search preview and a dashboard list all show
/// first, so it has to be a title rather than the first fifty bytes of a
/// sentence: a byte slice cut Polish text mid-word and, because nothing marked
/// the cut, read as a complete — and wrong — statement.
///
/// Three rules, in order: the first sentence when it is short enough to *be* a
/// title, otherwise the longest prefix that still ends on a word boundary, and
/// an ellipsis whenever text was left out. Character-based, not byte-based, for
/// the same reason: cutting between two bytes of a character is not a
/// truncation, it is mojibake.
fn decision_title(decision: &str) -> String {
    const LIMIT: usize = 60;

    let text = decision.trim();
    let first_sentence = first_sentence(text);

    // A first sentence that ends early is the better title even when it is
    // shorter than the limit, but only when it is complete: `first_sentence`
    // hands back the whole text when no terminator was found, and a title that
    // ran past the limit is not one.
    let title: &str = match first_sentence {
        Some(sentence) if sentence.chars().count() <= LIMIT => sentence,
        _ if text.chars().count() <= LIMIT => text,
        // Cut at the last word boundary inside the limit rather than at the
        // limit itself, and fall back to the hard cut when the text has no
        // whitespace inside the limit at all — a single long token — because a
        // title of nothing would be worse than a mid-token one.
        _ => match text
            .char_indices()
            .take(LIMIT + 1)
            .filter(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i)
            .last()
        {
            Some(boundary) => &text[..boundary],
            None => &text[..text.floor_char_boundary(text.len().min(LIMIT * 4))],
        },
    };

    if title.trim_end().chars().count() < text.chars().count() {
        format!("{}…", title.trim_end())
    } else {
        title.trim_end().to_string()
    }
}

/// The first sentence of `text`, if it contains one.
///
/// A terminator only ends a sentence when whitespace follows it, which is what
/// keeps a decision such as "use v1.2" from being titled "use v1.2" cut at the
/// dot of a version. The terminator is kept: a title that dropped the full stop
/// would read as an unfinished phrase.
///
/// `None` when the text is one unbroken sentence, which is the caller's signal
/// to fall back to a length cut.
fn first_sentence(text: &str) -> Option<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (position, (offset, character)) in chars.iter().enumerate() {
        if !matches!(character, '.' | '!' | '?') {
            continue;
        }
        let ends_here = match chars.get(position + 1) {
            Some((_, next)) => next.is_whitespace(),
            None => true,
        };
        if ends_here {
            return Some(&text[..offset + character.len_utf8()]);
        }
    }
    None
}

/// Remember a code pattern
async fn execute_remember_pattern(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &CodebaseArgs,
) -> Result<Value, String> {
    let name = args
        .name
        .as_ref()
        .ok_or("'name' is required for remember_pattern action")?;
    let description = args
        .description
        .as_ref()
        .ok_or("'description' is required for remember_pattern action")?;

    if name.trim().is_empty() {
        return Err("Pattern name cannot be empty".to_string());
    }

    // Build content with structured format. The affected files are deliberately
    // absent: they travel as code anchors instead, where a reader can see
    // whether the file is still the one the pattern was written about. A path in
    // the text would answer that question wrongly and silently.
    let content = format!("# Code Pattern: {}\n\n{}", name, description);

    // Build tags
    let mut tags = vec!["pattern".to_string(), "codebase".to_string()];
    if let Some(ref codebase) = args.codebase {
        tags.push(format!("codebase:{}", codebase));
    }

    let prepared = prepare(&content, FileRefs::from(args.files.as_ref()), false);
    if let Some(refusal) = prepared.refusal {
        return Ok(refusal);
    }

    let input = IngestInput {
        content,
        node_type: "pattern".to_string(),
        source: args.codebase.clone(),
        sentiment_score: 0.0,
        sentiment_magnitude: 0.0,
        tags,
        valid_from: None,
        valid_until: None,
        provenance: None,
        self_contained: Some(prepared.marker),
        self_contained_findings: prepared.findings.clone(),
        anchors: prepared.anchors.clone(),
        ..Default::default()
    };

    let storage_clone = storage.clone();
    let node =
        tokio::task::spawn_blocking(move || storage_clone.ingest(input).map_err(|e| e.to_string()))
            .await
            .map_err(|e| format!("remember_pattern task panicked: {}", e))??;
    let node_id = node.id.clone();

    // ====================================================================
    // COGNITIVE: Cross-project pattern recording
    // ====================================================================
    if let Ok(cog) = cognitive.try_lock() {
        let codebase_name = args.codebase.as_deref().unwrap_or("default");
        cog.cross_project
            .record_project_memory(&node_id, codebase_name, None);

        // Also index in hippocampal index for fast retrieval
        let _ = cog.hippocampal_index.index_memory(
            &node_id,
            &format!("{}: {}", name, description),
            "pattern",
            chrono::Utc::now(),
            None,
        );
    } else {
        crate::cognitive::try_lock_metrics::record_miss("codebase_pattern");
    }

    let mut response = serde_json::json!({
        "action": "remember_pattern",
        "success": true,
        "nodeId": node_id,
        "patternName": name,
        "message": format!("Pattern '{}' remembered successfully", name),
    });
    if let Some(anchors) = prepared.response_anchors() {
        response["anchors"] = anchors;
    }
    if let Some(marker) = prepared.response_marker() {
        response["self_contained"] = marker;
    }
    Ok(response)
}

/// Remember an architectural decision
async fn execute_remember_decision(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &CodebaseArgs,
) -> Result<Value, String> {
    let decision = args
        .decision
        .as_ref()
        .ok_or("'decision' is required for remember_decision action")?;
    let rationale = args
        .rationale
        .as_ref()
        .ok_or("'rationale' is required for remember_decision action")?;

    if decision.trim().is_empty() {
        return Err("Decision cannot be empty".to_string());
    }

    // Build content with structured format (ADR-like). The title is derived, not
    // sliced: the heading is what a reader and a search preview see first, so it
    // has to end on a word and say when it was cut.
    let mut content = format!(
        "# Decision: {}\n\n## Context\n\n{}\n\n## Decision\n\n{}",
        decision_title(decision),
        rationale,
        decision
    );

    if let Some(ref alternatives) = args.alternatives
        && !alternatives.is_empty()
    {
        content.push_str("\n\n## Alternatives Considered:\n");
        for alt in alternatives {
            content.push_str(&format!("- {}\n", alt));
        }
    }

    // Build tags
    let mut tags = vec![
        "decision".to_string(),
        "architecture".to_string(),
        "codebase".to_string(),
    ];
    if let Some(ref codebase) = args.codebase {
        tags.push(format!("codebase:{}", codebase));
    }

    // The affected files left the content on purpose: they are the anchors below,
    // where the store can re-check them and a reader can see the verdict instead
    // of trusting a path that rots without a word.
    let prepared = prepare(&content, FileRefs::from(args.files.as_ref()), false);
    if let Some(refusal) = prepared.refusal {
        return Ok(refusal);
    }

    let input = IngestInput {
        content,
        node_type: "decision".to_string(),
        source: args.codebase.clone(),
        sentiment_score: 0.0,
        sentiment_magnitude: 0.0,
        tags,
        valid_from: None,
        valid_until: None,
        provenance: None,
        self_contained: Some(prepared.marker),
        self_contained_findings: prepared.findings.clone(),
        anchors: prepared.anchors.clone(),
        ..Default::default()
    };

    let storage_clone = storage.clone();
    let node =
        tokio::task::spawn_blocking(move || storage_clone.ingest(input).map_err(|e| e.to_string()))
            .await
            .map_err(|e| format!("remember_decision task panicked: {}", e))??;
    let node_id = node.id.clone();

    // ====================================================================
    // COGNITIVE: Cross-project decision recording
    // ====================================================================
    if let Ok(cog) = cognitive.try_lock() {
        let codebase_name = args.codebase.as_deref().unwrap_or("default");
        cog.cross_project
            .record_project_memory(&node_id, codebase_name, None);

        // Index in hippocampal index
        let _ = cog.hippocampal_index.index_memory(
            &node_id,
            &format!("Decision: {}", decision),
            "decision",
            chrono::Utc::now(),
            None,
        );
    } else {
        crate::cognitive::try_lock_metrics::record_miss("codebase_decision");
    }

    let mut response = serde_json::json!({
        "action": "remember_decision",
        "success": true,
        "nodeId": node_id,
        "message": "Architectural decision remembered successfully",
    });
    if let Some(anchors) = prepared.response_anchors() {
        response["anchors"] = anchors;
    }
    if let Some(marker) = prepared.response_marker() {
        response["self_contained"] = marker;
    }
    Ok(response)
}

/// Remember a structured decision matrix (Proposal C).
///
/// Validates the payload with [`DecisionPayload::validate`] *before* writing,
/// so malformed matrices never reach storage. On success the structured
/// payload is persisted under `extra_json.decision` and a human-readable
/// Markdown rendering is stored in `content` so the regular search and
/// dashboard views keep working without knowing about the matrix shape.
async fn execute_remember_decision_v2(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &CodebaseArgs,
) -> Result<Value, String> {
    let question = args
        .question
        .as_ref()
        .ok_or("'question' is required for remember_decision_v2 action")?
        .trim()
        .to_string();
    if question.is_empty() {
        return Err("Question cannot be empty".to_string());
    }
    let rationale = args
        .rationale
        .as_ref()
        .ok_or("'rationale' is required for remember_decision_v2 action")?
        .clone();
    let choices = args
        .choices
        .clone()
        .ok_or("'choices' is required for remember_decision_v2 action")?;
    let criteria = args.criteria.clone().unwrap_or_default();
    let score_matrix = args.score_matrix.clone().unwrap_or_default();

    let payload = DecisionPayload {
        question: question.clone(),
        rationale: rationale.clone(),
        choices,
        criteria,
        score_matrix,
        valid_until: args.valid_until,
        supersedes: args.supersedes.clone().unwrap_or_default(),
    };
    payload
        .validate()
        .map_err(|e| format!("Invalid decision payload: {}", e))?;

    let chosen_label = payload
        .chosen_choice()
        .map(|c| c.label.clone())
        .unwrap_or_else(|| "<unknown>".to_string());

    // Markdown rendering of the matrix. Stored in `content` so legacy search
    // and dashboard list views surface a useful preview even without
    // knowing about the structured payload.
    let mut content = format!(
        "# Decision: {}\n\n**Chosen:** {}\n\n## Rationale\n\n{}\n\n## Choices\n",
        question, chosen_label, rationale
    );
    for c in &payload.choices {
        let marker = if c.chosen { "✓" } else { " " };
        let summary = c.summary.as_deref().unwrap_or("");
        content.push_str(&format!("- [{marker}] **{}** — {}\n", c.label, summary));
    }
    if !payload.criteria.is_empty() {
        content.push_str("\n## Criteria\n");
        for cr in &payload.criteria {
            content.push_str(&format!("- {} (weight {:.1})\n", cr.label, cr.weight));
        }
    }
    // The gate and the anchors run here too. They were left out of this branch
    // while the version rule refused any `N.N` token, which made every criterion
    // ("(weight 1.5)") look like a product version — a matrix that says what was
    // decided and why is exactly the memory worth keeping, so refusing it was
    // the rule's fault, not the matrix's. With the rule narrowed to shapes that
    // are actually version claims, this path gets the same contract as the other
    // two: flagged is written and marked, refused writes nothing.
    let prepared = prepare(&content, FileRefs::from(args.files.as_ref()), false);
    if let Some(refusal) = prepared.refusal {
        return Ok(refusal);
    }

    let mut tags = vec![
        "decision".to_string(),
        "decision-matrix".to_string(),
        "architecture".to_string(),
        "codebase".to_string(),
    ];
    if let Some(ref codebase) = args.codebase {
        tags.push(format!("codebase:{}", codebase));
    }

    let extra_json = serde_json::json!({ "decision": payload });

    let input = IngestInput {
        content,
        node_type: "decision".to_string(),
        source: args.codebase.clone(),
        tags,
        extra_json: Some(extra_json),
        self_contained: Some(prepared.marker),
        self_contained_findings: prepared.findings.clone(),
        anchors: prepared.anchors.clone(),
        ..Default::default()
    };

    let storage_clone = storage.clone();
    let node =
        tokio::task::spawn_blocking(move || storage_clone.ingest(input).map_err(|e| e.to_string()))
            .await
            .map_err(|e| format!("remember_decision_v2 task panicked: {}", e))??;
    let node_id = node.id.clone();

    if let Ok(cog) = cognitive.try_lock() {
        let codebase_name = args.codebase.as_deref().unwrap_or("default");
        cog.cross_project
            .record_project_memory(&node_id, codebase_name, None);
        let _ = cog.hippocampal_index.index_memory(
            &node_id,
            &format!("Decision: {}", question),
            "decision",
            chrono::Utc::now(),
            None,
        );
    } else {
        crate::cognitive::try_lock_metrics::record_miss("codebase_decision_v2");
    }

    Ok(serde_json::json!({
        "action": "remember_decision_v2",
        "success": true,
        "nodeId": node_id,
        "question": question,
        "chosen": chosen_label,
        "message": "Structured decision matrix remembered successfully",
        "anchors": prepared.response_anchors(),
        "self_contained": prepared.response_marker(),
    }))
}

/// Get codebase context (patterns and decisions)
async fn execute_get_context(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: &CodebaseArgs,
) -> Result<Value, String> {
    let limit = args.limit.unwrap_or(10).clamp(1, 50);

    // Build tag filter for codebase
    let tag_filter = args.codebase.as_ref().map(|cb| format!("codebase:{}", cb));

    // Query patterns by node_type and tag
    let patterns = storage
        .get_nodes_by_type_and_tag("pattern", tag_filter.as_deref(), limit)
        .unwrap_or_default();

    // Query decisions by node_type and tag
    let decisions = storage
        .get_nodes_by_type_and_tag("decision", tag_filter.as_deref(), limit)
        .unwrap_or_default();

    let formatted_patterns: Vec<Value> = patterns
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content,
                "tags": n.tags,
                "retentionStrength": n.retention_strength,
                "createdAt": n.created_at.to_rfc3339(),
            })
        })
        .collect();

    let formatted_decisions: Vec<Value> = decisions
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content,
                "tags": n.tags,
                "retentionStrength": n.retention_strength,
                "createdAt": n.created_at.to_rfc3339(),
            })
        })
        .collect();

    // ====================================================================
    // COGNITIVE: Cross-project knowledge discovery
    // ====================================================================
    let mut universal_patterns = Vec::new();
    if let Some(codebase_name) = &args.codebase {
        match cognitive.try_lock() {
            Ok(cog) => {
                let context = vestige_core::advanced::cross_project::ProjectContext {
                    path: None,
                    name: Some(codebase_name.clone()),
                    languages: Vec::new(),
                    frameworks: Vec::new(),
                    file_types: std::collections::HashSet::new(),
                    dependencies: Vec::new(),
                    structure: Vec::new(),
                };
                let applicable = cog.cross_project.detect_applicable(&context);
                for knowledge in applicable {
                    universal_patterns.push(serde_json::json!({
                        "pattern": format!("{:?}", knowledge),
                    }));
                }
            }
            Err(_) => {
                crate::cognitive::try_lock_metrics::record_miss("codebase_get_context");
            }
        }
    }

    Ok(serde_json::json!({
        "action": "get_context",
        "codebase": args.codebase,
        "patterns": {
            "count": formatted_patterns.len(),
            "items": formatted_patterns,
        },
        "decisions": {
            "count": formatted_decisions.len(),
            "items": formatted_decisions,
        },
        "crossProjectInsights": universal_patterns,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_structure() {
        let schema = schema();
        assert!(schema["properties"]["action"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["action"]));

        // Check action enum values
        let action_enum = &schema["properties"]["action"]["enum"];
        assert!(
            action_enum
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("remember_pattern"))
        );
        assert!(
            action_enum
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("remember_decision"))
        );
        assert!(
            action_enum
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("get_context"))
        );
        assert!(
            action_enum
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("remember_decision_v2"))
        );
    }

    // === INTEGRATION TESTS ===

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    async fn test_storage() -> (Arc<Storage>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    #[tokio::test]
    async fn test_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_invalid_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "invalid" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid action"));
    }

    #[tokio::test]
    async fn test_remember_pattern_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_pattern",
            "name": "Error Handling Pattern",
            "description": "Use Result<T, E> with custom error types",
            "files": ["src/lib.rs"],
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "remember_pattern");
        assert_eq!(value["success"], true);
        assert!(value["nodeId"].is_string());
        assert_eq!(value["patternName"], "Error Handling Pattern");
    }

    #[tokio::test]
    async fn test_remember_pattern_missing_name_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_pattern",
            "description": "Some description"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'name' is required"));
    }

    #[tokio::test]
    async fn test_remember_pattern_missing_description_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_pattern",
            "name": "Test Pattern"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'description' is required"));
    }

    #[tokio::test]
    async fn test_remember_pattern_empty_name_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_pattern",
            "name": "   ",
            "description": "Some description"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_remember_decision_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "Use SQLite for storage",
            "rationale": "Embedded, no separate server needed",
            "alternatives": ["PostgreSQL", "Redis"],
            "files": ["src/storage/sqlite/nodes.rs"],
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "remember_decision");
        assert_eq!(value["success"], true);
        assert!(value["nodeId"].is_string());
    }

    #[tokio::test]
    async fn test_remember_decision_missing_decision_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "rationale": "Some rationale"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'decision' is required"));
    }

    #[tokio::test]
    async fn test_remember_decision_missing_rationale_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "Use SQLite"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'rationale' is required"));
    }

    #[tokio::test]
    async fn test_remember_decision_empty_decision_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "  ",
            "rationale": "Something"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_remember_decision_v2_persists_structured_payload() {
        let (storage, _dir) = test_storage().await;
        let cog = test_cognitive();
        let args = serde_json::json!({
            "action": "remember_decision_v2",
            "question": "Which database engine?",
            "rationale": "Postgres has the best JSONB story.",
            "choices": [
                { "id": "postgres", "label": "PostgreSQL", "summary": "Native JSONB", "chosen": true },
                { "id": "mysql", "label": "MySQL", "chosen": false }
            ],
            "criteria": [
                { "id": "performance", "label": "Read performance", "weight": 1.5 },
                { "id": "ops", "label": "Operational complexity" }
            ],
            "scoreMatrix": {
                "performance": { "postgres": 4, "mysql": 3 },
                "ops": { "postgres": 4, "mysql": 4 }
            },
            "codebase": "vestige",
            "files": ["src/absent_fixture_v2.rs"]
        });
        let result = execute(&storage, &cog, Some(args)).await.unwrap();
        assert_eq!(result["action"], "remember_decision_v2");
        assert_eq!(result["chosen"], "PostgreSQL");

        let node_id = result["nodeId"].as_str().unwrap().to_string();
        let storage_clone = storage.clone();
        let node_for_anchors = node_id.clone();
        let node = tokio::task::spawn_blocking(move || storage_clone.get_node(&node_id))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let payload = vestige_core::memory::extract_decision(node.extra_json.as_ref())
            .expect("extra_json.decision should round-trip");
        assert_eq!(payload.question, "Which database engine?");
        assert_eq!(payload.choices.len(), 2);
        assert_eq!(payload.chosen_choice().unwrap().id, "postgres");
        assert_eq!(payload.criteria.len(), 2);
        assert_eq!(payload.score_matrix.len(), 2);
        assert!(node.tags.iter().any(|t| t == "decision-matrix"));

        // This branch runs the same pre-write step as the other two actions:
        // `files` become anchors rather than text, and the gate's verdict is
        // persisted. The criterion above renders as "(weight 1.5)" — the shape
        // the version rule used to read as a product version, which is why this
        // branch had no gate at all until that rule was narrowed.
        assert!(
            !node.content.contains("Affected Files") && !node.content.contains("absent_fixture_v2"),
            "a matrix must carry its files as anchors, not as text: {}",
            node.content
        );
        assert!(
            node.self_contained.is_some(),
            "the gate ran on this content and everyone must be able to see what it said"
        );
        let anchors = storage.code_refs_for(&node_for_anchors).unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].anchor.path, "src/absent_fixture_v2.rs");
    }

    #[tokio::test]
    async fn test_remember_decision_v2_rejects_invalid_payload() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision_v2",
            "question": "Which?",
            "rationale": "Because",
            "choices": [
                { "id": "a", "label": "A", "chosen": false },
                { "id": "b", "label": "B", "chosen": false }
            ]
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid decision payload"));
        assert!(err.contains("exactly one choice"));
    }

    #[tokio::test]
    async fn test_remember_decision_v2_requires_question() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision_v2",
            "rationale": "x",
            "choices": [
                { "id": "a", "label": "A", "chosen": true },
                { "id": "b", "label": "B", "chosen": false }
            ]
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'question' is required"));
    }

    // === SELF-CONTAINEDNESS GATE (write path) ===

    /// The marker the gate exists to leave, asked of the file rather than of a
    /// read path that might filter.
    ///
    /// `NULL` in that column means "never checked", so a memory written through
    /// this path has to come back with a verdict — `true` when the gate found
    /// nothing, `false` when it flagged the memory as needing its conversation.
    fn stored_marker(dir: &tempfile::TempDir, node_id: &str) -> Option<bool> {
        store_conn(dir)
            .query_row(
                "SELECT self_contained FROM knowledge_nodes WHERE id = ?1",
                rusqlite::params![node_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .unwrap()
            .map(|v| v == 1)
    }

    /// A direct connection to the store file, for assertions about what was
    /// written. The `Storage` writer is still open in WAL mode, which permits a
    /// second reader.
    fn store_conn(dir: &tempfile::TempDir) -> rusqlite::Connection {
        rusqlite::Connection::open(dir.path().join("test.db")).unwrap()
    }

    #[tokio::test]
    async fn test_decisions_run_the_self_containedness_gate() {
        let (storage, dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "Vestige keeps memories in SQLite instead of a separate server",
            "rationale": "The server ships as one binary, so an embedded engine removes an \
                          operational dependency that had no owner",
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let node_id = result["nodeId"].as_str().unwrap().to_string();

        assert_eq!(
            stored_marker(&dir, &node_id),
            Some(true),
            "the gate ran and found nothing, so the marker must say so: {result}"
        );
        assert!(
            result.get("self_contained").is_none(),
            "a clean memory must not carry a marker in the response: {result}"
        );
    }

    #[tokio::test]
    async fn test_patterns_run_the_self_containedness_gate() {
        let (storage, dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_pattern",
            "name": "Errors as values",
            "description": "Prefer Result over panicking when the caller can recover",
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let node_id = result["nodeId"].as_str().unwrap().to_string();

        assert_eq!(
            stored_marker(&dir, &node_id),
            Some(true),
            "the gate ran and found nothing, so the marker must say so: {result}"
        );
    }

    /// A flagged memory is written and marked, with the marker on the response
    /// exactly as `smart_ingest` reports it — the finding names the rule, the
    /// text that fired it and what to write instead.
    #[tokio::test]
    async fn test_a_flagged_decision_is_written_and_marked() {
        let (storage, dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "Reject the migration the way we discussed last week",
            "rationale": "The staging box dropped the write lock under load",
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let node_id = result["nodeId"].as_str().unwrap().to_string();

        assert_eq!(
            result["success"], true,
            "a fixable memory is written: {result}"
        );
        assert_eq!(
            result["self_contained"]["requiresContext"], true,
            "{result}"
        );
        assert_eq!(result["self_contained"]["rejected"], false, "{result}");
        assert!(
            result["self_contained"]["findings"]
                .as_array()
                .is_some_and(|f| !f.is_empty()),
            "the marker must carry the findings: {result}"
        );
        assert_eq!(
            stored_marker(&dir, &node_id),
            Some(false),
            "a flagged memory stores the flag, not NULL"
        );
    }

    /// The gate's one non-negotiable outcome: content the repository owns is
    /// refused, and nothing is written. `write_preparation` runs before the
    /// ingest for exactly this reason.
    #[tokio::test]
    async fn test_a_decision_the_repository_owns_writes_nothing() {
        let (storage, dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "Store the loader like this:\n```rust\nfn load() {}\n```",
            "rationale": "The snippet speaks for itself",
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        assert_eq!(result["decision"], "reject", "{result}");
        assert_eq!(result["stored"], false, "{result}");
        assert!(
            result["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "a refusal must say why: {result}"
        );
        assert!(
            result["findings"].as_array().is_some_and(|f| !f.is_empty()),
            "a refusal must carry the findings: {result}"
        );

        let count: i64 = store_conn(&dir)
            .query_row("SELECT COUNT(*) FROM knowledge_nodes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "a refused decision must not reach the store");
    }

    // === FILES AS CODE ANCHORS ===

    /// A path this store keeps as prose rots silently; an anchor can be
    /// re-checked. So `files` becomes an anchor — and only an anchor.
    #[tokio::test]
    async fn test_files_become_code_anchors_not_content() {
        let (storage, dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "Vestige keeps the ADR renderer in one module",
            "rationale": "Both the decision and the pattern tools need the same heading",
            "files": ["src/absent_fixture.rs"],
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let node_id = result["nodeId"].as_str().unwrap().to_string();

        let conn = store_conn(&dir);
        let content: String = conn
            .query_row(
                "SELECT content FROM knowledge_nodes WHERE id = ?1",
                rusqlite::params![node_id],
                |row| row.get(0),
            )
            .unwrap();
        let (path, commit, verdict): (String, Option<String>, String) = conn
            .query_row(
                "SELECT path, commit_sha, verdict FROM code_refs WHERE node_id = ?1",
                rusqlite::params![node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(path, "src/absent_fixture.rs");
        assert_eq!(
            commit, None,
            "a commit is never invented for an unresolvable anchor"
        );
        assert_eq!(
            verdict, "unchecked",
            "no checkout of that path exists, so the honest verdict is 'not verified'"
        );
        assert!(
            !content.contains("src/absent_fixture.rs") && !content.contains("Affected Files"),
            "a stored path is fail-silent in both directions: {content}"
        );
    }

    #[tokio::test]
    async fn test_pattern_files_become_code_anchors_not_content() {
        let (storage, dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "remember_pattern",
            "name": "Errors as values",
            "description": "Prefer Result over panicking when the caller can recover",
            "files": ["src/absent_fixture.rs"],
            "codebase": "vestige"
        });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let node_id = result["nodeId"].as_str().unwrap().to_string();

        let conn = store_conn(&dir);
        let content: String = conn
            .query_row(
                "SELECT content FROM knowledge_nodes WHERE id = ?1",
                rusqlite::params![node_id],
                |row| row.get(0),
            )
            .unwrap();
        let path: String = conn
            .query_row(
                "SELECT path FROM code_refs WHERE node_id = ?1",
                rusqlite::params![node_id],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(path, "src/absent_fixture.rs");
        assert!(
            !content.contains("src/absent_fixture.rs") && !content.contains("## Files"),
            "the pattern description must not carry the file list: {content}"
        );
    }

    /// The precondition for dropping the `## Affected Files` list from the
    /// content: a reader must still be able to see which files a decision
    /// touched. Search is the path a reader meets a memory on, so the check runs
    /// end to end — file in, anchor stored, anchor out of a search result —
    /// rather than against the `code_refs` table that this file already
    /// writes.
    #[tokio::test]
    async fn test_search_surfaces_the_files_a_decision_was_anchored_to() {
        let (storage, _dir) = test_storage().await;
        let cog = test_cognitive();
        let args = serde_json::json!({
            "action": "remember_decision",
            "decision": "Vestige keeps the ADR renderer in one module",
            "rationale": "Both the decision and the pattern tools need the same heading",
            "files": ["src/absent_fixture.rs"],
            "codebase": "vestige"
        });
        let node_id = execute(&storage, &cog, Some(args)).await.unwrap()["nodeId"]
            .as_str()
            .unwrap()
            .to_string();

        let found = crate::tools::search_unified::execute(
            &storage,
            &cog,
            Some(serde_json::json!({ "query": "ADR renderer module", "limit": 5 })),
        )
        .await
        .unwrap();

        let result = found["results"]
            .as_array()
            .expect("search must return a results array")
            .iter()
            .find(|r| r["id"] == serde_json::json!(node_id))
            .unwrap_or_else(|| panic!("the decision must be retrievable: {found}"));

        assert_eq!(
            result["codeRefs"][0]["path"], "src/absent_fixture.rs",
            "a reader must see the files the decision touched: {result}"
        );
        assert_eq!(result["codeRefs"][0]["verdict"], "unchecked", "{result}");
    }

    // === DECISION TITLE ===

    /// The heading is the first thing a reader — and a search result — sees. A
    /// byte slice of the decision text cut Polish text mid-word, with no
    /// ellipsis to say it had been cut at all.
    ///
    /// The subject itself carries a sentence, so this also pins which of the two
    /// rules fires: a first sentence short enough to be a title is preferred
    /// over a cut, and the full stop is kept.
    #[test]
    fn test_decision_title_cuts_on_a_word_boundary() {
        let title = decision_title(
            "Materiał zaczyna się od stanowiska pracy. Instalacja i konfiguracja narzędzia \
             zajmuje pół dnia, bo zależności są wersjonowane osobno",
        );

        assert!(
            title.ends_with('…'),
            "a cut title must say it was cut: {title:?}"
        );
        assert!(
            !title.contains("instal"),
            "the cut must land on a word boundary, not inside a word: {title:?}"
        );
        assert_eq!(title, "Materiał zaczyna się od stanowiska pracy.…");
    }

    /// A decision with no sentence break is cut on a word boundary rather than
    /// at the character limit, which is the shape that produced
    /// `# Decision: Materiał zaczyna się od stanowiska pracy: instal`.
    #[test]
    fn test_decision_title_cuts_a_sentence_with_no_terminator() {
        let title = decision_title(
            "Materiał zaczyna się od stanowiska pracy: instalacja i konfiguracja narzędzia",
        );

        assert_eq!(
            title,
            "Materiał zaczyna się od stanowiska pracy: instalacja i…"
        );
        assert!(
            !title.ends_with("instal…"),
            "half a word is not a title: {title:?}"
        );
    }

    #[test]
    fn test_decision_title_prefers_a_short_first_sentence() {
        assert_eq!(
            decision_title("Use SQLite. It is embedded, so there is no server to operate."),
            "Use SQLite.…"
        );
        assert_eq!(decision_title("Use SQLite"), "Use SQLite");
        assert_eq!(
            decision_title("Use SQLite.\nBecause it is embedded."),
            "Use SQLite.…"
        );
    }

    #[tokio::test]
    async fn test_get_context_empty() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "get_context",
            "codebase": "nonexistent"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "get_context");
        assert_eq!(value["patterns"]["count"], 0);
        assert_eq!(value["decisions"]["count"], 0);
    }

    #[tokio::test]
    async fn test_get_context_retrieves_saved_patterns() {
        let (storage, _dir) = test_storage().await;
        let cog = test_cognitive();
        // Save a pattern first
        let save_args = serde_json::json!({
            "action": "remember_pattern",
            "name": "Test Pattern",
            "description": "A test pattern",
            "codebase": "myproject"
        });
        execute(&storage, &cog, Some(save_args)).await.unwrap();

        // Now retrieve
        let get_args = serde_json::json!({
            "action": "get_context",
            "codebase": "myproject"
        });
        let result = execute(&storage, &cog, Some(get_args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value["patterns"]["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_get_context_no_codebase() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "get_context" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "get_context");
        assert!(value["codebase"].is_null());
    }
}

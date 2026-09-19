//! Unified Codebase Tool
//!
//! Merges remember_pattern, remember_decision, and get_codebase_context into a single
//! `codebase` tool with action-based dispatch.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
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
                "description": "Files where this pattern is used or affected by this decision"
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

    // Build content with structured format
    let mut content = format!("# Code Pattern: {}\n\n{}", name, description);

    if let Some(ref files) = args.files
        && !files.is_empty()
    {
        content.push_str("\n\n## Files:\n");
        for f in files {
            content.push_str(&format!("- {}\n", f));
        }
    }

    // Build tags
    let mut tags = vec!["pattern".to_string(), "codebase".to_string()];
    if let Some(ref codebase) = args.codebase {
        tags.push(format!("codebase:{}", codebase));
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

    Ok(serde_json::json!({
        "action": "remember_pattern",
        "success": true,
        "nodeId": node_id,
        "patternName": name,
        "message": format!("Pattern '{}' remembered successfully", name),
    }))
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

    // Build content with structured format (ADR-like)
    let mut content = format!(
        "# Decision: {}\n\n## Context\n\n{}\n\n## Decision\n\n{}",
        &decision[..decision.floor_char_boundary(50)],
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

    if let Some(ref files) = args.files
        && !files.is_empty()
    {
        content.push_str("\n\n## Affected Files:\n");
        for f in files {
            content.push_str(&format!("- {}\n", f));
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

    Ok(serde_json::json!({
        "action": "remember_decision",
        "success": true,
        "nodeId": node_id,
        "message": "Architectural decision remembered successfully",
    }))
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
    if let Some(ref files) = args.files
        && !files.is_empty()
    {
        content.push_str("\n## Affected Files\n");
        for f in files {
            content.push_str(&format!("- {}\n", f));
        }
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
            "codebase": "vestige"
        });
        let result = execute(&storage, &cog, Some(args)).await.unwrap();
        assert_eq!(result["action"], "remember_decision_v2");
        assert_eq!(result["chosen"], "PostgreSQL");

        let node_id = result["nodeId"].as_str().unwrap().to_string();
        let storage_clone = storage.clone();
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

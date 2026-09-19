//! `erase` tool — GDPR Article 17 hard-deletion of a memory or a tag.
//!
//! ## Why this tool exists
//!
//! `Storage::right_to_erasure` and `Storage::erase_by_tag` have been in core
//! for a long time, and they are the *only* deletion paths that remove the
//! derived data a memory leaves behind: connections, embeddings, the access
//! log, state transitions, and every insight whose `source_memories` names the
//! memory. `memory(action="delete")` and `gc` do not: they drop the node row
//! and leave the rest behind, so a subject who exercises Art. 17 was still
//! described by insights derived from the erased memory.
//!
//! Nothing exposed those two functions — not MCP, not REST, not the CLI — so
//! the documented "GDPR-style complete removal" had no product surface. This
//! module is that surface.
//!
//! ## Safety model
//!
//! Erasure is irreversible, so the tool is safe by default:
//!
//! 1. `dry_run` defaults to `true`. The default call reports the exact number
//!    of matching memories plus up to [`MAX_REPORTED_IDS`] of their ids and
//!    deletes nothing.
//! 2. The destructive variant additionally requires `confirmed: true`
//!    ([`crate::tools::common::is_confirmed`]) — the same gate `gc` and
//!    `memory(action="delete")` use. A missing acknowledgement is a refusal,
//!    never a partial erasure.
//!
//! Both flags are deliberately *not* typed into [`EraseArgs`]: `confirmed` is
//! read from the raw JSON so a hallucinated boolean can never be logged as a
//! typed parameter, and `dry_run` is resolved before the gate runs.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use vestige_core::Storage;

use crate::tools::common;

/// How many memory ids a report carries before it is truncated.
///
/// A tag erasure can match an entire database; echoing every id into a tool
/// result would burn the client's token budget for no gain. The count is
/// always exact — only the id list is capped.
pub const MAX_REPORTED_IDS: usize = 100;

/// JSON schema for the `erase` MCP tool.
///
/// Per-action requirements are declared here (not only enforced at runtime) so
/// a model can see that `action="memory"` needs `id` and `action="tag"` needs
/// `tag` without a failed round-trip. `confirmed` stays optional: the gate
/// answers a refused call with an actionable message, which is the
/// elicitation flow — see `tools::common`.
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["memory", "tag"],
                "description": "Erasure scope: 'memory' hard-deletes one memory by id, 'tag' hard-deletes every memory carrying that exact tag."
            },
            "id": {
                "type": "string",
                "minLength": 1,
                "maxLength": 64,
                "description": "Memory id to erase. Required when action='memory'."
            },
            "tag": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "description": "Tag to erase. Required when action='tag'. Matched as an exact tag element after the same canonicalization the writers apply (trim, lowercase, spaces/underscores to '-'), so 'code' never matches 'codebase'."
            },
            "dry_run": {
                "type": "boolean",
                "default": true,
                "description": "If true (default), report the count and ids that WOULD be erased and delete nothing."
            },
            "confirmed": {
                "type": "boolean",
                "default": false,
                "description": "Required to be `true` when `dry_run` is `false`. Acts as an explicit acknowledgement that the irreversible erasure has been authorised. Mirrors the MCP `elicitation/create` flow for transports that cannot prompt mid-call."
            }
        },
        "required": ["action"],
        "allOf": [
            {
                "if": { "properties": { "action": { "const": "memory" } }, "required": ["action"] },
                "then": { "required": ["id"] }
            },
            {
                "if": { "properties": { "action": { "const": "tag" } }, "required": ["action"] },
                "then": { "required": ["tag"] }
            }
        ]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EraseArgs {
    action: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    // camelCase is the primary wire spelling; the snake_case alias keeps the
    // tool callable with `dry_run`, which is what every neighbouring tool and
    // the CLI use.
    #[serde(default, alias = "dry_run")]
    dry_run: Option<bool>,
}

/// Why an erase request did not run.
///
/// Typed rather than a bare `String` because the three cases map to different
/// HTTP statuses in the dashboard router. Matching on the message text to pick
/// a status code is how a confirmation refusal ends up reported as a 500 —
/// i.e. as a server fault instead of a client that skipped the gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EraseError {
    /// `confirmed: true` was not supplied for the destructive variant.
    Refused(String),
    /// The arguments are unusable: unknown action, missing/blank id or tag.
    Invalid(String),
    /// The storage layer failed. Core runs each erasure in one transaction,
    /// so this means nothing was deleted (or everything rolled back).
    Storage(String),
}

impl std::fmt::Display for EraseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EraseError::Refused(msg) | EraseError::Invalid(msg) | EraseError::Storage(msg) => {
                f.write_str(msg)
            }
        }
    }
}

impl std::error::Error for EraseError {}

/// What an erase call did — or, on a dry run, what it would have done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EraseOutcome {
    /// `"memory"` or `"tag"`.
    pub action: &'static str,
    pub dry_run: bool,
    /// Number of memories erased (or matching, for a dry run). Exact, even
    /// when `ids` is truncated.
    pub matched: u64,
    /// The memory ids, capped at [`MAX_REPORTED_IDS`].
    pub ids: Vec<String>,
    /// True when `ids` is shorter than `matched`.
    pub ids_truncated: bool,
    /// Rows removed across every table (connections, embeddings, access log,
    /// states, insights, node). Zero on a dry run.
    pub artifacts_erased: i64,
}

impl EraseOutcome {
    /// One-line human summary of what happened (or would happen).
    ///
    /// Shared by the MCP payload and the dashboard DTO so the two surfaces
    /// cannot describe the same outcome differently.
    pub fn message(&self) -> String {
        if self.dry_run {
            format!(
                "{} memories would be erased. Re-issue with dry_run=false and confirmed=true to erase them.",
                self.matched
            )
        } else {
            format!(
                "Erased {} memories and {} related artifacts.",
                self.matched, self.artifacts_erased
            )
        }
    }

    /// Tool-result payload. `tool` is echoed for parity with `gc`/`backup`;
    /// the dashboard DTO ignores it.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "tool": "erase",
            "action": self.action,
            "dryRun": self.dry_run,
            "matched": self.matched,
            "ids": self.ids,
            "idsTruncated": self.ids_truncated,
            "artifactsErased": self.artifacts_erased,
            "message": self.message(),
        })
    }
}

/// Canonicalize a tag for the *read* side of the erasure.
///
/// Mirrors `vestige_core::storage::sqlite::tags::normalize_tags`, which is
/// `pub(crate)` in core: `erase_by_tag` canonicalizes internally, while the
/// dry-run reads (`count_nodes_filtered` / `get_all_nodes_filtered`) compare
/// the stored, already-lowercased value directly. If the two ever diverge, the
/// dry run reports a different set from the one the destructive call removes —
/// which is exactly what `dry_run_matches_the_erase_for_canonicalized_tags`
/// guards against.
fn canonical_tag(tag: &str) -> String {
    tag.trim().to_lowercase().replace([' ', '_'], "-")
}

fn storage_err(e: vestige_core::StorageError) -> EraseError {
    EraseError::Storage(e.to_string())
}

/// Blocking entry point: parse, gate, execute.
///
/// Shared by all three surfaces so the confirmation requirement cannot drift
/// between MCP, the dashboard route and the CLI — they all call this function.
pub fn run(storage: &Storage, args: Option<Value>) -> Result<EraseOutcome, EraseError> {
    let raw = args.unwrap_or_else(|| serde_json::json!({}));
    let parsed: EraseArgs = serde_json::from_value(raw.clone())
        .map_err(|e| EraseError::Invalid(format!("Invalid arguments: {e}")))?;

    let dry_run = parsed.dry_run.unwrap_or(true);

    // Destructive-op gate (see `tools::common`). Only the variant that
    // actually deletes requires the acknowledgement — `dry_run: true` is safe
    // by construction.
    if !dry_run && !common::is_confirmed(&raw) {
        return Err(EraseError::Refused(common::missing_confirmation_error(
            "erase",
            "Erasing is irreversible: right_to_erasure hard-deletes the memory, its connections, \
             embeddings, access log, state history and every insight derived from it.",
        )));
    }

    match parsed.action.as_str() {
        "memory" => {
            let Some(id) = parsed
                .id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Err(EraseError::Invalid(
                    "action='memory' requires a non-empty `id`".to_string(),
                ));
            };

            // Read first: `right_to_erasure` returns the number of *artifacts*,
            // not whether the node existed, so without this the report could
            // not distinguish "erased one memory" from "nothing matched".
            let existed = storage.get_node(id).map_err(storage_err)?.is_some();
            let artifacts = if dry_run {
                0
            } else {
                storage.right_to_erasure(id).map_err(storage_err)?
            };

            Ok(EraseOutcome {
                action: "memory",
                dry_run,
                matched: u64::from(existed),
                ids: if existed {
                    vec![id.to_string()]
                } else {
                    Vec::new()
                },
                ids_truncated: false,
                artifacts_erased: artifacts,
            })
        }
        "tag" => {
            let Some(tag) = parsed
                .tag
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Err(EraseError::Invalid(
                    "action='tag' requires a non-empty `tag`".to_string(),
                ));
            };
            let canonical = canonical_tag(tag);
            if canonical.is_empty() {
                return Err(EraseError::Invalid(
                    "action='tag' requires a tag that is non-empty after canonicalization"
                        .to_string(),
                ));
            }

            // Peek at the matching set before deleting: the dry run needs it,
            // and the destructive report names the memories it removed. Both
            // reads use the same exact-element predicate (`json_each`, not a
            // LIKE prefix) as the erasure itself.
            let ids: Vec<String> = storage
                .get_all_nodes_filtered(MAX_REPORTED_IDS as i32, 0, None, Some(&canonical), None)
                .map_err(storage_err)?
                .into_iter()
                .map(|node| node.id)
                .collect();

            if dry_run {
                let matched = storage
                    .count_nodes_filtered(None, Some(&canonical), None)
                    .map_err(storage_err)? as u64;
                return Ok(EraseOutcome {
                    action: "tag",
                    dry_run: true,
                    matched,
                    ids_truncated: matched as usize > ids.len(),
                    ids,
                    artifacts_erased: 0,
                });
            }

            // `erase_by_tag` canonicalizes the tag itself and returns
            // (memories_erased, artifacts_erased) — authoritative, so `matched`
            // comes from it rather than from the peek above.
            let (removed, artifacts) = storage.erase_by_tag(tag).map_err(storage_err)?;
            let removed = removed.max(0) as usize;
            Ok(EraseOutcome {
                action: "tag",
                dry_run: false,
                matched: removed as u64,
                ids_truncated: removed > ids.len(),
                ids,
                artifacts_erased: artifacts,
            })
        }
        other => Err(EraseError::Invalid(format!(
            "Unknown action '{other}': expected 'memory' or 'tag'"
        ))),
    }
}

/// MCP-facing wrapper: runs [`run`] on the blocking pool and renders the tool
/// result. `Err` becomes a normal `isError: true` tool result in
/// `server::dispatch`, not a JSON-RPC protocol error.
pub async fn execute(storage: &Arc<Storage>, args: Option<Value>) -> Result<Value, String> {
    execute_checked(storage, args)
        .await
        .map(|outcome| outcome.to_json())
        .map_err(|e| e.to_string())
}

/// Same as [`execute`], but keeps the typed error so the dashboard route can
/// pick a status code (refusal/invalid → 400, storage → 500).
pub async fn execute_checked(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<EraseOutcome, EraseError> {
    let storage = Arc::clone(storage);
    tokio::task::spawn_blocking(move || run(&storage, args))
        .await
        .map_err(|e| EraseError::Storage(format!("erase task panicked: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use vestige_core::{IngestInput, InsightRecord};

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    fn node(content: &str, tags: &[&str]) -> IngestInput {
        IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        }
    }

    fn ingest(storage: &Storage, content: &str, tags: &[&str]) -> String {
        storage.ingest(node(content, tags)).unwrap().id
    }

    fn args(value: Value) -> Option<Value> {
        Some(value)
    }

    // ---- schema -----------------------------------------------------------

    #[test]
    fn schema_requires_id_or_tag_per_action() {
        // The audit's complaint about tool schemas: requirements lived only in
        // the runtime, so the model learned them by failing a round-trip.
        let schema = schema();
        let all_of = schema["allOf"].as_array().expect("allOf declared");
        assert_eq!(all_of.len(), 2);

        assert_eq!(
            all_of[0]["if"]["properties"]["action"]["const"],
            serde_json::json!("memory")
        );
        assert_eq!(all_of[0]["then"]["required"], serde_json::json!(["id"]));
        assert_eq!(
            all_of[1]["if"]["properties"]["action"]["const"],
            serde_json::json!("tag")
        );
        assert_eq!(all_of[1]["then"]["required"], serde_json::json!(["tag"]));

        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert_eq!(
            actions,
            &vec![serde_json::json!("memory"), serde_json::json!("tag")]
        );
    }

    #[test]
    fn schema_declares_bounds_and_safe_defaults() {
        let schema = schema();
        assert_eq!(schema["properties"]["dry_run"]["default"], true);
        assert_eq!(schema["properties"]["confirmed"]["default"], false);
        assert_eq!(schema["properties"]["id"]["maxLength"], 64);
        assert_eq!(schema["properties"]["tag"]["maxLength"], 128);
        assert_eq!(schema["properties"]["id"]["minLength"], 1);
        assert_eq!(schema["properties"]["tag"]["minLength"], 1);
        // `confirmed` must stay optional: requiring it would turn the gate's
        // explanatory refusal into a client-side validation error.
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

    // ---- confirmation gate ------------------------------------------------

    #[tokio::test]
    async fn refuses_without_confirmed_and_deletes_nothing() {
        let (storage, _dir) = test_storage().await;
        let id = ingest(&storage, "erase me", &["gdpr"]);

        let err = execute_checked(
            &storage,
            args(serde_json::json!({ "action": "memory", "id": id, "dry_run": false })),
        )
        .await
        .expect_err("an unconfirmed destructive erase must be refused");

        assert!(
            matches!(err, EraseError::Refused(_)),
            "expected a refusal, got {err:?}"
        );
        assert!(
            err.to_string().contains("requires explicit confirmation"),
            "the refusal must tell the caller how to proceed: {err}"
        );
        assert!(
            storage.get_node(&id).unwrap().is_some(),
            "a refused erase must leave the memory untouched"
        );
    }

    #[tokio::test]
    async fn confirmed_erase_removes_the_memory_and_its_derived_insight() {
        let (storage, _dir) = test_storage().await;
        let kept = ingest(&storage, "unrelated memory", &["other"]);
        let doomed = ingest(&storage, "the subject's memory", &["gdpr"]);
        storage
            .save_insight(&InsightRecord {
                id: "insight-1".to_string(),
                insight: "derived from the subject's memory".to_string(),
                source_memories: vec![doomed.clone()],
                insight_type: "pattern".to_string(),
                ..Default::default()
            })
            .unwrap();

        let outcome = execute_checked(
            &storage,
            args(serde_json::json!({
                "action": "memory", "id": doomed, "dry_run": false, "confirmed": true
            })),
        )
        .await
        .unwrap();

        assert_eq!(outcome.matched, 1);
        assert!(outcome.artifacts_erased >= 2, "node + insight at minimum");
        assert!(storage.get_node(&doomed).unwrap().is_none());
        assert!(
            storage.get_node(&kept).unwrap().is_some(),
            "erasing one memory must not touch another"
        );
        assert!(
            storage.get_insights(10).unwrap().is_empty(),
            "an insight derived from an erased memory must not survive it"
        );
    }

    // ---- dry run ----------------------------------------------------------

    #[tokio::test]
    async fn dry_run_reports_the_match_set_without_deleting() {
        let (storage, _dir) = test_storage().await;
        let first = ingest(&storage, "one", &["gdpr"]);
        let second = ingest(&storage, "two", &["gdpr"]);

        let outcome = execute(
            &storage,
            args(serde_json::json!({ "action": "tag", "tag": "gdpr" })),
        )
        .await
        .unwrap();

        assert_eq!(outcome["dryRun"], true, "dry_run defaults to true");
        assert_eq!(outcome["matched"], 2);
        assert_eq!(outcome["artifactsErased"], 0);
        assert_eq!(outcome["ids"].as_array().unwrap().len(), 2);
        assert_eq!(outcome["idsTruncated"], false);
        assert!(storage.get_node(&first).unwrap().is_some());
        assert!(storage.get_node(&second).unwrap().is_some());
    }

    // ---- exact tag matching ----------------------------------------------

    #[tokio::test]
    async fn tag_erasure_matches_exactly_and_spares_longer_tags() {
        let (storage, _dir) = test_storage().await;
        let code = ingest(&storage, "a memory tagged code", &["code"]);
        let codebase = ingest(&storage, "a memory tagged codebase", &["codebase"]);

        let preview = execute(
            &storage,
            args(serde_json::json!({ "action": "tag", "tag": "code" })),
        )
        .await
        .unwrap();
        assert_eq!(
            preview["matched"], 1,
            "'code' must not count 'codebase' as a match"
        );
        assert_eq!(preview["ids"], serde_json::json!([code]));

        let outcome = execute(
            &storage,
            args(serde_json::json!({
                "action": "tag", "tag": "code", "dry_run": false, "confirmed": true
            })),
        )
        .await
        .unwrap();
        assert_eq!(outcome["matched"], 1);
        assert!(storage.get_node(&code).unwrap().is_none());
        assert!(
            storage.get_node(&codebase).unwrap().is_some(),
            "prefix-adjacent tags must survive a bulk erasure"
        );
    }

    #[tokio::test]
    async fn dry_run_matches_the_erase_for_canonicalized_tags() {
        // `normalize_tags` is `pub(crate)` in core, so this module mirrors it.
        // If the mirror drifts, the dry run reports a different set from the
        // one the destructive call removes — the failure this pins.
        let (storage, _dir) = test_storage().await;
        let id = ingest(&storage, "spaced tag", &["Code Base"]);

        let preview = execute(
            &storage,
            args(serde_json::json!({ "action": "tag", "tag": "Code Base" })),
        )
        .await
        .unwrap();
        assert_eq!(preview["matched"], 1, "canonicalization must reach storage");
        assert_eq!(preview["ids"], serde_json::json!([id]));

        let outcome = execute(
            &storage,
            args(serde_json::json!({
                "action": "tag", "tag": "Code_Base", "dry_run": false, "confirmed": true
            })),
        )
        .await
        .unwrap();
        assert_eq!(outcome["matched"], 1);
        assert!(storage.get_node(&id).unwrap().is_none());
    }

    #[tokio::test]
    async fn dry_run_of_an_unknown_id_reports_zero_matches() {
        let (storage, _dir) = test_storage().await;
        let outcome = execute(
            &storage,
            args(serde_json::json!({ "action": "memory", "id": "no-such-id" })),
        )
        .await
        .unwrap();
        assert_eq!(outcome["matched"], 0);
        assert_eq!(outcome["ids"], serde_json::json!([]));
    }

    // ---- argument handling ------------------------------------------------

    #[tokio::test]
    async fn missing_id_or_tag_is_rejected_before_any_write() {
        let (storage, _dir) = test_storage().await;

        let err = execute_checked(&storage, args(serde_json::json!({ "action": "memory" })))
            .await
            .expect_err("memory erasure without an id must be refused");
        assert!(matches!(err, EraseError::Invalid(_)), "got {err:?}");

        let err = execute_checked(
            &storage,
            args(serde_json::json!({ "action": "tag", "tag": " " })),
        )
        .await
        .expect_err("a blank tag must be refused");
        assert!(matches!(err, EraseError::Invalid(_)), "got {err:?}");

        let err = execute_checked(
            &storage,
            args(serde_json::json!({ "action": "everything" })),
        )
        .await
        .expect_err("an unknown action must be refused");
        assert!(matches!(err, EraseError::Invalid(_)), "got {err:?}");
    }
}

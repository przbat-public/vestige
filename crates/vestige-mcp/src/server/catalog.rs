//! Tool and resource catalogs advertised by the MCP server.
//!
//! Extracted from `server.rs` (b15) to keep that file focused on request
//! routing. When a new MCP tool ships, declare it here and update the count in
//! `scripts/check-version-and-tools.sh` plus the comment near the top of this
//! file. The CI guard will fail otherwise.
//!
//! # Annotations vs `outputSchema`
//!
//! Every tool ships an explicit [`ToolAnnotations`] payload built via the
//! helpers below (`read_only_safe`, `mutating`, `destructive_idempotent`).
//! Cursor and other MCP clients use these hints to decide whether a call
//! needs human approval — the spec defaults are intentionally pessimistic
//! (`destructiveHint=true, openWorldHint=true`), so silence here would
//! prompt the user before every search.
//!
//! We deliberately do NOT publish `outputSchema` yet. The MCP `2025-06-18`
//! revision pairs `outputSchema` with `structuredContent`, but every Vestige
//! tool currently returns plain `text` content (Markdown reports, JSON
//! strings, etc.). Declaring an output schema while still emitting text
//! would advertise a contract we do not honor and break strict clients.
//! Migrating the tools to dual-format responses (text + structuredContent)
//! is tracked separately.
//!
//! v3.2.1: 27 tools advertised in tools/list. Categories:
//!   - 4 unified  : search, memory, codebase, intention
//!   - 1 core     : smart_ingest
//!   - 2 temporal : memory_timeline, memory_changelog
//!   - 7 maint    : system_status, consolidate, backup, export, gc,
//!     split_memories, regenerate_embeddings
//!   - 2 dedup    : importance_score, find_duplicates
//!   - 3 cog      : dream, explore_connections, predict
//!   - 1 restore  : restore
//!   - 1 context  : session_context
//!   - 2 autonom  : memory_health, memory_graph
//!   - 3 meta     : reflect, temporal, confidence
//!   - 1 reasoning: deep_reference
//!
//! Deprecated tools still work via redirects in `handle_tools_call` but are
//! NOT listed here.

use crate::protocol::messages::{ResourceDescription, ToolAnnotations, ToolDescription};
use crate::tools;

// ---------------------------------------------------------------------------
// Annotation helpers
// ---------------------------------------------------------------------------
//
// MCP spec defaults are deliberately worst-case (assume mutating, destructive,
// non-idempotent, open-world). For Vestige every flag is the opposite of the
// default, so being explicit is cheaper than relying on defaults.

/// Read-only tool that only inspects state. Safe to auto-call without
/// asking the user.
fn read_only_safe(title: &str) -> ToolAnnotations {
    ToolAnnotations {
        title: Some(title.to_string()),
        read_only_hint: Some(true),
        // `destructive_hint` is meaningless when read-only, but we set it to
        // `false` so simple consumers that do not implement the
        // "ignore destructive when read-only" rule still see the right value.
        destructive_hint: Some(false),
        idempotent_hint: Some(true),
        open_world_hint: Some(false),
    }
}

/// Mutating tool whose updates are additive (adds a memory, attaches a tag,
/// records an outcome). Repeated calls may stack but do not delete data.
fn mutating(title: &str, idempotent: bool) -> ToolAnnotations {
    ToolAnnotations {
        title: Some(title.to_string()),
        read_only_hint: Some(false),
        destructive_hint: Some(false),
        idempotent_hint: Some(idempotent),
        open_world_hint: Some(false),
    }
}

/// Destructive tool that may delete memories, overwrite content, or replace
/// versioned facts. Clients should require human approval.
fn destructive(title: &str, idempotent: bool) -> ToolAnnotations {
    ToolAnnotations {
        title: Some(title.to_string()),
        read_only_hint: Some(false),
        destructive_hint: Some(true),
        idempotent_hint: Some(idempotent),
        open_world_hint: Some(false),
    }
}

/// Convenience wrapper that fills `title`, `description`, `input_schema`, and
/// `annotations` so the catalog body below stays narrow and scannable.
fn tool(
    name: &str,
    title: &str,
    description: &str,
    input_schema: serde_json::Value,
    annotations: ToolAnnotations,
) -> ToolDescription {
    ToolDescription {
        name: name.to_string(),
        title: Some(title.to_string()),
        description: Some(description.to_string()),
        input_schema,
        annotations: Some(annotations),
    }
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

/// Build the canonical `tools/list` payload (27 entries).
pub(super) fn build_tools_list() -> Vec<ToolDescription> {
    vec![
        // ================================================================
        // UNIFIED TOOLS (v1.1+)
        // ================================================================
        tool(
            "search",
            "Search memories",
            "Unified search tool. Hybrid search (BM25 keyword + semantic embedding) fused with Reciprocal Rank Fusion (RRF), then run through the 8-stage cognitive pipeline (gating, dedup, scoring, spreading activation, token budget). Auto-strengthens memories on access (Testing Effect).",
            tools::search_unified::schema(),
            // Search performs FSRS strengthening on hits, but the user-visible
            // contract is "read"; clients can auto-call without prompting.
            read_only_safe("Search memories"),
        ),
        tool(
            "memory",
            "Memory CRUD",
            "Unified memory management tool. Actions: 'get' (retrieve full node by id), 'get_batch' (retrieve up to 20 nodes in one call via 'ids' array — useful for expanding search results), 'delete' (remove memory), 'state' (get accessibility state), 'promote' (thumbs up — increases retrieval strength), 'demote' (thumbs down — decreases retrieval strength, does NOT delete), 'edit' (update content in-place, preserves FSRS state).",
            tools::memory_unified::schema(),
            // Routes 'delete' and 'edit' alongside read-only 'get'/'get_batch'/'state' —
            // worst-case wins, so this is a destructive tool.
            destructive("Memory CRUD", false),
        ),
        tool(
            "codebase",
            "Codebase patterns and decisions",
            "Unified codebase tool. Actions: 'remember_pattern' (store code pattern), 'remember_decision' (legacy free-form Markdown decision), 'remember_decision_v2' (structured Decision Matrix — question + choices + criteria + 1–5 score matrix + optional validUntil; reflect auto-flags decisions whose validUntil has passed), 'get_context' (retrieve patterns and decisions). Prefer 'remember_decision_v2' for non-trivial decisions so the Decision Matrix dashboard can render them and the reflect tool can detect staleness automatically.",
            tools::codebase_unified::schema(),
            // 'remember_pattern' / 'remember_decision' / 'remember_decision_v2'
            // are additive writes; 'get_context' is read-only. Pick the
            // worst-case (mutating).
            mutating("Codebase patterns and decisions", false),
        ),
        tool(
            "intention",
            "Intentions (prospective memory)",
            "Unified intention management tool. Actions: 'set' (create), 'check' (find triggered), 'update' (complete/snooze/cancel), 'list' (show intentions).",
            tools::intention_unified::schema(),
            mutating("Intentions (prospective memory)", false),
        ),
        // ================================================================
        // CORE MEMORY (v1.7: smart_ingest absorbs ingest + checkpoint)
        // ================================================================
        tool(
            "smart_ingest",
            "Save memory (smart ingest)",
            "INTELLIGENT memory ingestion with Prediction Error Gating. Single mode: provide 'content' to auto-decide CREATE/UPDATE/SUPERSEDE. Batch mode: provide 'items' array (max 20) for session-end saves — each item runs the full cognitive pipeline (importance scoring, intent detection, synaptic tagging).",
            tools::smart_ingest::schema(),
            // SUPERSEDE branch can replace existing facts; treat as destructive.
            destructive("Save memory (smart ingest)", false),
        ),
        // ================================================================
        // TEMPORAL TOOLS (v1.2+)
        // ================================================================
        tool(
            "memory_timeline",
            "Browse memories chronologically",
            "Browse memories chronologically. Returns memories in a time range, grouped by day. Defaults to last 7 days.",
            tools::timeline::schema(),
            read_only_safe("Browse memories chronologically"),
        ),
        tool(
            "memory_changelog",
            "Memory changelog",
            "View audit trail of memory changes. Per-memory: state transitions. System-wide: consolidations + recent state changes.",
            tools::changelog::schema(),
            read_only_safe("Memory changelog"),
        ),
        // ================================================================
        // MAINTENANCE TOOLS (v1.7: system_status replaces health_check + stats)
        // ================================================================
        tool(
            "system_status",
            "System status",
            "Combined system health and statistics. Returns status (healthy/degraded/critical/empty), full stats, FSRS preview, cognitive module health, state distribution, warnings, and recommendations.",
            tools::maintenance::system_status_schema(),
            read_only_safe("System status"),
        ),
        tool(
            "consolidate",
            "Run consolidation cycle",
            "Run FSRS-6 memory consolidation cycle. Applies decay, generates embeddings, and performs maintenance. Use when memories seem stale.",
            tools::maintenance::consolidate_schema(),
            // Decay updates retention scores in place. Idempotent within a
            // session-level window because FSRS clamps based on elapsed time.
            mutating("Run consolidation cycle", true),
        ),
        tool(
            "backup",
            "Backup database",
            "Create a SQLite database backup. Returns the backup file path.",
            tools::maintenance::backup_schema(),
            // Writes a new file each call but never modifies live data.
            mutating("Backup database", false),
        ),
        tool(
            "export",
            "Export memories",
            "Export memories as JSON or JSONL. Supports tag and date filters.",
            tools::maintenance::export_schema(),
            read_only_safe("Export memories"),
        ),
        tool(
            "gc",
            "Garbage collect stale memories",
            "Garbage collect stale memories below retention threshold. Defaults to dry_run=true for safety.",
            tools::maintenance::gc_schema(),
            destructive("Garbage collect stale memories", true),
        ),
        tool(
            "split_memories",
            "Split compound memories",
            "Find compound/multi-topic memories that should be split into atomic pieces. Returns memories with splitting suggestions. Use dry_run=false to auto-delete compounds after reading them. Then re-ingest each as separate atomic items via smart_ingest batch mode.",
            tools::maintenance::split_memories_schema(),
            // dry_run=true is the safe default, dry_run=false deletes — pick
            // the worst case for the annotation.
            destructive("Split compound memories", false),
        ),
        tool(
            "regenerate_embeddings",
            "Regenerate embeddings",
            "Backfill or rebuild memory embeddings without the per-call cap that consolidate has. Use force=false (default) to fill missing embeddings (has_embedding=0/NULL); force=true to rebuild all. Optional node_ids to scope to specific memories. Use after upgrading the embedding model, after a long stretch where the model was unavailable at ingest time, or to recover from silent-skip situations.",
            tools::maintenance::regenerate_embeddings_schema(),
            // Overwrites embedding vectors but keeps content intact; idempotent
            // for force=true with the same model.
            mutating("Regenerate embeddings", true),
        ),
        // ================================================================
        // AUTO-SAVE & DEDUP TOOLS (v1.3+)
        // ================================================================
        tool(
            "importance_score",
            "Score content importance",
            "Score content importance using 4-channel neuroscience model (novelty/arousal/reward/attention). Returns composite score, channel breakdown, encoding boost, and explanations.",
            tools::importance::schema(),
            read_only_safe("Score content importance"),
        ),
        tool(
            "find_duplicates",
            "Find duplicate memories",
            "Find duplicate and near-duplicate memory clusters using cosine similarity on embeddings. Returns clusters with suggested actions (merge/review). Use to clean up redundant memories.",
            tools::dedup::schema(),
            read_only_safe("Find duplicate memories"),
        ),
        // ================================================================
        // COGNITIVE TOOLS (v1.5+)
        // ================================================================
        tool(
            "dream",
            "Dream (memory consolidation)",
            "Trigger memory dreaming — replays recent memories to discover hidden connections, synthesize insights, and strengthen important patterns. Returns insights, connections, and dream stats.",
            tools::dream::schema(),
            // Strengthens connections, can create new insight memories — additive.
            mutating("Dream (memory consolidation)", false),
        ),
        tool(
            "explore_connections",
            "Explore memory connections",
            "Graph exploration tool for memory connections. Actions: 'chain' (build reasoning path between memories), 'associations' (find related memories via spreading activation + hippocampal index), 'bridges' (find connecting memories between two nodes).",
            tools::explore::schema(),
            read_only_safe("Explore memory connections"),
        ),
        tool(
            "predict",
            "Predict next memories",
            "Proactive memory prediction — predicts what memories you'll need next based on context, recent activity, and learned patterns. Returns predictions, suggestions, and speculative retrievals.",
            tools::predict::schema(),
            read_only_safe("Predict next memories"),
        ),
        // ================================================================
        // RESTORE TOOL (v1.5+)
        // ================================================================
        tool(
            "restore",
            "Restore from backup",
            "Restore memories from a JSON backup file. Supports MCP wrapper format, RecallResult format, and direct memory array format.",
            tools::restore::schema(),
            destructive("Restore from backup", false),
        ),
        // ================================================================
        // CONTEXT PACKETS (v1.8+)
        // ================================================================
        tool(
            "session_context",
            "Session context packet",
            "One-call session initialization. Combines search, intentions, status, predictions, and codebase context into a single token-budgeted response. Replaces 5 separate calls at session start.",
            tools::session_context::schema(),
            read_only_safe("Session context packet"),
        ),
        // ================================================================
        // AUTONOMIC TOOLS (v1.9+)
        // ================================================================
        tool(
            "memory_health",
            "Memory retention dashboard",
            "Retention dashboard. Returns avg retention, retention distribution (buckets: 0-20%, 20-40%, etc.), trend (improving/declining/stable), and recommendation. Lightweight alternative to full system_status focused on memory quality.",
            tools::health::schema(),
            read_only_safe("Memory retention dashboard"),
        ),
        tool(
            "memory_graph",
            "Memory subgraph (visualization)",
            "Subgraph export for visualization. Input: center_id or query, depth (1-3), max_nodes. Returns nodes with force-directed layout positions and edges with weights. Powers memory graph visualization.",
            tools::graph::schema(),
            read_only_safe("Memory subgraph (visualization)"),
        ),
        // ================================================================
        // METACOGNITIVE TOOLS (v2.1+)
        // ================================================================
        tool(
            "reflect",
            "Metacognitive reflection",
            "Deliberate metacognitive reflection — analyzes memories for contradictions, knowledge gaps, stale decisions, overconfident memories, and pattern clusters. Unlike 'dream' (unconscious consolidation), 'reflect' is active self-examination. Returns actionable insights.",
            tools::reflect::schema(),
            read_only_safe("Metacognitive reflection"),
        ),
        tool(
            "temporal",
            "Temporal fact versioning",
            "Temporal fact versioning — query time-sensitive knowledge. Actions: 'current' (valid-now facts), 'expired' (no-longer-valid), 'history' (evolution of a topic over time), 'invalidate' (mark a fact as no longer valid).",
            tools::temporal::schema(),
            // 'invalidate' marks facts expired (mutating). Read actions are
            // pure. Worst-case wins.
            destructive("Temporal fact versioning", true),
        ),
        tool(
            "confidence",
            "Confidence calibration",
            "Confidence scoring for opinions and beliefs. Actions: 'score' (evaluate a single memory), 'audit' (find poorly-calibrated memories), 'calibrate' (compare opinions vs facts retention).",
            tools::confidence::schema(),
            read_only_safe("Confidence calibration"),
        ),
        // ================================================================
        // COGNITIVE REASONING (v3.2.1+)
        // ================================================================
        tool(
            "deep_reference",
            "Deep reference (cognitive reasoning)",
            "Cognitive reasoning engine across memories. Combines hybrid search, FSRS-6 trust scoring, intent classification, temporal supersession, contradiction analysis, dream insight integration, and structured synthesis. Use for factual questions, fact-checking, timelines, root-cause analysis, comparisons, and topic synthesis. 'cross_reference' is a backward-compatible alias.",
            tools::cross_reference::schema(),
            read_only_safe("Deep reference (cognitive reasoning)"),
        ),
    ]
}

/// Build the canonical `resources/list` payload (11 entries).
pub(super) fn build_resources_list() -> Vec<ResourceDescription> {
    vec![
        // Memory resources
        ResourceDescription {
            uri: "memory://stats".to_string(),
            name: "Memory Statistics".to_string(),
            description: Some("Current memory system statistics and health status".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDescription {
            uri: "memory://recent".to_string(),
            name: "Recent Memories".to_string(),
            description: Some("Recently added memories (last 10)".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDescription {
            uri: "memory://decaying".to_string(),
            name: "Decaying Memories".to_string(),
            description: Some("Memories with low retention that need review".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDescription {
            uri: "memory://due".to_string(),
            name: "Due for Review".to_string(),
            description: Some("Memories scheduled for review today".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        // Codebase resources
        ResourceDescription {
            uri: "codebase://structure".to_string(),
            name: "Codebase Structure".to_string(),
            description: Some("Remembered project structure and organization".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDescription {
            uri: "codebase://patterns".to_string(),
            name: "Code Patterns".to_string(),
            description: Some("Remembered code patterns and conventions".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDescription {
            uri: "codebase://decisions".to_string(),
            name: "Architectural Decisions".to_string(),
            description: Some("Remembered architectural and design decisions".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        // Consolidation resources
        ResourceDescription {
            uri: "memory://insights".to_string(),
            name: "Consolidation Insights".to_string(),
            description: Some("Insights generated during memory consolidation".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDescription {
            uri: "memory://consolidation-log".to_string(),
            name: "Consolidation Log".to_string(),
            description: Some("History of memory consolidation runs".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        // Prospective memory resources
        ResourceDescription {
            uri: "memory://intentions".to_string(),
            name: "Active Intentions".to_string(),
            description: Some(
                "Future intentions (prospective memory) waiting to be triggered".to_string(),
            ),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDescription {
            uri: "memory://intentions/due".to_string(),
            name: "Triggered Intentions".to_string(),
            description: Some("Intentions that have been triggered or are overdue".to_string()),
            mime_type: Some("application/json".to_string()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// b15 split guard: any drift in the catalog must update
    /// scripts/check-version-and-tools.sh and the comment above.
    #[test]
    fn tools_list_has_exactly_27_entries() {
        let tools = build_tools_list();
        assert_eq!(
            tools.len(),
            27,
            "build_tools_list must advertise exactly 27 tools; update scripts/check-version-and-tools.sh if you add or remove one"
        );
    }

    #[test]
    fn resources_list_has_exactly_11_entries() {
        let resources = build_resources_list();
        assert_eq!(resources.len(), 11);
    }

    #[test]
    fn every_tool_has_description_and_schema() {
        for tool in build_tools_list() {
            assert!(
                tool.description.is_some(),
                "tool {} missing description",
                tool.name
            );
            assert!(
                tool.input_schema.is_object(),
                "tool {} must expose a JSON-Schema object",
                tool.name,
            );
        }
    }

    /// b16: every tool must publish behavior annotations so MCP clients can
    /// decide auto-approval. The spec defaults are intentionally pessimistic
    /// (`destructiveHint=true`, `openWorldHint=true`) — silence here would
    /// downgrade the UX of every read-only tool to "needs human approval".
    #[test]
    fn every_tool_has_explicit_annotations() {
        for tool in build_tools_list() {
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("tool {} missing annotations", tool.name));

            for (field, value) in [
                ("readOnlyHint", annotations.read_only_hint),
                ("destructiveHint", annotations.destructive_hint),
                ("idempotentHint", annotations.idempotent_hint),
                ("openWorldHint", annotations.open_world_hint),
            ] {
                assert!(
                    value.is_some(),
                    "tool {} missing explicit `{}` annotation",
                    tool.name,
                    field,
                );
            }

            assert_eq!(
                annotations.open_world_hint,
                Some(false),
                "tool {} declared open-world; Vestige tools are local-only",
                tool.name,
            );
        }
    }

    /// b16: every tool must surface a display title for human-facing UIs.
    /// The MCP spec puts `title` both at the tool level and on
    /// `ToolAnnotations`. We populate both — clients on the
    /// `2025-11-25` schema read the top-level field, older clients only
    /// see the annotation field.
    #[test]
    fn every_tool_has_display_title() {
        for tool in build_tools_list() {
            assert!(
                tool.title.is_some(),
                "tool {} missing top-level title",
                tool.name
            );
            assert!(
                tool.annotations
                    .as_ref()
                    .and_then(|a| a.title.as_ref())
                    .is_some(),
                "tool {} missing annotation title",
                tool.name,
            );
        }
    }

    /// c24: every tool must publish a usable JSON-Schema. We don't reach for a
    /// full Draft 2020-12 validator (extra dep, slow) but we do enforce the
    /// minimum shape every MCP client relies on:
    ///   * `type` is present and equals "object"
    ///   * `properties` is an object (possibly empty)
    ///   * `required` (if present) is an array of strings that all reference
    ///     keys declared in `properties`
    #[test]
    fn every_tool_schema_has_object_type_and_consistent_required() {
        for tool in build_tools_list() {
            let schema = tool
                .input_schema
                .as_object()
                .unwrap_or_else(|| panic!("schema for {} is not an object", tool.name));

            let kind = schema
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("schema for {} missing string `type`", tool.name));
            assert_eq!(
                kind, "object",
                "schema for {} must use type=object",
                tool.name
            );

            let properties = schema
                .get("properties")
                .unwrap_or_else(|| panic!("schema for {} missing `properties`", tool.name))
                .as_object()
                .unwrap_or_else(|| panic!("schema for {} has non-object `properties`", tool.name));

            if let Some(required) = schema.get("required") {
                let required_arr = required
                    .as_array()
                    .unwrap_or_else(|| panic!("schema for {} has non-array `required`", tool.name));
                for entry in required_arr {
                    let key = entry.as_str().unwrap_or_else(|| {
                        panic!(
                            "schema for {} has non-string entry in `required`",
                            tool.name
                        )
                    });
                    assert!(
                        properties.contains_key(key),
                        "schema for {} requires `{}` but it is not declared in `properties`",
                        tool.name,
                        key,
                    );
                }
            }
        }
    }

    /// c24: tool names must be unique within the catalog. Duplicates would
    /// make `tools/call` dispatch ambiguous for MCP clients.
    #[test]
    fn tool_names_are_unique() {
        let tools = build_tools_list();
        let mut seen = std::collections::HashSet::new();
        for tool in &tools {
            assert!(
                seen.insert(tool.name.clone()),
                "duplicate tool name detected in catalog: {}",
                tool.name,
            );
        }
        assert_eq!(seen.len(), tools.len());
    }

    /// c24: tool names must use the snake_case convention every MCP client
    /// already expects. Catches accidental drift to camelCase or kebab-case.
    #[test]
    fn tool_names_are_snake_case() {
        for tool in build_tools_list() {
            for (idx, ch) in tool.name.chars().enumerate() {
                let ok = ch.is_ascii_lowercase()
                    || ch.is_ascii_digit()
                    || ch == '_'
                    || (idx == 0 && ch.is_ascii_lowercase());
                assert!(
                    ok,
                    "tool name `{}` contains non-snake_case char `{}` at byte {}",
                    tool.name, ch, idx,
                );
            }
        }
    }

    #[test]
    fn every_resource_has_description() {
        for resource in build_resources_list() {
            assert!(
                resource.description.is_some(),
                "resource {} missing description",
                resource.uri,
            );
        }
    }

    /// c24: every resource URI must include a scheme so the dispatch logic in
    /// `handle_resources_read` can route it correctly.
    #[test]
    fn every_resource_uri_has_scheme() {
        for resource in build_resources_list() {
            assert!(
                resource.uri.contains("://"),
                "resource {} missing `scheme://` prefix",
                resource.uri,
            );
        }
    }
}

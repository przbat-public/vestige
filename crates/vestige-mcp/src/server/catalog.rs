//! Tool and resource catalogs advertised by the MCP server.
//!
//! Extracted from `server.rs` (b15) to keep that file focused on request
//! routing. When a new MCP tool ships, declare it here and update the count in
//! `scripts/check-version-and-tools.sh` plus the comment near the top of this
//! file. The CI guard will fail otherwise.
//!
//! v3.2.1: 27 tools advertised in tools/list. Categories:
//!   - 4 unified  : search, memory, codebase, intention
//!   - 1 core     : smart_ingest
//!   - 2 temporal : memory_timeline, memory_changelog
//!   - 7 maint    : system_status, consolidate, backup, export, gc,
//!                  split_memories, regenerate_embeddings
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

use crate::protocol::messages::{ResourceDescription, ToolDescription};
use crate::tools;

/// Build the canonical `tools/list` payload (27 entries).
pub(super) fn build_tools_list() -> Vec<ToolDescription> {
    vec![
        // ================================================================
        // UNIFIED TOOLS (v1.1+)
        // ================================================================
        ToolDescription {
            name: "search".to_string(),
            description: Some("Unified search tool. Uses hybrid search (keyword + semantic + convex combination fusion) internally. Auto-strengthens memories on access (Testing Effect).".to_string()),
            input_schema: tools::search_unified::schema(),
        },
        ToolDescription {
            name: "memory".to_string(),
            description: Some("Unified memory management tool. Actions: 'get' (retrieve full node), 'delete' (remove memory), 'state' (get accessibility state), 'promote' (thumbs up — increases retrieval strength), 'demote' (thumbs down — decreases retrieval strength, does NOT delete), 'edit' (update content in-place, preserves FSRS state).".to_string()),
            input_schema: tools::memory_unified::schema(),
        },
        ToolDescription {
            name: "codebase".to_string(),
            description: Some("Unified codebase tool. Actions: 'remember_pattern' (store code pattern), 'remember_decision' (store architectural decision), 'get_context' (retrieve patterns and decisions).".to_string()),
            input_schema: tools::codebase_unified::schema(),
        },
        ToolDescription {
            name: "intention".to_string(),
            description: Some("Unified intention management tool. Actions: 'set' (create), 'check' (find triggered), 'update' (complete/snooze/cancel), 'list' (show intentions).".to_string()),
            input_schema: tools::intention_unified::schema(),
        },
        // ================================================================
        // CORE MEMORY (v1.7: smart_ingest absorbs ingest + checkpoint)
        // ================================================================
        ToolDescription {
            name: "smart_ingest".to_string(),
            description: Some("INTELLIGENT memory ingestion with Prediction Error Gating. Single mode: provide 'content' to auto-decide CREATE/UPDATE/SUPERSEDE. Batch mode: provide 'items' array (max 20) for session-end saves — each item runs the full cognitive pipeline (importance scoring, intent detection, synaptic tagging).".to_string()),
            input_schema: tools::smart_ingest::schema(),
        },
        // ================================================================
        // TEMPORAL TOOLS (v1.2+)
        // ================================================================
        ToolDescription {
            name: "memory_timeline".to_string(),
            description: Some("Browse memories chronologically. Returns memories in a time range, grouped by day. Defaults to last 7 days.".to_string()),
            input_schema: tools::timeline::schema(),
        },
        ToolDescription {
            name: "memory_changelog".to_string(),
            description: Some("View audit trail of memory changes. Per-memory: state transitions. System-wide: consolidations + recent state changes.".to_string()),
            input_schema: tools::changelog::schema(),
        },
        // ================================================================
        // MAINTENANCE TOOLS (v1.7: system_status replaces health_check + stats)
        // ================================================================
        ToolDescription {
            name: "system_status".to_string(),
            description: Some("Combined system health and statistics. Returns status (healthy/degraded/critical/empty), full stats, FSRS preview, cognitive module health, state distribution, warnings, and recommendations.".to_string()),
            input_schema: tools::maintenance::system_status_schema(),
        },
        ToolDescription {
            name: "consolidate".to_string(),
            description: Some("Run FSRS-6 memory consolidation cycle. Applies decay, generates embeddings, and performs maintenance. Use when memories seem stale.".to_string()),
            input_schema: tools::maintenance::consolidate_schema(),
        },
        ToolDescription {
            name: "backup".to_string(),
            description: Some("Create a SQLite database backup. Returns the backup file path.".to_string()),
            input_schema: tools::maintenance::backup_schema(),
        },
        ToolDescription {
            name: "export".to_string(),
            description: Some("Export memories as JSON or JSONL. Supports tag and date filters.".to_string()),
            input_schema: tools::maintenance::export_schema(),
        },
        ToolDescription {
            name: "gc".to_string(),
            description: Some("Garbage collect stale memories below retention threshold. Defaults to dry_run=true for safety.".to_string()),
            input_schema: tools::maintenance::gc_schema(),
        },
        ToolDescription {
            name: "split_memories".to_string(),
            description: Some("Find compound/multi-topic memories that should be split into atomic pieces. Returns memories with splitting suggestions. Use dry_run=false to auto-delete compounds after reading them. Then re-ingest each as separate atomic items via smart_ingest batch mode.".to_string()),
            input_schema: tools::maintenance::split_memories_schema(),
        },
        ToolDescription {
            name: "regenerate_embeddings".to_string(),
            description: Some("Backfill or rebuild memory embeddings without the per-call cap that consolidate has. Use force=false (default) to fill missing embeddings (has_embedding=0/NULL); force=true to rebuild all. Optional node_ids to scope to specific memories. Use after upgrading the embedding model, after a long stretch where the model was unavailable at ingest time, or to recover from silent-skip situations.".to_string()),
            input_schema: tools::maintenance::regenerate_embeddings_schema(),
        },
        // ================================================================
        // AUTO-SAVE & DEDUP TOOLS (v1.3+)
        // ================================================================
        ToolDescription {
            name: "importance_score".to_string(),
            description: Some("Score content importance using 4-channel neuroscience model (novelty/arousal/reward/attention). Returns composite score, channel breakdown, encoding boost, and explanations.".to_string()),
            input_schema: tools::importance::schema(),
        },
        ToolDescription {
            name: "find_duplicates".to_string(),
            description: Some("Find duplicate and near-duplicate memory clusters using cosine similarity on embeddings. Returns clusters with suggested actions (merge/review). Use to clean up redundant memories.".to_string()),
            input_schema: tools::dedup::schema(),
        },
        // ================================================================
        // COGNITIVE TOOLS (v1.5+)
        // ================================================================
        ToolDescription {
            name: "dream".to_string(),
            description: Some("Trigger memory dreaming — replays recent memories to discover hidden connections, synthesize insights, and strengthen important patterns. Returns insights, connections, and dream stats.".to_string()),
            input_schema: tools::dream::schema(),
        },
        ToolDescription {
            name: "explore_connections".to_string(),
            description: Some("Graph exploration tool for memory connections. Actions: 'chain' (build reasoning path between memories), 'associations' (find related memories via spreading activation + hippocampal index), 'bridges' (find connecting memories between two nodes).".to_string()),
            input_schema: tools::explore::schema(),
        },
        ToolDescription {
            name: "predict".to_string(),
            description: Some("Proactive memory prediction — predicts what memories you'll need next based on context, recent activity, and learned patterns. Returns predictions, suggestions, and speculative retrievals.".to_string()),
            input_schema: tools::predict::schema(),
        },
        // ================================================================
        // RESTORE TOOL (v1.5+)
        // ================================================================
        ToolDescription {
            name: "restore".to_string(),
            description: Some("Restore memories from a JSON backup file. Supports MCP wrapper format, RecallResult format, and direct memory array format.".to_string()),
            input_schema: tools::restore::schema(),
        },
        // ================================================================
        // CONTEXT PACKETS (v1.8+)
        // ================================================================
        ToolDescription {
            name: "session_context".to_string(),
            description: Some("One-call session initialization. Combines search, intentions, status, predictions, and codebase context into a single token-budgeted response. Replaces 5 separate calls at session start.".to_string()),
            input_schema: tools::session_context::schema(),
        },
        // ================================================================
        // AUTONOMIC TOOLS (v1.9+)
        // ================================================================
        ToolDescription {
            name: "memory_health".to_string(),
            description: Some("Retention dashboard. Returns avg retention, retention distribution (buckets: 0-20%, 20-40%, etc.), trend (improving/declining/stable), and recommendation. Lightweight alternative to full system_status focused on memory quality.".to_string()),
            input_schema: tools::health::schema(),
        },
        ToolDescription {
            name: "memory_graph".to_string(),
            description: Some("Subgraph export for visualization. Input: center_id or query, depth (1-3), max_nodes. Returns nodes with force-directed layout positions and edges with weights. Powers memory graph visualization.".to_string()),
            input_schema: tools::graph::schema(),
        },
        // ================================================================
        // METACOGNITIVE TOOLS (v2.1+)
        // ================================================================
        ToolDescription {
            name: "reflect".to_string(),
            description: Some("Deliberate metacognitive reflection — analyzes memories for contradictions, knowledge gaps, stale decisions, overconfident memories, and pattern clusters. Unlike 'dream' (unconscious consolidation), 'reflect' is active self-examination. Returns actionable insights.".to_string()),
            input_schema: tools::reflect::schema(),
        },
        ToolDescription {
            name: "temporal".to_string(),
            description: Some("Temporal fact versioning — query time-sensitive knowledge. Actions: 'current' (valid-now facts), 'expired' (no-longer-valid), 'history' (evolution of a topic over time), 'invalidate' (mark a fact as no longer valid).".to_string()),
            input_schema: tools::temporal::schema(),
        },
        ToolDescription {
            name: "confidence".to_string(),
            description: Some("Confidence scoring for opinions and beliefs. Actions: 'score' (evaluate a single memory), 'audit' (find poorly-calibrated memories), 'calibrate' (compare opinions vs facts retention).".to_string()),
            input_schema: tools::confidence::schema(),
        },
        // ================================================================
        // COGNITIVE REASONING (v3.2.1+)
        // ================================================================
        ToolDescription {
            name: "deep_reference".to_string(),
            description: Some("Cognitive reasoning engine across memories. Combines hybrid search, FSRS-6 trust scoring, intent classification, temporal supersession, contradiction analysis, dream insight integration, and structured synthesis. Use for factual questions, fact-checking, timelines, root-cause analysis, comparisons, and topic synthesis. 'cross_reference' is a backward-compatible alias.".to_string()),
            input_schema: tools::cross_reference::schema(),
        },
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
            description: Some("Future intentions (prospective memory) waiting to be triggered".to_string()),
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
            assert!(tool.description.is_some(), "tool {} missing description", tool.name);
            assert!(
                tool.input_schema.is_object(),
                "tool {} must expose a JSON-Schema object",
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
            assert_eq!(kind, "object", "schema for {} must use type=object", tool.name);

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
                        panic!("schema for {} has non-string entry in `required`", tool.name)
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

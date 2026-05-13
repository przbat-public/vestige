//! Main execute function — assembles search results, status, intentions, predictions.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::Storage;

use super::args::SessionContextArgs;
use super::helpers::{check_intention_triggered, first_sentence};

/// Execute session_context tool — one-call session initialization.
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: SessionContextArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => SessionContextArgs::default(),
    };

    let max_budget: i32 = std::env::var("VESTIGE_MAX_TOKEN_BUDGET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100000);
    let token_budget = args.token_budget.unwrap_or(1000).clamp(100, max_budget) as usize;
    let budget_chars = token_budget * 4;
    let include_status = args.include_status.unwrap_or(true);
    let include_intentions = args.include_intentions.unwrap_or(true);
    let include_predictions = args.include_predictions.unwrap_or(true);
    let queries = args
        .queries
        .unwrap_or_else(|| vec!["user preferences".to_string()]);

    let mut context_parts: Vec<String> = Vec::new();
    let mut expandable_ids: Vec<String> = Vec::new();
    let mut char_count = 0;

    // ====================================================================
    // 1. Search queries — extract first sentence per result, dedup by ID.
    //    All searches + Testing Effect strengthen run on the blocking pool;
    //    each hybrid_search bursts FTS5 + embedding similarity (heavy SQL).
    // ====================================================================
    let mut seen_ids = HashSet::new();
    let mut memory_lines: Vec<String> = Vec::new();

    let storage_search = storage.clone();
    let queries_for_task = queries.clone();
    let search_results: Vec<Vec<vestige_core::SearchResult>> =
        tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut out = Vec::with_capacity(queries_for_task.len());
            for query in &queries_for_task {
                let results = storage_search
                    .hybrid_search(query, 5, 0.3, 0.7)
                    .map_err(|e| e.to_string())?;
                out.push(results);
            }
            Ok(out)
        })
        .await
        .map_err(|e| format!("session_context search task panicked: {}", e))??;

    for results in search_results {
        for r in results {
            if seen_ids.contains(&r.node.id) {
                continue;
            }
            let summary = first_sentence(&r.node.content);
            let date_str = r.node.updated_at.format("%b %d, %Y").to_string();
            let line = format!("- ({}) {}", date_str, summary);
            let line_len = line.len() + 1;

            if char_count + line_len > budget_chars {
                expandable_ids.push(r.node.id.clone());
            } else {
                memory_lines.push(line);
                char_count += line_len;
            }
            seen_ids.insert(r.node.id.clone());
        }
    }

    // Auto-strengthen accessed memories (Testing Effect)
    let accessed_ids: Vec<String> = seen_ids.iter().cloned().collect();
    if !accessed_ids.is_empty() {
        let storage_strengthen = storage.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = accessed_ids.iter().map(|s| s.as_str()).collect();
            storage_strengthen.strengthen_batch_on_access(&refs)
        })
        .await;
    }

    if !memory_lines.is_empty() {
        context_parts.push(format!("**Memories:**\n{}", memory_lines.join("\n")));
    }

    // ====================================================================
    // 2. Intentions — find triggered + pending high-priority
    // ====================================================================
    if include_intentions {
        let storage_int = storage.clone();
        let intentions = tokio::task::spawn_blocking(move || {
            storage_int
                .get_active_intentions()
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("get_active_intentions task panicked: {}", e))??;
        let now = Utc::now();
        let mut triggered_lines: Vec<String> = Vec::new();

        for intention in &intentions {
            let is_overdue = intention.deadline.map(|d| d < now).unwrap_or(false);

            // Check context-based triggers
            let is_context_triggered = if let Some(ctx) = &args.context {
                check_intention_triggered(intention, ctx, now)
            } else {
                false
            };

            if is_overdue || is_context_triggered || intention.priority >= 3 {
                let priority_str = match intention.priority {
                    4 => " (critical)",
                    3 => " (high)",
                    _ => "",
                };
                let deadline_str = intention
                    .deadline
                    .map(|d| format!(" [due {}]", d.format("%b %d")))
                    .unwrap_or_default();
                let line = format!(
                    "- {}{}{}",
                    first_sentence(&intention.content),
                    priority_str,
                    deadline_str
                );
                let line_len = line.len() + 1;
                if char_count + line_len <= budget_chars {
                    triggered_lines.push(line);
                    char_count += line_len;
                }
            }
        }

        if !triggered_lines.is_empty() {
            context_parts.push(format!("**Triggered:**\n{}", triggered_lines.join("\n")));
        }
    }

    // ====================================================================
    // 3. System status — compact one-liner. Stats + last dream + count of
    //    saves since last dream all on the blocking pool, one hop.
    // ====================================================================
    let storage_status = storage.clone();
    let (stats, last_dream, saves_since_last_dream) = tokio::task::spawn_blocking(
        move || -> Result<(vestige_core::MemoryStats, Option<DateTime<Utc>>, i64), String> {
            let stats = storage_status.get_stats().map_err(|e| e.to_string())?;
            let last_dream = storage_status.get_last_dream().ok().flatten();
            let saves_since_last_dream = match &last_dream {
                Some(dt) => storage_status.count_memories_since(*dt).unwrap_or(0),
                None => stats.total_nodes,
            };
            Ok((stats, last_dream, saves_since_last_dream))
        },
    )
    .await
    .map_err(|e| format!("session_context status task panicked: {}", e))??;
    let status = if stats.total_nodes == 0 {
        "empty"
    } else if stats.average_retention < 0.3 {
        "critical"
    } else if stats.average_retention < 0.5 {
        "degraded"
    } else {
        "healthy"
    };

    let last_backup = Storage::get_last_backup_timestamp();
    let now = Utc::now();

    let needs_dream = last_dream
        .map(|dt| now - dt > Duration::hours(24) || saves_since_last_dream > 50)
        .unwrap_or(true);
    let needs_backup = last_backup
        .map(|dt| now - dt > Duration::days(7))
        .unwrap_or(true);
    let needs_gc = status == "degraded" || status == "critical";

    if include_status {
        let embedding_pct = if stats.total_nodes > 0 {
            (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
        } else {
            0.0
        };
        let status_line = format!(
            "**Status:** {} memories | {} | {:.0}% embeddings",
            stats.total_nodes, status, embedding_pct
        );
        let status_len = status_line.len() + 1;
        if char_count + status_len <= budget_chars {
            context_parts.push(status_line);
            char_count += status_len;
        }

        // Needs line (only if any automation needed)
        let mut needs: Vec<&str> = Vec::new();
        if needs_dream {
            needs.push("dream");
        }
        if needs_backup {
            needs.push("backup");
        }
        if needs_gc {
            needs.push("gc");
        }
        if !needs.is_empty() {
            let needs_line = format!("**Needs:** {}", needs.join(", "));
            let needs_len = needs_line.len() + 1;
            if char_count + needs_len <= budget_chars {
                context_parts.push(needs_line);
                char_count += needs_len;
            }
        }
    }

    // ====================================================================
    // 4. Predictions — top 3 with content preview
    // ====================================================================
    if include_predictions {
        let cog = cognitive.lock().await;

        let session_ctx =
            vestige_core::neuroscience::predictive_retrieval::SessionContext {
                started_at: Utc::now(),
                current_focus: args
                    .context
                    .as_ref()
                    .and_then(|c| c.topics.as_ref())
                    .and_then(|t| t.first())
                    .cloned(),
                active_files: args
                    .context
                    .as_ref()
                    .and_then(|c| c.file.as_ref())
                    .map(|f| vec![f.clone()])
                    .unwrap_or_default(),
                accessed_memories: Vec::new(),
                recent_queries: Vec::new(),
                detected_intent: None,
                project_context: args.context.as_ref().and_then(|c| c.codebase.as_ref()).map(
                    |name| vestige_core::neuroscience::predictive_retrieval::ProjectContext {
                        name: name.to_string(),
                        path: String::new(),
                        technologies: Vec::new(),
                        primary_language: None,
                    },
                ),
            };

        let predictions = cog
            .predictive_memory
            .predict_needed_memories(&session_ctx)
            .unwrap_or_default();

        if !predictions.is_empty() {
            let pred_lines: Vec<String> = predictions
                .iter()
                .take(3)
                .map(|p| {
                    format!(
                        "- {} ({:.0}%)",
                        first_sentence(&p.content_preview),
                        p.confidence * 100.0
                    )
                })
                .collect();

            let pred_section = format!("**Predicted:**\n{}", pred_lines.join("\n"));
            let pred_len = pred_section.len() + 1;
            if char_count + pred_len <= budget_chars {
                context_parts.push(pred_section);
                char_count += pred_len;
            }
        }
    }

    // ====================================================================
    // 5. Codebase patterns/decisions (if codebase specified)
    // ====================================================================
    if let Some(ref ctx) = args.context
        && let Some(ref codebase) = ctx.codebase
    {
        let codebase_tag = format!("codebase:{}", codebase);
        let mut cb_lines: Vec<String> = Vec::new();

        // Fetch patterns + decisions in a single blocking hop.
        let storage_cb = storage.clone();
        let codebase_tag_owned = codebase_tag.clone();
        let (patterns, decisions) = tokio::task::spawn_blocking(move || {
            let patterns = storage_cb
                .get_nodes_by_type_and_tag("pattern", Some(&codebase_tag_owned), 3)
                .unwrap_or_default();
            let decisions = storage_cb
                .get_nodes_by_type_and_tag("decision", Some(&codebase_tag_owned), 3)
                .unwrap_or_default();
            (patterns, decisions)
        })
        .await
        .map_err(|e| format!("session_context codebase task panicked: {}", e))?;

        for p in &patterns {
            let line = format!("- [pattern] {}", first_sentence(&p.content));
            let line_len = line.len() + 1;
            if char_count + line_len <= budget_chars {
                cb_lines.push(line);
                char_count += line_len;
            }
        }
        for d in &decisions {
            let line = format!("- [decision] {}", first_sentence(&d.content));
            let line_len = line.len() + 1;
            if char_count + line_len <= budget_chars {
                cb_lines.push(line);
                char_count += line_len;
            }
        }

        if !cb_lines.is_empty() {
            context_parts.push(format!(
                "**Codebase ({}):**\n{}",
                codebase,
                cb_lines.join("\n")
            ));
        }
    }

    // ====================================================================
    // 6. Assemble final response
    // ====================================================================
    let header = format!("## Session ({} memories, {})\n", stats.total_nodes, status);
    let context_text = format!("{}{}", header, context_parts.join("\n\n"));
    let tokens_used = context_text.len() / 4;

    Ok(serde_json::json!({
        "context": context_text,
        "tokensUsed": tokens_used,
        "tokenBudget": token_budget,
        "expandable": expandable_ids,
        "automationTriggers": {
            "needsDream": needs_dream,
            "needsBackup": needs_backup,
            "needsGc": needs_gc,
        },
    }))
}

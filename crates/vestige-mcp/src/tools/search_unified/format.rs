//! Result formatting helpers shared across MCP tools.

use serde_json::Value;

/// The anchors a result carries, as the reader sees them.
///
/// This is the read half of the design's one rule: a memory that cites code must
/// not be returned as fact when the citation no longer holds. The verdict and
/// its one-line reason travel with the result, so a reader learns "this anchor
/// is stale" instead of trusting a path that silently points at an old copy.
///
/// `note` is computed here rather than stored: it is a pure function of the
/// verdict, the anchor and when the check ran, and persisting prose that can be
/// recomputed is how a row's text drifts away from its columns.
pub(crate) fn code_refs_json(code_refs: &[vestige_core::CodeRef]) -> Option<Value> {
    if code_refs.is_empty() {
        return None;
    }
    Some(Value::Array(
        code_refs
            .iter()
            .map(|code_ref| {
                let anchor = &code_ref.anchor;
                serde_json::json!({
                    "reference": anchor.reference(),
                    "path": anchor.path,
                    "symbol": anchor.symbol,
                    "commit": anchor.commit_sha,
                    // A hint, and named as one: nothing resolves through it.
                    "hintLine": anchor.hint_line,
                    "verdict": code_ref.verdict,
                    "note": vestige_core::code_refs::verdict_note(
                        code_ref.verdict,
                        anchor,
                        code_ref.resolved_at,
                    ),
                    "resolvedAt": code_ref.resolved_at.map(|at| at.to_rfc3339()),
                })
            })
            .collect(),
    ))
}

/// Format a search result based on the requested detail level.
///
/// `code_refs` are the anchors stored for this memory, empty when it has none.
/// Every detail level carries them: `brief` is what a reader scans first, and an
/// anchor verdict that only appears at `full` is a warning most readers never
/// reach.
///
/// `as_of` is the record time the answer was asked for, if any. When it is set,
/// the result gains the two fields that make the answer readable: the memory's
/// own `recordedAt` (how old the belief already was at that instant — `brief`
/// otherwise drops dates) and `validityAtAsOf`, the valid-time window's standing
/// *at that instant*. The window is reported rather than filtered on, because
/// "what did we believe then" and "what was true then" are different questions
/// and one flag cannot answer both.
pub(super) fn format_search_result(
    r: &vestige_core::SearchResult,
    detail_level: &str,
    code_refs: &[vestige_core::CodeRef],
    as_of: Option<chrono::DateTime<chrono::Utc>>,
) -> Value {
    let anchors = code_refs_json(code_refs);
    let mut formatted = match detail_level {
        "brief" => serde_json::json!({
            "id": r.node.id,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "combinedScore": r.combined_score,
        }),
        "full" => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "storageStrength": r.node.storage_strength,
            "retrievalStrength": r.node.retrieval_strength,
            "source": r.node.source,
            "sentimentScore": r.node.sentiment_score,
            "sentimentMagnitude": r.node.sentiment_magnitude,
            "createdAt": r.node.created_at.to_rfc3339(),
            "updatedAt": r.node.updated_at.to_rfc3339(),
            // When we learned this, as opposed to when the row was created or when
            // it was last touched: the reader needs the record time to judge how the
            // picture has changed since.
            "recordedAt": r.node.recorded_at.to_rfc3339(),
            "lastAccessed": r.node.last_accessed.to_rfc3339(),
            "nextReview": r.node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": r.node.stability,
            "difficulty": r.node.difficulty,
            "reps": r.node.reps,
            "lapses": r.node.lapses,
            "validFrom": r.node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": r.node.valid_until.map(|dt| dt.to_rfc3339()),
            "matchType": format!("{:?}", r.match_type),
            "epistemicStatus": r.node.epistemic_status().to_string(),
            "memorySystem": r.node.memory_system().to_string(),
            "provenance": r.node.provenance,
        }),
        // "summary" (default) — includes dates so AI never has to guess when a memory is from
        _ => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            // What the memory is *about*, in the one field that names it. A memory
            // reading "the panel showed horizontal bands" identifies nothing on its
            // own; the project name lives here, and the reader at the default detail
            // level used to be the one reader who could not see it.
            "source": r.node.source,
            "createdAt": r.node.created_at.to_rfc3339(),
            "updatedAt": r.node.updated_at.to_rfc3339(),
            // When we learned this, as opposed to when the row was created or when
            // it was last touched: the reader needs the record time to judge how the
            // picture has changed since.
            "recordedAt": r.node.recorded_at.to_rfc3339(),
            "epistemicStatus": r.node.epistemic_status().to_string(),
            "memorySystem": r.node.memory_system().to_string(),
        }),
    };
    if let Some(anchors) = anchors {
        formatted["codeRefs"] = anchors;
    }
    if let Some(at) = as_of {
        if formatted.get("recordedAt").is_none() {
            formatted["recordedAt"] = serde_json::json!(r.node.recorded_at.to_rfc3339());
        }
        formatted["validityAtAsOf"] = serde_json::json!(r.node.validity_at(at).as_str());
    }
    formatted
}

/// The first `limit` characters of a revision's text, plus whether it was cut.
///
/// Character-based, not byte-based: a byte slice would panic on a multi-byte
/// boundary, and half a character is not a truncation a reader can act on.
fn capped(text: Option<&str>, limit: usize) -> (Option<String>, bool) {
    match text {
        None => (None, false),
        Some(text) if text.chars().count() <= limit => (Some(text.to_string()), false),
        Some(text) => (Some(text.chars().take(limit).collect()), true),
    }
}

/// One step of a memory's content timeline, as `memory_changelog` and
/// `temporal history` both render it.
///
/// The shape is shared on purpose: a reader who learns "`oldContentTruncated`
/// means the text was cut at `contentLimitChars`" in one tool must not have to
/// learn a second shape in the other.
///
/// `contentField` names what `oldContent` / `newContent` actually hold. For
/// every kind but `invalidate` they are the memory's text; an `invalidate`
/// stores the previous and new `valid_until` instead, because that is the state
/// that operation changed — without the label those two values read as text and
/// a reader would take a timestamp for prose.
pub fn revision_json(revision: &vestige_core::MemoryRevision, content_limit_chars: usize) -> Value {
    use vestige_core::RevisionKind;

    let field = match revision.kind {
        RevisionKind::Invalidate => "validUntil",
        _ => "content",
    };
    let (old_content, old_truncated) = capped(revision.old_content.as_deref(), content_limit_chars);
    let (new_content, new_truncated) = capped(revision.new_content.as_deref(), content_limit_chars);

    serde_json::json!({
        "id": revision.id,
        "recordedAt": revision.recorded_at.to_rfc3339(),
        "kind": revision.kind.as_str(),
        "contentField": field,
        "oldContent": old_content,
        "newContent": new_content,
        "oldContentTruncated": old_truncated,
        "newContentTruncated": new_truncated,
        "reason": revision.reason,
        "actor": revision.actor,
    })
}

/// A memory's content history as a timeline: oldest step first.
///
/// `newest_first` is what storage returns, because that is the order a *page*
/// wants (the newest N). A timeline is read forward, so the page is reversed
/// here: the caller gets the newest N steps in the order they happened, which is
/// the order that answers "how did this change" and "what did it say on date X".
pub fn revision_timeline_json(
    newest_first: &[vestige_core::MemoryRevision],
    content_limit_chars: usize,
) -> Vec<Value> {
    newest_first
        .iter()
        .rev()
        .map(|revision| revision_json(revision, content_limit_chars))
        .collect()
}

/// Format a KnowledgeNode based on the requested detail level.
/// Reusable across search, timeline, and other tools.
pub fn format_node(node: &vestige_core::KnowledgeNode, detail_level: &str) -> Value {
    match detail_level {
        "brief" => serde_json::json!({
            "id": node.id,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
        }),
        "full" => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
            "storageStrength": node.storage_strength,
            "retrievalStrength": node.retrieval_strength,
            "source": node.source,
            "sentimentScore": node.sentiment_score,
            "sentimentMagnitude": node.sentiment_magnitude,
            "createdAt": node.created_at.to_rfc3339(),
            "updatedAt": node.updated_at.to_rfc3339(),
            "recordedAt": node.recorded_at.to_rfc3339(),
            "lastAccessed": node.last_accessed.to_rfc3339(),
            "nextReview": node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": node.stability,
            "difficulty": node.difficulty,
            "reps": node.reps,
            "lapses": node.lapses,
            "validFrom": node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": node.valid_until.map(|dt| dt.to_rfc3339()),
            "provenance": node.provenance,
        }),
        // "summary" (default)
        _ => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
            // Same reason as in `format_search_result`: without it, a memory that
            // names no subject cannot be placed by the reader.
            "source": node.source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Truncation is character-based. A byte slice would panic on a multi-byte
    /// boundary, and this store is bilingual — the memories it truncates are
    /// exactly the ones that contain Polish text.
    #[test]
    fn capped_cuts_on_character_boundaries() {
        let text = "ż".repeat(600);
        let (cut, truncated) = capped(Some(&text), 500);

        assert!(truncated, "600 characters must be reported as cut at 500");
        assert_eq!(cut.unwrap().chars().count(), 500);

        let (whole, truncated) = capped(Some("krótko"), 500);
        assert!(!truncated);
        assert_eq!(whole.as_deref(), Some("krótko"));

        let (absent, truncated) = capped(None, 500);
        assert!(absent.is_none());
        assert!(!truncated, "a revision with no text is not a truncated one");
    }
}

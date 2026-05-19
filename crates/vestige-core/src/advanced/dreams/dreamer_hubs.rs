//! Topic Hub generation (Proposal A — `docs/TOPIC-HUBS-DESIGN.md`).
//!
//! Inherent methods on [`MemoryDreamer`] that turn a list of
//! pre-clustered memories into [`HubCandidate`]s. Persistence is **not**
//! this module's job — the dream tool collects candidates from the
//! [`DreamResult`] and writes them to storage with full provenance,
//! the same dual-write pattern used for [`SynthesizedInsight`] →
//! `KnowledgeNode<insight>`.
//!
//! Generation pipeline:
//!   1. For each cluster passing the size/quality gates, hash its
//!      sorted member IDs into a stable `cluster_signature`.
//!   2. Compute dominant tags (tags that appear in ≥ half the cluster).
//!   3. Render the template body — N members, date span, top tags,
//!      and a leading sentence from up to three memories.
//!   4. Stamp `generation_method = "template_v1"` so future LLM
//!      variants can A/B side-by-side without dropping the template.
//!
//! Phase 2 (LLM synthesis behind `VESTIGE_HUB_SYNTHESIS=llm`) is out of
//! scope here — it would call into an extractor and replace `content`
//! while leaving every other field intact.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use super::dreamer::MemoryDreamer;
use super::types::{DreamMemory, DreamStats, HubCandidate};
use crate::memory::cluster_signature;

/// Minimum members for a cluster to be promoted into a hub. Matches
/// the design doc (≥5) — small clusters are noise.
const MIN_HUB_CLUSTER_SIZE: usize = 5;
/// How many top-tagged excerpts we render in the template body.
const HUB_TEMPLATE_EXCERPT_COUNT: usize = 3;
/// Hard ceiling on tags carried in `dominant_tags` — matches the
/// 16-tag dashboard chip budget (also enforced by `HubMetadata`).
const HUB_MAX_DOMINANT_TAGS: usize = 16;

impl MemoryDreamer {
    /// Build [`HubCandidate`]s from clusters discovered earlier in the
    /// dream cycle. Filters out anything smaller than
    /// [`MIN_HUB_CLUSTER_SIZE`]; everything else gets a deterministic
    /// signature, a template body, and the metadata needed for
    /// downstream dedup/storage.
    ///
    /// Returns an empty vector when no cluster qualifies. Callers must
    /// not assume the output order matches `clusters` — duplicates
    /// (same signature) are dropped.
    pub(super) fn generate_hub_candidates(
        &self,
        memories: &[&DreamMemory],
        clusters: &[Vec<String>],
        stats: &mut DreamStats,
    ) -> Vec<HubCandidate> {
        let mut out = Vec::new();
        let mut seen_signatures = HashSet::new();

        for cluster in clusters {
            // Resolve cluster IDs back to memory references. Drop
            // anything we can't find (the dream pipeline can stage
            // memories that get evicted before persistence).
            let members: Vec<&&DreamMemory> = cluster
                .iter()
                .filter_map(|id| memories.iter().find(|m| &m.id == id))
                .collect();

            if members.len() < MIN_HUB_CLUSTER_SIZE {
                continue;
            }

            let mut child_ids: Vec<String> =
                members.iter().map(|m| m.id.clone()).collect();
            child_ids.sort_unstable();
            child_ids.dedup();

            let signature = cluster_signature(&child_ids);

            // Same physical cluster surfaced twice in the same dream
            // (rare but possible with the union-find merge) — emit
            // once.
            if !seen_signatures.insert(signature.clone()) {
                continue;
            }

            let dominant_tags = compute_dominant_tags(&members);
            let date_range = compute_date_range(&members);
            let content = render_template_body(
                &members,
                &dominant_tags,
                date_range,
            );

            let mut node_tags: Vec<String> = vec![
                "hub".into(),
                "auto-generated".into(),
            ];
            node_tags.extend(dominant_tags.iter().take(8).cloned());

            out.push(HubCandidate {
                cluster_signature: signature,
                child_ids,
                dominant_tags,
                date_range,
                content,
                generation_method: "template_v1".into(),
                tags: node_tags,
            });
        }

        stats.hub_candidates_generated = out.len();
        out
    }
}

/// Tags that appear on **more than half** of the cluster members,
/// sorted by frequency descending and capped at
/// [`HUB_MAX_DOMINANT_TAGS`]. Empty when no tag is broadly shared —
/// a hub with no dominant tags is still useful (we render the date
/// span and member count) so we don't gate hub creation on this.
fn compute_dominant_tags(members: &[&&DreamMemory]) -> Vec<String> {
    use std::collections::HashMap;

    if members.is_empty() {
        return Vec::new();
    }
    let half = members.len() / 2;

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for m in members {
        // Dedup per-member so a memory tagged ["rust", "rust"] only
        // votes once.
        let unique: HashSet<&str> = m.tags.iter().map(String::as_str).collect();
        for t in unique {
            *counts.entry(t).or_insert(0) += 1;
        }
    }

    let mut ranked: Vec<(&str, usize)> = counts
        .into_iter()
        .filter(|(_, c)| *c > half)
        .collect();
    // Sort by frequency desc, tie-break on tag string for determinism.
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    ranked.truncate(HUB_MAX_DOMINANT_TAGS);

    ranked.into_iter().map(|(t, _)| t.to_string()).collect()
}

fn compute_date_range(members: &[&&DreamMemory]) -> (DateTime<Utc>, DateTime<Utc>) {
    let first = members[0].created_at;
    members.iter().fold((first, first), |(lo, hi), m| {
        (lo.min(m.created_at), hi.max(m.created_at))
    })
}

fn render_template_body(
    members: &[&&DreamMemory],
    dominant_tags: &[String],
    date_range: (DateTime<Utc>, DateTime<Utc>),
) -> String {
    let tag_summary = if dominant_tags.is_empty() {
        "(no shared tags)".to_string()
    } else {
        dominant_tags
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };

    let date_fmt = format!(
        "{} – {}",
        date_range.0.format("%Y-%m-%d"),
        date_range.1.format("%Y-%m-%d"),
    );

    let mut excerpts = Vec::with_capacity(HUB_TEMPLATE_EXCERPT_COUNT);
    for m in members.iter().take(HUB_TEMPLATE_EXCERPT_COUNT) {
        excerpts.push(format!("- {}", first_sentence(&m.content)));
    }

    format!(
        "[Topic Hub: {tag_summary}]\n\nThis cluster contains {n} memories ({date_fmt}) about {tag_summary}.\n\nKey threads:\n{excerpts}",
        n = members.len(),
        excerpts = excerpts.join("\n"),
    )
}

/// Take everything up to the first `.`, `?`, or `!` plus that mark,
/// or the first 160 chars if no sentence boundary is found. Strips
/// embedded newlines so the resulting line fits in the hub body.
fn first_sentence(content: &str) -> String {
    let trimmed = content.trim();
    let bytes = trimmed.as_bytes();
    let mut cut = trimmed.len().min(160);
    for (i, b) in bytes.iter().enumerate().take(cut) {
        if matches!(*b, b'.' | b'?' | b'!') {
            cut = i + 1;
            break;
        }
    }
    trimmed[..cut].replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn mem(id: &str, content: &str, tags: &[&str], days_ago: i64) -> DreamMemory {
        DreamMemory {
            id: id.into(),
            content: content.into(),
            embedding: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            created_at: Utc::now() - Duration::days(days_ago),
            access_count: 1,
        }
    }

    fn cluster_of(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn skips_clusters_below_min_size() {
        let dreamer = MemoryDreamer::new();
        let pool = vec![
            mem("a", "Rust async is hard.", &["rust"], 1),
            mem("b", "Tokio runtime quirks.", &["rust"], 2),
        ];
        let refs: Vec<&DreamMemory> = pool.iter().collect();
        let clusters = vec![cluster_of(&["a", "b"])];
        let mut stats = DreamStats::default();
        let out = dreamer.generate_hub_candidates(&refs, &clusters, &mut stats);
        assert!(out.is_empty());
        assert_eq!(stats.hub_candidates_generated, 0);
    }

    #[test]
    fn generates_hub_for_qualifying_cluster() {
        let dreamer = MemoryDreamer::new();
        let pool: Vec<DreamMemory> = (0..6)
            .map(|i| {
                mem(
                    &format!("m{i}"),
                    &format!("Memory {i} about Rust async patterns. More text here."),
                    &["rust", "async"],
                    i as i64,
                )
            })
            .collect();
        let refs: Vec<&DreamMemory> = pool.iter().collect();
        let cluster: Vec<String> = pool.iter().map(|m| m.id.clone()).collect();
        let mut stats = DreamStats::default();
        let out = dreamer.generate_hub_candidates(&refs, &[cluster], &mut stats);

        assert_eq!(out.len(), 1);
        assert_eq!(stats.hub_candidates_generated, 1);

        let hub = &out[0];
        assert_eq!(hub.child_ids.len(), 6);
        assert!(hub.dominant_tags.contains(&"rust".to_string()));
        assert!(hub.dominant_tags.contains(&"async".to_string()));
        assert!(hub.tags.iter().any(|t| t == "hub"));
        assert_eq!(hub.generation_method, "template_v1");
        assert!(hub.content.contains("Topic Hub"));
        assert!(hub.content.contains("6 memories"));

        // The signature should be deterministic regardless of order.
        let s1 = hub.cluster_signature.clone();
        let mut reversed = hub.child_ids.clone();
        reversed.reverse();
        assert_eq!(cluster_signature(&reversed), s1);
    }

    #[test]
    fn deduplicates_clusters_with_identical_signature() {
        let dreamer = MemoryDreamer::new();
        let pool: Vec<DreamMemory> = (0..5)
            .map(|i| mem(&format!("m{i}"), "x", &["rust"], i as i64))
            .collect();
        let refs: Vec<&DreamMemory> = pool.iter().collect();
        let ids: Vec<String> = pool.iter().map(|m| m.id.clone()).collect();
        // Same cluster, presented twice with different ordering.
        let mut alt = ids.clone();
        alt.reverse();
        let clusters = vec![ids, alt];
        let mut stats = DreamStats::default();
        let out = dreamer.generate_hub_candidates(&refs, &clusters, &mut stats);
        assert_eq!(out.len(), 1, "second presentation must be skipped");
    }

    #[test]
    fn rejects_unknown_member_ids_silently() {
        let dreamer = MemoryDreamer::new();
        let pool: Vec<DreamMemory> = (0..5)
            .map(|i| mem(&format!("m{i}"), "x", &["rust"], i as i64))
            .collect();
        let refs: Vec<&DreamMemory> = pool.iter().collect();
        let cluster = vec![
            "m0".into(),
            "m1".into(),
            "m2".into(),
            "ghost".into(),
            "m3".into(),
            "m4".into(),
        ];
        let mut stats = DreamStats::default();
        let out = dreamer.generate_hub_candidates(&refs, &[cluster], &mut stats);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].child_ids.len(), 5);
        assert!(!out[0].child_ids.contains(&"ghost".to_string()));
    }

    #[test]
    fn first_sentence_picks_up_to_first_period() {
        assert_eq!(first_sentence("Hello world. Goodbye."), "Hello world.");
        assert_eq!(first_sentence("Just one fragment"), "Just one fragment");
        assert_eq!(first_sentence("Quick? Yes."), "Quick?");
    }

    #[test]
    fn first_sentence_truncates_long_input() {
        let long = "a".repeat(500);
        let s = first_sentence(&long);
        assert!(s.len() <= 160);
    }

    #[test]
    fn dominant_tags_uses_majority_rule() {
        // 5 members; "rust" appears on 4 (>2 = majority), "async" on 3
        // (>2 = majority), "obscure" on 1 (≤2 = below threshold).
        let pool: Vec<DreamMemory> = vec![
            mem("a", "x", &["rust", "async"], 1),
            mem("b", "x", &["rust", "async"], 1),
            mem("c", "x", &["rust", "async"], 1),
            mem("d", "x", &["rust"], 1),
            mem("e", "x", &["obscure"], 1),
        ];
        let refs: Vec<&DreamMemory> = pool.iter().collect();
        let borrowed: Vec<&&DreamMemory> = refs.iter().collect();
        let tags = compute_dominant_tags(&borrowed);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"async".to_string()));
        assert!(!tags.contains(&"obscure".to_string()));
    }

    #[test]
    fn dominant_tags_ranked_by_frequency_then_alpha() {
        // 6 members. Counts: "rust"=6, "tokio"=4, "async"=4, "perf"=4.
        // All three minority tags tie at 4 (>3 = majority), so they
        // should be sorted alphabetically: async, perf, tokio.
        let pool: Vec<DreamMemory> = vec![
            mem("a", "x", &["rust", "tokio", "async", "perf"], 1),
            mem("b", "x", &["rust", "tokio", "async", "perf"], 1),
            mem("c", "x", &["rust", "tokio", "async", "perf"], 1),
            mem("d", "x", &["rust", "tokio", "async", "perf"], 1),
            mem("e", "x", &["rust"], 1),
            mem("f", "x", &["rust"], 1),
        ];
        let refs: Vec<&DreamMemory> = pool.iter().collect();
        let borrowed: Vec<&&DreamMemory> = refs.iter().collect();
        let tags = compute_dominant_tags(&borrowed);
        assert_eq!(tags[0], "rust", "highest-frequency tag first");
        // Remaining three are tied; alphabetical: async, perf, tokio.
        assert_eq!(&tags[1..], &["async", "perf", "tokio"]);
    }
}

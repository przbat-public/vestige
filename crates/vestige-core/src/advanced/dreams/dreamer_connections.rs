//! Phase 1 of the dream cycle: discover pairwise connections between memories,
//! compute similarity, classify the connection type.
//!
//! Extracted from `dreamer.rs` to keep that file focused on the public surface
//! and orchestration. All methods are inherent on [`MemoryDreamer`].

use std::collections::HashSet;

use super::dreamer::MemoryDreamer;
use super::similarity::{contains_word, cosine_similarity, truncate};
use super::types::{DiscoveredConnection, DiscoveredConnectionType, DreamMemory, DreamStats};

impl MemoryDreamer {
    /// Compare every pair of memories in `memories` and emit a
    /// [`DiscoveredConnection`] whenever similarity clears the configured
    /// threshold.
    pub(super) fn discover_connections(
        &self,
        memories: &[&DreamMemory],
        stats: &mut DreamStats,
    ) -> Vec<DiscoveredConnection> {
        let mut connections = Vec::new();

        for i in 0..memories.len() {
            for j in (i + 1)..memories.len() {
                stats.connections_evaluated += 1;

                let mem_a = &memories[i];
                let mem_b = &memories[j];

                let similarity = self.calculate_similarity(mem_a, mem_b);

                if similarity >= self.config.min_similarity {
                    let connection_type = self.determine_connection_type(mem_a, mem_b, similarity);
                    let reasoning =
                        self.generate_connection_reasoning(mem_a, mem_b, &connection_type);

                    connections.push(DiscoveredConnection {
                        from_id: mem_a.id.clone(),
                        to_id: mem_b.id.clone(),
                        similarity,
                        connection_type,
                        reasoning,
                    });
                }
            }
        }

        connections
    }

    fn calculate_similarity(&self, a: &DreamMemory, b: &DreamMemory) -> f64 {
        // Primary: embedding similarity when both sides have a vector.
        if let (Some(emb_a), Some(emb_b)) = (&a.embedding, &b.embedding) {
            return cosine_similarity(emb_a, emb_b);
        }

        // Fallback: blended tag + content similarity. Tunable mix matters less
        // than just having something when embeddings are missing.
        let tag_sim = self.tag_similarity(&a.tags, &b.tags);
        let content_sim = self.content_similarity(&a.content, &b.content);

        tag_sim * 0.4 + content_sim * 0.6
    }

    pub(super) fn tag_similarity(&self, tags_a: &[String], tags_b: &[String]) -> f64 {
        if tags_a.is_empty() && tags_b.is_empty() {
            return 0.0;
        }

        let set_a: HashSet<_> = tags_a.iter().collect();
        let set_b: HashSet<_> = tags_b.iter().collect();

        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    fn content_similarity(&self, content_a: &str, content_b: &str) -> f64 {
        // Jaccard over words longer than 3 chars — cheap proxy when no
        // embedding is available.
        let words_a: HashSet<_> = content_a
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() > 3)
            .collect();

        let words_b: HashSet<_> = content_b
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() > 3)
            .collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    fn determine_connection_type(
        &self,
        a: &DreamMemory,
        b: &DreamMemory,
        similarity: f64,
    ) -> DiscoveredConnectionType {
        // High similarity + diverging negation markers → contradiction.
        if similarity > 0.7 && Self::has_negation_divergence(&a.content, &b.content) {
            return DiscoveredConnectionType::Contradiction;
        }

        let shared_tags = a.tags.iter().filter(|t| b.tags.contains(t)).count();
        if shared_tags >= 2 {
            return DiscoveredConnectionType::SharedConcept;
        }

        let time_diff = (a.created_at - b.created_at).num_hours().abs();
        if time_diff <= 24 && similarity > 0.6 {
            return DiscoveredConnectionType::Temporal;
        }

        if similarity > 0.8 {
            return DiscoveredConnectionType::Semantic;
        }

        DiscoveredConnectionType::Complementary
    }

    /// Heuristic: two texts about the same topic but with opposing stance.
    /// Looks for negation markers appearing in one but not the other.
    /// Uses word-boundary matching to avoid substring false positives
    /// (e.g. "nie" inside "poprawnie").
    pub(super) fn has_negation_divergence(a: &str, b: &str) -> bool {
        const NEGATION_MARKERS: &[&str] = &[
            "not",
            "don't",
            "doesn't",
            "didn't",
            "won't",
            "can't",
            "cannot",
            "never",
            "no longer",
            "stopped",
            "removed",
            "deprecated",
            "nie",
            "nigdy",
            "przestał",
            "usunięto",
            "nieprawidłow",
            "incorrect",
            "wrong",
            "false",
            "broken",
            "failed",
        ];
        let a_low = a.to_lowercase();
        let b_low = b.to_lowercase();
        let a_neg = NEGATION_MARKERS
            .iter()
            .filter(|m| contains_word(&a_low, m))
            .count();
        let b_neg = NEGATION_MARKERS
            .iter()
            .filter(|m| contains_word(&b_low, m))
            .count();
        (a_neg as i32 - b_neg as i32).unsigned_abs() >= 2
    }

    fn generate_connection_reasoning(
        &self,
        a: &DreamMemory,
        b: &DreamMemory,
        conn_type: &DiscoveredConnectionType,
    ) -> String {
        match conn_type {
            DiscoveredConnectionType::Semantic => format!(
                "High semantic similarity between '{}...' and '{}...'",
                truncate(&a.content, 30),
                truncate(&b.content, 30)
            ),
            DiscoveredConnectionType::SharedConcept => {
                let shared: Vec<_> = a.tags.iter().filter(|t| b.tags.contains(t)).collect();
                format!("Shared concepts: {:?}", shared)
            }
            DiscoveredConnectionType::Temporal => "Created within close time proximity".to_string(),
            DiscoveredConnectionType::Complementary => {
                "Memories provide complementary information".to_string()
            }
            DiscoveredConnectionType::CausalChain => {
                "Potential cause-effect relationship".to_string()
            }
            DiscoveredConnectionType::Contradiction => {
                format!(
                    "Contradiction detected: '{}...' vs '{}...'",
                    truncate(&a.content, 30),
                    truncate(&b.content, 30)
                )
            }
        }
    }
}

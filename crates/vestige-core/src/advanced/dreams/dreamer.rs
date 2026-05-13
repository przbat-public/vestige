//! `MemoryDreamer` — phases of a single dream cycle: discover connections,
//! cluster them, detect contradictions, generate insights, and identify
//! memories to strengthen or compress.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::constants::MIN_MEMORIES_FOR_INSIGHT;
use super::similarity::{contains_word, cosine_similarity, truncate};
use super::types::{
    ContradictionPair, DiscoveredConnection, DiscoveredConnectionType, DreamConfig, DreamMemory,
    DreamResult, DreamStats, InsightType, SynthesizedInsight,
};

/// Memory dreamer for enhanced consolidation
#[derive(Debug)]
pub struct MemoryDreamer {
    /// Configuration
    config: DreamConfig,
    /// Dream history
    dream_history: Arc<RwLock<Vec<DreamResult>>>,
    /// Generated insights (persisted separately)
    insights: Arc<RwLock<Vec<SynthesizedInsight>>>,
    /// Discovered connections
    connections: Arc<RwLock<Vec<DiscoveredConnection>>>,
}

impl MemoryDreamer {
    /// Create a new memory dreamer with default config
    pub fn new() -> Self {
        Self::with_config(DreamConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: DreamConfig) -> Self {
        Self {
            config,
            dream_history: Arc::new(RwLock::new(Vec::new())),
            insights: Arc::new(RwLock::new(Vec::new())),
            connections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Run a dream cycle on provided memories
    pub async fn dream(&self, memories: &[DreamMemory]) -> DreamResult {
        let start = std::time::Instant::now();
        let mut stats = DreamStats::default();

        // Filter memories based on config
        let working_memories: Vec<_> = if self.config.focus_tags.is_empty() {
            memories
                .iter()
                .take(self.config.max_memories_per_dream)
                .collect()
        } else {
            memories
                .iter()
                .filter(|m| m.tags.iter().any(|t| self.config.focus_tags.contains(t)))
                .take(self.config.max_memories_per_dream)
                .collect()
        };

        stats.memories_analyzed = working_memories.len();

        // Phase 1: Discover new connections
        let new_connections = self.discover_connections(&working_memories, &mut stats);

        // Phase 2: Find clusters/patterns
        let clusters = self.find_clusters(&working_memories, &new_connections);
        stats.clusters_found = clusters.len();

        // Phase 3: Detect contradictions (active forgetting)
        let contradictions = self.detect_contradictions(&working_memories, &new_connections);
        let memories_demoted: Vec<String> = contradictions
            .iter()
            .map(|c| c.demoted_id.clone())
            .collect();

        // Phase 4: Generate insights (contradictions become Contradiction-type insights)
        let insights = self.generate_insights(&working_memories, &clusters, &mut stats);

        // Phase 5: Strengthen important memories (would update storage)
        let memories_strengthened = if self.config.enable_strengthening {
            self.identify_memories_to_strengthen(&working_memories, &new_connections)
        } else {
            0
        };

        // Phase 6: Identify compression candidates (would compress in storage)
        let memories_compressed = if self.config.enable_compression {
            self.identify_compression_candidates(&working_memories)
        } else {
            0
        };

        // Store results
        self.store_connections(&new_connections);
        self.store_insights(&insights);

        let result = DreamResult {
            new_connections_found: new_connections.len(),
            memories_strengthened,
            memories_compressed,
            insights_generated: insights,
            contradictions_found: contradictions,
            memories_demoted,
            duration_ms: start.elapsed().as_millis() as u64,
            dreamed_at: Utc::now(),
            stats,
        };

        // Store in history
        if let Ok(mut history) = self.dream_history.write() {
            history.push(result.clone());
            // Keep last 100 dreams
            if history.len() > 100 {
                history.remove(0);
            }
        }

        result
    }

    /// Synthesize insights from memories without full dream cycle
    pub fn synthesize_insights(&self, memories: &[DreamMemory]) -> Vec<SynthesizedInsight> {
        let mut stats = DreamStats::default();

        // Find clusters
        let connections =
            self.discover_connections(&memories.iter().collect::<Vec<_>>(), &mut stats);
        let clusters = self.find_clusters(&memories.iter().collect::<Vec<_>>(), &connections);

        // Generate insights
        self.generate_insights(&memories.iter().collect::<Vec<_>>(), &clusters, &mut stats)
    }

    /// Get all generated insights
    pub fn get_insights(&self) -> Vec<SynthesizedInsight> {
        self.insights.read().map(|i| i.clone()).unwrap_or_default()
    }

    /// Get insights by type
    pub fn get_insights_by_type(&self, insight_type: &InsightType) -> Vec<SynthesizedInsight> {
        self.insights
            .read()
            .map(|insights| {
                insights
                    .iter()
                    .filter(|i| {
                        std::mem::discriminant(&i.insight_type)
                            == std::mem::discriminant(insight_type)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get dream history
    pub fn get_dream_history(&self) -> Vec<DreamResult> {
        self.dream_history
            .read()
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    /// Get discovered connections
    pub fn get_connections(&self) -> Vec<DiscoveredConnection> {
        self.connections
            .read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    // ========================================================================
    // Private implementation
    // ========================================================================

    pub(super) fn discover_connections(
        &self,
        memories: &[&DreamMemory],
        stats: &mut DreamStats,
    ) -> Vec<DiscoveredConnection> {
        let mut connections = Vec::new();

        // Compare each pair of memories
        for i in 0..memories.len() {
            for j in (i + 1)..memories.len() {
                stats.connections_evaluated += 1;

                let mem_a = &memories[i];
                let mem_b = &memories[j];

                // Calculate similarity
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
        // Primary: embedding similarity
        if let (Some(emb_a), Some(emb_b)) = (&a.embedding, &b.embedding) {
            return cosine_similarity(emb_a, emb_b);
        }

        // Fallback: tag overlap + content similarity
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
        // Simple word overlap (Jaccard)
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
        // High similarity + negation signals → contradiction
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

    fn find_clusters(
        &self,
        _memories: &[&DreamMemory],
        connections: &[DiscoveredConnection],
    ) -> Vec<Vec<String>> {
        // Simple clustering based on connections
        let mut clusters: Vec<HashSet<String>> = Vec::new();

        for conn in connections {
            // Find existing cluster containing either endpoint
            let mut found_cluster = None;
            for (i, cluster) in clusters.iter().enumerate() {
                if cluster.contains(&conn.from_id) || cluster.contains(&conn.to_id) {
                    found_cluster = Some(i);
                    break;
                }
            }

            match found_cluster {
                Some(i) => {
                    clusters[i].insert(conn.from_id.clone());
                    clusters[i].insert(conn.to_id.clone());
                }
                None => {
                    let mut new_cluster = HashSet::new();
                    new_cluster.insert(conn.from_id.clone());
                    new_cluster.insert(conn.to_id.clone());
                    clusters.push(new_cluster);
                }
            }
        }

        // Merge overlapping clusters
        let mut merged = true;
        while merged {
            merged = false;
            for i in 0..clusters.len() {
                for j in (i + 1)..clusters.len() {
                    if !clusters[i].is_disjoint(&clusters[j]) {
                        let to_merge: HashSet<_> = clusters[j].drain().collect();
                        clusters[i].extend(to_merge);
                        merged = true;
                        break;
                    }
                }
                if merged {
                    clusters.retain(|c| !c.is_empty());
                    break;
                }
            }
        }

        // Convert to Vec<Vec<String>>
        clusters
            .into_iter()
            .filter(|c| c.len() >= MIN_MEMORIES_FOR_INSIGHT)
            .map(|c| c.into_iter().collect())
            .collect()
    }

    fn generate_insights(
        &self,
        memories: &[&DreamMemory],
        clusters: &[Vec<String>],
        stats: &mut DreamStats,
    ) -> Vec<SynthesizedInsight> {
        let mut insights = Vec::new();
        let memory_map: HashMap<_, _> = memories.iter().map(|m| (&m.id, *m)).collect();

        for cluster in clusters {
            stats.candidates_considered += 1;

            // Get memories in this cluster
            let cluster_memories: Vec<_> = cluster
                .iter()
                .filter_map(|id| memory_map.get(&id).copied())
                .collect();

            if cluster_memories.len() < MIN_MEMORIES_FOR_INSIGHT {
                continue;
            }

            // Try to generate insight from this cluster
            if let Some(insight) = self.generate_insight_from_cluster(&cluster_memories)
                && insight.novelty_score >= self.config.min_novelty
            {
                insights.push(insight);
            }

            if insights.len() >= self.config.max_insights {
                break;
            }
        }

        insights
    }

    fn generate_insight_from_cluster(
        &self,
        memories: &[&DreamMemory],
    ) -> Option<SynthesizedInsight> {
        if memories.is_empty() {
            return None;
        }

        // Collect all tags
        let all_tags: HashSet<_> = memories
            .iter()
            .flat_map(|m| m.tags.iter().cloned())
            .collect();

        // Find common themes
        let common_tags: Vec<_> = all_tags
            .iter()
            .filter(|t| {
                memories.iter().filter(|m| m.tags.contains(*t)).count() > memories.len() / 2
            })
            .cloned()
            .collect();

        // Generate insight based on cluster characteristics
        let (insight_text, insight_type) = self.synthesize_insight_text(memories, &common_tags);

        // Calculate novelty (simplified)
        let novelty = self.calculate_novelty(&insight_text, memories);

        // Calculate confidence based on cluster cohesion
        let confidence = self.calculate_insight_confidence(memories);

        Some(SynthesizedInsight {
            id: format!("insight-{}", Uuid::new_v4()),
            insight: insight_text,
            source_memories: memories.iter().map(|m| m.id.clone()).collect(),
            confidence,
            novelty_score: novelty,
            insight_type,
            generated_at: Utc::now(),
            tags: common_tags,
        })
    }

    fn synthesize_insight_text(
        &self,
        memories: &[&DreamMemory],
        common_tags: &[String],
    ) -> (String, InsightType) {
        // Determine insight type based on memory characteristics
        let time_range = memories
            .iter()
            .map(|m| m.created_at)
            .fold((Utc::now(), Utc::now() - Duration::days(365)), |acc, t| {
                (acc.0.min(t), acc.1.max(t))
            });

        let time_span_days = (time_range.1 - time_range.0).num_days();

        if time_span_days > 30 {
            // Temporal trend
            let insight = format!(
                "Pattern observed over {} days in '{}': recurring theme across {} related memories",
                time_span_days,
                common_tags.first().map(|s| s.as_str()).unwrap_or("topic"),
                memories.len()
            );
            (insight, InsightType::TemporalTrend)
        } else if common_tags.len() >= 2 {
            // Hidden connection
            let insight = format!(
                "Connection between '{}' and '{}' found across {} memories",
                common_tags.first().map(|s| s.as_str()).unwrap_or("A"),
                common_tags.get(1).map(|s| s.as_str()).unwrap_or("B"),
                memories.len()
            );
            (insight, InsightType::HiddenConnection)
        } else if memories.len() >= 3 {
            // Recurring pattern
            let insight = format!(
                "Recurring pattern in '{}': {} instances identified with common characteristics",
                common_tags.first().map(|s| s.as_str()).unwrap_or("topic"),
                memories.len()
            );
            (insight, InsightType::RecurringPattern)
        } else {
            // Synthesis
            let insight = format!(
                "Synthesis: {} related memories about '{}' suggest broader understanding",
                memories.len(),
                common_tags.first().map(|s| s.as_str()).unwrap_or("topic")
            );
            (insight, InsightType::Synthesis)
        }
    }

    fn calculate_novelty(&self, insight: &str, source_memories: &[&DreamMemory]) -> f64 {
        // Novelty = how different is the insight from source memories

        // Count unique words in insight not heavily present in sources
        let insight_words: HashSet<_> = insight
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() > 3)
            .collect();

        let source_words: HashSet<_> = source_memories
            .iter()
            .flat_map(|m| m.content.split_whitespace())
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() > 3)
            .collect();

        let novel_words = insight_words.difference(&source_words).count();
        let total_words = insight_words.len();

        if total_words == 0 {
            return 0.3; // Default low novelty
        }

        // Base novelty from word difference
        let word_novelty = (novel_words as f64 / total_words as f64) * 0.5;

        // Boost novelty if connecting multiple sources
        let source_bonus = ((source_memories.len() as f64 - 2.0) * 0.1).clamp(0.0, 0.3);

        (word_novelty + source_bonus + 0.2).min(1.0)
    }

    fn calculate_insight_confidence(&self, memories: &[&DreamMemory]) -> f64 {
        // Confidence based on:
        // 1. Number of supporting memories
        // 2. Access patterns of source memories
        // 3. Tag overlap

        let count_factor = (memories.len() as f64 / 5.0).min(1.0) * 0.4;

        let avg_access =
            memories.iter().map(|m| m.access_count as f64).sum::<f64>() / memories.len() as f64;
        let access_factor = (avg_access / 10.0).min(1.0) * 0.3;

        let tag_overlap = self.average_tag_overlap(memories);
        let tag_factor = tag_overlap * 0.3;

        (count_factor + access_factor + tag_factor).min(0.95)
    }

    fn average_tag_overlap(&self, memories: &[&DreamMemory]) -> f64 {
        if memories.len() < 2 {
            return 0.0;
        }

        let mut total_overlap = 0.0;
        let mut comparisons = 0;

        for i in 0..memories.len() {
            for j in (i + 1)..memories.len() {
                total_overlap += self.tag_similarity(&memories[i].tags, &memories[j].tags);
                comparisons += 1;
            }
        }

        if comparisons == 0 {
            0.0
        } else {
            total_overlap / comparisons as f64
        }
    }

    fn identify_memories_to_strengthen(
        &self,
        _memories: &[&DreamMemory],
        connections: &[DiscoveredConnection],
    ) -> usize {
        // Memories with many connections should be strengthened
        let mut connection_counts: HashMap<&str, usize> = HashMap::new();

        for conn in connections {
            *connection_counts.entry(&conn.from_id).or_insert(0) += 1;
            *connection_counts.entry(&conn.to_id).or_insert(0) += 1;
        }

        // Count memories with above-average connections
        let avg_connections = if connection_counts.is_empty() {
            0.0
        } else {
            connection_counts.values().sum::<usize>() as f64 / connection_counts.len() as f64
        };

        connection_counts
            .values()
            .filter(|&&count| count as f64 > avg_connections)
            .count()
    }

    fn identify_compression_candidates(&self, memories: &[&DreamMemory]) -> usize {
        let now = Utc::now();
        let old_threshold = now - Duration::days(60);

        memories
            .iter()
            .filter(|m| m.created_at < old_threshold && m.access_count < 3)
            .count()
            / 3
    }

    /// Detect contradictions and pick which memory to demote.
    ///
    /// Heuristic: when two memories share high similarity (same topic)
    /// but diverge in negation markers, the newer or more-accessed one
    /// is the "survivor" and the other becomes the demotion candidate.
    pub(super) fn detect_contradictions(
        &self,
        memories: &[&DreamMemory],
        connections: &[DiscoveredConnection],
    ) -> Vec<ContradictionPair> {
        let mem_map: HashMap<&str, &DreamMemory> =
            memories.iter().map(|m| (m.id.as_str(), *m)).collect();

        connections
            .iter()
            .filter(|c| c.connection_type == DiscoveredConnectionType::Contradiction)
            .filter_map(|c| {
                let a = mem_map.get(c.from_id.as_str())?;
                let b = mem_map.get(c.to_id.as_str())?;
                let (survivor, demoted) = if a.access_count > b.access_count {
                    (a, b)
                } else if b.access_count > a.access_count {
                    (b, a)
                } else if a.created_at > b.created_at {
                    (a, b)
                } else {
                    (b, a)
                };
                Some(ContradictionPair {
                    survivor_id: survivor.id.clone(),
                    demoted_id: demoted.id.clone(),
                    similarity: c.similarity,
                    reason: c.reasoning.clone(),
                })
            })
            .collect()
    }

    fn store_connections(&self, connections: &[DiscoveredConnection]) {
        if let Ok(mut stored) = self.connections.write() {
            stored.extend(connections.iter().cloned());
            // Keep last 1000 connections
            let len = stored.len();
            if len > 1000 {
                stored.drain(0..(len - 1000));
            }
        }
    }

    fn store_insights(&self, insights: &[SynthesizedInsight]) {
        if let Ok(mut stored) = self.insights.write() {
            stored.extend(insights.iter().cloned());
            // Keep last 500 insights
            let len = stored.len();
            if len > 500 {
                stored.drain(0..(len - 500));
            }
        }
    }
}

impl Default for MemoryDreamer {
    fn default() -> Self {
        Self::new()
    }
}

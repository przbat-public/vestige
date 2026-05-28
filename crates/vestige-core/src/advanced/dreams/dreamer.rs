//! `MemoryDreamer` — orchestrates one full dream cycle: discover connections,
//! cluster, detect contradictions, generate insights, identify
//! strengthen/compress candidates.
//!
//! The per-phase implementation lives in sibling modules:
//!
//! - [`dreamer_connections`](super::dreamer_connections) — Phase 1 (similarity + typing)
//! - [`dreamer_clustering`](super::dreamer_clustering) — Phase 2 (cluster build)
//! - [`dreamer_contradictions`](super::dreamer_contradictions) — Phase 3 (contradictions + demotion)
//! - [`dreamer_insights`](super::dreamer_insights) — Phase 4 (insight synthesis)
//! - [`dreamer_lifecycle`](super::dreamer_lifecycle) — Phases 5/6 (strengthen/compress) + storage helpers
//!
//! Struct fields are `pub(super)` so each phase module can reach into them
//! directly. They stay invisible outside the `dreams` parent module.

use std::sync::{Arc, RwLock};

use chrono::Utc;

use super::types::{
    DiscoveredConnection, DreamConfig, DreamMemory, DreamResult, DreamStats, HubCandidate,
    InsightType, SynthesizedInsight,
};

/// Memory dreamer for enhanced consolidation.
#[derive(Debug)]
#[allow(
    clippy::field_scoped_visibility_modifiers,
    reason = "Sibling submodules (`dreamer_*`) read and write these fields directly as part of a split-by-responsibility refactor; they share the parent's trust boundary."
)]
pub struct MemoryDreamer {
    pub(super) config: DreamConfig,
    pub(super) dream_history: Arc<RwLock<Vec<DreamResult>>>,
    pub(super) insights: Arc<RwLock<Vec<SynthesizedInsight>>>,
    pub(super) connections: Arc<RwLock<Vec<DiscoveredConnection>>>,
}

impl MemoryDreamer {
    /// Create a new memory dreamer with default config.
    pub fn new() -> Self {
        Self::with_config(DreamConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(config: DreamConfig) -> Self {
        Self {
            config,
            dream_history: Arc::new(RwLock::new(Vec::new())),
            insights: Arc::new(RwLock::new(Vec::new())),
            connections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Run a dream cycle on provided memories.
    pub async fn dream(&self, memories: &[DreamMemory]) -> DreamResult {
        let start = std::time::Instant::now();
        let mut stats = DreamStats::default();

        // Filter memories based on config.
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

        // Phase 1: Discover new connections.
        let new_connections = self.discover_connections(&working_memories, &mut stats);

        // Phase 2: Find clusters/patterns.
        let clusters = self.find_clusters(&working_memories, &new_connections);
        stats.clusters_found = clusters.len();

        // Phase 3: Detect contradictions (active forgetting).
        let contradictions = self.detect_contradictions(&working_memories, &new_connections);
        let memories_demoted: Vec<String> = contradictions
            .iter()
            .map(|c| c.demoted_id.clone())
            .collect();

        // Phase 4: Generate insights.
        let insights = self.generate_insights(&working_memories, &clusters, &mut stats);

        // Phase 4b: Generate Topic Hub candidates from the same clusters
        // (Proposal A). The dream tool decides whether to persist them
        // — this just produces drafts.
        let hub_candidates: Vec<HubCandidate> =
            self.generate_hub_candidates(&working_memories, &clusters, &mut stats);

        // Phase 5: Strengthen important memories (would update storage).
        let memories_strengthened = if self.config.enable_strengthening {
            self.identify_memories_to_strengthen(&working_memories, &new_connections)
        } else {
            0
        };

        // Phase 6: Identify compression candidates (would compress in storage).
        let memories_compressed = if self.config.enable_compression {
            self.identify_compression_candidates(&working_memories)
        } else {
            0
        };

        // Store results.
        self.store_connections(&new_connections);
        self.store_insights(&insights);

        let result = DreamResult {
            new_connections_found: new_connections.len(),
            memories_strengthened,
            memories_compressed,
            insights_generated: insights,
            contradictions_found: contradictions,
            memories_demoted,
            hub_candidates,
            duration_ms: start.elapsed().as_millis() as u64,
            dreamed_at: Utc::now(),
            stats,
        };

        // Store in history, bounded to last 100.
        if let Ok(mut history) = self.dream_history.write() {
            history.push(result.clone());
            if history.len() > 100 {
                history.remove(0);
            }
        }

        result
    }

    /// Synthesize insights from memories without running a full dream cycle.
    pub fn synthesize_insights(&self, memories: &[DreamMemory]) -> Vec<SynthesizedInsight> {
        let mut stats = DreamStats::default();

        let connections =
            self.discover_connections(&memories.iter().collect::<Vec<_>>(), &mut stats);
        let clusters = self.find_clusters(&memories.iter().collect::<Vec<_>>(), &connections);

        self.generate_insights(&memories.iter().collect::<Vec<_>>(), &clusters, &mut stats)
    }

    /// Synthesize Topic Hub candidates from memories without running a
    /// full dream cycle (Proposal A). Used by `tools/dream` which
    /// orchestrates persistence outside the `dream()` happy path.
    ///
    /// The returned candidates are draft hubs — same shape as those in
    /// [`DreamResult::hub_candidates`]. The caller decides whether to
    /// persist; the engine doesn't touch storage from here.
    pub fn synthesize_hubs(&self, memories: &[DreamMemory]) -> Vec<HubCandidate> {
        let mut stats = DreamStats::default();
        let refs: Vec<&DreamMemory> = memories.iter().collect();
        let connections = self.discover_connections(&refs, &mut stats);
        let clusters = self.find_clusters(&refs, &connections);
        self.generate_hub_candidates(&refs, &clusters, &mut stats)
    }

    /// Get all generated insights.
    pub fn get_insights(&self) -> Vec<SynthesizedInsight> {
        self.insights.read().map(|i| i.clone()).unwrap_or_default()
    }

    /// Get insights by type.
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

    /// Get dream history.
    pub fn get_dream_history(&self) -> Vec<DreamResult> {
        self.dream_history
            .read()
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    /// Get discovered connections.
    pub fn get_connections(&self) -> Vec<DiscoveredConnection> {
        self.connections
            .read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }
}

impl Default for MemoryDreamer {
    fn default() -> Self {
        Self::new()
    }
}

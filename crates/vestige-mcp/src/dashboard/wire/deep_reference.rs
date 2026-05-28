//! Deep reference reasoning DTOs.
//!
//! Mirrors the `Value` shape returned by `tools::cross_reference::execute`
//! (the deep_reference tool). The tool composes 7 cognitive stages
//! (retrieval → trust → activation → contradiction → temporal → dream
//! → synthesis); each stage may be skipped if upstream state is locked,
//! so most response fields are best-effort and modeled as `Option` /
//! defaults.
//!
//! See `tools::cross_reference` for the upstream shape; this DTO is the
//! single source of truth the dashboard relies on.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One memory used as evidence in a deep_reference answer. Mirrors the
/// `evidence[]` entries the tool emits — content + FSRS trust signals
/// + the combined retrieval score that surfaced it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeepRefEvidenceDto.ts", rename_all = "camelCase")]
pub struct DeepRefEvidenceDto {
    pub id: String,
    pub content: String,
    pub trust: f64,
    pub retention: f64,
    pub stability: f64,
    pub reps: i32,
    pub lapses: i32,
    #[serde(default)]
    pub tags: Vec<String>,
    pub node_type: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub combined_score: f64,
    /// Optional provenance marker. Empty for primary retrieval evidence;
    /// set to `"spreading_activation"` for memories surfaced through the
    /// activation network so the dashboard can render them as
    /// "related — via connections" rather than direct hits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
}

/// A pair of memories that contradict each other (stage 5 of the
/// reasoning pipeline). `relation` is the classifier's verdict —
/// "contradicts" is the only value worth surfacing in the dashboard;
/// "supersedes" lands in `superseded` instead.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeepRefContradictionDto.ts", rename_all = "camelCase")]
pub struct DeepRefContradictionDto {
    pub memory_a: String,
    pub memory_b: String,
    pub content_a: String,
    pub content_b: String,
    pub trust_a: f64,
    pub trust_b: f64,
    pub relation: String,
}

/// A memory replaced by a newer one carrying the same topic (stage 5
/// temporal supersession). The dashboard renders these as
/// strikethrough'd evidence so the user knows the old answer still
/// exists in history.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeepRefSupersededDto.ts", rename_all = "camelCase")]
pub struct DeepRefSupersededDto {
    pub superseded_id: String,
    pub superseded_by: String,
    pub reason: String,
}

/// One node on the evolution timeline (stage 7). Same id as a piece of
/// evidence, but reduced to first-sentence + trust + timestamp so the
/// dashboard can render a left-to-right strip without dragging the
/// full content payload.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeepRefEvolutionDto.ts", rename_all = "camelCase")]
pub struct DeepRefEvolutionDto {
    pub id: String,
    pub content: String,
    pub trust: f64,
    pub timestamp: String,
}

/// A dream insight that pattern-matches the query (stage 6). The
/// dashboard uses these to surface "the system noticed this earlier"
/// callouts above the evidence list.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeepRefInsightDto.ts", rename_all = "camelCase")]
pub struct DeepRefInsightDto {
    pub insight: String,
    pub confidence: f64,
    pub source: String,
}

/// Which of the optional reasoning stages actually ran. Locked
/// cognitive engine → `spreading_activation = false`; insights DB
/// unavailable → `dream_insights = false`. The dashboard uses these
/// to grey out the relevant sections instead of pretending the data
/// is "empty by design".
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeepRefStagesDto.ts", rename_all = "camelCase")]
pub struct DeepRefStagesDto {
    #[serde(default)]
    pub spreading_activation: bool,
    #[serde(default)]
    pub dream_insights: bool,
}

/// Full deep_reference reasoning response. The tool routes intent
/// classification through 5 categories (fact_check, timeline,
/// root_cause, comparison, synthesis); the dashboard uses `intent`
/// only as a hint for which sub-panel to lead with.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeepReferenceResultDto.ts", rename_all = "camelCase")]
pub struct DeepReferenceResultDto {
    pub intent: String,
    pub query: String,
    pub reasoning: String,
    /// Highest-trust evidence picked as the lead answer. `None` when
    /// the query matched zero memories.
    #[serde(default)]
    pub recommended: Option<DeepRefEvidenceDto>,
    #[serde(default)]
    pub evidence: Vec<DeepRefEvidenceDto>,
    #[serde(default)]
    pub contradictions: Vec<DeepRefContradictionDto>,
    #[serde(default)]
    pub superseded: Vec<DeepRefSupersededDto>,
    #[serde(default)]
    pub evolution: Vec<DeepRefEvolutionDto>,
    #[serde(default)]
    pub related_insights: Vec<DeepRefEvidenceDto>,
    #[serde(default)]
    pub dream_insights: Vec<DeepRefInsightDto>,
    /// 0.0-1.0, biased downward by contradictions (0.1 per pair).
    pub confidence: f64,
    pub depth: i64,
    pub memories_analyzed: i64,
    #[serde(default)]
    pub stages_completed: DeepRefStagesDto,
}


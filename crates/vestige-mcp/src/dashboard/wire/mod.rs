//! Wire-format DTOs for the dashboard REST + WebSocket APIs.
//!
//! Every type in this module is the **single source of truth** for what
//! the dashboard receives over the wire. The Rust struct carries
//! `#[derive(serde::Serialize, ts_rs::TS)]` and is exported to the
//! frontend at `apps/dashboard/src/types/generated/<TypeName>.ts`
//! whenever `cargo test -p vestige-mcp` runs (ts-rs writes the file as a
//! side effect of a generated test).
//!
//! ## Why a separate `wire` layer?
//!
//! The internal domain types (`KnowledgeNode`, `Intention`, etc.) carry
//! ~30 fields each, several of which are private FSRS state we don't
//! want to publish. A wire DTO is a deliberate projection that:
//!
//! 1. Drops fields the dashboard has no use for
//! 2. Renames fields to camelCase via `#[serde(rename_all = "camelCase")]`
//! 3. Formats `DateTime<Utc>` as `String` (ISO-8601) rather than
//!    `chrono::DateTime` so the JS side parses it with `new Date()`
//!    without surprises
//! 4. Lets us evolve the wire shape without touching storage
//!
//! ## Adding a new endpoint type
//!
//! 1. Add a `pub struct FooDto` here with `#[derive(Serialize, TS)]`
//!    and `#[ts(export, export_to = "FooDto.ts")]`
//! 2. Implement `From<&DomainType> for FooDto` so handlers can call
//!    `.into()` on the storage value
//! 3. Use `Json(dto)` in the handler instead of `Json(json!({...}))`
//! 4. Run `cargo test -p vestige-mcp` to regenerate
//!    `apps/dashboard/src/types/generated/FooDto.ts`
//! 5. Re-export from `apps/dashboard/src/types/index.ts` so call-sites
//!    keep importing from `@/types`
//!
//! ## CI gate
//!
//! `cargo test` regenerates the bindings into the dashboard tree.
//! `git diff --exit-code apps/dashboard/src/types/generated/` in CI
//! fails the PR if the developer changed a wire struct without
//! regenerating — this is the mechanism that prevents the camelCase /
//! field-rename drift we hit before introducing this layer.

pub mod cognitive;
pub mod decisions;
pub mod deep_reference;
pub mod erasure;
pub mod graph;
pub mod history;
pub mod hubs;
pub mod insights;
pub mod intentions;
pub mod limits;
pub mod memory;
pub mod metacognitive;
pub mod observability;
pub mod review;

pub use cognitive::{
    ConsolidationResultDto, ImportanceChannelsDto, ImportanceScoreDto, PredictResponseDto,
    PredictedMemoryDto,
};
pub use decisions::{
    DecisionChoiceDto, DecisionCriterionDto, DecisionDto, DecisionListResponseDto,
    DecisionScoreCellDto,
};
pub use deep_reference::{
    DeepRefContradictionDto, DeepRefEvidenceDto, DeepRefEvolutionDto, DeepRefInsightDto,
    DeepRefStagesDto, DeepRefSupersededDto, DeepReferenceResultDto,
};
pub use erasure::{EraseRequestDto, EraseResponseDto};
pub use graph::{
    ExploreResponseDto, ExploreResultDto, GraphEdgeDto, GraphNodeDto, GraphResponseDto,
};
pub use history::{
    MemoryChangelogDto, MemoryChangelogEntryDto, MemoryRevisionDto, MemoryRevisionsDto,
    TimelineDayDto, TimelineMemoryDto, TimelineResponseDto,
};
pub use hubs::{HubDto, HubListResponseDto};
pub use insights::{InsightDto, InsightListResponseDto};
pub use intentions::{
    CreateIntentionResponseDto, IntentionItemDto, IntentionListResponseDto,
    UpdateIntentionRequestDto, UpdateIntentionResponseDto,
};
pub use limits::DashboardLimitsDto;
pub use memory::{
    EpistemicStatusDto, MemoryDto, MemoryListResponseDto, MemoryStatusDto, MemorySystemDto,
    MemoryUpdateResultDto, SelfContainedFindingDto,
};
pub use metacognitive::{
    ConfidenceDimensionsDto, ConfidenceEntryDto, ConfidenceResultDto, ReflectInsightDto,
    ReflectResultDto, ReflectStatus, TemporalEntryDto, TemporalResultDto,
};
pub use observability::{
    EndangeredMemoryDto, HealthCheckDto, HealthStatus, RetentionBucketDto,
    RetentionDistributionDto, SearchResultDto, SystemStatsDto,
};
pub use review::{ReviewItemDto, ReviewQueueResponseDto, ReviewResultDto};

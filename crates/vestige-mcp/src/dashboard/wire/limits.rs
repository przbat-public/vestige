//! Single source of truth for dashboard limits.
//!
//! These values used to live in three places: the Rust handler clamp
//! (`graph::list_graph` had `clamp(1, 500)`), the dashboard's hard-coded
//! `defaultMaxNodes`, and a Storybook tweak that drifted independently.
//! Now they live here, get exposed at `GET /api/_meta/limits`, and the
//! dashboard fetches them once on boot.
//!
//! When tightening or relaxing a limit:
//!   1. Edit the constant below.
//!   2. Update the corresponding `clamp(...)` call in the relevant
//!      handler (the test in `dashboard::wire::limits` will fail and
//!      remind you which one).
//!   3. Re-run `cargo test -p vestige-mcp --lib dashboard` to refresh
//!      `apps/dashboard/src/types/generated/DashboardLimitsDto.ts`.
//!
//! No frontend changes required — the hook in
//! `useDashboardLimits.ts` reads the live values.

use serde::Serialize;
use ts_rs::TS;

/// Compile-time defaults exposed to the dashboard. Mirrors the runtime
/// `clamp(...)` calls in handlers so a contributor can't relax one
/// without the other (the limits-clamp parity tests in
/// `crates/vestige-mcp/src/dashboard/wire/limits_test.rs` pin them
/// together).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DashboardLimitsDto.ts", rename_all = "camelCase")]
pub struct DashboardLimitsDto {
    /// Default node cap when the user hasn't overridden it.
    pub graph_max_nodes_default: u32,
    /// Hard ceiling for the node cap. Above this the rendering pipeline
    /// stalls on the main thread and FPS collapses; verified empirically
    /// at ~600 nodes on M1 Air. Raised from 500 to 1000 as a deliberate UX
    /// call — users on faster hardware can opt in to denser graphs while
    /// the LOD/clustering work in `Graph3D` is still pending. Going much
    /// further than 1000 is irresponsible until that lands.
    pub graph_max_nodes_max: u32,
    /// Default BFS depth — covers most exploratory queries without
    /// pulling 90% of the graph in.
    pub graph_depth_default: u32,
    /// Hard ceiling for BFS depth. Beyond this `get_memory_subgraph`
    /// degrades into a near-full-graph scan.
    pub graph_depth_max: u32,
    /// Default `?limit=` for `/api/search`. Matches what the dashboard
    /// renders without "load more" pagination.
    pub search_limit_default: u32,
    /// Hard ceiling for `/api/search`. The dashboard never asks for
    /// more, but external API users get a 400 above this.
    pub search_limit_max: u32,
    /// Maximum WebSocket events the client retains in the live stream.
    /// Pure UI cap — backend doesn't care, but exposing it here means
    /// the value lives next to the others instead of in `useWebSocket`.
    pub ws_max_events: u32,
}

impl DashboardLimitsDto {
    /// Compile-time defaults — the values handlers should clamp to.
    pub const DEFAULT: Self = Self {
        graph_max_nodes_default: 50,
        graph_max_nodes_max: 1000,
        graph_depth_default: 1,
        graph_depth_max: 5,
        search_limit_default: 20,
        search_limit_max: 100,
        ws_max_events: 200,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity check: defaults must be within the maximums. Easy to
    /// violate when bumping one value without the other.
    #[test]
    fn defaults_within_maximums() {
        let d = DashboardLimitsDto::DEFAULT;
        assert!(d.graph_max_nodes_default <= d.graph_max_nodes_max);
        assert!(d.graph_depth_default <= d.graph_depth_max);
        assert!(d.search_limit_default <= d.search_limit_max);
    }

    /// Pin the values so a casual change shows up in CR. Updating any
    /// of these is a deliberate UX call, not a refactor.
    #[test]
    fn defaults_match_pinned_values() {
        let d = DashboardLimitsDto::DEFAULT;
        assert_eq!(d.graph_max_nodes_default, 50);
        assert_eq!(d.graph_max_nodes_max, 1000);
        assert_eq!(d.graph_depth_default, 1);
        assert_eq!(d.graph_depth_max, 5);
        assert_eq!(d.search_limit_default, 20);
        assert_eq!(d.search_limit_max, 100);
        assert_eq!(d.ws_max_events, 200);
    }
}

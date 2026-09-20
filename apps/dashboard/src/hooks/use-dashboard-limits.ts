// Single source of truth for shared limits between backend and
// dashboard. Fetched once on app boot via TanStack Query (with a long
// `staleTime` because these only change on Rust-side recompile) and
// cached forever for the session.
//
// Components reading the live values:
//   - GraphPage / MemoryLocalGraph (max_nodes, depth)
//   - useWebSocket (events ring buffer cap)
//   - SearchPage (default page size)
//   - MemoryRevisionsPanel (content-history page size)
//
// Falls back to `DEFAULT_LIMITS` when the endpoint hasn't responded yet
// — the values match the backend `DashboardLimitsDto::DEFAULT` exactly,
// so the UI never renders unbounded inputs while the network is in
// flight. After the fetch settles, components re-read.

import { useQuery } from '@tanstack/react-query';
import type { DashboardLimitsDto } from '@/types';

/**
 * Local fallbacks. Must match `wire::limits::DashboardLimitsDto::DEFAULT`
 * — the limits-parity test in `wire::limits::tests::defaults_match_pinned_values`
 * pins the Rust side; this comment is the only thing that pins the TS
 * side, so don't drift it.
 */
export const DEFAULT_LIMITS: DashboardLimitsDto = {
  graphMaxNodesDefault: 50,
  graphMaxNodesMax: 1000,
  graphDepthDefault: 1,
  graphDepthMax: 5,
  searchLimitDefault: 20,
  searchLimitMax: 100,
  wsMaxEvents: 200,
  revisionHistoryLimitDefault: 30,
  revisionHistoryLimitMax: 200,
};

async function fetchLimits(): Promise<DashboardLimitsDto> {
  const res = await fetch('/api/_meta/limits');
  if (!res.ok) {
    throw new Error(`/api/_meta/limits failed (${res.status})`);
  }
  return (await res.json()) as DashboardLimitsDto;
}

/**
 * Returns the live limits and falls back to compile-time defaults
 * before the first fetch completes. Always returns a value — never
 * `undefined` — so callers don't need to thread loading states.
 *
 * Don't call this in tight render loops; the underlying TanStack Query
 * subscription is per-component, so prefer threading the result through
 * props from a near-root container.
 */
export function useDashboardLimits(): DashboardLimitsDto {
  const { data } = useQuery({
    queryKey: ['_meta', 'limits'] as const,
    queryFn: fetchLimits,
    // Limits are immutable per server build, so cache them aggressively
    // — refetching on every focus would be pure waste.
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
  return data ?? DEFAULT_LIMITS;
}

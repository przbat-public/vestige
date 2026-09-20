import { QueryClient } from '@tanstack/react-query';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      gcTime: 5 * 60_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

export const queryKeys = {
  stats: ['stats'] as const,
  health: ['health'] as const,
  retentionDistribution: ['retentionDistribution'] as const,
  /**
   * Prefix for every cached list page. Use this — never
   * `memories(undefined)` — when invalidating: `['memories', undefined]`
   * is a *different* key from `['memories', { limit, offset }]` and
   * `partialMatchKey` rejects it, so the invalidation silently does
   * nothing.
   */
  memoriesPrefix: ['memories'] as const,
  memories: (params: Record<string, string>) => ['memories', params] as const,
  memory: (id: string) => ['memory', id] as const,
  memoryChangelog: (id: string) => ['memory', id, 'changelog'] as const,
  /**
   * Content history (`memory_revisions`) — a different question from
   * `memoryChangelog`, which caches life-cycle state transitions. Keyed on the
   * page size too, because the limits endpoint can change it between builds.
   */
  memoryRevisions: (id: string, limit: number) => ['memory', id, 'revisions', limit] as const,
  graph: (params?: { query?: string; max_nodes?: number; depth?: number }) => ['graph', params] as const,
  search: (q: string, limit: number) => ['search', q, limit] as const,
  timeline: (days: number, limit: number) => ['timeline', days, limit] as const,
  intentions: (status: string) => ['intentions', status] as const,
  predictions: ['predictions'] as const,
  temporal: (action: string, topic?: string) => ['temporal', action, topic] as const,
  decisions: (limit?: number) => ['decisions', limit] as const,
  insights: (filter?: 'all' | 'validated' | 'unvalidated', limit?: number) => ['insights', filter, limit] as const,
  hubs: (limit?: number) => ['hubs', limit] as const,
  deepReference: (query: string, depth?: number) => ['deepReference', query, depth] as const,
};

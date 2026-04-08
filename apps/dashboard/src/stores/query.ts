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
  memories: (params?: Record<string, string>) => ['memories', params] as const,
  memory: (id: string) => ['memory', id] as const,
  graph: (params?: { query?: string; max_nodes?: number; depth?: number }) => ['graph', params] as const,
  search: (q: string, limit: number) => ['search', q, limit] as const,
  timeline: (days: number, limit: number) => ['timeline', days, limit] as const,
  intentions: (status: string) => ['intentions', status] as const,
  predictions: ['predictions'] as const,
  explore: (fromId: string, mode: string, toId?: string) => ['explore', fromId, mode, toId] as const,
  importance: (text: string) => ['importance', text] as const,
};

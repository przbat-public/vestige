import type {
  ConfidenceResult,
  ConsolidationResult,
  DreamResult,
  GraphResponse,
  HealthCheck,
  ImportanceScore,
  IntentionItem,
  IntentionPriority,
  Memory,
  MemoryListResponse,
  ReflectResult,
  RetentionDistribution,
  SearchResult,
  SystemStats,
  TemporalResult,
  TimelineResponse,
  TriggerType,
} from '@/types';

const BASE = '/api';

export class ApiError extends Error {
  constructor(
    public path: string,
    public status: number,
  ) {
    super(`${path} failed (${status})`);
    this.name = 'ApiError';
  }
}

async function fetcher<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  if (!res.ok) {
    throw new ApiError(path, res.status);
  }
  if (res.status === 204 || res.headers.get('content-length') === '0') {
    return undefined as T;
  }
  return res.json();
}

export const api = {
  memories: {
    list: (params?: Record<string, string>) => {
      const qs = params ? `?${new URLSearchParams(params).toString()}` : '';
      return fetcher<MemoryListResponse>(`/memories${qs}`);
    },
    get: (id: string) => fetcher<Memory>(`/memories/${id}`),
    delete: (id: string) => fetcher<{ deleted: boolean }>(`/memories/${id}`, { method: 'DELETE' }),
    promote: (id: string) => fetcher<Memory>(`/memories/${id}/promote`, { method: 'POST' }),
    demote: (id: string) => fetcher<Memory>(`/memories/${id}/demote`, { method: 'POST' }),
  },
  search: (q: string, limit = 20) => fetcher<SearchResult>(`/search?q=${encodeURIComponent(q)}&limit=${limit}`),
  stats: () => fetcher<SystemStats>('/stats'),
  health: () => fetcher<HealthCheck>('/health'),
  timeline: (days = 7, limit = 200) => fetcher<TimelineResponse>(`/timeline?days=${days}&limit=${limit}`),
  graph: (params?: { query?: string; center_id?: string; depth?: number; max_nodes?: number }) => {
    const qs = params
      ? `?${new URLSearchParams(
          Object.entries(params)
            .filter(([, v]) => v !== undefined)
            .map(([k, v]) => [k, String(v)]),
        ).toString()}`
      : '';
    return fetcher<GraphResponse>(`/graph${qs}`);
  },
  dream: () => fetcher<DreamResult>('/dream', { method: 'POST' }),
  explore: (fromId: string, action = 'associations', toId?: string, limit = 10) =>
    fetcher<Record<string, unknown>>('/explore', {
      method: 'POST',
      body: JSON.stringify({ from_id: fromId, action, to_id: toId, limit }),
    }),
  predict: () => fetcher<Record<string, unknown>>('/predict', { method: 'POST' }),
  importance: (content: string) =>
    fetcher<ImportanceScore>('/importance', {
      method: 'POST',
      body: JSON.stringify({ content }),
    }),
  consolidate: () => fetcher<ConsolidationResult>('/consolidate', { method: 'POST' }),
  retentionDistribution: () => fetcher<RetentionDistribution>('/retention-distribution'),
  intentions: (status = 'active') =>
    fetcher<{ intentions: IntentionItem[]; total: number; filter: string }>(`/intentions?status=${status}`),
  createIntention: (data: {
    content: string;
    trigger_type: TriggerType;
    trigger_value: string;
    priority: IntentionPriority;
    deadline?: string;
  }) =>
    fetcher<{ id: string; intention: IntentionItem }>('/intentions', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  reflect: (focus?: string, depth: 'quick' | 'standard' | 'deep' = 'standard') =>
    fetcher<ReflectResult>('/reflect', {
      method: 'POST',
      body: JSON.stringify({ focus, depth }),
    }),
  temporal: (action: string, topic?: string, memoryId?: string, limit = 20) =>
    fetcher<TemporalResult>('/temporal', {
      method: 'POST',
      body: JSON.stringify({ action, topic, memory_id: memoryId, limit }),
    }),
  confidence: (action: string, memoryId?: string, limit = 20) =>
    fetcher<ConfidenceResult>('/confidence', {
      method: 'POST',
      body: JSON.stringify({ action, memory_id: memoryId, limit }),
    }),
};

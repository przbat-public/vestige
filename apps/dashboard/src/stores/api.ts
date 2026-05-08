import type {
  ConfidenceResult,
  ConsolidationResult,
  DreamResult,
  ExploreResponse,
  FsrsRating,
  GraphResponse,
  HealthCheck,
  ImportanceScore,
  IntentionItem,
  IntentionPriority,
  Memory,
  MemoryChangelog,
  MemoryListResponse,
  PredictResponse,
  ReflectResult,
  RetentionDistribution,
  ReviewQueueResponse,
  ReviewResult,
  SearchResult,
  SystemStats,
  TimelineResponse,
  TriggerType,
} from '@/types';

const BASE = '/api';

export class ApiError extends Error {
  constructor(
    public path: string,
    public status: number,
    public code?: string,
    detail?: string,
  ) {
    // Prefer the server-provided detail (e.g. "Field `to_id` is required …")
    // over the bare "POST /explore failed (400)" string. The detail is what
    // QueryErrorPanel surfaces when the user clicks "Show technical details".
    super(detail ?? `${path} failed (${status})`);
    this.name = 'ApiError';
  }
}

async function fetcher<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  if (!res.ok) {
    let code: string | undefined;
    let detail: string | undefined;
    if (res.headers.get('content-type')?.includes('application/json')) {
      try {
        const body = (await res.json()) as { error?: { code?: string; message?: string } };
        code = body.error?.code;
        detail = body.error?.message;
      } catch {
        // body wasn't valid JSON; fall through with undefined fields
      }
    }
    throw new ApiError(path, res.status, code, detail);
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
    update: (id: string, body: { content?: string; tags?: string[] }) =>
      fetcher<Memory>(`/memories/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(body),
      }),
    changelog: (id: string) => fetcher<MemoryChangelog>(`/memories/${id}/changelog`),
    review: (id: string, rating: FsrsRating) =>
      fetcher<ReviewResult>(`/memories/${id}/review`, {
        method: 'POST',
        body: JSON.stringify({ rating }),
      }),
  },
  review: {
    queue: (limit = 50) =>
      fetcher<ReviewQueueResponse>(`/review/queue?limit=${limit}`),
  },
  maintenance: {
    regenerateEmbeddings: (body?: { force?: boolean; node_ids?: string[] }) =>
      fetcher<{
        successful: number;
        failed: number;
        skipped: number;
        errors: string[];
        durationMs: number;
      }>('/maintenance/regenerate-embeddings', {
        method: 'POST',
        body: body ? JSON.stringify(body) : undefined,
      }),
    findDuplicates: (body?: { similarity_threshold?: number; limit?: number; tags?: string[] }) =>
      fetcher<{
        clusters: Array<{
          clusterId: number;
          size: number;
          members: Array<{
            id: string;
            contentPreview: string;
            similarityToAnchor: string;
            retention: number;
            createdAt: string;
            tags: string[];
          }>;
          suggestedAction: string;
        }>;
        totalMemories: number;
        totalWithEmbeddings: number;
        totalClusters: number;
        threshold: number;
        warning?: string;
      }>('/maintenance/find-duplicates', {
        method: 'POST',
        body: body ? JSON.stringify(body) : undefined,
      }),
    gc: (body?: { min_retention?: number; max_age_days?: number; dry_run?: boolean }) =>
      fetcher<{
        dryRun: boolean;
        candidateCount: number;
        deleted?: number;
        sample?: Array<{ id: string; retention: number; ageDays: number; contentPreview: string }>;
      }>('/maintenance/gc', {
        method: 'POST',
        body: JSON.stringify(body ?? { dry_run: true }),
      }),
    backup: () =>
      fetcher<{ path: string; sizeBytes: number; timestamp: string }>('/maintenance/backup', {
        method: 'POST',
      }),
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
  explore: async (fromId: string, action = 'associations', toId?: string, limit = 10) => {
    const raw = await fetcher<ExploreResponse>('/explore', {
      method: 'POST',
      body: JSON.stringify({ from_id: fromId, action, to_id: toId, limit }),
    });
    return {
      ...raw,
      results: raw.results || raw.nodes || raw.chain || raw.bridges || [],
    };
  },
  predict: () => fetcher<PredictResponse>('/predict', { method: 'POST' }),
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
  confidence: (action: string, memoryId?: string, limit = 20) =>
    fetcher<ConfidenceResult>('/confidence', {
      method: 'POST',
      body: JSON.stringify({ action, memory_id: memoryId, limit }),
    }),
};

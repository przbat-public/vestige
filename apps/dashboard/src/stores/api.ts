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
  TemporalAction,
  TemporalEntry,
  TemporalResult,
  TimelineResponse,
  TriggerType,
} from '@/types';

/**
 * Raw shape of a temporal entry as serialised by `tools/temporal.rs`. Used
 * only inside `api.temporal` for the snake_case → camelCase conversion;
 * components consume `TemporalEntry` exclusively.
 */
interface RawTemporalEntry {
  id: string;
  content: string;
  valid_from?: string;
  valid_until?: string;
  expired_at?: string;
  days_expired?: number;
  // `current` returns retention pre-formatted as "0.87"; `expired` and
  // `history` omit it. Normalised to a number in `mapTemporalEntry`.
  retention?: string | number;
  tags?: string[];
}

interface RawTemporalResult {
  action: TemporalAction;
  topic?: string;
  count: number;
  memories: RawTemporalEntry[];
}

function mapTemporalEntry(raw: RawTemporalEntry): TemporalEntry {
  const retention = typeof raw.retention === 'string' ? Number.parseFloat(raw.retention) : (raw.retention ?? 0);
  return {
    id: raw.id,
    content: raw.content,
    validFrom: raw.valid_from,
    // `expired` action stores the expiry timestamp under `expired_at` so the
    // dashboard reads the correct field whether the row is still live or not.
    validUntil: raw.valid_until ?? raw.expired_at,
    daysExpired: raw.days_expired,
    retention: Number.isFinite(retention) ? retention : 0,
    tags: raw.tags ?? [],
  };
}

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
    // Backend returns a status blob (`{ promoted: true, id, retentionStrength }`),
    // not the full Memory object. The dashboard previously over-declared the
    // shape as `Memory`, which would break any caller that consumed the result —
    // today nobody does, but the lying type hid the divergence from reviewers.
    promote: (id: string) =>
      fetcher<{ promoted: boolean; id: string; retentionStrength: number }>(`/memories/${id}/promote`, {
        method: 'POST',
      }),
    demote: (id: string) =>
      fetcher<{ demoted: boolean; id: string; retentionStrength: number }>(`/memories/${id}/demote`, {
        method: 'POST',
      }),
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
    /**
     * Create a memory through the full smart-ingest pipeline (importance
     * scoring, intent detection, preprocessing, prediction-error gating).
     * The dashboard's "Add memory" affordance uses this — the engine may
     * decide to update or merge into an existing memory rather than create
     * a new one, and the response carries the decision so the UI can
     * surface it (e.g. "We merged this with an existing memory because…").
     */
    smartIngest: (body: {
      content: string;
      tags?: string[];
      nodeType?: string;
      source?: string;
      forceCreate?: boolean;
    }) =>
      fetcher<{
        success: boolean;
        decision: string;
        nodeId: string;
        message: string;
        hasEmbedding: boolean;
        similarity?: number;
        predictionError: number;
        supersededId?: string;
        importanceScore: number;
        reason: string;
        explanation?: string;
        compound_content_warning?: string;
        near_duplicate_warning?: string;
      }>('/smart_ingest', {
        method: 'POST',
        body: JSON.stringify(body),
      }),
  },
  review: {
    queue: (limit = 50) => fetcher<ReviewQueueResponse>(`/review/queue?limit=${limit}`),
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
  /**
   * Query temporal versioning. Wraps `POST /api/temporal` and normalises the
   * snake_case wire shape (`valid_from`, `expired_at`, retention as string)
   * into the camelCase `TemporalResult` consumed by components.
   *
   * Use `current` and `expired` for topic-scoped or DB-wide scans; use
   * `history` to follow a single concept's evolution; use `invalidate` to
   * mark a memory as superseded (server-side state change).
   */
  temporal: async (action: TemporalAction, opts?: { topic?: string; memoryId?: string; limit?: number }) => {
    const raw = await fetcher<RawTemporalResult>('/temporal', {
      method: 'POST',
      body: JSON.stringify({
        action,
        topic: opts?.topic,
        memory_id: opts?.memoryId,
        limit: opts?.limit ?? 20,
      }),
    });
    return {
      action: raw.action,
      topic: raw.topic,
      count: raw.count,
      memories: raw.memories.map(mapTemporalEntry),
    } satisfies TemporalResult;
  },
};

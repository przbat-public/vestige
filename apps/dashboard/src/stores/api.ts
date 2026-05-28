import type {
  ConfidenceResult,
  ConsolidationResult,
  DecisionListResponse,
  DeepReferenceResult,
  DreamResult,
  ExploreResponse,
  FsrsRating,
  GraphResponse,
  HealthCheck,
  HubListResponse,
  ImportanceScore,
  InsightListResponse,
  IntentionItem,
  IntentionPriority,
  Memory,
  MemoryChangelog,
  PredictResponse,
  RetentionDistribution,
  ReviewQueueResponse,
  ReviewResult,
  SystemStats,
  TemporalAction,
  TimelineResponse,
  TriggerType,
} from '@/types';
import { wire } from '@/types/runtime';

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
    list: async (params?: Record<string, string>) => {
      const qs = params ? `?${new URLSearchParams(params).toString()}` : '';
      // Runtime-validated: this is the dashboard's home page; a silent
      // wire drift here would break every list-driven view.
      return wire.memoryList(await fetcher<unknown>(`/memories${qs}`));
    },
    get: async (id: string) => wire.memory(await fetcher<unknown>(`/memories/${id}`)),
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
  search: async (q: string, limit = 20) =>
    wire.search(await fetcher<unknown>(`/search?q=${encodeURIComponent(q)}&limit=${limit}`)),
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
  /**
   * Query the explore engine. The wire DTO unifies all three modes
   * ("associations" | "chains" | "bridges") under a single `results`
   * array — earlier versions of this client used to fall back to
   * `nodes` / `chain` / `bridges` because the backend used to fan them
   * out to separate fields. The DTO refactor collapsed those, so the
   * shim is gone.
   */
  explore: (fromId: string, action = 'associations', toId?: string, limit = 10) =>
    fetcher<ExploreResponse>('/explore', {
      method: 'POST',
      body: JSON.stringify({ from_id: fromId, action, to_id: toId, limit }),
    }),
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
  /**
   * PATCH /api/intentions/{id} — flip the lifecycle status.
   *
   * Allowed statuses: `fulfilled` | `cancelled` | `snoozed` | `active`.
   * Anything else returns 400 from the server (the dashboard never
   * sends those, but if it ever does the user gets a toast).
   */
  updateIntention: (id: string, status: 'fulfilled' | 'cancelled' | 'snoozed' | 'active') =>
    fetcher<{ id: string; status: string; updated: boolean }>(`/intentions/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),
  reflect: async (focus?: string, depth: 'quick' | 'standard' | 'deep' = 'standard') =>
    wire.reflect(
      await fetcher<unknown>('/reflect', {
        method: 'POST',
        body: JSON.stringify({ focus, depth }),
      }),
    ),
  confidence: (action: string, memoryId?: string, limit = 20) =>
    fetcher<ConfidenceResult>('/confidence', {
      method: 'POST',
      body: JSON.stringify({ action, memory_id: memoryId, limit }),
    }),
  /**
   * Query temporal versioning. Wraps `POST /api/temporal`. The wire
   * DTO already lands in camelCase with retention as a `number`, so
   * this is a thin pass-through — earlier versions had a snake→camel
   * mapper that's now redundant.
   *
   * Use `current` and `expired` for topic-scoped or DB-wide scans; use
   * `history` to follow a single concept's evolution; use `invalidate`
   * to mark a memory as superseded (server-side state change).
   */
  temporal: async (action: TemporalAction, opts?: { topic?: string; memoryId?: string; limit?: number }) =>
    wire.temporal(
      await fetcher<unknown>('/temporal', {
        method: 'POST',
        body: JSON.stringify({
          action,
          topic: opts?.topic,
          memory_id: opts?.memoryId,
          limit: opts?.limit ?? 20,
        }),
      }),
    ),
  /**
   * List structured decisions written through `codebase.remember_decision_v2`.
   * Each entry exposes the human-readable question, the choice matrix, the
   * recommended pick, and validity / supersession metadata so the dashboard
   * can render a comparison-table view instead of free-form Markdown.
   *
   * Backed by `GET /api/decisions`; the handler filters memories with a
   * `extra_json.decision` payload and projects them through `DecisionDto`.
   */
  decisions: (limit = 50) => fetcher<DecisionListResponse>(`/decisions?limit=${limit}`),
  /**
   * List structured Proposal B insights — synthesised observations the
   * dream cycle has decided are worth promoting to first-class memories.
   * `filter` narrows the result to validated/unvalidated insights;
   * defaults to `all`. `limit` clamps to [1, 500] server-side.
   *
   * Backed by `GET /api/insights`; the handler walks `node_type =
   * "insight"` and projects `extra_json.insight` through `InsightDto`.
   */
  insights: (filter: 'all' | 'validated' | 'unvalidated' = 'all', limit = 50) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (filter === 'validated') params.set('only_validated', 'true');
    if (filter === 'unvalidated') params.set('only_unvalidated', 'true');
    return fetcher<InsightListResponse>(`/insights?${params.toString()}`);
  },
  /**
   * List Topic Hubs (Proposal A) — clusters of related memories the
   * dream cycle materialised as `node_type="hub"`. Hubs are sorted by
   * `lastRegeneratedAt` desc server-side; `limit` clamps to [1, 500].
   *
   * Backed by `GET /api/hubs`; the handler walks `node_type = "hub"` and
   * projects `extra_json.hub` through `HubDto`.
   */
  hubs: (limit = 50) => fetcher<HubListResponse>(`/hubs?limit=${limit}`),
  /**
   * Run the deep_reference cognitive reasoning pipeline against the
   * memory store. The backend composes 7 stages (broad retrieval →
   * FSRS-6 trust scoring → spreading activation → contradiction
   * detection → temporal supersession → dream insight integration →
   * structured synthesis) and returns a single typed answer with
   * supporting evidence. `depth` clamps to [5, 50].
   *
   * Backed by `POST /api/deep_reference`. Use this for "why did we
   * decide X?", "what contradicts Y?", "show me the evolution of Z"
   * — questions a single search query can't answer cleanly.
   */
  deepReference: (query: string, depth = 20) =>
    fetcher<DeepReferenceResult>('/deep_reference', {
      method: 'POST',
      body: JSON.stringify({ query, depth }),
    }),
};

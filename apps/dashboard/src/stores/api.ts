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
  MemoryChangelog,
  MemoryRevisions,
  MemoryStatus,
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

/**
 * `acceptStatuses` exists for endpoints that use a non-2xx code to report a
 * *result* rather than a failure — `/api/health` answers 503 when the store is
 * critical, which is the endpoint doing its job. Without it the generic
 * non-2xx throw would discard the DTO body in exactly the state an operator
 * most needs to read it.
 */
type FetcherOptions = RequestInit & { acceptStatuses?: number[] };

async function fetcher<T>(path: string, options?: FetcherOptions): Promise<T> {
  const { acceptStatuses, ...init } = options ?? {};
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
  const accepted = acceptStatuses ? acceptStatuses.includes(res.status) : res.ok;
  if (!accepted) {
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

/**
 * One rule the write-time self-containedness gate fired.
 *
 * `kind` names the rule, `span` is the exact text that tripped it, `hint` says
 * what to write instead. Identical to the persisted
 * `SelfContainedFindingDto` the detail view receives, so both surfaces can
 * render the same list.
 */
export interface SmartIngestFinding {
  kind: string;
  span: string;
  hint: string;
}

/** The gate's verdict attached to a write it flagged (never to a clean one). */
export interface SmartIngestSelfContained {
  ok: boolean;
  requiresContext: boolean;
  rejected: boolean;
  rejectReason?: string;
  findings: SmartIngestFinding[];
}

/**
 * Response of `POST /api/smart_ingest`.
 *
 * Fields are optional because the tool's outcomes differ: a refusal carries
 * `stored`, `reason`, `guidance` and `findings` and no `nodeId`; a flagged
 * write carries `self_contained`; a clean write carries neither.
 */
export interface SmartIngestResult {
  success: boolean;
  decision: string;
  /** `false` only on a refusal — the flag that says nothing was written. */
  stored?: boolean;
  nodeId?: string;
  message?: string;
  hasEmbedding?: boolean;
  similarity?: number;
  predictionError?: number;
  supersededId?: string;
  importanceScore?: number;
  reason?: string;
  /** A refusal's "what to do instead", when the gate wrote one. */
  guidance?: string;
  explanation?: string;
  /** Findings on a *refusal* response. Flagged writes carry them under `self_contained`. */
  findings?: SmartIngestFinding[];
  compound_content_warning?: string;
  near_duplicate_warning?: string;
  /** Present only when the gate flagged the write as needing its context. */
  self_contained?: SmartIngestSelfContained;
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
    // All three mutating endpoints answer with a DTO, not a booleans blob:
    // `DELETE` / `POST …/promote` / `POST …/demote` share
    // `MemoryStatusDto { ok, id, retentionStrength, action }`. The dashboard
    // used to hand-write `{ deleted: boolean }` / `{ promoted: boolean, … }`,
    // which lied about the field names — a future caller reading
    // `result.promoted` would have silently received `undefined`.
    //
    // `confirmed: true` is required, matching the MCP
    // `memory(action="delete")` gate: the route refuses without it. The literal
    // type makes an unconfirmed delete unrepresentable in the dashboard — every
    // caller is a user-initiated affordance (bulk: themed alertdialog; single:
    // the Delete action with its undo window).
    delete: (id: string, acknowledgement: { confirmed: true }) =>
      fetcher<MemoryStatus>(`/memories/${id}?confirmed=${acknowledgement.confirmed}`, { method: 'DELETE' }),
    promote: (id: string) => fetcher<MemoryStatus>(`/memories/${id}/promote`, { method: 'POST' }),
    demote: (id: string) => fetcher<MemoryStatus>(`/memories/${id}/demote`, { method: 'POST' }),
    // `PATCH` answers with `MemoryUpdateResultDto { memory, field }`, not a
    // bare `Memory` — validate it so a drift here fails loudly instead of
    // caching `undefined` under `queryKeys.memory(undefined)`.
    update: async (id: string, body: { content?: string; tags?: string[] }) =>
      wire.memoryUpdate(
        await fetcher<unknown>(`/memories/${id}`, {
          method: 'PATCH',
          body: JSON.stringify(body),
        }),
      ),
    changelog: (id: string) => fetcher<MemoryChangelog>(`/memories/${id}/changelog`),
    /**
     * A memory's *content* history, newest first (`memory_revisions`, V17).
     *
     * Separate from `changelog` on purpose: that one lists life-cycle state
     * transitions, this one lists what the memory used to say. `limit` is
     * clamped server-side against `GET /api/_meta/limits`.
     */
    revisions: (id: string, limit: number) => fetcher<MemoryRevisions>(`/memories/${id}/revisions?limit=${limit}`),
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
     *
     * The route returns the MCP tool's `Value` verbatim (`handlers/memory.rs`
     * `smart_ingest_memory`), so the shape below is the tool's contract, not a
     * DTO. Two outcomes are not saves and the dialog must branch on them:
     * `stored: false` / `decision: "reject"` (nothing was written — the
     * response carries `reason`, `guidance` and `findings`), and
     * `self_contained.requiresContext` (written, and flagged as leaning on the
     * conversation it came from).
     */
    smartIngest: (body: {
      content: string;
      tags?: string[];
      nodeType?: string;
      source?: string;
      forceCreate?: boolean;
    }) =>
      fetcher<SmartIngestResult>('/smart_ingest', {
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
    gc: (body?: { min_retention?: number; max_age_days?: number; dry_run?: boolean; confirmed?: boolean }) =>
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
  // 503 means "critical", not "request failed": the body still carries the
  // health DTO, and the pages that render the critical pill need it.
  health: () => fetcher<HealthCheck>('/health', { acceptStatuses: [200, 503] }),
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

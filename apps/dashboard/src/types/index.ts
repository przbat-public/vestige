export interface Memory {
  id: string;
  content: string;
  nodeType: string;
  tags: string[];
  retentionStrength: number;
  storageStrength: number;
  retrievalStrength: number;
  createdAt: string;
  updatedAt: string;
  source?: string;
  reviewCount?: number;
  combinedScore?: number;
  sentimentScore?: number;
  sentimentMagnitude?: number;
  lastAccessedAt?: string;
  nextReviewAt?: string;
  validFrom?: string;
  validUntil?: string;
  epistemicStatus?: 'world' | 'experience' | 'observation' | 'opinion';
  memorySystem?: 'episodic' | 'semantic' | 'procedural';
}

export interface SearchResult {
  query: string;
  total: number;
  durationMs: number;
  results: Memory[];
}

export interface MemoryListResponse {
  total: number;
  memories: Memory[];
}

export interface MemoryChangelogEntry {
  fromState: string;
  toState: string;
  reasonType: string;
  reasonData?: string | null;
  timestamp: string;
}

export interface MemoryChangelog {
  memoryId: string;
  memoryContent: string;
  totalTransitions: number;
  transitions: MemoryChangelogEntry[];
}

/** FSRS-6 rating values. 1=Again (forgot), 2=Hard, 3=Good, 4=Easy. */
export type FsrsRating = 1 | 2 | 3 | 4;

export interface ReviewResult {
  id: string;
  rating: 'again' | 'hard' | 'good' | 'easy';
  previousRetention: number;
  newRetention: number;
  previousStability: number;
  newStability: number;
  difficulty: number;
  reps: number;
  lapses: number;
  nextReviewAt: string | null;
}

/** A memory in the review queue — slimmer than `Memory` but compatible. */
export interface ReviewItem extends Memory {
  difficulty: number;
  stability: number;
}

export interface ReviewQueueResponse {
  total: number;
  memories: ReviewItem[];
}

export interface SystemStats {
  totalMemories: number;
  dueForReview: number;
  averageRetention: number;
  averageStorageStrength: number;
  averageRetrievalStrength: number;
  withEmbeddings: number;
  embeddingCoverage: number;
  embeddingModel: string;
  oldestMemory?: string;
  newestMemory?: string;
}

export interface HealthCheck {
  status: 'healthy' | 'degraded' | 'critical' | 'empty';
  totalMemories: number;
  averageRetention: number;
  version: string;
}

export interface TimelineDay {
  date: string;
  count: number;
  memories: Memory[];
}

export interface TimelineResponse {
  days: number;
  totalMemories: number;
  timeline: TimelineDay[];
}

export interface GraphNode {
  id: string;
  label: string;
  type: string;
  retention: number;
  tags: string[];
  createdAt: string;
  updatedAt: string;
  isCenter: boolean;
}

export interface GraphEdge {
  source: string;
  target: string;
  weight: number;
  type: string;
}

export interface GraphResponse {
  nodes: GraphNode[];
  edges: GraphEdge[];
  centerId: string;
  depth: number;
  nodeCount: number;
  edgeCount: number;
}

/**
 * A pair of memories that the dream engine flagged as contradictory.
 *
 * The pair is symmetric — neither side is the "winner" — so the wire shape
 * uses `memoryA`/`memoryB`. Resolution (which one to demote, edit, or merge)
 * is a separate UI affordance, not part of the engine output.
 */
export interface ContradictionPair {
  memoryA: string;
  memoryB: string;
  confidence: number;
  insight: string;
}

export interface DreamPhase {
  phase: string;
  durationMs: number;
  memoriesProcessed: number;
  actions: string[];
}

export interface DreamStats {
  creativeConnectionsFound: number;
  connectionsPersisted: number;
  memoriesStrengthened: number;
  memoriesDownscaled: number;
  emotionalProcessed: number;
  contradictionsFound: number;
  insightsGenerated: number;
  durationMs: number;
}

export interface DreamResult {
  status: string;
  memoriesReplayed: number;
  wakingTagsProcessed: number;
  wakingTagsCleared: number;
  connectionsPersisted: number;
  insights: DreamInsight[];
  contradictions: ContradictionPair[];
  /**
   * Currently always empty — engine only counts demotions, not their IDs.
   * Kept on the wire so the dashboard can render the count consistently
   * with the strengthened/downscaled stats.
   */
  memoriesDemoted: string[];
  phases: DreamPhase[];
  stats: DreamStats;
}

export interface DreamInsight {
  type: string;
  insight: string;
  sourceMemories: string[];
  confidence: number;
  noveltyScore: number;
}

export interface ImportanceScore {
  composite: number;
  channels: {
    novelty: number;
    arousal: number;
    reward: number;
    attention: number;
  };
  recommendation: 'save' | 'skip';
}

export interface RetentionDistribution {
  distribution: { range: string; count: number }[];
  byType: Record<string, number>;
  endangered: Memory[];
  total: number;
}

export interface ConsolidationResult {
  nodesProcessed: number;
  decayApplied: number;
  embeddingsGenerated: number;
  duplicatesMerged: number;
  activationsComputed: number;
  durationMs: number;
}

export type VestigeEventType =
  | 'Connected'
  | 'MemoryCreated'
  | 'MemoryUpdated'
  | 'MemoryDeleted'
  | 'MemoryPromoted'
  | 'MemoryDemoted'
  | 'SearchPerformed'
  | 'DreamStarted'
  | 'DreamProgress'
  | 'DreamCompleted'
  | 'ConsolidationStarted'
  | 'ConsolidationCompleted'
  | 'RetentionDecayed'
  | 'ConnectionDiscovered'
  | 'ActivationSpread'
  | 'ImportanceScored'
  | 'Heartbeat';

export interface VestigeEvent {
  type: VestigeEventType;
  data: Record<string, unknown>;
}

export interface IdentifiedEvent extends VestigeEvent {
  _id: number;
}

export type TriggerType = 'context' | 'time' | 'event';
export type IntentionPriority = 'low' | 'medium' | 'high';
export type IntentionStatus = 'active' | 'fulfilled' | 'expired' | 'snoozed' | 'completed' | 'cancelled';

export interface IntentionItem {
  id: string;
  content: string;
  triggerType: TriggerType;
  triggerValue: string;
  status: IntentionStatus;
  priority: IntentionPriority;
  createdAt: string;
  deadline?: string;
  snoozedUntil?: string;
}

// Metacognitive tools (v2.1)
/**
 * Output of `tools/reflect.rs`. The wire shape carries two parallel
 * insight representations: legacy free-form `insights: string[]` for MCP
 * clients that already consume them, plus `structuredInsights` for the
 * dashboard's "Memory Sources" panel — each entry points back at the
 * memories that triggered it.
 *
 * Field names match the wire camelCase exactly. Older versions of this
 * type declared `insights: ReflectInsight[]` which was a lie — the
 * server returned strings — so the dashboard rendered nothing for
 * reflection. Fixing both sides of the contract here.
 */
export interface ReflectResult {
  focus: string | null;
  depth: string;
  insights: string[];
  structuredInsights: ReflectInsight[];
  summary: string;
  memoriesAnalyzed: number;
}

export interface ReflectInsight {
  type: 'contradiction' | 'stale_decision' | 'overconfident' | 'knowledge_gap' | string;
  description: string;
  /**
   * Memory IDs that produced this insight. Empty for `knowledge_gap`,
   * which is topic-scoped — those carry `tags` instead.
   */
  sourceMemoryIds: string[];
  severity: 'high' | 'medium' | 'low' | string;
  suggestion?: string;
  /** Topic tags for `knowledge_gap` insights; absent otherwise. */
  tags?: string[];
}

export type TemporalAction = 'current' | 'expired' | 'history' | 'invalidate';

/**
 * One memory in a temporal-versioning result. Field names mirror the wire
 * shape from `tools/temporal.rs` exactly (snake_case, retention as a
 * formatted string for the `current` action). The dashboard mapper in
 * `api.temporal()` normalises these into the camelCase shape consumed by
 * components.
 *
 * Kept as a structural type rather than a class because we want JSON.parse
 * output to flow through unchanged.
 */
export interface TemporalEntry {
  id: string;
  content: string;
  /** ISO-8601, present whenever the engine has stored a validity window. */
  validFrom?: string;
  /** Either future (still valid) or past (expired) — caller filters. */
  validUntil?: string;
  /** Days since `validUntil` for `expired` action; undefined elsewhere. */
  daysExpired?: number;
  /** Numeric in `[0, 1]`. Backend serialises with `format!("{:.2}", ...)`
   * for `current`; `api.temporal` converts to a number. */
  retention: number;
  tags: string[];
}

export interface TemporalResult {
  action: TemporalAction;
  topic?: string;
  count: number;
  memories: TemporalEntry[];
}

export interface ConfidenceResult {
  action: string;
  results: ConfidenceEntry[];
  summary?: string;
}

export interface ConfidenceEntry {
  id: string;
  content: string;
  confidence: number;
  dimensions: {
    encoding: number;
    retrieval: number;
    temporal: number;
    evidence: number;
  };
  classification: string;
  concern?: string;
}

export interface ExploreResult {
  content?: string;
  nodeType?: string;
  score?: number;
  similarity?: number;
  retention?: number;
  connectionType?: string;
}

export interface ExploreResponse {
  results: ExploreResult[];
  nodes?: ExploreResult[];
  chain?: ExploreResult[];
  bridges?: ExploreResult[];
}

/**
 * One memory the predict engine guesses the user will need next, based on
 * recent activity. Currently a deterministic "last 10 by recency" heuristic
 * — the dashboard surfaces it so the cognitive engine can be observed and,
 * later, swapped for a real spreading-activation predictor without touching
 * UI code.
 */
export interface PredictedMemory {
  id: string;
  content: string;
  nodeType: string;
  retention: number;
  /** Backend bucket: "low" | "medium" | "high". Treat as opaque ranking. */
  predictedNeed: string;
}

export interface PredictResponse {
  predictions: PredictedMemory[];
  /** What the prediction model is conditioning on (e.g. "recent_activity"). */
  basedOn: string;
}

export const NODE_TYPE_COLORS: Record<string, string> = {
  fact: '#00A8FF',
  concept: '#9D00FF',
  event: '#FFB800',
  person: '#00FFD1',
  place: '#00D4FF',
  note: '#8B95A5',
  pattern: '#FF3CAC',
  decision: '#FF4757',
};

export const EVENT_TYPE_COLORS: Record<string, string> = {
  MemoryCreated: '#00FFD1',
  MemoryUpdated: '#00A8FF',
  MemoryDeleted: '#FF4757',
  MemoryPromoted: '#00FF88',
  MemoryDemoted: '#FF6B35',
  SearchPerformed: '#818CF8',
  DreamStarted: '#9D00FF',
  DreamProgress: '#B44AFF',
  DreamCompleted: '#C084FC',
  ConsolidationStarted: '#FFB800',
  ConsolidationCompleted: '#FF9500',
  RetentionDecayed: '#FF4757',
  ConnectionDiscovered: '#00D4FF',
  ActivationSpread: '#14E8C6',
  ImportanceScored: '#FF3CAC',
  Heartbeat: '#8B95A5',
};

export function retentionColor(r: number): string {
  if (r > 0.7) return '#10b981';
  if (r > 0.4) return '#f59e0b';
  return '#ef4444';
}

export const EPISTEMIC_STATUS_COLORS: Record<string, string> = {
  world: '#00A8FF',
  experience: '#FFB800',
  observation: '#FF3CAC',
  opinion: '#9D00FF',
};

export const MEMORY_SYSTEM_COLORS: Record<string, string> = {
  episodic: '#FFB800',
  semantic: '#00A8FF',
  procedural: '#00FFD1',
};

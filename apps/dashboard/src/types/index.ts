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

export interface ContradictionPair {
  survivorId: string;
  demotedId: string;
  similarity: number;
  reason: string;
}

export interface DreamResult {
  status: string;
  memoriesReplayed: number;
  connectionsPersisted: number;
  insights: DreamInsight[];
  contradictions: ContradictionPair[];
  memoriesDemoted: string[];
  stats: {
    new_connections_found: number;
    connections_persisted: number;
    memories_strengthened: number;
    memories_compressed: number;
    contradictions_found: number;
    memories_demoted: number;
    insights_generated: number;
    duration_ms: number;
  };
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
export interface ReflectResult {
  focus: string | null;
  depth: string;
  insights: ReflectInsight[];
  summary: string;
  memoriesAnalyzed: number;
}

export interface ReflectInsight {
  type: string;
  description: string;
  memoryIds: string[];
  severity: string;
  suggestion?: string;
}

export interface TemporalResult {
  action: string;
  topic?: string;
  results: TemporalEntry[];
  total: number;
}

export interface TemporalEntry {
  id: string;
  content: string;
  validFrom?: string;
  validUntil?: string;
  nodeType: string;
  retention: number;
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

export interface PredictResponse {
  predictions: {
    topic: string;
    probability: number;
    relatedMemories: string[];
  }[];
  context?: Record<string, unknown>;
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


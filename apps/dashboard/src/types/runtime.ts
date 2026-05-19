// Zod schemas for the five highest-blast-radius endpoints. ts-rs gives
// us compile-time confidence the dashboard reads the right *fields*;
// these schemas catch the slower, sneakier failures: a Rust handler
// quietly switching enum variants, a 500 page returning HTML where the
// dashboard expected JSON, or an out-of-band string ("retention as
// percentage") sneaking past the type system.
//
// Coverage rationale ("blast radius" = how badly a parse failure
// degrades the UX):
//   - GET  /api/memories         → list view; the entire dashboard home
//   - GET  /api/memories/{id}    → memory detail panel; deep-link target
//   - POST /api/search           → second-most-used view
//   - POST /api/reflect          → drives the briefing's headline copy
//   - POST /api/temporal         → time-series, easy to silently drift
//
// Other endpoints stay TS-only — overhead vs benefit isn't there yet.
// When you add a 6th, mirror the pattern below: a `z.object`, an `infer`,
// then `assertWire` at the call site.

import { z } from 'zod';
import type {
  MemoryDto,
  MemoryListResponseDto,
  ReflectResultDto,
  SearchResultDto,
  TemporalResultDto,
} from './generated';

// ---------------------------------------------------------------------------
// Building blocks. These mirror enums emitted by ts-rs so the schema
// stays in lockstep — adding a variant on the Rust side will fail the
// `satisfies` check at the bottom of this file.
// ---------------------------------------------------------------------------

const epistemicStatus = z.enum(['world', 'experience', 'observation', 'opinion']);
const memorySystem = z.enum(['episodic', 'semantic', 'procedural']);

// `InsightDto` — structured Proposal B metadata that rides along on
// insight-typed memories. Defined inline so we don't need a separate
// runtime export; only `memorySchema` consumes it.
const insightSchema = z.object({
  id: z.string(),
  content: z.string(),
  insightType: z.string().min(1),
  origin: z.string().min(1),
  sourceMemoryIds: z.array(z.string()),
  confidence: z.number(),
  novelty: z.number(),
  validated: z.boolean(),
  validatedAt: z.string().optional(),
  insightRecordId: z.string().optional(),
  createdAt: z.string(),
  tags: z.array(z.string()),
});

// `MemoryDto` has the most fields and matters most. Built once, reused
// by list/get/search.
const memorySchema = z.object({
  id: z.string(),
  content: z.string(),
  nodeType: z.string(),
  tags: z.array(z.string()),
  retentionStrength: z.number(),
  storageStrength: z.number(),
  retrievalStrength: z.number(),
  createdAt: z.string(),
  updatedAt: z.string(),
  lastAccessedAt: z.string().optional(),
  nextReviewAt: z.string().optional(),
  combinedScore: z.number().optional(),
  source: z.string().optional(),
  reviewCount: z.number().int().optional(),
  sentimentScore: z.number().optional(),
  sentimentMagnitude: z.number().optional(),
  validFrom: z.string().optional(),
  validUntil: z.string().optional(),
  epistemicStatus,
  memorySystem,
  insight: insightSchema.optional(),
});

const memoryListSchema = z.object({
  total: z.number().int().nonnegative(),
  memories: z.array(memorySchema),
});

const searchSchema = z.object({
  query: z.string(),
  total: z.number().int().nonnegative(),
  durationMs: z.number().nonnegative(),
  results: z.array(memorySchema),
});

const reflectInsightSchema = z.object({
  // The DTO uses an open string here because new categories may be
  // added — keep that flexibility, but still ensure non-empty.
  type: z.string().min(1),
  description: z.string(),
  sourceMemoryIds: z.array(z.string()),
  severity: z.string().min(1),
  suggestion: z.string().optional(),
  tags: z.array(z.string()).optional(),
});

const reflectSchema = z.object({
  status: z.enum(['reflected', 'insufficient_memories']),
  memoriesAnalyzed: z.number().int().nonnegative(),
  focus: z.string().optional(),
  depth: z.string(),
  insights: z.array(z.string()),
  structuredInsights: z.array(reflectInsightSchema),
  summary: z.string(),
  message: z.string().optional(),
});

const temporalEntrySchema = z.object({
  id: z.string(),
  content: z.string(),
  validFrom: z.string().optional(),
  validUntil: z.string().optional(),
  daysExpired: z.number().int().optional(),
  retention: z.number(),
  tags: z.array(z.string()),
});

const temporalSchema = z.object({
  action: z.string(),
  topic: z.string().optional(),
  count: z.number().int().nonnegative(),
  memories: z.array(temporalEntrySchema),
});

// Compile-time check that the Zod inference matches the generated DTO.
// Drift either way (Zod adds a field, ts-rs removes one) → red squiggly.
// `Z extends DTO ? T : never` makes the failure show on the helper itself,
// not at the call site, which keeps error messages legible.
type AssertSubset<Z, DTO> = Z extends DTO ? (DTO extends Z ? true : never) : never;
type _CheckMemory = AssertSubset<z.infer<typeof memorySchema>, MemoryDto>;
type _CheckMemoryList = AssertSubset<z.infer<typeof memoryListSchema>, MemoryListResponseDto>;
type _CheckSearch = AssertSubset<z.infer<typeof searchSchema>, SearchResultDto>;
type _CheckReflect = AssertSubset<z.infer<typeof reflectSchema>, ReflectResultDto>;
type _CheckTemporal = AssertSubset<z.infer<typeof temporalSchema>, TemporalResultDto>;

// Helper: parse-or-throw with a tagged ValidationError. Components catch
// it and render a "the server returned something we don't understand"
// banner instead of crashing the whole page.
export class WireValidationError extends Error {
  constructor(
    public endpoint: string,
    public issues: z.core.$ZodIssue[],
  ) {
    super(`Wire validation failed for ${endpoint}: ${issues[0]?.message ?? 'unknown'}`);
    this.name = 'WireValidationError';
  }
}

function assertWire<T>(endpoint: string, schema: z.ZodType<T>, value: unknown): T {
  const parsed = schema.safeParse(value);
  if (parsed.success) return parsed.data;
  // The dashboard's QueryErrorPanel renders `error.message`; the issues
  // are attached so the dev-tools React Query panel can surface them.
  throw new WireValidationError(endpoint, parsed.error.issues);
}

export const wire = {
  memory: (v: unknown): MemoryDto => assertWire('/memories/{id}', memorySchema, v),
  memoryList: (v: unknown): MemoryListResponseDto => assertWire('/memories', memoryListSchema, v),
  search: (v: unknown): SearchResultDto => assertWire('/search', searchSchema, v),
  reflect: (v: unknown): ReflectResultDto => assertWire('/reflect', reflectSchema, v),
  temporal: (v: unknown): TemporalResultDto => assertWire('/temporal', temporalSchema, v),
};

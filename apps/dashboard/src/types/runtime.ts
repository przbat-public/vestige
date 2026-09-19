// Zod schemas for the six highest-blast-radius endpoints. ts-rs gives
// us compile-time confidence the dashboard reads the right *fields*;
// these schemas catch the slower, sneakier failures: a Rust handler
// quietly switching enum variants, a 500 page returning HTML where the
// dashboard expected JSON, or an out-of-band string ("retention as
// percentage") sneaking past the type system.
//
// Coverage rationale ("blast radius" = how badly a parse failure
// degrades the UX):
//   - GET   /api/memories         → list view; the entire dashboard home
//   - GET   /api/memories/{id}    → memory detail panel; deep-link target
//   - PATCH /api/memories/{id}    → edit form; the handler wraps the DTO
//                                   in `{ memory, field }` (see below)
//   - GET   /api/search           → second-most-used view
//   - POST  /api/reflect          → drives the briefing's headline copy
//   - POST  /api/temporal         → time-series, easy to silently drift
//
// Other endpoints stay TS-only — overhead vs benefit isn't there yet.
// When you add a 7th, mirror the pattern below: a `z.object`, an `infer`,
// then `assertWire` at the call site.

import { z } from 'zod';
import type {
  MemoryDto,
  MemoryListResponseDto,
  MemoryUpdateResultDto,
  ReflectResultDto,
  SearchResultDto,
  TemporalResultDto,
} from './generated';

// ---------------------------------------------------------------------------
// Building blocks. These mirror enums emitted by ts-rs so the schema
// stays in lockstep — adding a variant on the Rust side fails the
// compile-time assertions at the bottom of this file.
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
//
// Optional fields use `optional()`, not `nullish()`: `From<&KnowledgeNode>`
// always populates them and `skip_serializing_if = "Option::is_none"`
// *omits* the key when a handler blanks it (e.g. `into_list_view()`, or the
// temporal/sentiment window `PATCH` clears) — the server never sends an
// explicit `null` here.
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

// `PATCH /api/memories/{id}` wraps the post-update DTO in an envelope:
// `MemoryUpdateResultDto { memory, field }` (`wire/memory.rs`). The
// dashboard used to declare the bare `Memory` here, which meant
// `memory.id` was `undefined` at the call site and the per-id cache
// entry was written under `['memory', undefined]`.
const memoryUpdateSchema = z.object({
  memory: memorySchema,
  field: z.string(),
});

// ---------------------------------------------------------------------------
// Compile-time checks that each Zod inference matches its generated DTO.
//
// These MUST stay `const` declarations assigned `true`: a bare
// `type _Check = AssertSubset<…>` alias without a value is never
// instantiated, so `tsc` silently skips it (the previous shape of this
// block was a no-op). The assignments below are checked eagerly and fail
// with TS2322 ("Type 'true' is not assignable to type 'never'") when the
// two sides disagree.
//
// The comparison deliberately normalises `null`/`undefined` rather than
// enumerating nullable fields, so a new nullable field doesn't require
// touching this block. That keeps the two things worth catching:
//   • a property that exists on one side only (schema/ts-rs drift), and
//   • a property whose *value* type disagrees (`string` vs `number`, …),
// while still rejecting `required` on one side and `optional` on the other.
// ---------------------------------------------------------------------------

/** Collapse `.extend()` intersections so error messages stay readable. */
type Memoize<T> = { [K in keyof T]: T[K] } & {};

/** Drop `null` from every property, recursively through arrays/objects. */
type WithoutNull<T> = T extends readonly (infer U)[]
  ? WithoutNull<U>[]
  : T extends object
    ? { [K in keyof T]: NormalizeOne<T[K]> }
    : T;

/**
 * A DTO property that accepts `undefined` is optional: treat `null` as
 * equivalent (ts-rs emits `field?: T`, a handler may serialise `null`).
 * A required DTO property gets no such leniency.
 */
type NormalizeOne<V> = undefined extends V ? Exclude<WithoutNull<V>, null> | undefined : WithoutNull<V>;

/** Reject Zod-side properties that the DTO doesn't declare at all. */
type NoExtraProps<Z, DTO> = [Exclude<keyof Z, keyof DTO>] extends [never] ? true : false;

/** Reject `undefined` on DTO properties the DTO itself declares required. */
type DtoRequiredOk<Z, DTO> = {
  [K in keyof DTO]-?: undefined extends DTO[K]
    ? true
    : K extends keyof Z
      ? undefined extends Z[K]
        ? false
        : true
      : false;
}[keyof DTO] extends true
  ? true
  : false;

type AssertSubset<Z, DTO> =
  NoExtraProps<Z, DTO> extends true
    ? WithoutNull<Z> extends WithoutNull<DTO>
      ? DtoRequiredOk<Z, DTO> extends true
        ? true
        : never
      : never
    : never;

export const _checkMemory: AssertSubset<Memoize<z.infer<typeof memorySchema>>, MemoryDto> = true;
export const _checkMemoryList: AssertSubset<Memoize<z.infer<typeof memoryListSchema>>, MemoryListResponseDto> = true;
export const _checkSearch: AssertSubset<Memoize<z.infer<typeof searchSchema>>, SearchResultDto> = true;
export const _checkReflect: AssertSubset<Memoize<z.infer<typeof reflectSchema>>, ReflectResultDto> = true;
export const _checkTemporal: AssertSubset<Memoize<z.infer<typeof temporalSchema>>, TemporalResultDto> = true;
export const _checkMemoryUpdate: AssertSubset<
  Memoize<z.infer<typeof memoryUpdateSchema>>,
  MemoryUpdateResultDto
> = true;

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
  memoryUpdate: (v: unknown): MemoryUpdateResultDto => assertWire('PATCH /memories/{id}', memoryUpdateSchema, v),
  search: (v: unknown): SearchResultDto => assertWire('/search', searchSchema, v),
  reflect: (v: unknown): ReflectResultDto => assertWire('/reflect', reflectSchema, v),
  temporal: (v: unknown): TemporalResultDto => assertWire('/temporal', temporalSchema, v),
};

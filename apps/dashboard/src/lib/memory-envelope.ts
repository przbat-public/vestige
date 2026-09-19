/**
 * Envelope helpers for the memory-list cache.
 *
 * Every list-shaped cache entry under `['memories', params]` holds a
 * `MemoryListResponseDto` — `{ total, memories }` — because that is what
 * `GET /api/memories` returns and `wire.memoryList` re-validates. Both the
 * single-row undo delete and the bulk undo delete used to filter a phantom
 * `{ items }` / bare-array envelope, so the optimistic strip was a silent
 * no-op and the row stayed visible for the whole undo window.
 *
 * Keep the shape knowledge in one place: a `{ items }` envelope or a bare
 * array is treated as "not a list cache" and left untouched, so a future
 * envelope change degrades to "no optimistic update" instead of corrupting
 * the cache.
 */

import type { Memory, MemoryListResponseDto } from '@/types';

/** True for the `{ total, memories }` envelope emitted by the list endpoint. */
function isListEnvelope(value: unknown): value is MemoryListResponseDto {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as { memories?: unknown; total?: unknown };
  if (!Array.isArray(candidate.memories)) return false;
  return typeof candidate.total === 'number';
}

/**
 * Remove every memory whose id is in `ids`, decrementing `total` by the
 * number of rows actually removed. Returns `undefined` for cache entries
 * that aren't a list envelope, which tells React Query to leave them be.
 */
export function withoutMemories(data: unknown, ids: ReadonlySet<string>): MemoryListResponseDto | undefined {
  if (!isListEnvelope(data)) return undefined;
  if (ids.size === 0) return data;
  const kept = data.memories.filter((m: Memory) => !ids.has(m.id));
  if (kept.length === data.memories.length) return data;
  return {
    ...data,
    memories: kept,
    total: Math.max(0, data.total - (data.memories.length - kept.length)),
  };
}

/** Single-id convenience wrapper around {@link withoutMemories}. */
export function withoutMemory(data: unknown, id: string): MemoryListResponseDto | undefined {
  return withoutMemories(data, new Set([id]));
}

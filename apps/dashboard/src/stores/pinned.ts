/**
 * Pinned memories — client-side favorites that float to the top of the
 * memories list. Pure local state because the server already has its
 * own importance signal (FSRS retention, promotion votes). Pinning is
 * about *this user's* working set right now, not about the canonical
 * value of the memory.
 *
 * Storage: a localStorage-backed Set of memory IDs (serialised as an
 * array). Reads are O(n) over a small list; we keep the API around a
 * Set so callers don't have to think about it.
 */

import { useEffect, useState } from 'react';

const KEY = 'vestige.pinned-memories.v1';
const MAX = 64;

function read(): Set<string> {
  if (typeof window === 'undefined') return new Set();
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((v): v is string => typeof v === 'string').slice(0, MAX));
  } catch {
    return new Set();
  }
}

function write(ids: Set<string>): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(KEY, JSON.stringify(Array.from(ids).slice(0, MAX)));
  } catch {
    // Quota exceeded or storage disabled — best-effort.
  }
}

const listeners = new Set<() => void>();

function notify() {
  for (const fn of listeners) fn();
}

export function getPinned(): Set<string> {
  return read();
}

export function isPinned(id: string): boolean {
  return read().has(id);
}

export function togglePinned(id: string): boolean {
  const set = read();
  const next = !set.has(id);
  if (next) set.add(id);
  else set.delete(id);
  write(set);
  notify();
  return next;
}

export function clearPinned(): void {
  write(new Set());
  notify();
}

/**
 * React subscription to the pinned set. Re-renders on every toggle so
 * UI badges stay in sync across components. The `set` is a fresh
 * snapshot per render — never mutate it directly; use the helpers.
 */
export function usePinned(): Set<string> {
  const [snapshot, setSnapshot] = useState<Set<string>>(() => read());
  useEffect(() => {
    const onChange = () => setSnapshot(read());
    listeners.add(onChange);
    return () => {
      listeners.delete(onChange);
    };
  }, []);
  return snapshot;
}

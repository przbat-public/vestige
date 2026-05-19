/**
 * Tutorial progress — which sections the user has actually read (or at least
 * scrolled past long enough to count). Stored in localStorage so it survives
 * reloads but never leaves the machine.
 *
 * "Read" here means: the section was in the viewport for at least 1.5s.
 * Time-based gating, not click-based, because a section that flashes through
 * scroll doesn't mean the user actually saw it.
 *
 * The store also tracks the last-seen Vestige version so we can show a
 * "what's new" banner when the dashboard ships new tutorial content.
 */

import { useEffect, useState } from 'react';

const KEY_SECTIONS = 'vestige.tutorial-progress.v1';
const KEY_LAST_VERSION = 'vestige.tutorial-last-version.v1';
const MAX_SECTIONS = 64;

function readSections(): Set<string> {
  if (typeof window === 'undefined') return new Set();
  try {
    const raw = window.localStorage.getItem(KEY_SECTIONS);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((v): v is string => typeof v === 'string').slice(0, MAX_SECTIONS));
  } catch {
    return new Set();
  }
}

function writeSections(ids: Set<string>): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(KEY_SECTIONS, JSON.stringify(Array.from(ids).slice(0, MAX_SECTIONS)));
  } catch {
    // Quota exhausted or storage disabled — best-effort.
  }
}

const listeners = new Set<() => void>();
function notify() {
  for (const fn of listeners) fn();
}

export function getExploredSections(): Set<string> {
  return readSections();
}

export function markSectionExplored(id: string): void {
  const set = readSections();
  if (set.has(id)) return;
  set.add(id);
  writeSections(set);
  notify();
}

export function resetTutorialProgress(): void {
  writeSections(new Set());
  notify();
}

/** React subscription — re-renders any consumer when sections change. */
export function useExploredSections(): Set<string> {
  const [snapshot, setSnapshot] = useState<Set<string>>(() => readSections());
  useEffect(() => {
    const onChange = () => setSnapshot(readSections());
    listeners.add(onChange);
    return () => {
      listeners.delete(onChange);
    };
  }, []);
  return snapshot;
}

/* ----- last-seen version tracking ------------------------------------- */

export function getLastSeenVersion(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    return window.localStorage.getItem(KEY_LAST_VERSION);
  } catch {
    return null;
  }
}

export function setLastSeenVersion(version: string): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(KEY_LAST_VERSION, version);
  } catch {
    // Best-effort.
  }
}

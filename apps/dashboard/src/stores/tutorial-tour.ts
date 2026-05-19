/**
 * Interactive tour state. One boolean: has the user dismissed the tour CTA?
 * Used by the tutorial header to stop nagging once they've taken the tour
 * (or explicitly skipped it).
 */

import { useEffect, useState } from 'react';

const KEY = 'vestige.tutorial-tour.v1';

function read(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    return window.localStorage.getItem(KEY) === '1';
  } catch {
    return false;
  }
}

function write(value: boolean): void {
  if (typeof window === 'undefined') return;
  try {
    if (value) window.localStorage.setItem(KEY, '1');
    else window.localStorage.removeItem(KEY);
  } catch {
    // Best-effort.
  }
}

const listeners = new Set<() => void>();
function notify() {
  for (const fn of listeners) fn();
}

export function hasTakenTour(): boolean {
  return read();
}

export function markTourTaken(): void {
  write(true);
  notify();
}

export function resetTourState(): void {
  write(false);
  notify();
}

export function useTourTaken(): boolean {
  const [snapshot, setSnapshot] = useState<boolean>(() => read());
  useEffect(() => {
    const onChange = () => setSnapshot(read());
    listeners.add(onChange);
    return () => {
      listeners.delete(onChange);
    };
  }, []);
  return snapshot;
}

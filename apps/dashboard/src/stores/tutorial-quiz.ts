/**
 * Tutorial quiz result — a tiny localStorage cache of the user's last attempt.
 * Two reasons to persist:
 *
 *   1) When the user comes back to the tutorial we can show "You scored 4/4 last
 *      time" instead of pretending nothing happened.
 *   2) A perfect score unlocks the "completed" badge — a soft signal we can use
 *      in the future ("Skip the intro, you've done it").
 *
 * The schema is deliberately tiny so a future quiz refresh doesn't have to
 * migrate richer state.
 */

import { useEffect, useState } from 'react';

const KEY = 'vestige.tutorial-quiz.v1';

export interface QuizResult {
  /** Number of correct answers. */
  score: number;
  /** Total number of questions when the attempt was made. */
  total: number;
  /** Epoch ms of the attempt. */
  at: number;
}

function read(): QuizResult | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (
      parsed &&
      typeof parsed === 'object' &&
      typeof parsed.score === 'number' &&
      typeof parsed.total === 'number' &&
      typeof parsed.at === 'number'
    ) {
      return parsed as QuizResult;
    }
    return null;
  } catch {
    return null;
  }
}

function write(value: QuizResult | null): void {
  if (typeof window === 'undefined') return;
  try {
    if (value === null) window.localStorage.removeItem(KEY);
    else window.localStorage.setItem(KEY, JSON.stringify(value));
  } catch {
    // Best-effort.
  }
}

const listeners = new Set<() => void>();
function notify() {
  for (const fn of listeners) fn();
}

export function getQuizResult(): QuizResult | null {
  return read();
}

export function saveQuizResult(result: QuizResult): void {
  write(result);
  notify();
}

export function clearQuizResult(): void {
  write(null);
  notify();
}

export function useQuizResult(): QuizResult | null {
  const [snapshot, setSnapshot] = useState<QuizResult | null>(() => read());
  useEffect(() => {
    const onChange = () => setSnapshot(read());
    listeners.add(onChange);
    return () => {
      listeners.delete(onChange);
    };
  }, []);
  return snapshot;
}

/**
 * Lightweight client-side telemetry.
 *
 * Purpose: produce evidence about how the dashboard actually gets used so
 * the next iteration is data-driven instead of vibe-driven. We deliberately
 * do NOT ship to any external analytics provider — events accumulate in a
 * bounded localStorage ring buffer and surface via `getEvents()` / the
 * `vestige.telemetry` console handle for inspection.
 *
 * What we capture:
 *   - `page_view`   — every route entry, with duration on exit.
 *   - `event`       — discrete user actions (cmd palette open, mutation
 *                     fired, undo clicked, etc.). Pages instrument the
 *                     interactions they care about.
 *
 * What we deliberately do NOT capture:
 *   - Free-text content (query strings, memory content) — to keep this
 *     local and PII-safe by construction.
 *   - High-frequency events (mousemove, keystrokes) — they'd drown signal
 *     in noise and bloat localStorage.
 */

const STORAGE_KEY = 'vestige-telemetry';
// Cap at ~3 days of moderate use to keep localStorage <128 KB. Older events
// fall off the front of the buffer when we hit the limit.
const MAX_EVENTS = 500;

export interface TelemetryEvent {
  t: number; // epoch ms
  kind: 'page_view' | 'event';
  name: string;
  meta?: Record<string, string | number | boolean>;
}

/** Read the current event buffer. Useful for `vestige.telemetry.dump()`. */
export function getEvents(): TelemetryEvent[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    // If localStorage is unavailable (private browsing on some browsers)
    // or the buffer is corrupt, behave as if we're starting fresh —
    // telemetry is best-effort, never block on it.
    return [];
  }
}

function persist(events: TelemetryEvent[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(events));
  } catch {
    // Storage quota or private mode: silently drop the write. Losing
    // telemetry is never a reason to surface an error to the user.
  }
}

/** Record one event. The wrapper hooks (`useTrackPageView`, `track()`) are
 * the public API; prefer them so we get consistent naming. */
function record(kind: TelemetryEvent['kind'], name: string, meta?: TelemetryEvent['meta']) {
  const all = getEvents();
  all.push({ t: Date.now(), kind, name, meta });
  // Ring-buffer: trim from the front so the newest events always survive.
  if (all.length > MAX_EVENTS) all.splice(0, all.length - MAX_EVENTS);
  persist(all);
}

/**
 * Fire a discrete event. Call sites should pick names from a small,
 * stable vocabulary — see EVENT below for the canonical list.
 */
export function track(name: string, meta?: TelemetryEvent['meta']) {
  record('event', name, meta);
}

/** Canonical event names — single source of truth so reviewers can search
 * for `EVENT.command_palette_open` and find every call-site. */
export const EVENT = {
  command_palette_open: 'command_palette_open',
  command_palette_select: 'command_palette_select',
  shortcuts_dialog_open: 'shortcuts_dialog_open',
  add_memory_open: 'add_memory_open',
  add_memory_submit: 'add_memory_submit',
  memory_delete: 'memory_delete',
  memory_delete_undo: 'memory_delete_undo',
  memory_demote_undo: 'memory_demote_undo',
  dream_cycle_run: 'dream_cycle_run',
  reasoning_run: 'reasoning_run',
  search_submit: 'search_submit',
  search_recent_use: 'search_recent_use',
  density_toggle: 'density_toggle',
  theme_toggle: 'theme_toggle',
  temporal_invalidate: 'temporal_invalidate',
  memory_pin: 'memory_pin',
  memory_unpin: 'memory_unpin',
  tutorial_mode_change: 'tutorial_mode_change',
  tutorial_section_explored: 'tutorial_section_explored',
  tutorial_glossary_open: 'tutorial_glossary_open',
  tutorial_page_cta: 'tutorial_page_cta',
  tutorial_tour_start: 'tutorial_tour_start',
  tutorial_tour_complete: 'tutorial_tour_complete',
  tutorial_tour_skip: 'tutorial_tour_skip',
  tutorial_quiz_submit: 'tutorial_quiz_submit',
  tutorial_quiz_replay: 'tutorial_quiz_replay',
  tutorial_curve_drag: 'tutorial_curve_drag',
  tutorial_search_use: 'tutorial_search_use',
} as const;

/** Mount-once hook that records a page view and its duration on unmount. */
import { useEffect } from 'react';

export function useTrackPageView(name: string) {
  useEffect(() => {
    const start = Date.now();
    record('page_view', name);
    return () => {
      // We emit the duration as a separate event rather than mutating the
      // mount event in place so the buffer stays append-only — simpler to
      // reason about, simpler to ship anywhere else later.
      record('event', 'page_view_end', { name, duration_ms: Date.now() - start });
    };
  }, [name]);
}

/** Clear the buffer. Useful for "I want to record a clean session". */
export function clearEvents() {
  persist([]);
}

// Expose a debug handle on `window.vestige.telemetry` so we (and curious
// users) can poke at the buffer from the browser console without importing
// anything. Guarded so a test or SSR context doesn't crash.
if (typeof window !== 'undefined') {
  const w = window as unknown as {
    vestige?: { telemetry?: unknown };
  };
  w.vestige = w.vestige ?? {};
  w.vestige.telemetry = {
    dump: getEvents,
    clear: clearEvents,
    track,
    events: EVENT,
  };
}

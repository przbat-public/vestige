import { useEffect } from 'react';
import { EVENT, track } from '@/stores/telemetry';
import { markSectionExplored } from '@/stores/tutorial-progress';
import { TUTORIAL_SECTIONS } from './sections';

/**
 * Auto-marks a section as "explored" once it has stayed in the viewport
 * for `dwellMs` milliseconds.
 *
 * Time-based gate, not click-based — accidental scrolls past a section
 * shouldn't count, but a section that the user pauses on does. 1.5s is
 * the default, roughly the time it takes to read a section heading and
 * the first sentence.
 *
 * Marking is idempotent (the store dedups), so the observer can fire
 * the same section multiple times without inflating the counter.
 */
/** Cancel a pending dwell timer for a section that scrolled out of view. */
function cancelDwell(timers: Map<string, number>, id: string): void {
  const tid = timers.get(id);
  if (tid !== undefined) {
    window.clearTimeout(tid);
    timers.delete(id);
  }
}

/** Schedule a dwell timer that marks the section once it fires. */
function scheduleDwell(timers: Map<string, number>, id: string, dwellMs: number): void {
  if (timers.has(id)) return;
  const tid = window.setTimeout(() => {
    markSectionExplored(id);
    track(EVENT.tutorial_section_explored, { id });
    timers.delete(id);
  }, dwellMs);
  timers.set(id, tid);
}

export function useSectionTracking(dwellMs = 1500) {
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const targets = TUTORIAL_SECTIONS.map((s) => document.getElementById(s.id)).filter((el): el is HTMLElement => !!el);
    if (targets.length === 0) return;

    const timers = new Map<string, number>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const id = entry.target.id;
          const isReading = entry.isIntersecting && entry.intersectionRatio >= 0.3;
          if (isReading) scheduleDwell(timers, id, dwellMs);
          else cancelDwell(timers, id);
        }
      },
      { threshold: [0, 0.3, 0.6] },
    );
    for (const el of targets) observer.observe(el);

    return () => {
      observer.disconnect();
      for (const tid of timers.values()) window.clearTimeout(tid);
    };
  }, [dwellMs]);
}

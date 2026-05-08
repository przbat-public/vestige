import { useEffect, useState } from 'react';

/**
 * Reactive hook for the user's `prefers-reduced-motion` setting (WCAG 2.3.3).
 *
 * Returns `true` when the user has requested reduced motion at the OS level
 * (macOS Accessibility, Windows Animations off, iOS Reduce Motion, etc.).
 *
 * Use it to suppress non-essential animations:
 * - Graph auto-rotate
 * - Particle systems
 * - Decorative pulses on idle
 *
 * Do NOT use it to remove user-triggered animations (button presses, hover
 * focus rings) — those are signals, not decoration. See WCAG 2.2 SC 2.3.3.
 *
 * SSR-safe: returns `false` when window is unavailable.
 */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState<boolean>(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return false;
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  });

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mql = window.matchMedia('(prefers-reduced-motion: reduce)');
    const handler = (e: MediaQueryListEvent) => setReduced(e.matches);
    mql.addEventListener('change', handler);
    return () => mql.removeEventListener('change', handler);
  }, []);

  return reduced;
}

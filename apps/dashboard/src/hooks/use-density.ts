import { useCallback, useEffect, useState } from 'react';

export type Density = 'comfortable' | 'compact';

const STORAGE_KEY = 'vestige-density';

function getInitial(): Density {
  if (typeof window === 'undefined') return 'comfortable';
  const saved = localStorage.getItem(STORAGE_KEY);
  return saved === 'compact' ? 'compact' : 'comfortable';
}

/**
 * Density preference, mirroring how `useTheme` works.
 *
 * Two values:
 *  - `comfortable` (default) — generous padding, readable for casual review.
 *  - `compact` — denser cards/lists, suitable for power users scanning long
 *    feeds (memories, hubs, decisions).
 *
 * The hook writes `data-density="..."` on `<html>` so the global CSS in
 * `app.css` can target it without every component needing density-aware
 * Tailwind utilities.
 */
export function useDensity() {
  const [density, setDensityState] = useState<Density>(getInitial);

  useEffect(() => {
    document.documentElement.setAttribute('data-density', density);
    localStorage.setItem(STORAGE_KEY, density);
  }, [density]);

  const toggle = useCallback(() => {
    setDensityState((prev) => (prev === 'comfortable' ? 'compact' : 'comfortable'));
  }, []);

  const setDensity = useCallback((next: Density) => setDensityState(next), []);

  return { density, toggle, setDensity, isCompact: density === 'compact' };
}

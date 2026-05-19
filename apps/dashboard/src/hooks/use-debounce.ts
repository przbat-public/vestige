import { useEffect, useState } from 'react';

/**
 * Returns a debounced copy of `value` that updates only after `delayMs`
 * of stillness. The canonical use case is search-as-you-type: bind the
 * input to immediate state, pass that state through `useDebounce`, and
 * feed the debounced value to the network query.
 *
 * Why not lodash/throttle: this is six lines, no dependency, and the
 * "trailing edge only" behaviour is exactly what we want for search.
 * Throttle (which fires on the leading edge too) would issue an extra
 * request the moment the user starts typing — wasted work.
 */
export function useDebounce<T>(value: T, delayMs = 250): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(id);
  }, [value, delayMs]);
  return debounced;
}

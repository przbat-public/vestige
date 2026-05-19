/**
 * Recent searches — a tiny localStorage-backed MRU list (no PII guarantees:
 * whatever the user types ends up here, so we cap and let them clear it).
 *
 * Why this lives in `stores/` and not as a hook: it's a plain values API
 * with no React lifecycle. Pages that want reactivity wrap it in their own
 * `useState` + effect (see MemoriesPage). Keeping it framework-free means
 * the Command Palette can also pull it without a hook.
 */

const KEY = 'vestige.recent-searches.v1';
const MAX = 8;

function read(): string[] {
  if (typeof window === 'undefined') return [];
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((v): v is string => typeof v === 'string').slice(0, MAX);
  } catch {
    return [];
  }
}

function write(items: string[]): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(KEY, JSON.stringify(items.slice(0, MAX)));
  } catch {
    // Quota exceeded or storage disabled — recent searches are best-effort.
  }
}

export function getRecentSearches(): string[] {
  return read();
}

/**
 * Push a query to the top of the MRU list. Empty / whitespace-only queries
 * are ignored — they're not useful and would crowd out real recents.
 * Duplicates are deduped (case-insensitive) and the deduped entry moves to
 * the top, matching what browsers do with URL history.
 */
export function pushRecentSearch(query: string): void {
  const q = query.trim();
  if (!q) return;
  const lower = q.toLowerCase();
  const existing = read().filter((v) => v.toLowerCase() !== lower);
  write([q, ...existing]);
}

export function clearRecentSearches(): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.removeItem(KEY);
  } catch {
    // Best-effort.
  }
}

import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { EVENT, track } from '@/stores/telemetry';
import { TUTORIAL_SECTIONS } from './sections';

/**
 * Tiny in-page search over tutorial sections.
 *
 * Strategy:
 *   - Match the user's query against the section label AND its keyword list.
 *     Keywords carry the synonyms the section text doesn't say outright
 *     (e.g. "decay" matches "forgetting curve").
 *   - Hit Enter or click a suggestion to scroll to the section. We don't
 *     filter the page down to matches — that hides context the user might
 *     want when they land there.
 *
 * Why not a "real" fuzzy match (fuse.js, etc.): the section count is small
 * (≤20), translations vary in length, and starts-with / contains beats a
 * fuzzy library at this scale for both clarity and bundle size.
 */
export function TutorialSearch() {
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const inputId = useId();

  const q = query.trim().toLowerCase();
  const hits = q
    ? TUTORIAL_SECTIONS.filter((s) => {
        const label = t(s.labelKey).toLowerCase();
        if (label.includes(q)) return true;
        return s.keywords.some((kw) => kw.toLowerCase().includes(q));
      }).slice(0, 6)
    : [];

  const jumpTo = (id: string) => {
    track(EVENT.tutorial_search_use, { id });
    setOpen(false);
    setQuery('');
    const target = document.getElementById(id);
    if (!target) return;
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    if (typeof window !== 'undefined' && window.history?.replaceState) {
      window.history.replaceState(null, '', `#${id}`);
    }
  };

  return (
    <div className="relative max-w-sm">
      <label htmlFor={inputId} className="sr-only">
        {t('tutorial.search.label')}
      </label>
      <input
        id={inputId}
        type="search"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        // Delay close so a click on a result can register before blur
        // swallows the synthesised mousedown.
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && hits.length > 0) {
            e.preventDefault();
            jumpTo(hits[0].id);
          }
          if (e.key === 'Escape') {
            setQuery('');
            setOpen(false);
          }
        }}
        placeholder={t('tutorial.search.placeholder')}
        className="w-full h-9 px-3 text-sm rounded-lg border border-input bg-background focus:outline-none focus:ring-2 focus:ring-ring"
      />
      {open && hits.length > 0 && (
        <ul className="absolute z-30 top-full left-0 right-0 mt-1 max-h-64 overflow-y-auto rounded-lg border border-border bg-popover shadow-lg">
          {hits.map((hit) => (
            <li key={hit.id}>
              <button
                type="button"
                // Use mousedown so we beat the input's onBlur timeout.
                onMouseDown={(e) => {
                  e.preventDefault();
                  jumpTo(hit.id);
                }}
                className="w-full text-left px-3 py-2 text-xs text-foreground hover:bg-accent focus:bg-accent focus:outline-none"
              >
                {t(hit.labelKey)}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

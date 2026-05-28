/**
 * Sidebar information architecture.
 *
 * Sixteen routes fit into five mental buckets. The audit recommendation was
 * "group 17 routes into 5 sections" — we have 16 (briefing is the landing,
 * counted once) and the split below reflects the user journey rather than
 * alphabetical convenience:
 *
 *  1. Daily   — the four routes a logged-in user touches every session.
 *  2. Memory  — primary browse surfaces over the corpus.
 *  3. Explore — derived / analytic views (graph, reasoning, decisions).
 *  4. System  — configuration of Vestige itself.
 *  5. Help    — onboarding aides.
 *
 * Exported as data instead of JSX so unit tests can assert the IA shape
 * (no orphans, no duplicates, every route in exactly one section) without
 * standing up the entire QueryClient + Router + Zustand stack the live
 * Sidebar component needs.
 */
export interface NavItem {
  /** Route segment (relative to the `/` root). */
  readonly to: string;
  /** i18n key under `nav.*`. */
  readonly labelKey: string;
}

export interface NavSection {
  /** Identifier used for `aria-labelledby` headings. */
  readonly id: 'daily' | 'memory' | 'explore' | 'system' | 'help';
  /** i18n key for the section heading. */
  readonly labelKey: string;
  readonly items: readonly NavItem[];
}

export const NAV_SECTIONS: readonly NavSection[] = [
  {
    id: 'daily',
    labelKey: 'nav.section.daily',
    items: [
      { to: 'briefing', labelKey: 'nav.briefing' },
      { to: 'review', labelKey: 'nav.review' },
      { to: 'intentions', labelKey: 'nav.intentions' },
      { to: 'feed', labelKey: 'nav.feed' },
    ],
  },
  {
    id: 'memory',
    labelKey: 'nav.section.memory',
    items: [
      { to: 'memories', labelKey: 'nav.memories' },
      { to: 'timeline', labelKey: 'nav.timeline' },
      { to: 'hubs', labelKey: 'nav.hubs' },
      { to: 'temporal', labelKey: 'nav.temporal' },
    ],
  },
  {
    id: 'explore',
    labelKey: 'nav.section.explore',
    items: [
      { to: 'graph', labelKey: 'nav.graph' },
      { to: 'explore', labelKey: 'nav.explore' },
      { to: 'reasoning', labelKey: 'nav.reasoning' },
      { to: 'decisions', labelKey: 'nav.decisions' },
      { to: 'insights', labelKey: 'nav.insights' },
    ],
  },
  {
    id: 'system',
    labelKey: 'nav.section.system',
    items: [
      { to: 'stats', labelKey: 'nav.stats' },
      { to: 'settings', labelKey: 'nav.settings' },
    ],
  },
  {
    id: 'help',
    labelKey: 'nav.section.help',
    items: [{ to: 'tutorial', labelKey: 'nav.tutorial' }],
  },
];

/**
 * Flat list of every route the Sidebar surfaces. Useful for command-palette
 * fuzzy search and for the IA invariant tests below. Order matches a
 * top-to-bottom read of the sidebar.
 */
export const NAV_ITEMS_FLAT: readonly NavItem[] = NAV_SECTIONS.flatMap((s) => s.items);

import enResources from '@/i18n/en.json';
import plResources from '@/i18n/pl.json';
import { NAV_ITEMS_FLAT, NAV_SECTIONS } from './nav-sections';

type Bag = Record<string, unknown>;

/**
 * Walk a dotted i18n path through a JSON bag, returning the leaf string or
 * `undefined` if any segment is missing. Treating the resources as `unknown`
 * keeps the test honest if the schema ever drifts to nested arrays.
 */
function readKey(bag: Bag, key: string): string | undefined {
  const parts = key.split('.');
  let cursor: unknown = bag;
  for (const p of parts) {
    if (cursor && typeof cursor === 'object' && p in (cursor as Bag)) {
      cursor = (cursor as Bag)[p];
    } else {
      return undefined;
    }
  }
  return typeof cursor === 'string' ? cursor : undefined;
}

describe('sidebar IA — NAV_SECTIONS', () => {
  it('groups every route into exactly five sections', () => {
    expect(NAV_SECTIONS).toHaveLength(5);
    const ids = NAV_SECTIONS.map((s) => s.id);
    expect(new Set(ids).size).toBe(5);
  });

  it('contains exactly sixteen routes across all sections', () => {
    // Touching this number is a load-bearing decision — the sidebar is the
    // dashboard's spine. If you add or remove a route, update the count
    // here AND verify there is still a section that makes sense for it.
    expect(NAV_ITEMS_FLAT).toHaveLength(16);
  });

  it('has no duplicate route segments', () => {
    const tos = NAV_ITEMS_FLAT.map((i) => i.to);
    expect(new Set(tos).size).toBe(tos.length);
  });

  it('includes the historically-required routes', () => {
    // Pinning the exact membership avoids accidental orphans during future
    // refactors. If you genuinely retire a route, delete it from THIS list
    // and from `nav-sections.ts` in the same PR.
    const expected = [
      'briefing',
      'review',
      'intentions',
      'feed',
      'memories',
      'timeline',
      'hubs',
      'temporal',
      'graph',
      'explore',
      'reasoning',
      'decisions',
      'insights',
      'stats',
      'settings',
      'tutorial',
    ];
    const actual = NAV_ITEMS_FLAT.map((i) => i.to);
    expect(actual.sort()).toEqual(expected.sort());
  });

  it('orders Daily first so the user lands on actionable surfaces', () => {
    // Briefing is the landing route (see App.tsx Navigate). The Daily
    // section must be top so a returning user doesn't have to scan past
    // analytic noise to find their inbox.
    expect(NAV_SECTIONS[0]?.id).toBe('daily');
    expect(NAV_SECTIONS[0]?.items[0]?.to).toBe('briefing');
  });

  it.each([
    ['en', enResources as Bag],
    ['pl', plResources as Bag],
  ])('exposes section + item labels in %s', (_locale: string, bag: Bag) => {
    for (const section of NAV_SECTIONS) {
      expect(readKey(bag, section.labelKey)).toBeTruthy();
      for (const item of section.items) {
        expect(readKey(bag, item.labelKey)).toBeTruthy();
      }
    }
  });
});

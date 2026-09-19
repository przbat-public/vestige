import { QueryClient } from '@tanstack/react-query';
import { queryKeys } from './query';

/**
 * Regression tests for the query-key factory.
 *
 * Two bugs motivated these: `queryKeys.memories()` used to build
 * `['memories', undefined]`, which `partialMatchKey` does NOT treat as a
 * prefix of `['memories', { limit: '100', offset: '0' }]`, so three
 * invalidations were silent no-ops; and nothing pinned the relationship
 * between the factory's keys and the keys the pages actually query.
 */

const LIST_KEY = queryKeys.memories({ limit: '100', offset: '0' });

describe('queryKeys', () => {
  it('memoriesPrefix is the bare prefix every list page key starts with', () => {
    expect(LIST_KEY.slice(0, queryKeys.memoriesPrefix.length)).toEqual([...queryKeys.memoriesPrefix]);
    expect(LIST_KEY).toHaveLength(queryKeys.memoriesPrefix.length + 1);
  });

  it('invalidating the prefix reaches every list page, filtered or not', async () => {
    const qc = new QueryClient();
    const filtered = queryKeys.memories({ limit: '100', offset: '0', node_type: 'fact', tag: 'x' });
    qc.setQueryData(LIST_KEY, { total: 0, memories: [] });
    qc.setQueryData(filtered, { total: 0, memories: [] });

    await qc.invalidateQueries({ queryKey: queryKeys.memoriesPrefix });

    expect(qc.getQueryState(LIST_KEY)?.isInvalidated).toBe(true);
    expect(qc.getQueryState(filtered)?.isInvalidated).toBe(true);
  });

  it('an explicit undefined param does NOT match the list key (the old bug)', async () => {
    const qc = new QueryClient();
    qc.setQueryData(LIST_KEY, { total: 0, memories: [] });

    // `['memories', undefined]` serialises to `['memories', null]` and
    // partialMatchKey rejects it as a prefix. Keep this pinned so nobody
    // reintroduces the optional-parameter factory.
    await qc.invalidateQueries({ queryKey: ['memories', undefined] });

    expect(qc.getQueryState(LIST_KEY)?.isInvalidated).toBe(false);
  });

  it('memoryChangelog has its own cache line under the memory prefix', async () => {
    const qc = new QueryClient();
    const changelog = queryKeys.memoryChangelog('a');
    const detail = queryKeys.memory('a');
    qc.setQueryData(detail, { id: 'a' });
    qc.setQueryData(changelog, { entries: [] });

    await qc.invalidateQueries({ queryKey: detail });

    // Fuzzy match: refreshing the memory also refreshes its changelog…
    expect(qc.getQueryState(changelog)?.isInvalidated).toBe(true);

    qc.setQueryData(detail, { id: 'a' });
    qc.setQueryData(changelog, { entries: [] });
    await qc.invalidateQueries({ queryKey: changelog, exact: true });

    // …but an exact changelog refetch must not blow away the detail entry.
    expect(qc.getQueryState(detail)?.isInvalidated).toBe(false);
  });

  it('every factory key is a serialisable array led by a stable prefix', () => {
    const keys: Array<readonly unknown[]> = [
      queryKeys.stats,
      queryKeys.health,
      queryKeys.retentionDistribution,
      LIST_KEY,
      queryKeys.memory('a'),
      queryKeys.memoryChangelog('a'),
      queryKeys.graph({ query: 'x' }),
      queryKeys.search('q', 50),
      queryKeys.timeline(7, 200),
      queryKeys.intentions('active'),
      queryKeys.predictions,
      queryKeys.temporal('current', 'topic'),
      queryKeys.decisions(50),
      queryKeys.insights('all', 50),
      queryKeys.hubs(50),
      queryKeys.deepReference('q', 20),
    ];
    for (const key of keys) {
      expect(Array.isArray(key)).toBe(true);
      expect(typeof key[0]).toBe('string');
      expect(JSON.parse(JSON.stringify(key))).toEqual(key);
    }
  });
});

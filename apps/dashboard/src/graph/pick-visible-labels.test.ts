import { type LabelCandidate, type PickVisibleLabelsOptions, pickVisibleLabels } from './pick-visible-labels';

function cand(id: string, retention: number, distSq: number): LabelCandidate {
  return { id, retention, distSq };
}

function opts(partial: Partial<PickVisibleLabelsOptions> = {}): PickVisibleLabelsOptions {
  return {
    hoveredId: null,
    selectedId: null,
    focusedId: null,
    focusConnected: new Set(),
    maxLabels: 30,
    maxDistSq: 80 * 80,
    ...partial,
  };
}

describe('pickVisibleLabels', () => {
  it('returns an empty set when nothing is hovered/selected and no candidates exist', () => {
    expect(pickVisibleLabels([], opts())).toEqual(new Set());
  });

  it('always includes the hovered node even if outside the distance budget', () => {
    const candidates = [cand('hover', 0.1, 10_000_000)]; // way past maxDistSq
    const result = pickVisibleLabels(candidates, opts({ hoveredId: 'hover' }));
    expect(result.has('hover')).toBe(true);
  });

  it('always includes the selected node even if outside the distance budget', () => {
    const candidates = [cand('sel', 0.1, 10_000_000)];
    const result = pickVisibleLabels(candidates, opts({ selectedId: 'sel' }));
    expect(result.has('sel')).toBe(true);
  });

  it('always includes the keyboard-focused node', () => {
    const candidates = [cand('foc', 0.1, 999_999)];
    const result = pickVisibleLabels(candidates, opts({ focusedId: 'foc' }));
    expect(result.has('foc')).toBe(true);
  });

  it('always includes connected nodes (1-hop of focus)', () => {
    const candidates = [
      cand('hover', 0.5, 100),
      cand('connected1', 0.1, 1_000_000),
      cand('connected2', 0.1, 1_000_000),
      cand('elsewhere', 0.9, 100),
    ];
    const result = pickVisibleLabels(
      candidates,
      opts({
        hoveredId: 'hover',
        focusConnected: new Set(['connected1', 'connected2']),
        maxLabels: 4,
      }),
    );
    expect(result.has('connected1')).toBe(true);
    expect(result.has('connected2')).toBe(true);
  });

  it('top-N by retention among in-range candidates fills remaining budget', () => {
    const candidates = [cand('low', 0.1, 100), cand('mid', 0.5, 100), cand('high', 0.9, 100)];
    const result = pickVisibleLabels(candidates, opts({ maxLabels: 1 }));
    expect(result.has('high')).toBe(true);
    expect(result.has('mid')).toBe(false);
    expect(result.has('low')).toBe(false);
  });

  it('filters out candidates beyond maxDistSq even with budget to spare', () => {
    const candidates = [
      cand('near', 0.1, 100),
      cand('far', 0.99, 10_000), // distSq above 80×80=6400
    ];
    const result = pickVisibleLabels(candidates, opts({ maxLabels: 30, maxDistSq: 80 * 80 }));
    expect(result.has('near')).toBe(true);
    expect(result.has('far')).toBe(false);
  });

  it('does not double-count when hovered also has high retention', () => {
    const candidates = [cand('hover', 0.9, 100), cand('other', 0.8, 100), cand('third', 0.7, 100)];
    const result = pickVisibleLabels(candidates, opts({ hoveredId: 'hover', maxLabels: 2 }));
    expect(result.size).toBe(2);
    expect(result.has('hover')).toBe(true);
    expect(result.has('other')).toBe(true);
  });

  it('handles maxLabels=0 by still returning hovered/selected (essential set)', () => {
    const candidates = [cand('hover', 0.9, 100), cand('other', 0.8, 100)];
    const result = pickVisibleLabels(candidates, opts({ hoveredId: 'hover', selectedId: 'other', maxLabels: 0 }));
    expect(result.has('hover')).toBe(true);
    expect(result.has('other')).toBe(true);
  });

  it('breaks ties on retention by id for determinism (no Map-iteration leakage)', () => {
    const candidates = [cand('z', 0.5, 100), cand('a', 0.5, 100), cand('m', 0.5, 100)];
    const r1 = pickVisibleLabels(candidates, opts({ maxLabels: 1 }));
    const r2 = pickVisibleLabels(candidates, opts({ maxLabels: 1 }));
    expect(r1).toEqual(r2);
    expect(r1.has('a')).toBe(true);
  });

  it('returns connected ids even when they would otherwise miss the distance cap', () => {
    const candidates = [cand('hover', 0.9, 100), cand('far-connected', 0.1, 9_999_999)];
    const result = pickVisibleLabels(
      candidates,
      opts({
        hoveredId: 'hover',
        focusConnected: new Set(['far-connected']),
        maxDistSq: 80 * 80,
      }),
    );
    expect(result.has('far-connected')).toBe(true);
  });
});

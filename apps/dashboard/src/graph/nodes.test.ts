import type { GraphNode } from '@/types';
import { getNodeColor } from './nodes';

function makeNode(overrides: Partial<GraphNode> = {}): GraphNode {
  return {
    id: 'aaa',
    label: 'Test',
    type: 'fact',
    retention: 0.5,
    tags: [],
    createdAt: '2026-05-07T00:00:00Z',
    updatedAt: '2026-05-07T00:00:00Z',
    isCenter: false,
    ...overrides,
  };
}

describe('getNodeColor', () => {
  it('returns the type-palette colour in type mode', () => {
    const c = getNodeColor(makeNode({ type: 'fact' }), 'type', true);
    // NODE_TYPE_COLORS exists and has a fact entry; we only assert it's not the
    // generic fallback.
    expect(c).not.toBe('#8B95A5');
  });

  it('falls back to grey when the type is unknown in type mode', () => {
    const c = getNodeColor(makeNode({ type: 'mystery-type' }), 'type', true);
    expect(c).toBe('#8B95A5');
  });

  it('returns a tag-derived HSL colour in tag mode', () => {
    const c = getNodeColor(makeNode({ tags: ['acme'] }), 'tag', true);
    expect(c).toMatch(/^hsl\(\d+, 65%, 60%\)$/);
  });

  it('uses the FIRST tag for tag-mode colour (matches backend cluster rule)', () => {
    const a = getNodeColor(makeNode({ tags: ['acme', 'web-sdk'] }), 'tag', true);
    const b = getNodeColor(makeNode({ tags: ['acme', 'completely', 'different'] }), 'tag', true);
    expect(a).toBe(b);
  });

  it('different first tags produce different hues', () => {
    const acme = getNodeColor(makeNode({ tags: ['acme'] }), 'tag', true);
    const personal = getNodeColor(makeNode({ tags: ['personal'] }), 'tag', true);
    expect(acme).not.toBe(personal);
  });

  it('untagged nodes get a neutral grey in tag mode', () => {
    const c = getNodeColor(makeNode({ tags: [] }), 'tag', true);
    expect(c).toBe('#8B95A5');
  });

  it('lightness adapts to dark vs light theme in tag mode', () => {
    const dark = getNodeColor(makeNode({ tags: ['acme'] }), 'tag', true);
    const light = getNodeColor(makeNode({ tags: ['acme'] }), 'tag', false);
    expect(dark).toContain('60%)');
    expect(light).toContain('50%)');
  });
});

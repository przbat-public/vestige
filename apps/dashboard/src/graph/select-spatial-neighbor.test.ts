import { pickAnchorForDirection, selectSpatialNeighbor } from './select-spatial-neighbor';

type Pos = { x: number; y: number };

function pos(x: number, y: number): Pos {
  return { x, y };
}

describe('selectSpatialNeighbor', () => {
  it('returns null when positions map is empty', () => {
    expect(selectSpatialNeighbor('a', 'right', new Map())).toBeNull();
  });

  it('returns null when current id is unknown', () => {
    const positions = new Map<string, Pos>([['a', pos(0, 0)]]);
    // Hook should fall back to anchor logic if currentId is missing.
    expect(selectSpatialNeighbor('ghost', 'right', positions)).toBeNull();
  });

  it('returns null when there is only the current node', () => {
    const positions = new Map<string, Pos>([['a', pos(0, 0)]]);
    expect(selectSpatialNeighbor('a', 'right', positions)).toBeNull();
  });

  it('picks the node directly to the right (positive x axis)', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      ['b', pos(0.5, 0)], // right of a
      ['c', pos(-0.5, 0)], // left of a
    ]);
    expect(selectSpatialNeighbor('a', 'right', positions)).toBe('b');
  });

  it('picks the node directly to the left (negative x axis)', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      ['b', pos(0.5, 0)],
      ['c', pos(-0.5, 0)],
    ]);
    expect(selectSpatialNeighbor('a', 'left', positions)).toBe('c');
  });

  it('picks the node above (positive y axis = up in NDC)', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      ['b', pos(0, 0.5)], // above
      ['c', pos(0, -0.5)], // below
    ]);
    expect(selectSpatialNeighbor('a', 'up', positions)).toBe('b');
    expect(selectSpatialNeighbor('a', 'down', positions)).toBe('c');
  });

  it('prefers closer neighbours over farther ones in the same direction', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      ['near', pos(0.2, 0)],
      ['far', pos(0.8, 0)],
    ]);
    expect(selectSpatialNeighbor('a', 'right', positions)).toBe('near');
  });

  it('penalises off-axis nodes — slight bias still picks the more aligned one', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      // Both are roughly to the right, but `aligned` is straight right
      // while `tilted` is mostly right but with a vertical drift.
      ['aligned', pos(0.5, 0.02)],
      ['tilted', pos(0.4, 0.3)],
    ]);
    expect(selectSpatialNeighbor('a', 'right', positions)).toBe('aligned');
  });

  it('ignores nodes outside the directional cone', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      // Mostly to the left — should NOT win an "ArrowRight" search.
      ['behind', pos(-0.5, 0)],
      ['ahead', pos(0.5, 0.4)],
    ]);
    expect(selectSpatialNeighbor('a', 'right', positions)).toBe('ahead');
  });

  it('returns null when nothing falls inside the directional cone', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      // All directly behind — every direction except "left" should miss.
      ['b', pos(-0.5, 0)],
      ['c', pos(-0.7, 0)],
    ]);
    expect(selectSpatialNeighbor('a', 'right', positions)).toBeNull();
    expect(selectSpatialNeighbor('a', 'up', positions)).toBeNull();
    expect(selectSpatialNeighbor('a', 'down', positions)).toBeNull();
    expect(selectSpatialNeighbor('a', 'left', positions)).toBe('b'); // closest left
  });

  it('is deterministic when two nodes tie on score', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0, 0)],
      // Two perfect right-side candidates, equidistant. Tie-break MUST be
      // stable across calls (we use sorted id order to avoid Map-iteration
      // surprises that depend on insertion).
      ['z', pos(0.5, 0)],
      ['b', pos(0.5, 0)],
    ]);
    const first = selectSpatialNeighbor('a', 'right', positions);
    const second = selectSpatialNeighbor('a', 'right', positions);
    expect(first).toBe(second);
    // Alphabetic tie-break — 'b' comes before 'z'.
    expect(first).toBe('b');
  });
});

describe('pickAnchorForDirection', () => {
  it('returns null on empty positions', () => {
    expect(pickAnchorForDirection('right', new Map())).toBeNull();
  });

  it('picks the leftmost node when arrow is "right" (good starting point)', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0.5, 0)],
      ['b', pos(-0.5, 0)],
      ['c', pos(0.0, 0)],
    ]);
    expect(pickAnchorForDirection('right', positions)).toBe('b');
  });

  it('picks the rightmost node when arrow is "left"', () => {
    const positions = new Map<string, Pos>([
      ['a', pos(0.5, 0)],
      ['b', pos(-0.5, 0)],
      ['c', pos(0.0, 0)],
    ]);
    expect(pickAnchorForDirection('left', positions)).toBe('a');
  });

  it('picks bottom node for "up", top for "down" (NDC up=+y)', () => {
    const positions = new Map<string, Pos>([
      ['top', pos(0, 0.8)],
      ['bottom', pos(0, -0.8)],
    ]);
    expect(pickAnchorForDirection('up', positions)).toBe('bottom');
    expect(pickAnchorForDirection('down', positions)).toBe('top');
  });
});

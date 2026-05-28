/**
 * Spatial neighbour selection for keyboard navigation in the 3D graph.
 *
 * The previous arrow-key logic walked nodes in array-iteration order —
 * counter-intuitive when the graph is laid out in 3D space (ArrowRight
 * jumping to a node far on the other side of the scene). This module
 * picks the nearest neighbour in the requested screen-space direction,
 * filtered by a 90° cone so off-axis nodes don't win when something more
 * aligned is available.
 *
 * Coordinate system: positions are NDC-like (x ∈ [-1,1] right-positive,
 * y ∈ [-1,1] up-positive). The caller (Graph3D) projects world positions
 * through the camera before invoking, so this module never touches Three.js.
 * That keeps it unit-testable.
 */

export type Direction = 'left' | 'right' | 'up' | 'down';

export interface ScreenPos {
  x: number;
  y: number;
}

interface DirectionVector {
  dx: number;
  dy: number;
}

const DIRECTION_VECTORS: Record<Direction, DirectionVector> = {
  right: { dx: 1, dy: 0 },
  left: { dx: -1, dy: 0 },
  up: { dx: 0, dy: 1 },
  down: { dx: 0, dy: -1 },
};

// Minimum cos(angle) for a node to count as "in the requested direction".
// 0.5 ≈ 60° — generous enough that nodes slightly off-axis still qualify,
// strict enough that ArrowRight never returns something behind you.
const CONE_THRESHOLD = 0.5;

/**
 * Pick the best neighbour of `currentId` in the given screen-space direction.
 * Returns `null` when:
 * - the positions map is empty,
 * - `currentId` is not in the map,
 * - no node falls inside the directional cone (e.g. ArrowRight when every
 *   other node is to the left).
 */
export function selectSpatialNeighbor(
  currentId: string,
  direction: Direction,
  positions: ReadonlyMap<string, ScreenPos>,
): string | null {
  if (positions.size === 0) return null;
  const here = positions.get(currentId);
  if (!here) return null;

  const { dx, dy } = DIRECTION_VECTORS[direction];

  // Sort candidates by id for deterministic tie-breaking. Map iteration
  // order is insertion order in JS, which leaks the caller's data ordering
  // into our results — alphabetic id ordering is at least predictable.
  const candidateIds = Array.from(positions.keys())
    .filter((id) => id !== currentId)
    .sort();

  let bestId: string | null = null;
  let bestScore = Infinity;

  for (const id of candidateIds) {
    const p = positions.get(id);
    if (!p) continue;
    const vx = p.x - here.x;
    const vy = p.y - here.y;
    const len = Math.hypot(vx, vy);
    if (len < 1e-6) continue;
    const cosAngle = (vx * dx + vy * dy) / len;
    if (cosAngle < CONE_THRESHOLD) continue;
    // Score = euclidean distance, scaled inversely by alignment.
    // A perfectly aligned (cos=1) node scores its raw distance.
    // A 60°-off node (cos=0.5) scores 2× its distance.
    const score = len / Math.max(cosAngle, 0.001);
    if (score < bestScore) {
      bestScore = score;
      bestId = id;
    }
  }

  return bestId;
}

/**
 * When the user presses an arrow key with no current focus, pick the most
 * sensible "entry point" so they're not always thrown to the same node:
 * - ArrowRight → leftmost node (so the next press feels like progress)
 * - ArrowLeft  → rightmost node
 * - ArrowUp    → bottommost node (NDC up = +y)
 * - ArrowDown  → topmost node
 *
 * Returns `null` for an empty map. Ties are broken by id ordering for
 * stability across renders.
 */
export function pickAnchorForDirection(direction: Direction, positions: ReadonlyMap<string, ScreenPos>): string | null {
  if (positions.size === 0) return null;

  const { dx, dy } = DIRECTION_VECTORS[direction];
  const ids = Array.from(positions.keys()).sort();

  let bestId: string | null = null;
  // We want the node that is FURTHEST in the OPPOSITE direction, so a
  // subsequent press of the same arrow walks the user across the scene.
  let bestProjection = Infinity;

  for (const id of ids) {
    const p = positions.get(id);
    if (!p) continue;
    const projection = p.x * dx + p.y * dy;
    if (projection < bestProjection) {
      bestProjection = projection;
      bestId = id;
    }
  }

  return bestId;
}

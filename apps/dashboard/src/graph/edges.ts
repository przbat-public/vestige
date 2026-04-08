import * as THREE from 'three';
import type { GraphEdge } from '@/types';
import { getGraphTheme, type GraphThemeConfig } from '@/graph/theme';
import type { EdgeParticleSystem } from '@/graph/edge-particles';

const SEGMENTS = 16;

function easeOutCubic(t: number): number {
  return 1 - (1 - t) ** 3;
}

export interface EdgeEntry {
  line: THREE.Line;
  source: string;
  target: string;
  weight: number;
  curveSign: number;
  baseOpacity: number;
  currentOpacity: number;
  targetOpacity: number;
  index: number;
  dissolving: boolean;
}

interface GrowingEdge {
  entry: EdgeEntry;
  frame: number;
  totalFrames: number;
}

interface DissolvingEdge {
  entry: EdgeEntry;
  frame: number;
  totalFrames: number;
}

export class EdgeManager {
  group: THREE.Group;
  entries: EdgeEntry[] = [];
  focusNode: string | null = null;
  private growingEdges: GrowingEdge[] = [];
  private dissolvingEdges: DissolvingEdge[] = [];
  private theme: Readonly<GraphThemeConfig>;

  private readonly _mid = new THREE.Vector3();
  private readonly _dir = new THREE.Vector3();
  private readonly _perp = new THREE.Vector3();

  constructor() {
    this.group = new THREE.Group();
    this.theme = getGraphTheme();
  }

  createEdges(edges: GraphEdge[], positions: Map<string, THREE.Vector3>) {
    this.theme = getGraphTheme();
    this.entries = [];

    for (let i = 0; i < edges.length; i++) {
      const edge = edges[i];
      const sp = positions.get(edge.source);
      const tp = positions.get(edge.target);
      if (!sp || !tp) continue;

      const entry = this.makeEdgeEntry(edge, i);
      this.entries.push(entry);
      this.group.add(entry.line);
      this.writeCurve(entry, sp, tp, 1);
    }
  }

  private makeEdgeEntry(edge: GraphEdge, index: number): EdgeEntry {
    const verts = new Float32Array((SEGMENTS + 1) * 3);
    const geo = new THREE.BufferGeometry();
    const attr = new THREE.BufferAttribute(verts, 3);
    attr.setUsage(THREE.DynamicDrawUsage);
    geo.setAttribute('position', attr);

    const t = this.theme;
    const baseOp = Math.min(t.edgeBaseOpacity + edge.weight * 0.4, 0.6);
    const mat = new THREE.LineBasicMaterial({
      color: t.edgeColor,
      transparent: true,
      opacity: baseOp,
      blending: t.edgeAdditiveBlending ? THREE.AdditiveBlending : THREE.NormalBlending,
    });

    const line = new THREE.Line(geo, mat);
    line.userData = { source: edge.source, target: edge.target };

    return {
      line,
      source: edge.source,
      target: edge.target,
      weight: edge.weight,
      curveSign: index % 2 === 0 ? 1 : -1,
      baseOpacity: baseOp,
      currentOpacity: baseOp,
      targetOpacity: baseOp,
      index,
      dissolving: false,
    };
  }

  private writeCurve(
    e: EdgeEntry,
    src: THREE.Vector3,
    tgt: THREE.Vector3,
    progress: number,
  ): { cx: number; cy: number; cz: number } {
    const attr = e.line.geometry.attributes.position as THREE.BufferAttribute;

    this._mid.addVectors(src, tgt).multiplyScalar(0.5);
    this._dir.subVectors(tgt, src);
    const len = this._dir.length();
    this._dir.normalize();

    this._perp.set(0, 1, 0);
    this._perp.cross(this._dir);
    if (this._perp.lengthSq() < 0.001) {
      this._perp.set(1, 0, 0);
      this._perp.cross(this._dir);
    }
    this._perp.normalize();

    const curve = Math.min(len * 0.12, 6) * e.curveSign * this.theme.curveAmount;
    const cx = this._mid.x + this._perp.x * curve;
    const cy = this._mid.y + this._perp.y * curve;
    const cz = this._mid.z + this._perp.z * curve;

    for (let i = 0; i <= SEGMENTS; i++) {
      const t = Math.min(i / SEGMENTS, progress);
      const omt = 1 - t;
      attr.setXYZ(
        i,
        omt * omt * src.x + 2 * omt * t * cx + t * t * tgt.x,
        omt * omt * src.y + 2 * omt * t * cy + t * t * tgt.y,
        omt * omt * src.z + 2 * omt * t * cz + t * t * tgt.z,
      );
    }
    attr.needsUpdate = true;

    return { cx, cy, cz };
  }

  addEdge(edge: GraphEdge, positions: Map<string, THREE.Vector3>) {
    const sp = positions.get(edge.source);
    const tp = positions.get(edge.target);
    if (!sp || !tp) return;

    const entry = this.makeEdgeEntry(edge, this.entries.length);
    (entry.line.material as THREE.LineBasicMaterial).opacity = 0;
    entry.currentOpacity = 0;

    this.entries.push(entry);
    this.group.add(entry.line);
    this.growingEdges.push({ entry, frame: 0, totalFrames: 45 });
  }

  removeEdgesForNode(nodeId: string) {
    for (const entry of this.entries) {
      if (entry.dissolving) continue;
      if (entry.source === nodeId || entry.target === nodeId) {
        entry.dissolving = true;
        this.growingEdges = this.growingEdges.filter((g) => g.entry !== entry);
        this.dissolvingEdges.push({ entry, frame: 0, totalFrames: 40 });
      }
    }
  }

  setFocus(nodeId: string | null) {
    this.focusNode = nodeId;
    for (const e of this.entries) {
      if (e.dissolving) continue;
      if (!nodeId) {
        e.targetOpacity = e.baseOpacity;
      } else if (e.source === nodeId || e.target === nodeId) {
        e.targetOpacity = Math.min(e.baseOpacity * 2.5, 0.9);
      } else {
        e.targetOpacity = 0.03;
      }
    }
  }

  updatePositions(positions: Map<string, THREE.Vector3>, particles?: EdgeParticleSystem) {
    for (const entry of this.entries) {
      if (entry.dissolving) continue;

      let isGrowing = false;
      for (const g of this.growingEdges) {
        if (g.entry === entry) { isGrowing = true; break; }
      }
      if (isGrowing) continue;

      const sp = positions.get(entry.source);
      const tp = positions.get(entry.target);
      if (!sp || !tp) continue;

      const c = this.writeCurve(entry, sp, tp, 1);
      particles?.setCurve(entry.index, sp.x, sp.y, sp.z, c.cx, c.cy, c.cz, tp.x, tp.y, tp.z);
    }
  }

  animateEdges(positions: Map<string, THREE.Vector3>) {
    for (let i = this.growingEdges.length - 1; i >= 0; i--) {
      const g = this.growingEdges[i];
      g.frame++;
      const progress = easeOutCubic(Math.min(g.frame / g.totalFrames, 1));

      const sp = positions.get(g.entry.source);
      const tp = positions.get(g.entry.target);
      if (sp && tp) this.writeCurve(g.entry, sp, tp, progress);

      const mat = g.entry.line.material as THREE.LineBasicMaterial;
      mat.opacity = progress * g.entry.baseOpacity;

      if (g.frame >= g.totalFrames) {
        mat.opacity = g.entry.baseOpacity;
        g.entry.currentOpacity = g.entry.baseOpacity;
        this.growingEdges.splice(i, 1);
      }
    }

    for (let i = this.dissolvingEdges.length - 1; i >= 0; i--) {
      const d = this.dissolvingEdges[i];
      d.frame++;
      const mat = d.entry.line.material as THREE.LineBasicMaterial;
      mat.opacity = Math.max(0, 0.5 * (1 - d.frame / d.totalFrames));

      if (d.frame >= d.totalFrames) {
        this.group.remove(d.entry.line);
        d.entry.line.geometry.dispose();
        (d.entry.line.material as THREE.Material).dispose();
        this.entries = this.entries.filter((e) => e !== d.entry);
        this.dissolvingEdges.splice(i, 1);
      }
    }

    for (const e of this.entries) {
      if (e.dissolving) continue;
      if (Math.abs(e.currentOpacity - e.targetOpacity) < 0.003) continue;
      e.currentOpacity += (e.targetOpacity - e.currentOpacity) * 0.12;
      (e.line.material as THREE.LineBasicMaterial).opacity = e.currentOpacity;
    }
  }

  applyTheme() {
    this.theme = getGraphTheme();
  }

  dispose() {
    for (const child of this.group.children) {
      const line = child as THREE.Line;
      line.geometry?.dispose();
      (line.material as THREE.Material)?.dispose();
    }
    this.entries = [];
    this.growingEdges = [];
    this.dissolvingEdges = [];
  }
}

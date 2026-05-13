import * as THREE from 'three';
import { type GraphThemeConfig, getGraphTheme, isDarkMode } from '@/graph/theme';
import type { GraphNode } from '@/types';
import { NODE_TYPE_COLORS } from '@/types';

/** How to colour graph nodes: by node-type palette, or by primary tag (for cluster discovery). */
export type NodeColorMode = 'type' | 'tag';

/**
 * Deterministic HSL colour from an arbitrary string (tag, etc.).
 *
 * djb2 hash → hue [0..360], fixed saturation/lightness so all tag colours sit
 * in the same visual range and do not clash with node-type colours when the
 * user toggles modes. Lightness tracks dark/light theme so the dots do not
 * disappear on bright backgrounds.
 */
function hashString(s: string): number {
  let hash = 5381;
  for (let i = 0; i < s.length; i++) hash = ((hash << 5) + hash) ^ s.charCodeAt(i);
  return hash >>> 0;
}

function colorForTag(tag: string, isDark: boolean): string {
  const hue = hashString(tag) % 360;
  const sat = 65;
  const light = isDark ? 60 : 50;
  return `hsl(${hue}, ${sat}%, ${light}%)`;
}

/** Untagged memories get a neutral grey so they stay legible without forming a fake cluster. */
const UNTAGGED_COLOR = '#8B95A5';

/**
 * Resolve the colour for a node given the active mode. In `tag` mode we use
 * the FIRST tag (matching the dream/consolidation engine's "primary tag" rule
 * in `consolidation/phases.rs:588`), so visual clusters line up with how the
 * backend already groups memories.
 */
export function getNodeColor(node: GraphNode, mode: NodeColorMode, isDark: boolean): string {
  if (mode === 'tag') {
    const primary = node.tags?.[0];
    return primary ? colorForTag(primary, isDark) : UNTAGGED_COLOR;
  }
  return NODE_TYPE_COLORS[node.type] || UNTAGGED_COLOR;
}

function easeOutElastic(t: number): number {
  if (t === 0 || t === 1) return t;
  const p = 0.3;
  return 2 ** (-10 * t) * Math.sin(((t - p / 4) * (2 * Math.PI)) / p) + 1;
}

function easeInBack(t: number): number {
  const s = 1.70158;
  return t * t * ((s + 1) * t - s);
}

interface MaterializingNode {
  id: string;
  frame: number;
  totalFrames: number;
  mesh: THREE.Mesh;
  glow: THREE.Sprite;
  label: THREE.Sprite;
  targetScale: number;
}

interface DissolvingNode {
  id: string;
  frame: number;
  totalFrames: number;
  mesh: THREE.Mesh;
  glow: THREE.Sprite;
  label: THREE.Sprite;
  originalScale: number;
}

interface GrowingNode {
  id: string;
  frame: number;
  totalFrames: number;
  startScale: number;
  targetScale: number;
}

export class NodeManager {
  group: THREE.Group;
  meshMap = new Map<string, THREE.Mesh>();
  glowMap = new Map<string, THREE.Sprite>();
  positions = new Map<string, THREE.Vector3>();
  labelSprites = new Map<string, THREE.Sprite>();
  hoveredNode: string | null = null;
  selectedNode: string | null = null;
  focusNode: string | null = null;
  focusConnected = new Set<string>();
  private focusAlpha = new Map<string, number>();
  private theme: Readonly<GraphThemeConfig>;
  private meshesCache: THREE.Mesh[] | null = null;

  private materializingNodes: MaterializingNode[] = [];
  private dissolvingNodes: DissolvingNode[] = [];
  private growingNodes: GrowingNode[] = [];

  private animatingIdSet = new Set<string>();
  private colorMode: NodeColorMode = 'type';

  constructor() {
    this.group = new THREE.Group();
    this.theme = getGraphTheme();
  }

  /**
   * Switch the colour palette of all existing nodes.
   *
   * Updates `material.color`, `material.emissive`, glow sprite tint, and
   * mesh userData so subsequent focus/dimming logic still has correct
   * baseline colours to interpolate from. Does not rebuild geometry.
   */
  setColorMode(mode: NodeColorMode, nodeById: Map<string, GraphNode>, isDark: boolean): void {
    if (this.colorMode === mode) return;
    this.colorMode = mode;
    for (const [id, mesh] of this.meshMap) {
      const node = nodeById.get(id);
      if (!node) continue;
      const colorHex = getNodeColor(node, mode, isDark);
      const color = new THREE.Color(colorHex);
      const material = mesh.material as THREE.MeshStandardMaterial;
      material.color.copy(color);
      material.emissive.copy(color);
      const glow = this.glowMap.get(id);
      if (glow) (glow.material as THREE.SpriteMaterial).color.copy(color);
    }
  }

  /** Read-only accessor used by tests and dev tooling. */
  getColorMode(): NodeColorMode {
    return this.colorMode;
  }

  createNodes(nodes: GraphNode[]): Map<string, THREE.Vector3> {
    const phi = (1 + Math.sqrt(5)) / 2;
    const count = nodes.length;

    for (let i = 0; i < count; i++) {
      const node = nodes[i];

      const y = 1 - (2 * i) / (count - 1 || 1);
      const radius = Math.sqrt(1 - y * y);
      const theta = (2 * Math.PI * i) / phi;
      const spread = 30 + count * 0.5;

      const pos = new THREE.Vector3(radius * Math.cos(theta) * spread, y * spread, radius * Math.sin(theta) * spread);

      if (node.isCenter) pos.set(0, 0, 0);

      this.positions.set(node.id, pos);
      this.createNodeMeshes(node, pos, 1.0);
    }

    return this.positions;
  }

  private createNodeMeshes(node: GraphNode, pos: THREE.Vector3, initialScale: number) {
    const size = 0.5 + node.retention * 2;
    // Pull from getNodeColor (handles both modes) so scene rebuilds and
    // newly-added nodes pick up the active palette without a separate code path.
    const color = getNodeColor(node, this.colorMode, isDarkMode());
    const t = this.theme;

    const geometry = new THREE.SphereGeometry(size, 24, 24);
    const material = new THREE.MeshStandardMaterial({
      color: new THREE.Color(color),
      emissive: new THREE.Color(color),
      emissiveIntensity: (0.3 + node.retention * 0.5) * t.nodeEmissiveScale,
      roughness: t.nodeRoughness,
      metalness: t.nodeMetalness,
      transparent: true,
      opacity: 0.5 + node.retention * 0.5,
    });

    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.copy(pos);
    mesh.scale.setScalar(initialScale);
    mesh.userData = { nodeId: node.id, type: node.type, retention: node.retention, label: node.label };
    this.meshMap.set(node.id, mesh);
    this.meshesCache = null;
    this.group.add(mesh);

    const spriteMat = new THREE.SpriteMaterial({
      color: new THREE.Color(color),
      transparent: true,
      opacity: t.glowVisible && initialScale > 0 ? 0.12 + node.retention * 0.15 : 0,
      blending: t.glowVisible ? THREE.AdditiveBlending : THREE.NormalBlending,
    });
    const sprite = new THREE.Sprite(spriteMat);
    sprite.scale.set(size * 3.5 * initialScale, size * 3.5 * initialScale, 1);
    sprite.position.copy(pos);
    sprite.userData = { isGlow: true, nodeId: node.id };
    sprite.visible = t.glowVisible;
    this.glowMap.set(node.id, sprite);
    this.group.add(sprite);

    const labelText = node.label || node.type;
    const labelSprite = this.createTextSprite(labelText, t.labelColor);
    labelSprite.position.copy(pos);
    labelSprite.position.y += size * 2 + 1.5;
    labelSprite.userData = { isLabel: true, nodeId: node.id, offset: size * 2 + 1.5 };
    this.group.add(labelSprite);
    this.labelSprites.set(node.id, labelSprite);

    return { mesh, glow: sprite, label: labelSprite, size };
  }

  addNode(node: GraphNode, initialPosition?: THREE.Vector3): THREE.Vector3 {
    const pos =
      initialPosition?.clone() ??
      new THREE.Vector3((Math.random() - 0.5) * 40, (Math.random() - 0.5) * 40, (Math.random() - 0.5) * 40);

    this.positions.set(node.id, pos);

    const { mesh, glow, label } = this.createNodeMeshes(node, pos, 0);
    mesh.scale.setScalar(0.001);
    glow.scale.set(0.001, 0.001, 1);
    (glow.material as THREE.SpriteMaterial).opacity = 0;
    (label.material as THREE.SpriteMaterial).opacity = 0;

    this.materializingNodes.push({
      id: node.id,
      frame: 0,
      totalFrames: 30,
      mesh,
      glow,
      label,
      targetScale: 0.5 + node.retention * 2,
    });

    return pos;
  }

  removeNode(id: string) {
    const mesh = this.meshMap.get(id);
    const glow = this.glowMap.get(id);
    const label = this.labelSprites.get(id);
    if (!mesh || !glow || !label) return;

    this.materializingNodes = this.materializingNodes.filter((m) => m.id !== id);

    this.dissolvingNodes.push({
      id,
      frame: 0,
      totalFrames: 60,
      mesh,
      glow,
      label,
      originalScale: mesh.scale.x,
    });
  }

  setFocus(nodeId: string | null, connected: Set<string>) {
    this.focusNode = nodeId;
    this.focusConnected = connected;
  }

  clearFocus() {
    this.focusNode = null;
    this.focusConnected.clear();
  }

  growNode(id: string, newRetention: number) {
    const mesh = this.meshMap.get(id);
    if (!mesh) return;

    const currentScale = mesh.scale.x;
    const targetScale = 0.5 + newRetention * 2;
    mesh.userData.retention = newRetention;

    this.growingNodes.push({
      id,
      frame: 0,
      totalFrames: 30,
      startScale: currentScale,
      targetScale,
    });
  }

  private createTextSprite(text: string, color: string): THREE.Sprite {
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('Canvas 2D context unavailable');
    canvas.width = 512;
    canvas.height = 64;

    const label = text.length > 40 ? `${text.slice(0, 37)}...` : text;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.font = 'bold 28px -apple-system, BlinkMacSystemFont, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.shadowColor = 'rgba(0, 0, 0, 0.8)';
    ctx.shadowBlur = 6;
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 2;
    ctx.fillStyle = color;
    ctx.fillText(label, canvas.width / 2, canvas.height / 2);

    const texture = new THREE.CanvasTexture(canvas);
    texture.needsUpdate = true;

    const mat = new THREE.SpriteMaterial({
      map: texture,
      transparent: true,
      opacity: 0,
      depthTest: false,
      sizeAttenuation: true,
    });

    const sprite = new THREE.Sprite(mat);
    sprite.scale.set(12, 1.5, 1);
    return sprite;
  }

  updatePositions() {
    for (const child of this.group.children) {
      if (!child.userData.nodeId) continue;
      const pos = this.positions.get(child.userData.nodeId);
      if (!pos) continue;

      if (child.userData.isGlow) {
        child.position.copy(pos);
      } else if (child.userData.isLabel) {
        child.position.copy(pos);
        child.position.y += child.userData.offset;
      } else if (child instanceof THREE.Mesh) {
        child.position.copy(pos);
      }
    }
  }

  private rebuildAnimatingSet() {
    this.animatingIdSet.clear();
    for (const m of this.materializingNodes) this.animatingIdSet.add(m.id);
    for (const d of this.dissolvingNodes) this.animatingIdSet.add(d.id);
    for (const g of this.growingNodes) this.animatingIdSet.add(g.id);
  }

  // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: orchestrates materialization, dissolution, growth, breathing, and label visibility per frame
  animate(
    time: number,
    nodeById: Map<string, GraphNode>,
    camera: THREE.PerspectiveCamera,
    nodeOpacities?: Map<string, number>,
  ) {
    this.animateMaterializing();
    this.animateDissolving();
    this.animateGrowing();
    this.rebuildAnimatingSet();

    const emissiveScale = this.theme.nodeEmissiveScale;
    const breatheAmp = this.theme.glowVisible ? 0.06 : 0.03;

    let nodeIndex = 0;
    for (const [id, mesh] of this.meshMap) {
      if (this.animatingIdSet.has(id)) continue;
      const node = nodeById.get(id);
      if (!node) continue;

      let focusTarget = 1.0;
      if (this.focusNode) {
        focusTarget = this.focusConnected.has(id) ? 1.0 : 0.06;
      }
      let focusA = this.focusAlpha.get(id) ?? 1.0;
      focusA += (focusTarget - focusA) * 0.12;
      this.focusAlpha.set(id, focusA);

      const breathe = 1 + Math.sin(time * 1.2 + nodeIndex * 0.5) * breatheAmp * node.retention;
      nodeIndex++;
      mesh.scale.setScalar(breathe);

      const temporalOpacity = nodeOpacities?.get(id) ?? 1.0;
      const alpha = temporalOpacity * focusA;
      const mat = mesh.material as THREE.MeshStandardMaterial;
      mat.opacity = (0.5 + node.retention * 0.5) * alpha;

      if (id === this.hoveredNode) {
        mat.emissiveIntensity = 1.0 * emissiveScale;
      } else if (id === this.selectedNode) {
        mat.emissiveIntensity = 0.8 * emissiveScale;
      } else {
        const baseIntensity = (0.3 + node.retention * 0.5) * emissiveScale;
        const breatheIntensity = baseIntensity + Math.sin(time * (0.8 + node.retention * 0.7)) * 0.05 * node.retention;
        mat.emissiveIntensity = breatheIntensity * alpha;
      }

      const glow = this.glowMap.get(id);
      if (glow?.visible) {
        const glowMat = glow.material as THREE.SpriteMaterial;
        glowMat.opacity = (0.12 + node.retention * 0.15) * alpha;
      }
    }

    for (const [id, sprite] of this.labelSprites) {
      if (this.animatingIdSet.has(id)) continue;
      const pos = this.positions.get(id);
      if (!pos) continue;
      const dist = camera.position.distanceTo(pos);
      const focusA = this.focusAlpha.get(id) ?? 1.0;
      const mat = sprite.material as THREE.SpriteMaterial;
      const rawTarget =
        id === this.hoveredNode || id === this.selectedNode
          ? 1.0
          : dist < 40
            ? 0.9
            : dist < 80
              ? 0.9 * (1 - (dist - 40) / 40)
              : 0;
      mat.opacity += (rawTarget * focusA - mat.opacity) * 0.1;
    }
  }

  private animateMaterializing() {
    for (let i = this.materializingNodes.length - 1; i >= 0; i--) {
      const mn = this.materializingNodes[i];
      mn.frame++;
      const t = Math.min(mn.frame / mn.totalFrames, 1);
      const scale = easeOutElastic(t);

      mn.mesh.scale.setScalar(Math.max(0.001, scale));

      if (mn.frame >= 5) {
        const glowT = Math.min((mn.frame - 5) / 5, 1);
        const glowMat = mn.glow.material as THREE.SpriteMaterial;
        glowMat.opacity = glowT * 0.25;
        const glowSize = mn.targetScale * 4 * scale;
        mn.glow.scale.set(glowSize, glowSize, 1);
      }

      if (mn.frame >= 40) {
        const labelT = Math.min((mn.frame - 40) / 20, 1);
        (mn.label.material as THREE.SpriteMaterial).opacity = labelT * 0.9;
      }

      if (mn.frame >= 60) {
        this.materializingNodes.splice(i, 1);
      }
    }
  }

  private animateDissolving() {
    for (let i = this.dissolvingNodes.length - 1; i >= 0; i--) {
      const dn = this.dissolvingNodes[i];
      dn.frame++;
      const t = Math.min(dn.frame / dn.totalFrames, 1);
      const shrink = 1 - easeInBack(t);
      const scale = Math.max(0.001, dn.originalScale * shrink);

      dn.mesh.scale.setScalar(scale);
      const glowScale = scale * 4;
      dn.glow.scale.set(glowScale, glowScale, 1);

      const mat = dn.mesh.material as THREE.MeshStandardMaterial;
      mat.opacity *= 0.97;
      (dn.glow.material as THREE.SpriteMaterial).opacity *= 0.95;
      (dn.label.material as THREE.SpriteMaterial).opacity *= 0.93;

      if (dn.frame >= dn.totalFrames) {
        this.group.remove(dn.mesh);
        this.group.remove(dn.glow);
        this.group.remove(dn.label);
        dn.mesh.geometry.dispose();
        (dn.mesh.material as THREE.Material).dispose();
        (dn.glow.material as THREE.SpriteMaterial).map?.dispose();
        (dn.glow.material as THREE.Material).dispose();
        (dn.label.material as THREE.SpriteMaterial).map?.dispose();
        (dn.label.material as THREE.Material).dispose();

        this.meshMap.delete(dn.id);
        this.glowMap.delete(dn.id);
        this.labelSprites.delete(dn.id);
        this.positions.delete(dn.id);
        this.focusAlpha.delete(dn.id);
        this.meshesCache = null;

        this.dissolvingNodes.splice(i, 1);
      }
    }
  }

  private animateGrowing() {
    for (let i = this.growingNodes.length - 1; i >= 0; i--) {
      const gn = this.growingNodes[i];
      gn.frame++;
      const t = Math.min(gn.frame / gn.totalFrames, 1);
      const scale = gn.startScale + (gn.targetScale - gn.startScale) * easeOutElastic(t);

      const mesh = this.meshMap.get(gn.id);
      if (mesh) mesh.scale.setScalar(scale);

      const glow = this.glowMap.get(gn.id);
      if (glow) {
        const glowSize = scale * 4;
        glow.scale.set(glowSize, glowSize, 1);
      }

      if (gn.frame >= gn.totalFrames) {
        this.growingNodes.splice(i, 1);
      }
    }
  }

  applyTheme() {
    this.theme = getGraphTheme();
    for (const [id, mesh] of this.meshMap) {
      const mat = mesh.material as THREE.MeshStandardMaterial;
      mat.roughness = this.theme.nodeRoughness;
      mat.metalness = this.theme.nodeMetalness;

      const glow = this.glowMap.get(id);
      if (glow) {
        glow.visible = this.theme.glowVisible;
        if (this.theme.glowVisible) {
          (glow.material as THREE.SpriteMaterial).blending = THREE.AdditiveBlending;
        }
      }
    }
    for (const [id, sprite] of this.labelSprites) {
      const oldMat = sprite.material as THREE.SpriteMaterial;
      oldMat.map?.dispose();
      const nodeData = this.meshMap.get(id)?.userData;
      const text = nodeData?.label || nodeData?.type || id;
      const fresh = this.createTextSprite(text, this.theme.labelColor);
      oldMat.map = (fresh.material as THREE.SpriteMaterial).map;
      oldMat.needsUpdate = true;
    }
  }

  getMeshes(): THREE.Mesh[] {
    if (!this.meshesCache) {
      this.meshesCache = Array.from(this.meshMap.values());
    }
    return this.meshesCache;
  }

  dispose() {
    this.group.traverse((obj) => {
      if (obj instanceof THREE.Mesh) {
        obj.geometry?.dispose();
        (obj.material as THREE.Material)?.dispose();
      } else if (obj instanceof THREE.Sprite) {
        (obj.material as THREE.SpriteMaterial)?.map?.dispose();
        (obj.material as THREE.Material)?.dispose();
      }
    });
    this.materializingNodes = [];
    this.dissolvingNodes = [];
    this.growingNodes = [];
    this.meshesCache = null;
  }
}

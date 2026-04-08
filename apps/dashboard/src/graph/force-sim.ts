import * as THREE from 'three';
import type { GraphEdge } from '@/types';

export class ForceSimulation {
  positions: Map<string, THREE.Vector3>;
  velocities: Map<string, THREE.Vector3>;
  running = true;
  step = 0;

  private readonly repulsionStrength = 500;
  private readonly attractionStrength = 0.01;
  private readonly dampening = 0.9;
  private readonly layoutPhaseSteps = 300;
  private readonly maintenanceAlpha = 0.002;
  private readonly sameTypeRepulsionMod = 0.35;
  private cooldownExtension = 0;
  private maxActiveSteps = 300;
  private nodeTypes: Map<string, string>;
  private nodeIds: string[] = [];
  private nodeIdsDirty = true;

  private readonly _diff = new THREE.Vector3();
  private readonly _dir = new THREE.Vector3();
  private readonly _center = new THREE.Vector3();

  constructor(positions: Map<string, THREE.Vector3>, nodeTypes?: Map<string, string>) {
    this.positions = positions;
    this.nodeTypes = nodeTypes ?? new Map();
    this.velocities = new Map();
    for (const id of positions.keys()) {
      this.velocities.set(id, new THREE.Vector3());
    }
    this.nodeIdsDirty = true;
  }

  addNode(id: string, position: THREE.Vector3) {
    this.positions.set(id, position.clone());
    this.velocities.set(id, new THREE.Vector3());
    this.cooldownExtension = 100;
    this.maxActiveSteps = Math.max(this.maxActiveSteps, this.step + this.cooldownExtension);
    this.running = true;
    this.nodeIdsDirty = true;
  }

  removeNode(id: string) {
    this.positions.delete(id);
    this.velocities.delete(id);
    this.nodeIdsDirty = true;
  }

  private getNodeIds(): string[] {
    if (this.nodeIdsDirty) {
      this.nodeIds = Array.from(this.positions.keys());
      this.nodeIdsDirty = false;
    }
    return this.nodeIds;
  }

  // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: physics simulation with repulsion, attraction, and centering
  tick(edges: GraphEdge[]) {
    if (!this.running) return;
    this.step++;

    const inLayoutPhase = this.step <= this.maxActiveSteps;
    const alpha = inLayoutPhase
      ? Math.max(this.maintenanceAlpha, 1 - this.step / this.layoutPhaseSteps)
      : this.maintenanceAlpha;

    if (!inLayoutPhase && this.cooldownExtension > 0) {
      this.cooldownExtension = 0;
      this.maxActiveSteps = this.layoutPhaseSteps;
    }
    const ids = this.getNodeIds();

    for (let i = 0; i < ids.length; i++) {
      for (let j = i + 1; j < ids.length; j++) {
        const posA = this.positions.get(ids[i]);
        const posB = this.positions.get(ids[j]);
        if (!posA || !posB) continue;
        this._diff.subVectors(posA, posB);
        const dist = this._diff.length() || 1;
        const typeA = this.nodeTypes.get(ids[i]);
        const typeB = this.nodeTypes.get(ids[j]);
        const sameType = typeA && typeB && typeA === typeB;
        const repMod = sameType ? this.sameTypeRepulsionMod : 1.0;
        const force = (this.repulsionStrength / (dist * dist)) * alpha * repMod;
        this._dir.copy(this._diff).normalize().multiplyScalar(force);

        this.velocities.get(ids[i])?.add(this._dir);
        this.velocities.get(ids[j])?.sub(this._dir);
      }
    }

    for (const edge of edges) {
      const posA = this.positions.get(edge.source);
      const posB = this.positions.get(edge.target);
      if (!posA || !posB) continue;

      this._diff.subVectors(posB, posA);
      const dist = this._diff.length();
      const force = dist * this.attractionStrength * edge.weight * alpha;
      this._dir.copy(this._diff).normalize().multiplyScalar(force);

      this.velocities.get(edge.source)?.add(this._dir);
      this.velocities.get(edge.target)?.sub(this._dir);
    }

    for (const id of ids) {
      const pos = this.positions.get(id);
      const vel = this.velocities.get(id);
      if (!pos || !vel) continue;
      this._center.copy(pos).multiplyScalar(0.001 * alpha);
      vel.sub(this._center);
      vel.multiplyScalar(this.dampening);
      pos.add(vel);
    }
  }

  reset() {
    this.step = 0;
    this.running = true;
    this.nodeIdsDirty = true;
    for (const vel of this.velocities.values()) {
      vel.set(0, 0, 0);
    }
  }
}

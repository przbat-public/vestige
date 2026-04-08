import * as THREE from 'three';
import type { GraphEdge, GraphNode, VestigeEvent } from '@/types';
import { NODE_TYPE_COLORS } from '@/types';
import type { EdgeManager } from './edges';
import type { EffectManager } from './effects';
import type { ForceSimulation } from './force-sim';
import type { NodeManager } from './nodes';

/** Maximum number of live-spawned nodes before FIFO eviction */
const MAX_LIVE_NODES = 50;

export interface GraphMutationContext {
  effects: EffectManager;
  nodeManager: NodeManager;
  edgeManager: EdgeManager;
  forceSim: ForceSimulation;
  camera: THREE.Camera;
  onMutation: (mutation: GraphMutation) => void;
}

export type GraphMutation =
  | { type: 'nodeAdded'; node: GraphNode }
  | { type: 'nodeRemoved'; nodeId: string }
  | { type: 'edgeAdded'; edge: GraphEdge }
  | { type: 'edgesRemoved'; nodeId: string }
  | { type: 'nodeUpdated'; nodeId: string; retention: number };

/** Track live-spawned node IDs in insertion order for FIFO eviction */
const liveSpawnedNodes: string[] = [];

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: tag overlap scoring with fallback
function findSpawnPosition(
  newNode: { tags?: string[]; type?: string },
  existingNodes: GraphNode[],
  positions: Map<string, THREE.Vector3>,
): THREE.Vector3 {
  const tags = newNode.tags ?? [];
  const type = newNode.type ?? '';

  // Score existing nodes by tag overlap + type match
  let bestId: string | null = null;
  let bestScore = 0;

  for (const existing of existingNodes) {
    let score = 0;
    if (existing.type === type) score += 2;
    for (const tag of existing.tags) {
      if (tags.includes(tag)) score += 1;
    }
    if (score > bestScore) {
      bestScore = score;
      bestId = existing.id;
    }
  }

  if (bestId && bestScore > 0) {
    const nearPos = positions.get(bestId);
    if (nearPos) {
      // Spawn nearby with some jitter
      return new THREE.Vector3(
        nearPos.x + (Math.random() - 0.5) * 10,
        nearPos.y + (Math.random() - 0.5) * 10,
        nearPos.z + (Math.random() - 0.5) * 10,
      );
    }
  }

  // Fallback: random position in graph space
  return new THREE.Vector3((Math.random() - 0.5) * 40, (Math.random() - 0.5) * 40, (Math.random() - 0.5) * 40);
}

function evictOldestLiveNode(ctx: GraphMutationContext, allNodes: GraphNode[]) {
  if (liveSpawnedNodes.length <= MAX_LIVE_NODES) return;
  const evictId = liveSpawnedNodes.shift();
  if (!evictId) return;
  ctx.edgeManager.removeEdgesForNode(evictId);
  ctx.nodeManager.removeNode(evictId);
  ctx.forceSim.removeNode(evictId);
  ctx.onMutation({ type: 'edgesRemoved', nodeId: evictId });
  ctx.onMutation({ type: 'nodeRemoved', nodeId: evictId });
  // Remove from allNodes tracking
  const idx = allNodes.findIndex((n) => n.id === evictId);
  if (idx !== -1) allNodes.splice(idx, 1);
}

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: event dispatch maps many event types to effects
export function mapEventToEffects(event: VestigeEvent, ctx: GraphMutationContext, allNodes: GraphNode[]) {
  const { effects, nodeManager, edgeManager, forceSim, camera, onMutation } = ctx;
  const nodePositions = nodeManager.positions;
  const nodeMeshMap = nodeManager.meshMap;

  switch (event.type) {
    case 'MemoryCreated': {
      const data = event.data as {
        id?: string;
        content?: string;
        node_type?: string;
        tags?: string[];
        retention?: number;
      };
      if (!data.id) break;

      // Build a GraphNode from event data
      const newNode: GraphNode = {
        id: data.id,
        label: (data.content ?? '').slice(0, 60),
        type: data.node_type ?? 'fact',
        retention: data.retention ?? 0.9,
        tags: data.tags ?? [],
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        isCenter: false,
      };

      // Find spawn position near related nodes
      const spawnPos = findSpawnPosition(newNode, allNodes, nodePositions);

      // Add to all managers
      const pos = nodeManager.addNode(newNode, spawnPos);
      forceSim.addNode(data.id, pos);

      // FIFO eviction
      liveSpawnedNodes.push(data.id);
      evictOldestLiveNode(ctx, allNodes);

      // Spectacular effects: rainbow burst + double shockwave + ripple wave
      const color = new THREE.Color(NODE_TYPE_COLORS[newNode.type] || '#00ffd1');
      effects.createRainbowBurst(spawnPos, color);
      effects.createShockwave(spawnPos, color, camera);
      // Second shockwave, hue-shifted, delayed via smaller initial scale
      const hueShifted = color.clone();
      hueShifted.offsetHSL(0.15, 0, 0);
      setTimeout(() => {
        effects.createShockwave(spawnPos, hueShifted, camera);
      }, 166); // ~10 frames at 60fps
      effects.createRippleWave(spawnPos);

      onMutation({ type: 'nodeAdded', node: newNode });
      break;
    }

    case 'ConnectionDiscovered': {
      const data = event.data as {
        source_id?: string;
        target_id?: string;
        weight?: number;
        connection_type?: string;
      };
      if (!data.source_id || !data.target_id) break;

      const srcPos = nodePositions.get(data.source_id);
      const tgtPos = nodePositions.get(data.target_id);

      const newEdge: GraphEdge = {
        source: data.source_id,
        target: data.target_id,
        weight: data.weight ?? 0.5,
        type: data.connection_type ?? 'semantic',
      };

      // Add edge with growth animation
      edgeManager.addEdge(newEdge, nodePositions);

      // Cyan flash + pulse both endpoints
      if (srcPos && tgtPos) {
        effects.createConnectionFlash(srcPos, tgtPos, new THREE.Color(0x00d4ff));
      }
      if (data.source_id && nodeMeshMap.has(data.source_id)) {
        effects.addPulse(data.source_id, 1.0, new THREE.Color(0x00d4ff), 0.02);
      }
      if (data.target_id && nodeMeshMap.has(data.target_id)) {
        effects.addPulse(data.target_id, 1.0, new THREE.Color(0x00d4ff), 0.02);
      }

      onMutation({ type: 'edgeAdded', edge: newEdge });
      break;
    }

    case 'MemoryDeleted': {
      const data = event.data as { id?: string };
      if (!data.id) break;

      const pos = nodePositions.get(data.id);
      if (pos) {
        // Implosion effect first
        const color = new THREE.Color(0xff4757);
        effects.createImplosion(pos, color);
      }

      // Dissolve edges then node
      edgeManager.removeEdgesForNode(data.id);
      nodeManager.removeNode(data.id);
      forceSim.removeNode(data.id);

      // Remove from live tracking
      const liveIdx = liveSpawnedNodes.indexOf(data.id);
      if (liveIdx !== -1) liveSpawnedNodes.splice(liveIdx, 1);

      onMutation({ type: 'edgesRemoved', nodeId: data.id });
      onMutation({ type: 'nodeRemoved', nodeId: data.id });
      break;
    }

    case 'MemoryPromoted': {
      const data = event.data as { id?: string; new_retention?: number };
      const promoId = data?.id;
      if (!promoId) break;

      const newRetention = data.new_retention ?? 0.95;

      if (nodeMeshMap.has(promoId)) {
        // Grow the node
        nodeManager.growNode(promoId, newRetention);

        // Green pulse + shockwave + mini burst
        effects.addPulse(promoId, 1.2, new THREE.Color(0x00ff88), 0.01);
        const promoPos = nodePositions.get(promoId);
        if (promoPos) {
          effects.createShockwave(promoPos, new THREE.Color(0x00ff88), camera);
          effects.createSpawnBurst(promoPos, new THREE.Color(0x00ff88));
        }

        onMutation({ type: 'nodeUpdated', nodeId: promoId, retention: newRetention });
      }
      break;
    }

    case 'MemoryDemoted': {
      const data = event.data as { id?: string; new_retention?: number };
      const demoteId = data?.id;
      if (!demoteId) break;

      const newRetention = data.new_retention ?? 0.3;

      if (nodeMeshMap.has(demoteId)) {
        // Shrink the node
        nodeManager.growNode(demoteId, newRetention);

        // Red pulse — subtle
        effects.addPulse(demoteId, 0.8, new THREE.Color(0xff4757), 0.03);

        onMutation({ type: 'nodeUpdated', nodeId: demoteId, retention: newRetention });
      }
      break;
    }

    case 'MemoryUpdated': {
      const data = event.data as { id?: string; retention?: number };
      const updateId = data?.id;
      if (!updateId || !nodeMeshMap.has(updateId)) break;

      // Subtle blue pulse on update
      effects.addPulse(updateId, 0.6, new THREE.Color(0x818cf8), 0.02);

      if (data.retention !== undefined) {
        nodeManager.growNode(updateId, data.retention);
        onMutation({ type: 'nodeUpdated', nodeId: updateId, retention: data.retention });
      }
      break;
    }

    case 'SearchPerformed': {
      for (const id of nodeMeshMap.keys()) {
        effects.addPulse(id, 0.6 + Math.random() * 0.4, new THREE.Color(0x818cf8), 0.02);
      }
      break;
    }

    case 'DreamStarted': {
      for (const id of nodeMeshMap.keys()) {
        effects.addPulse(id, 1.0, new THREE.Color(0xa855f7), 0.005);
      }
      break;
    }

    case 'DreamProgress': {
      const memoryId = (event.data as { memory_id?: string })?.memory_id;
      if (memoryId && nodeMeshMap.has(memoryId)) {
        effects.addPulse(memoryId, 1.5, new THREE.Color(0xc084fc), 0.01);
      }
      break;
    }

    case 'DreamCompleted': {
      effects.createSpawnBurst(new THREE.Vector3(0, 0, 0), new THREE.Color(0xa855f7));
      effects.createShockwave(new THREE.Vector3(0, 0, 0), new THREE.Color(0xa855f7), camera);
      break;
    }

    case 'RetentionDecayed': {
      const decayId = (event.data as { id?: string })?.id;
      if (decayId && nodeMeshMap.has(decayId)) {
        effects.addPulse(decayId, 0.8, new THREE.Color(0xff4757), 0.03);
      }
      break;
    }

    case 'ConsolidationCompleted': {
      for (const id of nodeMeshMap.keys()) {
        effects.addPulse(id, 0.4 + Math.random() * 0.3, new THREE.Color(0xffb800), 0.015);
      }
      break;
    }

    case 'ActivationSpread': {
      const spreadData = event.data as { source_id?: string; target_ids?: string[] };
      if (spreadData.source_id && spreadData.target_ids) {
        const srcPos = nodePositions.get(spreadData.source_id);
        if (srcPos) {
          for (const targetId of spreadData.target_ids) {
            const tgtPos = nodePositions.get(targetId);
            if (tgtPos) {
              effects.createConnectionFlash(srcPos, tgtPos, new THREE.Color(0x14e8c6));
            }
          }
        }
      }
      break;
    }
  }
}

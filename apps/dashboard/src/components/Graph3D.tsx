import { useCallback, useEffect, useRef } from 'react';
import type * as THREE from 'three';
import { DreamMode } from '@/graph/dream-mode';
import { EdgeParticleSystem } from '@/graph/edge-particles';
import { EdgeManager } from '@/graph/edges';
import { EffectManager } from '@/graph/effects';
import { type GraphMutation, type GraphMutationContext, mapEventToEffects } from '@/graph/events';
import { ForceSimulation } from '@/graph/force-sim';
import { NodeManager } from '@/graph/nodes';
import { ParticleSystem } from '@/graph/particles';
import { applyTheme, createScene, disposeScene, resizeScene, type SceneContext } from '@/graph/scene';
import { createNebulaBackground, updateNebula } from '@/graph/shaders/nebula.frag';
import { createPostProcessing, type PostProcessingStack, updatePostProcessing } from '@/graph/shaders/post-processing';
import { isDarkMode } from '@/graph/theme';
import type { GraphEdge, GraphNode, VestigeEvent } from '@/types';

interface Props {
  nodes: GraphNode[];
  edges: GraphEdge[];
  centerId: string;
  events?: VestigeEvent[];
  isDreaming?: boolean;
  nodeOpacities?: Map<string, number>;
  onSelect?: (nodeId: string) => void;
  onGraphMutation?: (mutation: GraphMutation) => void;
}

interface SceneState {
  ctx: SceneContext;
  nodeManager: NodeManager;
  edgeManager: EdgeManager;
  edgeParticles: EdgeParticleSystem;
  particles: ParticleSystem;
  effects: EffectManager;
  forceSim: ForceSimulation;
  dreamMode: DreamMode;
  nebulaMesh: THREE.Mesh;
  nebulaMaterial: THREE.ShaderMaterial;
  postStack: PostProcessingStack;
  nodeById: Map<string, GraphNode>;
  processedEventCount: number;
  animationId: number;
  paused: boolean;
  lastDarkMode: boolean;
  lastHoveredNode: string | null;
}

function buildNodeById(nodes: GraphNode[]): Map<string, GraphNode> {
  const map = new Map<string, GraphNode>();
  for (const n of nodes) map.set(n.id, n);
  return map;
}

export function Graph3D({
  nodes,
  edges,
  centerId: _centerId,
  events = [],
  isDreaming = false,
  nodeOpacities,
  onSelect,
  onGraphMutation,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const stateRef = useRef<SceneState | null>(null);
  const propsRef = useRef({ edges, events, isDreaming, nodeOpacities, onSelect, onGraphMutation });
  propsRef.current = { edges, events, isDreaming, nodeOpacities, onSelect, onGraphMutation };
  const dataRef = useRef({ nodes, edges });
  dataRef.current = { nodes, edges };

  const onPointerMove = useCallback((event: PointerEvent) => {
    const s = stateRef.current;
    const container = containerRef.current;
    if (!s || !container) return;
    const rect = container.getBoundingClientRect();
    s.ctx.mouse.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    s.ctx.mouse.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    s.ctx.raycaster.setFromCamera(s.ctx.mouse, s.ctx.camera);
    const intersects = s.ctx.raycaster.intersectObjects(s.nodeManager.getMeshes());
    if (intersects.length > 0) {
      s.nodeManager.hoveredNode = intersects[0].object.userData.nodeId;
      container.style.cursor = 'pointer';
    } else {
      s.nodeManager.hoveredNode = null;
      container.style.cursor = 'grab';
    }
  }, []);

  const onClick = useCallback(() => {
    const s = stateRef.current;
    if (!s) return;
    if (s.nodeManager.hoveredNode) {
      s.nodeManager.selectedNode = s.nodeManager.hoveredNode;
      propsRef.current.onSelect?.(s.nodeManager.hoveredNode);
      const pos = s.nodeManager.positions.get(s.nodeManager.hoveredNode);
      if (pos) s.ctx.controls.target.lerp(pos.clone(), 0.5);
    }
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const ctx = createScene(container);
    const nebula = createNebulaBackground(ctx.scene);
    const postStack = createPostProcessing(ctx.composer);
    const particles = new ParticleSystem(ctx.scene);
    const nodeManager = new NodeManager();
    const edgeManager = new EdgeManager();
    const effects = new EffectManager(ctx.scene);
    const dreamMode = new DreamMode();

    const dark = isDarkMode();
    nebula.mesh.visible = dark;

    const { nodes: initNodes, edges: initEdges } = dataRef.current;
    const positions = nodeManager.createNodes(initNodes);
    edgeManager.createEdges(initEdges, positions);

    const edgeParticles = new EdgeParticleSystem(edgeManager.group);
    edgeParticles.rebuild(
      edgeManager.entries.length,
      edgeManager.entries.map((e) => e.weight),
    );

    const nodeTypes = new Map<string, string>();
    for (const n of initNodes) nodeTypes.set(n.id, n.type);
    const forceSim = new ForceSimulation(positions, nodeTypes);

    ctx.scene.add(edgeManager.group);
    ctx.scene.add(nodeManager.group);

    const state: SceneState = {
      ctx,
      nodeManager,
      edgeManager,
      edgeParticles,
      particles,
      effects,
      forceSim,
      dreamMode,
      nebulaMesh: nebula.mesh,
      nebulaMaterial: nebula.material,
      postStack,
      nodeById: buildNodeById(initNodes),
      processedEventCount: 0,
      animationId: 0,
      paused: false,
      lastDarkMode: dark,
      lastHoveredNode: null,
    };
    stateRef.current = state;

    function checkThemeChange() {
      const currentDark = isDarkMode();
      if (currentDark !== state.lastDarkMode) {
        state.lastDarkMode = currentDark;
        applyTheme(state.ctx);
        state.particles.applyTheme();
        state.nodeManager.applyTheme();
        state.edgeManager.applyTheme();
        state.edgeParticles.applyTheme();
        state.nebulaMesh.visible = currentDark;
      }
    }

    function onVisibilityChange() {
      if (document.hidden) {
        state.paused = true;
        cancelAnimationFrame(state.animationId);
      } else {
        state.paused = false;
        animate();
      }
    }

    function processEvents() {
      const { events: evts } = propsRef.current;
      if (!evts || evts.length <= state.processedEventCount) return;
      const newEvents = evts.slice(state.processedEventCount);
      state.processedEventCount = evts.length;
      const allNodes = Array.from(state.nodeById.values());
      const mutationCtx: GraphMutationContext = {
        effects: state.effects,
        nodeManager: state.nodeManager,
        edgeManager: state.edgeManager,
        forceSim: state.forceSim,
        camera: state.ctx.camera,
        onMutation: (mutation: GraphMutation) => {
          if (mutation.type === 'nodeAdded') {
            state.nodeById.set(mutation.node.id, mutation.node);
          } else if (mutation.type === 'nodeRemoved') {
            state.nodeById.delete(mutation.nodeId);
          }
          propsRef.current.onGraphMutation?.(mutation);
        },
      };
      for (const event of newEvents) {
        mapEventToEffects(event, mutationCtx, allNodes);
      }
    }

    let frameCount = 0;
    function animate() {
      if (state.paused) return;
      state.animationId = requestAnimationFrame(animate);
      const time = performance.now() * 0.001;

      frameCount++;
      if (frameCount % 60 === 0) checkThemeChange();

      const theme = state.ctx.theme;

      state.forceSim.tick(propsRef.current.edges);
      state.nodeManager.updatePositions();
      state.edgeManager.updatePositions(state.nodeManager.positions, state.edgeParticles);
      state.edgeManager.animateEdges(state.nodeManager.positions);
      state.edgeParticles.animate();
      state.particles.animate(time);

      const hovered = state.nodeManager.hoveredNode;
      if (hovered !== state.lastHoveredNode) {
        state.lastHoveredNode = hovered;
        if (hovered) {
          const connected = new Set<string>();
          connected.add(hovered);
          for (const e of propsRef.current.edges) {
            if (e.source === hovered) connected.add(e.target);
            if (e.target === hovered) connected.add(e.source);
          }
          state.nodeManager.setFocus(hovered, connected);
          state.edgeManager.setFocus(hovered);
          for (const entry of state.edgeManager.entries) {
            if (entry.dissolving) continue;
            const isConnected = entry.source === hovered || entry.target === hovered;
            state.edgeParticles.setEdgeFocus(entry.index, isConnected ? 1.0 : 0.05);
          }
        } else {
          state.nodeManager.clearFocus();
          state.edgeManager.setFocus(null);
          state.edgeParticles.clearFocus();
        }
      }

      state.nodeManager.animate(time, state.nodeById, state.ctx.camera, propsRef.current.nodeOpacities);

      state.dreamMode.setActive(propsRef.current.isDreaming ?? false);
      if (state.lastDarkMode) {
        state.dreamMode.update(state.ctx.scene, state.ctx.bloomPass, state.ctx.controls, state.ctx.lights, time);
      }

      const cw = container?.clientWidth ?? 800;
      const ch = container?.clientHeight ?? 600;
      if (state.nebulaMesh.visible) {
        updateNebula(state.nebulaMaterial, time, state.dreamMode.current.nebulaIntensity, cw, ch);
      }

      state.postStack.grain.uniforms.uIntensity.value = theme.postGrain;
      state.postStack.chromatic.uniforms.uIntensity.value = theme.postChromatic;
      state.postStack.vignette.uniforms.uRadius.value = theme.postVignetteRadius;
      if (state.lastDarkMode) {
        updatePostProcessing(state.postStack, time, state.dreamMode.current.nebulaIntensity);
      }

      processEvents();
      state.effects.update(state.nodeManager.meshMap, state.ctx.camera, state.nodeManager.positions);

      state.ctx.controls.update();
      state.ctx.composer.render();
    }

    function onResize() {
      if (!container) return;
      resizeScene(state.ctx, container);
    }

    animate();
    window.addEventListener('resize', onResize);
    document.addEventListener('visibilitychange', onVisibilityChange);
    container.addEventListener('pointermove', onPointerMove);
    container.addEventListener('click', onClick);

    return () => {
      cancelAnimationFrame(state.animationId);
      window.removeEventListener('resize', onResize);
      document.removeEventListener('visibilitychange', onVisibilityChange);
      container.removeEventListener('pointermove', onPointerMove);
      container.removeEventListener('click', onClick);
      state.effects.dispose();
      state.particles.dispose();
      state.edgeParticles.dispose();
      state.nodeManager.dispose();
      state.edgeManager.dispose();
      disposeScene(state.ctx);
      stateRef.current = null;
    };
  }, [onPointerMove, onClick]);

  useEffect(() => {
    const s = stateRef.current;
    if (!s) return;

    s.ctx.scene.remove(s.nodeManager.group);
    s.ctx.scene.remove(s.edgeManager.group);
    s.nodeManager.dispose();
    s.edgeManager.dispose();

    const freshNodeManager = new NodeManager();
    const freshEdgeManager = new EdgeManager();
    const positions = freshNodeManager.createNodes(nodes);
    freshEdgeManager.createEdges(edges, positions);

    s.edgeParticles.dispose();
    const freshParticles = new EdgeParticleSystem(freshEdgeManager.group);
    freshParticles.rebuild(
      freshEdgeManager.entries.length,
      freshEdgeManager.entries.map((e) => e.weight),
    );

    const nodeTypes = new Map<string, string>();
    for (const n of nodes) nodeTypes.set(n.id, n.type);

    s.ctx.scene.add(freshEdgeManager.group);
    s.ctx.scene.add(freshNodeManager.group);

    s.nodeManager = freshNodeManager;
    s.edgeManager = freshEdgeManager;
    s.edgeParticles = freshParticles;
    s.forceSim = new ForceSimulation(positions, nodeTypes);
    s.nodeById = buildNodeById(nodes);
    s.processedEventCount = 0;
    s.lastHoveredNode = null;
  }, [nodes, edges]);

  return <div ref={containerRef} className="w-full h-full" />;
}

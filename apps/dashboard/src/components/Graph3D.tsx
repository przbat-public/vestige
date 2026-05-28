import { useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import * as THREE from 'three';
import { DreamMode } from '@/graph/dream-mode';
import { EdgeParticleSystem } from '@/graph/edge-particles';
import { EdgeManager } from '@/graph/edges';
import { EffectManager } from '@/graph/effects';
import { type GraphMutation, type GraphMutationContext, mapEventToEffects } from '@/graph/events';
import { ForceSimulation } from '@/graph/force-sim';
import { NodeManager } from '@/graph/nodes';
import { ParticleSystem } from '@/graph/particles';
import {
  applyAutoRotate,
  applyTheme,
  createScene,
  disposeScene,
  easeCameraToTarget,
  frameAll,
  frameNode,
  resetCamera,
  resizeScene,
  type SceneContext,
} from '@/graph/scene';
import type { ScreenPos } from '@/graph/select-spatial-neighbor';
import { createNebulaBackground, updateNebula } from '@/graph/shaders/nebula.frag';
import { createPostProcessing, type PostProcessingStack, updatePostProcessing } from '@/graph/shaders/post-processing';
import { isDarkMode } from '@/graph/theme';
import { useGraphKeyboard } from '@/hooks/use-graph-keyboard';
import type { GraphEdge, GraphNode, VestigeEvent } from '@/types';

interface Props {
  nodes: GraphNode[];
  edges: GraphEdge[];
  centerId: string;
  events?: VestigeEvent[];
  isDreaming?: boolean;
  /**
   * When true, suppresses non-essential motion to honor `prefers-reduced-motion`:
   * disables auto-rotate, ambient particle drift, and dampens dream-mode
   * shaders to a static appearance. Hover/click feedback and structural updates
   * (force sim, node placement) still run — they are user-triggered or essential.
   */
  reducedMotion?: boolean;
  /**
   * Node colour palette. `'type'` colours by node-type (default), `'tag'`
   * colours by primary tag — useful for visually separating semantic clusters.
   */
  colorMode?: 'type' | 'tag';
  // Read-only — the parent passes a frozen empty Map when temporal filter
  // is off, so we mustn't mutate this. Downstream `nodeManager.animate`
  // already only calls `.get()`.
  nodeOpacities?: ReadonlyMap<string, number>;
  onSelect?: (nodeId: string) => void;
  /**
   * Called whenever the keyboard cursor lands on a different node. The
   * parent uses this to announce the focused node in an aria-live region —
   * the canvas itself has no DOM tree to attach SR-friendly text to.
   */
  onKeyboardFocus?: (nodeId: string | null) => void;
  onGraphMutation?: (mutation: GraphMutation) => void;
  /**
   * Called when the user clicks the empty canvas (no node under cursor).
   * Lets the parent clear its selection / collapse a detail panel.
   */
  onDeselect?: () => void;
  /**
   * `?` key — the parent owns the help overlay (it has the i18n strings).
   */
  onShowHelp?: () => void;
  /**
   * When this id changes, the camera eases its target to the matching
   * node's position (preserving viewing angle / distance). Used by the
   * parent to drive camera focus from breadcrumbs / history navigation
   * without going through hover or click.
   */
  centerOnId?: string | null;
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
  reducedMotion = false,
  colorMode = 'type',
  nodeOpacities,
  onSelect,
  onKeyboardFocus,
  onGraphMutation,
  onDeselect,
  onShowHelp,
  centerOnId,
}: Props) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);

  // Keyboard navigation is a separate concern from the WebGL renderer.
  // It is its own hook so a screen-reader user can move through nodes
  // without us trying to coerce Three.js meshes into an a11y tree.
  const handleKeyboardSelect = useCallback(
    (nodeId: string) => {
      onSelect?.(nodeId);
    },
    [onSelect],
  );

  // Spatial neighbour navigation needs each node's NDC position. Compute
  // them on demand — the camera moves every frame, so any cached value
  // would be stale by the time the user pressed a key.
  const getScreenPositions = useCallback((): ReadonlyMap<string, ScreenPos> => {
    const s = stateRef.current;
    if (!s) return new Map();
    const out = new Map<string, ScreenPos>();
    const tmp = new THREE.Vector3();
    for (const [id, pos] of s.nodeManager.positions) {
      tmp.copy(pos).project(s.ctx.camera);
      out.set(id, { x: tmp.x, y: tmp.y });
    }
    return out;
  }, []);

  // Bubble Escape up to the parent. The keyboard hook also clears its
  // own focusedNodeId, so by the time onDeselect runs both layers agree
  // the user has dismissed: focus gone, drawer closed. Pre-v3.4.2 we
  // only cleared focus, leaving an outdated MemoryDetail drawer open.
  const handleEscape = useCallback(() => {
    onDeselect?.();
  }, [onDeselect]);

  const {
    focusedNodeId,
    onKeyDown: keyboardOnKeyDown,
    containerProps,
  } = useGraphKeyboard(nodes, handleKeyboardSelect, {
    getScreenPositions,
    onEscape: handleEscape,
  });

  // Notify parent of cursor changes so it can update its aria-live region.
  // We don't notify on each render — only when the id actually changes.
  const lastFocusRef = useRef<string | null>(null);
  // Mirror of `focusedNodeId` for callbacks/effects that need the latest
  // value without re-running on every focus tick (the scene rebuild
  // effect, in particular, would explode if it re-ran on each keystroke).
  const focusedNodeIdRef = useRef<string | null>(null);
  focusedNodeIdRef.current = focusedNodeId;
  useEffect(() => {
    if (lastFocusRef.current === focusedNodeId) return;
    lastFocusRef.current = focusedNodeId;
    onKeyboardFocus?.(focusedNodeId);
    // Push the keyboard cursor down into NodeManager so the label-LOD
    // picks it up as an "essential" node on the next frame. We don't
    // wait for a render tick — the scene loop is decoupled from React.
    const s = stateRef.current;
    if (s) s.nodeManager.keyboardFocusedNode = focusedNodeId;
  }, [focusedNodeId, onKeyboardFocus]);
  const stateRef = useRef<SceneState | null>(null);
  const propsRef = useRef({
    edges,
    events,
    isDreaming,
    reducedMotion,
    colorMode,
    nodeOpacities,
    onSelect,
    onGraphMutation,
    onDeselect,
  });
  propsRef.current = {
    edges,
    events,
    isDreaming,
    reducedMotion,
    colorMode,
    nodeOpacities,
    onSelect,
    onGraphMutation,
    onDeselect,
  };
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
      if (pos) {
        // Eased camera target — preserves user's current viewing angle and
        // distance. Under prefers-reduced-motion we snap instead of
        // animating (still focuses the node, just without the swoop).
        easeCameraToTarget(s.ctx, pos, { instant: propsRef.current.reducedMotion });
      }
    } else {
      // Click on empty space: clear selection so the detail panel can
      // collapse. We don't move the camera here — losing both selection
      // AND focus at once is jarring.
      s.nodeManager.selectedNode = null;
      propsRef.current.onDeselect?.();
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
    let lastTime = performance.now() * 0.001;
    // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: WebGL render loop coordinates camera, easing, hover state, and pulsing mutations; splitting it would force shared mutable state through props
    function animate() {
      if (state.paused) return;
      state.animationId = requestAnimationFrame(animate);
      const time = performance.now() * 0.001;
      const deltaSeconds = time - lastTime;
      lastTime = time;

      frameCount++;
      if (frameCount % 60 === 0) checkThemeChange();

      const theme = state.ctx.theme;

      state.forceSim.tick(propsRef.current.edges);
      state.nodeManager.updatePositions();
      state.edgeManager.updatePositions(state.nodeManager.positions, state.edgeParticles);
      state.edgeManager.animateEdges(state.nodeManager.positions);
      // Edge flow particles & ambient particle field are decorative — suppress
      // them under prefers-reduced-motion. Force sim and node updates still run
      // so structural changes (new memory, deletion) remain visible.
      if (!propsRef.current.reducedMotion) {
        state.edgeParticles.animate();
        state.particles.animate(time);
      }

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

      const opacities = propsRef.current.nodeOpacities;
      state.nodeManager.animate(time, state.nodeById, state.ctx.camera, opacities);
      if (opacities && opacities.size > 0) {
        state.edgeManager.applyTemporalOpacities(opacities);
        for (const entry of state.edgeManager.entries) {
          const srcOp = opacities.get(entry.source) ?? 1;
          const tgtOp = opacities.get(entry.target) ?? 1;
          state.edgeParticles.setEdgeTemporalAlpha(entry.index, Math.min(srcOp, tgtOp));
        }
      } else {
        state.edgeParticles.clearTemporalAlpha();
      }

      state.dreamMode.setActive(propsRef.current.isDreaming ?? false);
      if (state.lastDarkMode) {
        state.dreamMode.update(state.ctx.scene, state.ctx.bloomPass, state.ctx, state.ctx.lights, time);
      }
      // DreamMode mutates ctx.autoRotateSpeed during transitions; clamp it to
      // zero AFTER its update to fully respect prefers-reduced-motion.
      if (propsRef.current.reducedMotion) {
        state.ctx.autoRotateSpeed = 0;
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

      // Manual auto-rotate (TrackballControls has no built-in autoRotate).
      // Apply BEFORE controls.update() so user input takes precedence over
      // the rotation drift in the same frame.
      applyAutoRotate(state.ctx, deltaSeconds);

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
    // Restore keyboard cursor on the new NodeManager — the focused node
    // survives a graph rebuild as long as the same id is still in the
    // refreshed node list. Use a ref so this effect doesn't have to
    // re-run on every cursor move.
    freshNodeManager.keyboardFocusedNode = focusedNodeIdRef.current;

    // Re-apply colour mode after rebuild — createNodes() defaults to 'type'
    // because that's the constructor state, but the user may have switched
    // to 'tag' mode before this rebuild (e.g. on tag filter change).
    if (propsRef.current.colorMode && propsRef.current.colorMode !== 'type') {
      freshNodeManager.setColorMode(propsRef.current.colorMode, s.nodeById, isDarkMode());
    }
  }, [nodes, edges]);

  // Switch palette without rebuilding the scene when only colorMode changes.
  // Cheaper than a full rebuild — just iterates existing meshes and updates
  // material colours.
  useEffect(() => {
    const s = stateRef.current;
    if (!s) return;
    s.nodeManager.setColorMode(colorMode, s.nodeById, isDarkMode());
  }, [colorMode]);

  // External camera focus driver (breadcrumbs, history). Watching the prop
  // is intentional — we don't track _every_ selection change here, only
  // explicit "go look at this node" gestures from the parent. Clicking a
  // node already eases the camera in `onClick`.
  useEffect(() => {
    const s = stateRef.current;
    if (!s || !centerOnId) return;
    const pos = s.nodeManager.positions.get(centerOnId);
    if (pos) {
      easeCameraToTarget(s.ctx, pos, { instant: propsRef.current.reducedMotion });
    }
  }, [centerOnId]);

  // Map an unmodified camera shortcut (`f`/`a`/`r`/`?`) to its action.
  // Returns true if the key was handled — caller is responsible for
  // calling preventDefault. Extracted so the parent dispatch stays
  // linear and below the cognitive-complexity threshold.
  const runCameraShortcut = useCallback(
    (key: string, state: SceneState): boolean => {
      const instant = propsRef.current.reducedMotion;
      switch (key.toLowerCase()) {
        case 'f': {
          // Prefer the user's last *intent*: the selected node if there
          // is one, otherwise the keyboard cursor. Without an anchor we
          // can't know what to frame, so we leave the camera where it is.
          const id = state.nodeManager.selectedNode ?? focusedNodeId;
          const pos = id ? state.nodeManager.positions.get(id) : null;
          if (pos) frameNode(state.ctx, pos, { instant });
          return true;
        }
        case 'a':
          frameAll(state.ctx, state.nodeManager.positions, { instant });
          return true;
        case 'r':
          resetCamera(state.ctx, { instant });
          return true;
        case '?':
          onShowHelp?.();
          return true;
        default:
          return false;
      }
    },
    [focusedNodeId, onShowHelp],
  );

  // Compose hot keys: useGraphKeyboard owns arrows/Home/End/Enter/Escape;
  // we layer F (frame node), A (frame all), R (reset), ? (help) on top
  // and let everything else through. Modifier-pressed shortcuts (Alt+F
  // etc.) fall through so the browser / OS can keep them.
  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      keyboardOnKeyDown(event);
      if (event.defaultPrevented) return;
      if (event.altKey || event.metaKey || event.ctrlKey) return;
      const s = stateRef.current;
      if (!s) return;
      if (runCameraShortcut(event.key, s)) event.preventDefault();
    },
    [keyboardOnKeyDown, runCameraShortcut],
  );

  return (
    <div
      ref={containerRef}
      // role="application" tells screen readers to switch off browse mode
      // and pass arrow keys directly to our keyboard handler. Without it
      // NVDA/JAWS swallow the arrows for their virtual cursor and the
      // graph becomes keyboard-unreachable.
      role="application"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: role="application" makes the div a focusable widget for AT; tabIndex={0} is required so keyboard users can enter the graph
      tabIndex={0}
      aria-activedescendant={containerProps['aria-activedescendant']}
      aria-label={t('graph.canvasLabel')}
      aria-roledescription={t('graph.canvasRole')}
      className="w-full h-full focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
      onKeyDown={onKeyDown}
    >
      {/* Mirror the node list as a hidden SR-only ul. The canvas itself
          has no DOM tree, so aria-activedescendant needs real ids to
          point at. NVDA/VoiceOver will read these on focus change. */}
      <ul className="sr-only" aria-hidden="false">
        {nodes.map((n) => (
          <li key={n.id} id={`graph-node-${n.id}`}>
            {n.label || n.id}
          </li>
        ))}
      </ul>
    </div>
  );
}

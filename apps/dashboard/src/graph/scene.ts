import * as THREE from 'three';
import { TrackballControls } from 'three/addons/controls/TrackballControls.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { detectGPU } from '@/graph/gpu';
import { type GraphThemeConfig, getGraphTheme } from '@/graph/theme';

detectGPU().then((caps) => {
  if (caps.webgpu && import.meta.env.DEV) {
    // biome-ignore lint/suspicious/noConsole: intentional dev-mode GPU capability log
    console.info('[vestige] WebGPU available — renderer upgrade possible', caps.renderer);
  }
});

export interface SceneContext {
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer;
  controls: TrackballControls;
  composer: EffectComposer;
  bloomPass: UnrealBloomPass;
  raycaster: THREE.Raycaster;
  mouse: THREE.Vector2;
  lights: {
    ambient: THREE.AmbientLight;
    point1: THREE.PointLight;
    point2: THREE.PointLight;
  };
  theme: Readonly<GraphThemeConfig>;
  /**
   * Auto-rotate angular speed in radians per second around the world Y axis.
   * TrackballControls does not provide built-in autoRotate (unlike OrbitControls),
   * so the animation loop applies it manually via `applyAutoRotate`.
   * Set to 0 to disable. DreamMode mutates this value during transitions.
   */
  autoRotateSpeed: number;
  /** True while the user is actively interacting with the controls. Pauses auto-rotate. */
  autoRotatePaused: boolean;
}

export function createScene(container: HTMLDivElement): SceneContext {
  const theme = getGraphTheme();

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(theme.background);
  if (theme.fogEnabled) {
    scene.fog = new THREE.FogExp2(theme.fogColor, theme.fogDensity);
  }

  const camera = new THREE.PerspectiveCamera(60, container.clientWidth / container.clientHeight, 0.1, 2000);
  camera.position.set(0, 20, 60);

  const renderer = new THREE.WebGLRenderer({
    antialias: true,
    alpha: false,
    powerPreference: 'high-performance',
  });
  renderer.setSize(container.clientWidth, container.clientHeight);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = theme.toneExposure;
  container.appendChild(renderer.domElement);

  // TrackballControls (instead of OrbitControls) gives full 6DOF: the camera
  // can orbit through the poles without flipping, because TrackballControls
  // does not lock to a constant world-up vector. This is what users expect
  // from a 3D graph viewer (vs the "stops at zenith" feel of OrbitControls).
  const controls = new TrackballControls(camera, renderer.domElement);
  controls.rotateSpeed = 2.5; // TrackballControls uses higher base rotateSpeed than OrbitControls
  controls.zoomSpeed = 1.2;
  controls.panSpeed = 0.8;
  controls.staticMoving = false;
  controls.dynamicDampingFactor = 0.15; // damping/inertia, similar feel to OrbitControls dampingFactor
  controls.minDistance = 10;
  controls.maxDistance = 500;
  // TrackballControls has no autoRotate; we implement it manually in the
  // animation loop using `autoRotateSpeed` below. See `applyAutoRotate`.

  const composer = new EffectComposer(renderer);
  composer.addPass(new RenderPass(scene, camera));
  const bloomPass = new UnrealBloomPass(
    new THREE.Vector2(container.clientWidth, container.clientHeight),
    theme.bloomStrength,
    theme.bloomRadius,
    theme.bloomThreshold,
  );
  composer.addPass(bloomPass);

  const ambient = new THREE.AmbientLight(theme.ambientColor, theme.ambientIntensity);
  scene.add(ambient);

  const point1 = new THREE.PointLight(theme.point1Color, theme.point1Intensity, 200);
  point1.position.set(50, 50, 50);
  scene.add(point1);

  const point2 = new THREE.PointLight(theme.point2Color, theme.point2Intensity, 200);
  point2.position.set(-50, -30, -50);
  scene.add(point2);

  const raycaster = new THREE.Raycaster();
  raycaster.params.Points = { threshold: 2 };
  const mouse = new THREE.Vector2();

  const ctx: SceneContext = {
    scene,
    camera,
    renderer,
    controls,
    composer,
    bloomPass,
    raycaster,
    mouse,
    lights: { ambient, point1, point2 },
    theme,
    autoRotateSpeed: theme.autoRotateSpeed,
    autoRotatePaused: false,
  };

  // Pause auto-rotate while the user is actively orbiting/panning/zooming.
  // TrackballControls emits 'start' on pointerdown and 'end' on pointerup.
  controls.addEventListener('start', () => {
    ctx.autoRotatePaused = true;
  });
  controls.addEventListener('end', () => {
    ctx.autoRotatePaused = false;
  });

  return ctx;
}

/**
 * Manual auto-rotate around the world Y axis. Called by the animation loop
 * to compensate for TrackballControls not having built-in autoRotate.
 *
 * @param ctx scene context (reads `autoRotateSpeed` and `autoRotatePaused`)
 * @param deltaSeconds time since last frame in seconds (frame-rate independent)
 */
const _autoRotateOffset = new THREE.Vector3();
const _autoRotateAxis = new THREE.Vector3(0, 1, 0);
export function applyAutoRotate(ctx: SceneContext, deltaSeconds: number) {
  if (ctx.autoRotateSpeed === 0 || ctx.autoRotatePaused) return;
  // Clamp delta to avoid huge jumps after tab was hidden
  const dt = Math.min(deltaSeconds, 0.1);
  _autoRotateOffset.copy(ctx.camera.position).sub(ctx.controls.target);
  _autoRotateOffset.applyAxisAngle(_autoRotateAxis, ctx.autoRotateSpeed * dt);
  ctx.camera.position.copy(ctx.controls.target).add(_autoRotateOffset);
  ctx.camera.lookAt(ctx.controls.target);
}

/**
 * Smoothly animate the camera and `controls.target` to a new target/camera
 * pair using easeOutCubic. Cancels any in-flight ease so rapid clicks always
 * converge on the most recent destination.
 *
 * Pass `instant: true` (or `durationMs <= 0`) to snap immediately — used
 * when `prefers-reduced-motion` is set or when the caller wants a guaranteed
 * frame-perfect reset.
 *
 * Why an explicit animator (vs `target.lerp(p, 0.5)` per frame in the render
 * loop): exponential-decay lerp chases the target indefinitely and feels
 * rubber-bandy. This easing is deterministic in duration and yields a
 * stable resting state — required for accessibility tests too.
 */
const _easeOffsetTmp = new THREE.Vector3();
const _easeStart = new THREE.Vector3();
const _easeEnd = new THREE.Vector3();
const _easeCameraStart = new THREE.Vector3();
const _easeCameraEnd = new THREE.Vector3();
let _easeFrameId: number | null = null;

export interface CameraEaseOptions {
  durationMs?: number;
  instant?: boolean;
}

export function easeCameraTo(
  ctx: SceneContext,
  target: THREE.Vector3,
  cameraPos: THREE.Vector3,
  options: CameraEaseOptions = {},
) {
  if (_easeFrameId !== null) {
    cancelAnimationFrame(_easeFrameId);
    _easeFrameId = null;
  }
  const durationMs = options.durationMs ?? 600;
  if (options.instant || durationMs <= 0) {
    ctx.controls.target.copy(target);
    ctx.camera.position.copy(cameraPos);
    ctx.camera.lookAt(ctx.controls.target);
    return;
  }
  _easeStart.copy(ctx.controls.target);
  _easeEnd.copy(target);
  _easeCameraStart.copy(ctx.camera.position);
  _easeCameraEnd.copy(cameraPos);
  const startTime = performance.now();
  function step() {
    const elapsed = performance.now() - startTime;
    const t = Math.min(elapsed / durationMs, 1);
    const e = 1 - (1 - t) ** 3; // easeOutCubic
    ctx.controls.target.lerpVectors(_easeStart, _easeEnd, e);
    ctx.camera.position.lerpVectors(_easeCameraStart, _easeCameraEnd, e);
    ctx.camera.lookAt(ctx.controls.target);
    if (t < 1) {
      _easeFrameId = requestAnimationFrame(step);
    } else {
      _easeFrameId = null;
    }
  }
  _easeFrameId = requestAnimationFrame(step);
}

/**
 * Pan the camera target to a new world position while preserving the
 * current camera-to-target offset (the viewing angle and distance stay
 * the same). Used for "click a node → centre on it" interactions.
 */
export function easeCameraToTarget(ctx: SceneContext, targetPosition: THREE.Vector3, options: CameraEaseOptions = {}) {
  _easeOffsetTmp.subVectors(ctx.camera.position, ctx.controls.target);
  const cameraPos = _easeEnd.copy(targetPosition).add(_easeOffsetTmp).clone();
  easeCameraTo(ctx, targetPosition, cameraPos, options);
}

/**
 * Frame a single node — move the target onto it and bring the camera in to
 * a fixed comfortable inspection distance. Used by the `F` hotkey ("focus
 * frame current") and by mouse clicks.
 *
 * @param nodeSize approximate radius hint so the camera distance scales
 *                 with how large the node is rendered. Falls back to 1.
 */
export function frameNode(
  ctx: SceneContext,
  position: THREE.Vector3,
  options: CameraEaseOptions & { nodeSize?: number } = {},
) {
  const nodeSize = options.nodeSize ?? 1;
  // Distance scaled by FOV so the node always occupies ~30% of viewport.
  const fov = ctx.camera.fov * (Math.PI / 180);
  const distance = Math.max(15, (nodeSize * 8) / Math.tan(fov / 2));
  const dir = _easeOffsetTmp.subVectors(ctx.camera.position, ctx.controls.target).normalize();
  if (dir.lengthSq() < 1e-6) {
    dir.set(0, 0.3, 1).normalize();
  }
  const cameraPos = new THREE.Vector3().copy(position).add(dir.multiplyScalar(distance));
  easeCameraTo(ctx, position, cameraPos, options);
}

/**
 * Frame all visible nodes — compute their bounding sphere and back the
 * camera off enough that the whole graph fits in view. Used by the `A`
 * hotkey ("frame all"). A `margin` factor > 1 leaves breathing room.
 *
 * No-op when the position map is empty (e.g. graph still loading).
 */
export function frameAll(
  ctx: SceneContext,
  positions: ReadonlyMap<string, THREE.Vector3>,
  options: CameraEaseOptions & { margin?: number } = {},
) {
  if (positions.size === 0) return;
  const box = new THREE.Box3();
  for (const p of positions.values()) {
    box.expandByPoint(p);
  }
  const center = new THREE.Vector3();
  box.getCenter(center);
  const size = new THREE.Vector3();
  box.getSize(size);
  const maxDim = Math.max(size.x, size.y, size.z, 1);
  const margin = options.margin ?? 1.4;
  const fov = ctx.camera.fov * (Math.PI / 180);
  const distance = (maxDim * margin) / (2 * Math.tan(fov / 2));
  const dir = _easeOffsetTmp.subVectors(ctx.camera.position, ctx.controls.target).normalize();
  if (dir.lengthSq() < 1e-6) {
    dir.set(0, 0.3, 1).normalize();
  }
  const cameraPos = new THREE.Vector3().copy(center).add(dir.multiplyScalar(distance));
  easeCameraTo(ctx, center, cameraPos, options);
}

/**
 * Reset the camera to the default home position used during scene creation.
 * Used by the `R` hotkey.
 */
export function resetCamera(ctx: SceneContext, options: CameraEaseOptions = {}) {
  easeCameraTo(ctx, new THREE.Vector3(0, 0, 0), new THREE.Vector3(0, 20, 60), options);
}

export function applyTheme(ctx: SceneContext) {
  const theme = getGraphTheme();
  ctx.theme = theme;

  (ctx.scene.background as THREE.Color).setHex(theme.background);
  if (theme.fogEnabled) {
    if (ctx.scene.fog instanceof THREE.FogExp2) {
      ctx.scene.fog.color.setHex(theme.fogColor);
      ctx.scene.fog.density = theme.fogDensity;
    } else {
      ctx.scene.fog = new THREE.FogExp2(theme.fogColor, theme.fogDensity);
    }
  } else {
    ctx.scene.fog = null;
  }

  ctx.renderer.toneMappingExposure = theme.toneExposure;

  ctx.bloomPass.strength = theme.bloomStrength;
  ctx.bloomPass.radius = theme.bloomRadius;
  ctx.bloomPass.threshold = theme.bloomThreshold;

  ctx.lights.ambient.color.setHex(theme.ambientColor);
  ctx.lights.ambient.intensity = theme.ambientIntensity;
  ctx.lights.point1.color.setHex(theme.point1Color);
  ctx.lights.point1.intensity = theme.point1Intensity;
  ctx.lights.point2.color.setHex(theme.point2Color);
  ctx.lights.point2.intensity = theme.point2Intensity;

  ctx.autoRotateSpeed = theme.autoRotateSpeed;
}

export function resizeScene(ctx: SceneContext, container: HTMLDivElement) {
  const w = container.clientWidth;
  const h = container.clientHeight;
  ctx.camera.aspect = w / h;
  ctx.camera.updateProjectionMatrix();
  ctx.renderer.setSize(w, h);
  ctx.composer.setSize(w, h);
}

export function disposeScene(ctx: SceneContext) {
  ctx.scene.traverse((obj: THREE.Object3D) => {
    if (obj instanceof THREE.Mesh || obj instanceof THREE.InstancedMesh) {
      obj.geometry?.dispose();
      if (Array.isArray(obj.material)) {
        for (const m of obj.material) m.dispose();
      } else if (obj.material) {
        (obj.material as THREE.Material).dispose();
      }
    }
  });
  ctx.renderer.dispose();
  ctx.composer.dispose();
}

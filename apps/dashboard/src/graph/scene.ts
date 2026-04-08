import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { detectGPU } from '@/graph/gpu';
import { getGraphTheme, type GraphThemeConfig } from '@/graph/theme';

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
  controls: OrbitControls;
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

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.rotateSpeed = 0.5;
  controls.zoomSpeed = 0.8;
  controls.minDistance = 10;
  controls.maxDistance = 500;
  controls.autoRotate = true;
  controls.autoRotateSpeed = theme.autoRotateSpeed;

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

  return {
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
  };
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

  ctx.controls.autoRotateSpeed = theme.autoRotateSpeed;
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

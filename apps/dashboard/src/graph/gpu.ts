/**
 * WebGPU capability detection and renderer selection.
 *
 * Three.js r168+ ships WebGPURenderer (three/addons/renderers/webgpu/).
 * When browser adoption reaches ~90%, swap createScene() to use it for:
 *   - Compute shader particle systems (10-50x throughput)
 *   - GPU-driven force simulation (offload from JS thread)
 *   - Storage buffer node instancing (single draw call for all nodes)
 *   - Indirect rendering for edge batching
 *
 * Migration checklist:
 *   1. Replace WebGLRenderer → WebGPURenderer
 *   2. Port UnrealBloomPass → PostProcessing node graph (TSL)
 *   3. Convert ShaderMaterial → TSL node materials
 *   4. Move force simulation to compute shader
 *   5. Use StorageBufferAttribute for node positions
 */

export interface GPUCapabilities {
  webgpu: boolean;
  webgl2: boolean;
  maxTextureSize: number;
  renderer: string;
}

let cached: GPUCapabilities | null = null;

export async function detectGPU(): Promise<GPUCapabilities> {
  if (cached) return cached;

  const webgl2 = !!document.createElement('canvas').getContext('webgl2');
  let maxTextureSize = 0;
  let renderer = 'unknown';

  if (webgl2) {
    const gl = document.createElement('canvas').getContext('webgl2');
    if (gl) {
      maxTextureSize = gl.getParameter(gl.MAX_TEXTURE_SIZE);
      const dbg = gl.getExtension('WEBGL_debug_renderer_info');
      if (dbg) renderer = gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL);
    }
  }

  let webgpu = false;
  if ('gpu' in navigator) {
    try {
      // biome-ignore lint/suspicious/noExplicitAny: WebGPU types not in default TS lib
      const gpu = (navigator as any).gpu;
      const adapter = await gpu.requestAdapter();
      webgpu = adapter !== null;
    } catch {
      /* WebGPU not available */
    }
  }

  cached = { webgpu, webgl2, maxTextureSize, renderer };
  return cached;
}


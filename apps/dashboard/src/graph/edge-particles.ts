import * as THREE from 'three';
import { getGraphTheme } from '@/graph/theme';

interface FlowParticle {
  edgeIdx: number;
  t: number;
  speed: number;
}

export class EdgeParticleSystem {
  private points: THREE.Points;
  private posAttr: THREE.BufferAttribute;
  private alphaAttr: THREE.BufferAttribute;
  private particles: FlowParticle[] = [];
  private maxCount: number;
  private edgeFocusAlpha: Float32Array;

  curveData = new Float32Array(0);
  edgeCount = 0;

  constructor(parent: THREE.Group, maxCount = 800) {
    this.maxCount = maxCount;
    this.edgeFocusAlpha = new Float32Array(0);

    const pos = new Float32Array(maxCount * 3);
    const alpha = new Float32Array(maxCount);

    const geo = new THREE.BufferGeometry();
    this.posAttr = new THREE.BufferAttribute(pos, 3);
    this.posAttr.setUsage(THREE.DynamicDrawUsage);
    this.alphaAttr = new THREE.BufferAttribute(alpha, 1);
    this.alphaAttr.setUsage(THREE.DynamicDrawUsage);
    geo.setAttribute('position', this.posAttr);
    geo.setAttribute('aAlpha', this.alphaAttr);

    const theme = getGraphTheme();
    const mat = new THREE.ShaderMaterial({
      uniforms: {
        uColor: { value: new THREE.Color(theme.flowParticleColor) },
        uSize: { value: theme.flowParticleSize },
      },
      vertexShader: `
        attribute float aAlpha;
        varying float vAlpha;
        uniform float uSize;
        void main() {
          vAlpha = aAlpha;
          vec4 mv = modelViewMatrix * vec4(position, 1.0);
          gl_PointSize = uSize * (200.0 / -mv.z);
          gl_Position = projectionMatrix * mv;
        }
      `,
      fragmentShader: `
        uniform vec3 uColor;
        varying float vAlpha;
        void main() {
          float d = length(gl_PointCoord - vec2(0.5));
          if (d > 0.5) discard;
          float g = smoothstep(0.5, 0.0, d);
          gl_FragColor = vec4(uColor, g * g * vAlpha);
        }
      `,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });

    this.points = new THREE.Points(geo, mat);
    parent.add(this.points);
  }

  rebuild(edgeCount: number, weights: number[]) {
    this.edgeCount = edgeCount;
    this.curveData = new Float32Array(edgeCount * 9);
    this.edgeFocusAlpha = new Float32Array(edgeCount).fill(1.0);
    this.particles = [];

    for (let i = 0; i < edgeCount; i++) {
      const w = weights[i] ?? 0.5;
      const n = Math.max(1, Math.round(w * 3));
      for (let j = 0; j < n && this.particles.length < this.maxCount; j++) {
        this.particles.push({
          edgeIdx: i,
          t: Math.random(),
          speed: 0.002 + w * 0.004 + Math.random() * 0.002,
        });
      }
    }
  }

  setEdgeFocus(edgeIdx: number, target: number) {
    if (edgeIdx < this.edgeFocusAlpha.length) {
      this.edgeFocusAlpha[edgeIdx] = target;
    }
  }

  clearFocus() {
    this.edgeFocusAlpha.fill(1.0);
  }

  setCurve(
    idx: number,
    sx: number, sy: number, sz: number,
    cx: number, cy: number, cz: number,
    tx: number, ty: number, tz: number,
  ) {
    const o = idx * 9;
    this.curveData[o] = sx;
    this.curveData[o + 1] = sy;
    this.curveData[o + 2] = sz;
    this.curveData[o + 3] = cx;
    this.curveData[o + 4] = cy;
    this.curveData[o + 5] = cz;
    this.curveData[o + 6] = tx;
    this.curveData[o + 7] = ty;
    this.curveData[o + 8] = tz;
  }

  animate() {
    const pa = this.posAttr.array as Float32Array;
    const aa = this.alphaAttr.array as Float32Array;
    const cd = this.curveData;
    const efa = this.edgeFocusAlpha;

    for (let i = 0; i < this.maxCount; i++) {
      if (i >= this.particles.length) {
        pa[i * 3] = pa[i * 3 + 1] = pa[i * 3 + 2] = 0;
        aa[i] = 0;
        continue;
      }

      const p = this.particles[i];
      p.t += p.speed;
      if (p.t >= 1) p.t -= 1;

      const o = p.edgeIdx * 9;
      const t = p.t;
      const omt = 1 - t;

      pa[i * 3] = omt * omt * cd[o] + 2 * omt * t * cd[o + 3] + t * t * cd[o + 6];
      pa[i * 3 + 1] = omt * omt * cd[o + 1] + 2 * omt * t * cd[o + 4] + t * t * cd[o + 7];
      pa[i * 3 + 2] = omt * omt * cd[o + 2] + 2 * omt * t * cd[o + 5] + t * t * cd[o + 8];

      const fi = Math.min(t * 5, 1);
      const fo = Math.min((1 - t) * 5, 1);
      const focusMul = p.edgeIdx < efa.length ? efa[p.edgeIdx] : 1.0;
      aa[i] = fi * fo * 0.9 * focusMul;
    }

    this.posAttr.needsUpdate = true;
    this.alphaAttr.needsUpdate = true;
  }

  applyTheme() {
    const theme = getGraphTheme();
    const mat = this.points.material as THREE.ShaderMaterial;
    mat.uniforms.uColor.value.setHex(theme.flowParticleColor);
    mat.uniforms.uSize.value = theme.flowParticleSize;
  }

  dispose() {
    this.points.geometry.dispose();
    (this.points.material as THREE.Material).dispose();
  }
}

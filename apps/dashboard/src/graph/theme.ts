export function isDarkMode(): boolean {
  return document.documentElement.classList.contains('dark');
}

export interface GraphThemeConfig {
  background: number;
  fogColor: number;
  fogDensity: number;
  fogEnabled: boolean;
  ambientColor: number;
  ambientIntensity: number;
  point1Color: number;
  point1Intensity: number;
  point2Color: number;
  point2Intensity: number;
  bloomStrength: number;
  bloomRadius: number;
  bloomThreshold: number;
  edgeColor: number;
  edgeAdditiveBlending: boolean;
  edgeBaseOpacity: number;
  nodeEmissiveScale: number;
  nodeRoughness: number;
  nodeMetalness: number;
  glowVisible: boolean;
  labelColor: string;
  particlesVisible: boolean;
  nebulaVisible: boolean;
  postGrain: number;
  postChromatic: number;
  postVignetteRadius: number;
  autoRotateSpeed: number;
  toneExposure: number;
  flowParticleColor: number;
  flowParticleSize: number;
  curveAmount: number;
}

const DARK_THEME: GraphThemeConfig = {
  background: 0x0a0a1a,
  fogColor: 0x0a0a1a,
  fogDensity: 0.005,
  fogEnabled: true,
  ambientColor: 0x2a2a4a,
  ambientIntensity: 0.6,
  point1Color: 0x6366f1,
  point1Intensity: 1.2,
  point2Color: 0xa855f7,
  point2Intensity: 0.8,
  bloomStrength: 0.5,
  bloomRadius: 0.3,
  bloomThreshold: 0.85,
  edgeColor: 0x8888cc,
  edgeAdditiveBlending: true,
  edgeBaseOpacity: 0.4,
  nodeEmissiveScale: 1.0,
  nodeRoughness: 0.3,
  nodeMetalness: 0.1,
  glowVisible: true,
  labelColor: '#e2e8f0',
  particlesVisible: true,
  nebulaVisible: true,
  postGrain: 0.02,
  postChromatic: 0.001,
  postVignetteRadius: 0.95,
  autoRotateSpeed: 0.4,
  toneExposure: 1.1,
  flowParticleColor: 0xa0b0ff,
  flowParticleSize: 5.0,
  curveAmount: 1.0,
};

const LIGHT_THEME: GraphThemeConfig = {
  background: 0xf8f9fb,
  fogColor: 0xf0f0f5,
  fogDensity: 0.003,
  fogEnabled: false,
  ambientColor: 0xffffff,
  ambientIntensity: 1.0,
  point1Color: 0x6366f1,
  point1Intensity: 0.5,
  point2Color: 0xa855f7,
  point2Intensity: 0.3,
  bloomStrength: 0.1,
  bloomRadius: 0.2,
  bloomThreshold: 0.95,
  edgeColor: 0x6b7a8f,
  edgeAdditiveBlending: false,
  edgeBaseOpacity: 0.5,
  nodeEmissiveScale: 0.2,
  nodeRoughness: 0.6,
  nodeMetalness: 0.0,
  glowVisible: false,
  labelColor: '#1e293b',
  particlesVisible: false,
  nebulaVisible: false,
  postGrain: 0,
  postChromatic: 0,
  postVignetteRadius: 1.5,
  autoRotateSpeed: 0.4,
  toneExposure: 1.0,
  flowParticleColor: 0x6366f1,
  flowParticleSize: 3.5,
  curveAmount: 0.7,
};

export function getGraphTheme(): Readonly<GraphThemeConfig> {
  return isDarkMode() ? DARK_THEME : LIGHT_THEME;
}

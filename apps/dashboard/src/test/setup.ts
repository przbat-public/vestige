import '@testing-library/jest-dom/vitest';
import { cleanup, configure } from '@testing-library/react';
import * as axeMatchers from 'vitest-axe/matchers';

expect.extend(axeMatchers);

/**
 * Testing Library gives every `findBy*` / `waitFor` a 1000 ms budget by
 * default, which is a deadline rather than a contract: a React Query fetch that
 * resolves in a millisecond on an idle machine can take longer than that when
 * the whole suite runs in parallel on a loaded CI runner. That produced a
 * genuine flake — `TemporalPage.test.tsx` failed roughly one full-suite run in
 * three with "Unable to find role=button", while passing every time in
 * isolation, because the row simply had not rendered yet when the budget
 * expired. Raising the budget keeps the assertion (the row must appear) and
 * removes the timing assumption.
 *
 * `testTimeout` in `vitest.config.ts` is raised to match: if it stayed at
 * Vitest's 5 s default, an assertion that never becomes true would be reported
 * as "test timed out", which hides which assertion failed.
 */
configure({ asyncUtilTimeout: 5000 });

/**
 * jsdom does not implement <canvas> 2D/WebGL contexts. Components like
 * RetentionCurve (uses canvas for the FSRS forecast) and Three.js renderers
 * (when reachable in tests) would otherwise log a noisy
 * "HTMLCanvasElement's getContext()" warning every render.
 *
 * We return a permissive stub that satisfies callers but does not actually
 * paint. Tests that need to observe canvas output should mock specifically
 * — this is just to keep the test log clean.
 */
type MinimalCtx2D = Pick<
  CanvasRenderingContext2D,
  | 'fillRect'
  | 'clearRect'
  | 'beginPath'
  | 'moveTo'
  | 'lineTo'
  | 'stroke'
  | 'fill'
  | 'arc'
  | 'closePath'
  | 'save'
  | 'restore'
  | 'translate'
  | 'rotate'
  | 'scale'
  | 'measureText'
  | 'fillText'
  | 'strokeText'
  | 'createLinearGradient'
  | 'createRadialGradient'
  | 'getImageData'
  | 'putImageData'
  | 'drawImage'
  | 'setTransform'
> & {
  fillStyle: unknown;
  strokeStyle: unknown;
  lineWidth: number;
  font: string;
  globalAlpha: number;
  globalCompositeOperation: string;
  textAlign: string;
  textBaseline: string;
  shadowColor: string;
  shadowBlur: number;
  shadowOffsetX: number;
  shadowOffsetY: number;
  lineCap: string;
  lineJoin: string;
  canvas: HTMLCanvasElement;
};

function createMockContext2D(canvas: HTMLCanvasElement): MinimalCtx2D {
  const noop = () => undefined;
  return {
    canvas,
    fillStyle: '#000',
    strokeStyle: '#000',
    lineWidth: 1,
    font: '10px sans-serif',
    globalAlpha: 1,
    globalCompositeOperation: 'source-over',
    textAlign: 'start',
    textBaseline: 'alphabetic',
    shadowColor: 'rgba(0, 0, 0, 0)',
    shadowBlur: 0,
    shadowOffsetX: 0,
    shadowOffsetY: 0,
    lineCap: 'butt',
    lineJoin: 'miter',
    fillRect: noop,
    clearRect: noop,
    beginPath: noop,
    moveTo: noop,
    lineTo: noop,
    stroke: noop,
    fill: noop,
    arc: noop,
    closePath: noop,
    save: noop,
    restore: noop,
    translate: noop,
    rotate: noop,
    scale: noop,
    measureText: () =>
      ({
        width: 0,
        actualBoundingBoxLeft: 0,
        actualBoundingBoxRight: 0,
        actualBoundingBoxAscent: 0,
        actualBoundingBoxDescent: 0,
        fontBoundingBoxAscent: 0,
        fontBoundingBoxDescent: 0,
        emHeightAscent: 0,
        emHeightDescent: 0,
        hangingBaseline: 0,
        alphabeticBaseline: 0,
        ideographicBaseline: 0,
      }) as TextMetrics,
    fillText: noop,
    strokeText: noop,
    createLinearGradient: () =>
      ({
        addColorStop: noop,
      }) as CanvasGradient,
    createRadialGradient: () =>
      ({
        addColorStop: noop,
      }) as CanvasGradient,
    getImageData: () =>
      ({
        data: new Uint8ClampedArray(4),
        width: 1,
        height: 1,
        colorSpace: 'srgb' as const,
      }) as ImageData,
    putImageData: noop,
    drawImage: noop,
    setTransform: noop,
  };
}

if (typeof HTMLCanvasElement !== 'undefined' && !HTMLCanvasElement.prototype.getContext.toString().includes('mock')) {
  // biome-ignore lint/suspicious/noExplicitAny: jsdom typing of getContext is overloaded; we replace with a stub.
  (HTMLCanvasElement.prototype as any).getContext = function mock(this: HTMLCanvasElement, type: string) {
    if (type === '2d') return createMockContext2D(this);
    // Three.js asks for 'webgl' / 'webgl2' — return null so it falls back
    // gracefully or raises a clear error in tests that exercise Three.js.
    return null;
  };
}

/**
 * jsdom does not implement <dialog>.showModal()/close(). Components like
 * CommandPalette use them; the polyfill mirrors the spec just enough that
 * `userEvent.click` and assertions work.
 */
if (typeof HTMLDialogElement !== 'undefined') {
  if (!HTMLDialogElement.prototype.showModal) {
    HTMLDialogElement.prototype.showModal = function showModal() {
      this.setAttribute('open', '');
    };
  }
  if (!HTMLDialogElement.prototype.close) {
    HTMLDialogElement.prototype.close = function close() {
      this.removeAttribute('open');
      this.dispatchEvent(new Event('close'));
    };
  }
}

afterEach(() => {
  cleanup();
});

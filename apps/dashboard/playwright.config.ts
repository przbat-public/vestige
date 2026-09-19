import { defineConfig } from '@playwright/test';
import { E2E, resolveServerBinary } from './e2e/env';

/**
 * Playwright configuration for the dashboard end-to-end suite.
 *
 * The suite is self-contained: `webServer` starts the `vestige-mcp` binary
 * built from this repository through `e2e/start-server.mjs`, which gives it a
 * throwaway data directory, throwaway ports and a token passed through
 * `VESTIGE_AUTH_TOKEN`. Nothing here reads the developer's own database or the
 * macOS-only `~/Library/Application Support/com.vestige.core/auth_token`, and
 * nothing talks to whatever might already be listening on the default 3927.
 *
 * WebGL: the graph page renders through Three.js in a continuous
 * `requestAnimationFrame` loop, and Playwright's web-first assertions poll on
 * the same frame clock — when the canvas falls back to software rendering,
 * every frame is slow enough to starve that polling and assertions time out
 * while the page is in fact correct. The mitigations below keep the frame
 * budget realistic: the full Chromium build (not the headless shell, which has
 * no GPU stack at all), a 1280x800 viewport, and `--enable-unsafe-swiftshader`
 * so a GPU-less CI runner still gets a WebGL context instead of failing to
 * render.
 */
const serverBinary = resolveServerBinary();

export default defineConfig({
  testDir: './e2e',
  // One shared server and one shared database: parallel files would make
  // "the graph shows exactly what the API returned" race with other writes.
  fullyParallel: false,
  workers: 1,
  timeout: 90_000,
  expect: { timeout: 20_000 },
  retries: process.env.CI ? 1 : 0,
  forbidOnly: !!process.env.CI,
  // Run artifacts live under `dist/` rather than Playwright's default
  // `test-results/` + `playwright-report/`: `biome check .` (the dashboard lint
  // gate) walks the whole package and rejects the JSON Playwright writes there,
  // so a local `pnpm test:e2e` used to leave the lint gate red. `dist/` is
  // ignored by Biome and by git, and this package builds into `build/`, so
  // nothing collides.
  outputDir: 'dist/test-results',
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'dist/playwright-report' }]],
  use: {
    baseURL: E2E.dashboardUrl,
    // The suite asserts English copy; the dashboard picks its language from
    // localStorage, so a fresh context plus an explicit locale pins it.
    locale: 'en-US',
    viewport: { width: 1280, height: 800 },
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    trace: 'retain-on-failure',
    // `channel: 'chromium'` selects the full browser in new-headless mode,
    // which can use the host GPU (macOS/desktop) and falls back to SwiftShader
    // where there is none.
    channel: 'chromium',
    launchOptions: {
      args: ['--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'],
    },
  },
  webServer: {
    command: 'node e2e/start-server.mjs',
    url: `${E2E.dashboardUrl}/api/health`,
    // Never adopt a server someone else started: the suite deletes memories
    // and would do it against that server's database.
    reuseExistingServer: false,
    timeout: E2E.serverStartupTimeoutMs,
    stdout: 'pipe',
    stderr: 'pipe',
    env: {
      VESTIGE_E2E_BIN: serverBinary,
      VESTIGE_E2E_DASHBOARD_PORT: String(E2E.dashboardPort),
      VESTIGE_E2E_MCP_PORT: String(E2E.mcpPort),
      VESTIGE_E2E_TOKEN: E2E.token,
    },
  },
});

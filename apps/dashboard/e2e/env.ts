/**
 * Environment contract shared by `playwright.config.ts`, the server launcher
 * (`start-server.mjs`) and every spec.
 *
 * The suite used to talk to whatever happened to be listening on the default
 * ports with the developer's own token file, which made it unrunnable in CI
 * and destructive to the developer's live database. Everything a spec needs is
 * now derived here from environment variables with throwaway defaults:
 *
 * - the ports default to 4327/4328, *not* the 3927/3928 a running Vestige
 *   instance uses, so `pnpm test:e2e` never touches a live server;
 * - the bearer token is a fixed local-only value supplied to the spawned
 *   server through `VESTIGE_AUTH_TOKEN`, so no file outside the repository is
 *   read (the old macOS-only `~/Library/.../auth_token` path is gone);
 * - `start-server.mjs` gives the server a fresh temporary data directory, so
 *   the suite writes to a database that is deleted when the run ends.
 */
import { existsSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Repository root — `apps/dashboard/e2e/env.ts` is three levels deep. */
export const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');

function readPort(name: string, fallback: number): number {
  const raw = process.env[name];
  if (raw === undefined || raw === '') return fallback;
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`${name} must be a TCP port number, got "${raw}"`);
  }
  return port;
}

const dashboardPort = readPort('VESTIGE_E2E_DASHBOARD_PORT', 4327);
const mcpPort = readPort('VESTIGE_E2E_MCP_PORT', 4328);

/**
 * Long enough to clear `MIN_TOKEN_LENGTH` in
 * `crates/vestige-mcp/src/protocol/auth.rs` (32 chars) so the server does not
 * log a brute-force warning on every run. Overridable for the rare case where
 * a developer wants to drive an already-running server.
 */
const token = process.env.VESTIGE_E2E_TOKEN ?? 'vestige-e2e-local-token-0123456789abcdef';

export const E2E = {
  dashboardPort,
  mcpPort,
  token,
  dashboardUrl: `http://127.0.0.1:${dashboardPort}`,
  mcpUrl: `http://127.0.0.1:${mcpPort}/mcp`,
  /** Path of the SPA route the suite navigates to most often. */
  graphPath: '/dashboard/graph',
  memoriesPath: '/dashboard/memories',
  feedPath: '/dashboard/feed',
  /**
   * Startup budget for the spawned server. A cold debug build applies ~16
   * SQLite migrations and hydrates the cognitive engine before it binds the
   * dashboard port; CI machines are slower than laptops, and a generous
   * ceiling costs nothing because Playwright polls the health endpoint and
   * proceeds as soon as it answers.
   */
  serverStartupTimeoutMs: 120_000,
} as const;

/**
 * Absolute path to the `vestige-mcp` binary that serves the suite.
 *
 * Resolution order: `VESTIGE_E2E_BIN` (relative paths are resolved against the
 * repository root, which is how CI points at `target/release/vestige-mcp`),
 * then the usual debug and release locations. Missing prerequisites throw
 * instead of silently falling back to a server someone else started — a suite
 * that quietly measures the wrong binary is worse than one that does not run.
 */
export function resolveServerBinary(): string {
  const configured = process.env.VESTIGE_E2E_BIN;
  if (configured) {
    const path = isAbsolute(configured) ? configured : resolve(REPO_ROOT, configured);
    if (!existsSync(path)) {
      throw new Error(
        `VESTIGE_E2E_BIN points at "${configured}" (resolved to ${path}), which does not exist.\n` +
          'Build the server first: cargo build -p vestige-mcp',
      );
    }
    return path;
  }

  const candidates = [
    join(REPO_ROOT, 'target', 'debug', 'vestige-mcp'),
    join(REPO_ROOT, 'target', 'debug', 'vestige-mcp.exe'),
    join(REPO_ROOT, 'target', 'release', 'vestige-mcp'),
    join(REPO_ROOT, 'target', 'release', 'vestige-mcp.exe'),
  ];
  const found = candidates.find((candidate) => existsSync(candidate));
  if (!found) {
    throw new Error(
      'No vestige-mcp binary found in target/debug or target/release, so the e2e suite has no server to start.\n' +
        'Build it first: cargo build -p vestige-mcp (or set VESTIGE_E2E_BIN to an existing binary).',
    );
  }
  return found;
}

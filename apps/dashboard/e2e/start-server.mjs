#!/usr/bin/env node
/**
 * Server launcher for the dashboard e2e suite.
 *
 * `playwright.config.ts` runs this script as its `webServer.command`, so
 * Playwright owns the process lifecycle (start, readiness polling, teardown)
 * and the suite never depends on a server someone forgot to start.
 *
 * Why a launcher instead of running the binary directly:
 *
 * 1. **stdin must stay open.** `vestige-mcp` serves MCP over stdio and treats
 *    stdin EOF as a shutdown signal, so a launcher that hands the child a
 *    closed or ignored stdin kills the server milliseconds after it binds its
 *    ports. Here the child gets a pipe we simply never write to or close.
 * 2. **The database has to be disposable.** The old spec targeted the
 *    developer's live database and cleaned up by deleting every memory whose
 *    content contained "E2E TEST". Each run now gets a fresh `mkdtemp`
 *    directory that is removed on exit.
 * 3. **No model downloads.** Startup spawns a task that loads the Jina
 *    cross-encoder reranker (~1.1 GB). `VESTIGE_TEST_MOCK_EMBEDDINGS=1` covers
 *    the embedding model, but nothing covers the reranker, so the cache path
 *    is pointed *inside a regular file* — directory creation fails, the load
 *    fails fast, and `main.rs` logs "cross-encoder unavailable at startup"
 *    before search falls back to BM25 term overlap. Set
 *    `VESTIGE_E2E_ALLOW_MODEL_DOWNLOAD=1` to opt into the real models.
 * 4. **Wrong-target protection.** If a port is already taken (typically a
 *    developer's Vestige instance), the launcher aborts instead of letting
 *    Playwright poll a server that belongs to someone else.
 *
 * Only `node:` builtins are used so this runs before `pnpm install` has
 * finished in a cold checkout.
 */
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const REQUIRED_ENV = [
  'VESTIGE_E2E_BIN',
  'VESTIGE_E2E_DASHBOARD_PORT',
  'VESTIGE_E2E_MCP_PORT',
  'VESTIGE_E2E_TOKEN',
];

const missing = REQUIRED_ENV.filter((name) => !process.env[name]);
if (missing.length > 0) {
  console.error(
    `[vestige-e2e] missing required environment: ${missing.join(', ')}\n` +
      '[vestige-e2e] this script is meant to be started by playwright.config.ts, not by hand.\n' +
      '[vestige-e2e] to debug the server on its own, run:\n' +
      '  VESTIGE_E2E_BIN=target/debug/vestige-mcp VESTIGE_E2E_DASHBOARD_PORT=4327 \\\n' +
      '  VESTIGE_E2E_MCP_PORT=4328 VESTIGE_E2E_TOKEN=vestige-e2e-local-token-0123456789abcdef \\\n' +
      '  node apps/dashboard/e2e/start-server.mjs',
  );
  process.exit(1);
}

const binary = process.env.VESTIGE_E2E_BIN;
const dashboardPort = Number(process.env.VESTIGE_E2E_DASHBOARD_PORT);
const mcpPort = Number(process.env.VESTIGE_E2E_MCP_PORT);

if (!existsSync(binary)) {
  console.error(
    `[vestige-e2e] server binary not found at ${binary}\n` +
      '[vestige-e2e] build it first: cargo build -p vestige-mcp',
  );
  process.exit(1);
}

/** Resolve `true` when something already accepts connections on `port`. */
function portInUse(port) {
  return new Promise((resolve) => {
    const socket = createConnection({ host: '127.0.0.1', port });
    const done = (inUse) => {
      socket.removeAllListeners();
      socket.destroy();
      resolve(inUse);
    };
    socket.once('connect', () => done(true));
    socket.once('error', () => done(false));
    socket.setTimeout(1500, () => done(false));
  });
}

for (const [port, label] of [
  [dashboardPort, 'dashboard'],
  [mcpPort, 'MCP HTTP'],
]) {
  if (await portInUse(port)) {
    console.error(
      `[vestige-e2e] port ${port} (${label}) is already in use — refusing to start.\n` +
        '[vestige-e2e] stop the process holding it, or pick other ports with\n' +
        `[vestige-e2e] VESTIGE_E2E_DASHBOARD_PORT / VESTIGE_E2E_MCP_PORT.`,
    );
    process.exit(1);
  }
}

const runDir = mkdtempSync(join(tmpdir(), 'vestige-e2e-'));
const dataDir = join(runDir, 'data');
const keepData = process.env.VESTIGE_E2E_KEEP_DATA === '1';

const childEnv = {
  ...process.env,
  RUST_LOG: process.env.RUST_LOG ?? 'info',
  VESTIGE_DASHBOARD_PORT: String(dashboardPort),
  VESTIGE_HTTP_PORT: String(mcpPort),
  VESTIGE_AUTH_TOKEN: process.env.VESTIGE_E2E_TOKEN,
};

if (process.env.VESTIGE_E2E_ALLOW_MODEL_DOWNLOAD !== '1') {
  // A file, not a directory: fastembed/hf-hub cannot create a cache directory
  // underneath it, so every model load fails immediately instead of pulling
  // ~1.1 GB from the Hub on each run.
  const blockedCache = join(runDir, 'model-cache-disabled');
  writeFileSync(blockedCache, '');
  childEnv.VESTIGE_TEST_MOCK_EMBEDDINGS = '1';
  childEnv.FASTEMBED_CACHE_PATH = blockedCache;
  childEnv.HF_HOME = blockedCache;
}

console.log(
  `[vestige-e2e] starting ${binary}\n` +
    `[vestige-e2e]   dashboard http://127.0.0.1:${dashboardPort}/dashboard\n` +
    `[vestige-e2e]   mcp       http://127.0.0.1:${mcpPort}/mcp\n` +
    `[vestige-e2e]   data      ${dataDir}${keepData ? ' (kept on exit)' : ''}`,
);

const child = spawn(binary, ['--data-dir', dataDir], {
  env: childEnv,
  // stdin stays an open pipe: the stdio MCP transport shuts the server down on
  // EOF, and Playwright's own webServer stdio handling would close it.
  stdio: ['pipe', 'pipe', 'pipe'],
});

function forward(stream, level) {
  let buffered = '';
  stream.setEncoding('utf8');
  stream.on('data', (chunk) => {
    buffered += chunk;
    const lines = buffered.split('\n');
    buffered = lines.pop() ?? '';
    for (const line of lines) {
      if (line.trim() !== '') console.log(`[vestige-e2e:${level}] ${line}`);
    }
  });
}

forward(child.stdout, 'server');
forward(child.stderr, 'server');

let shuttingDown = false;
function shutdown(signal) {
  if (shuttingDown) return;
  shuttingDown = true;
  child.kill('SIGTERM');
  // The server checkpoints its WAL before exiting; give it a moment, then stop
  // waiting — Playwright kills us anyway when its own teardown budget expires.
  setTimeout(() => child.kill('SIGKILL'), 5000).unref();
}

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => shutdown(signal));
}

child.on('exit', (code, signal) => {
  if (!keepData) rmSync(runDir, { recursive: true, force: true });
  const detail = signal ? `signal ${signal}` : `code ${code}`;
  console.log(`[vestige-e2e] server exited (${detail})`);
  process.exit(code ?? 1);
});

child.on('error', (error) => {
  console.error(`[vestige-e2e] failed to spawn ${binary}: ${error.message}`);
  if (!keepData) rmSync(runDir, { recursive: true, force: true });
  process.exit(1);
});

import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdirSync, openSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const REPO_ROOT = resolve(__dirname, '..', '..', '..');
const ENGINE_BIN = process.env.FTS_E2E_ENGINE_BIN
  ?? join(REPO_ROOT, 'target', 'release', 'fasttrackstudio');
const STATE_FILE = join(__dirname, '.engine-state.json');

function freePort(): Promise<number> {
  return new Promise((res, rej) => {
    const srv = createServer();
    srv.unref();
    srv.on('error', rej);
    srv.listen(0, '127.0.0.1', () => {
      const port = (srv.address() as { port: number }).port;
      srv.close(() => res(port));
    });
  });
}

async function waitForHealth(base: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastErr: unknown = 'no attempt';
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`${base}/health`);
      if (r.ok) return;
      lastErr = `status ${r.status}`;
    } catch (e) {
      lastErr = e;
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`engine /health never came up at ${base}: ${lastErr}`);
}

export default async function globalSetup() {
  const port = await freePort();
  const base = `http://127.0.0.1:${port}`;

  mkdirSync(join(__dirname, 'test-results'), { recursive: true });
  const logFd = openSync(join(__dirname, 'test-results', 'engine.log'), 'w');

  const child = spawn(ENGINE_BIN, ['--engine'], {
    env: {
      ...process.env,
      SIGNAL_ENGINE_ADDR: `127.0.0.1:${port}`,
    },
    stdio: ['ignore', logFd, logFd],
    detached: false,
  });
  child.unref();

  if (child.pid === undefined) {
    throw new Error(`failed to spawn engine binary at ${ENGINE_BIN}`);
  }

  writeFileSync(STATE_FILE, JSON.stringify({ pid: child.pid, base }));

  try {
    await waitForHealth(base, 30_000);
  } catch (e) {
    try { process.kill(child.pid, 'SIGKILL'); } catch { /* already gone */ }
    throw e;
  }

  // Workers inherit this; playwright.config.ts reads it for baseURL.
  process.env.FTS_E2E_BASEURL = base;
}

import { readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const STATE_FILE = join(__dirname, '.engine-state.json');

export default async function globalTeardown() {
  let state: { pid: number; base: string };
  try {
    state = JSON.parse(readFileSync(STATE_FILE, 'utf8'));
  } catch {
    return; // setup never got far enough
  }

  // Kill the exact child we spawned — never a pattern match (live rig rule).
  try {
    process.kill(state.pid, 'SIGTERM');
  } catch {
    return; // already exited
  }
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    try {
      process.kill(state.pid, 0); // still alive?
    } catch {
      rmSync(STATE_FILE, { force: true });
      return;
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  try { process.kill(state.pid, 'SIGKILL'); } catch { /* raced exit */ }
  rmSync(STATE_FILE, { force: true });
}

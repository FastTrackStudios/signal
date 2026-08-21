import { test, expect, type BrowserContext, type Page } from '@playwright/test';

// W7 — progressive range streaming. Runs in its OWN browser context (fresh
// OPFS/IDB: nothing cached), so the big packs must go PROGRESSIVE: plan +
// rank-0 → 'playable' in seconds, detail streamed behind, played holes
// (read misses) jumping the fetch queue.

type PackState = { name: string; state: string; bytes: number; total: number };

const PROGRESSIVE_MIN = 32 * 1024 * 1024; // mirror of PROGRESSIVE_THRESHOLD

test.describe.configure({ mode: 'serial' });

let context: BrowserContext;
let page: Page;

test.beforeAll(async ({ browser }) => {
  // A NEW context = cleared storage — the whole point of this suite.
  context = await browser.newContext();
  // Pin the page's engine target to THIS suite's scratch engine — the
  // page's dev-server heuristic must never route it to the live :4040.
  const wsUrl = `${process.env.FTS_E2E_BASEURL!.replace(/^http/, 'ws')}/vox`;
  await context.addInitScript((url) => {
    localStorage.setItem('fts.signal-engine-ws-url', url);
  }, wsUrl);
  page = await context.newPage();
});

test.afterAll(async () => {
  await context?.close();
});

async function packStates(): Promise<PackState[]> {
  const raw = await page.evaluate(() => {
    const rig = (window as any).__ftsRig;
    if (!rig) return null;
    const ps = rig.packStates();
    return typeof ps === 'string' ? ps : JSON.stringify(ps);
  });
  if (!raw) return [];
  return JSON.parse(raw) as PackState[];
}

async function masterPeak(): Promise<number | null> {
  return page.evaluate(() => {
    const p = (window as any).__ftsRig?.masterPeak();
    return typeof p === 'number' ? p : null;
  });
}

async function resolution(): Promise<number | null> {
  return page.evaluate(() => {
    const r = (window as any).__ftsRig?.resolution?.();
    return typeof r === 'number' ? r : null;
  });
}

async function pollUntil<T>(
  fn: () => Promise<T>,
  ok: (v: T) => boolean,
  timeoutMs: number,
  intervalMs = 500,
  label = 'condition',
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let last: T = undefined as T;
  while (Date.now() < deadline) {
    last = await fn();
    if (ok(last)) return last;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error(`timed out (${timeoutMs}ms) waiting for ${label}; last=${JSON.stringify(last)}`);
}

// Set once by the first test, used by the rest of the serial suite.
let pianoName = '';

test('cold boot: the piano pack turns playable in seconds, bytes << total', async () => {
  test.setTimeout(120_000);
  await page.goto('/rigs/keys/worship');
  await page.getByTestId('rig-start').click();

  // The largest pack must cross the progressive threshold and reach
  // 'playable' within 30 s of the pack list appearing — while holding only
  // a fraction of its bytes. (Whole-file streaming of an 800 MB piano
  // takes far longer; rank-0 is a few MB.)
  const withPiano = await pollUntil(
    packStates,
    (ps) => ps.some((p) => p.total > PROGRESSIVE_MIN && p.state === 'playable'),
    30_000, 250, 'a >32MB pack to reach playable',
  );
  const piano = withPiano
    .filter((p) => p.total > PROGRESSIVE_MIN && p.state === 'playable')
    .sort((a, b) => b.total - a.total)[0];
  pianoName = piano.name;
  expect(piano.bytes, `${piano.name} playable long before complete`).toBeLessThan(
    piano.total / 2,
  );

  // The rig-wide Resolution meter is on the page and honest about it.
  await expect(page.getByTestId('rig-resolution')).toBeVisible();
  const res = await resolution();
  expect(res).not.toBeNull();
  expect(res!).toBeLessThan(100);
});

test('HEADLINE: middle C sounds while the piano is still playable (not ready)', async () => {
  test.setTimeout(120_000);
  // Isolate the piano: mute every lane backed by a different pack, so the
  // peak we measure is the streaming piano, not an already-ready synth.
  // Lane access goes through the __ftsRig hook — the visible compat strip
  // (and its lane-row/lane-mute testids) is gone; the remote UI's mixer
  // owns the on-screen controls now.
  await page.evaluate((piano) => {
    const rig = (window as any).__ftsRig;
    const packs = JSON.parse(rig.packStates());
    const pianoKey = packs.find((p: any) => p.name === piano)?.key ?? '';
    const lanes = JSON.parse(rig.lanes());
    lanes.forEach((l: any, i: number) => {
      if (l.key !== pianoKey) {
        rig.setLaneMute(i, true);
      }
    });
  }, pianoName);

  // Press middle C repeatedly; it must sound WHILE the pack is still
  // 'playable' — its mid-velocity middle-C segment is at the front of the
  // plan, so this lands within a few presses.
  let stateAtPress = '';
  const peak = await pollUntil(
    async () => {
      const states = await packStates();
      stateAtPress = states.find((p) => p.name === pianoName)?.state ?? '';
      await page.evaluate(() => (window as any).__ftsRig.noteOn(60, 100));
      await new Promise((r) => setTimeout(r, 700));
      const p = await masterPeak();
      await page.evaluate(() => (window as any).__ftsRig.noteOff(60));
      return p;
    },
    (p) => typeof p === 'number' && p > 0.001,
    60_000, 1200, 'masterPeak() > 0.001 from the streaming piano (middle C)',
  );
  expect(peak).toBeGreaterThan(0.001);
  expect(stateAtPress, 'the piano was still filling when it sounded').toBe('playable');
  const res = await resolution();
  expect(res!, 'resolution honest while filling').toBeLessThan(100);
});

test('miss-driven: an extreme note starts silent, then its segment jumps the queue and sounds', async () => {
  test.setTimeout(120_000);
  // Wait for the last chord's tail to decay.
  await pollUntil(
    masterPeak,
    (p) => typeof p === 'number' && p < 0.0005,
    20_000, 200, 'masterPeak() decay before the extreme-note probe',
  );

  // A note far from middle C — its segments are at the BACK of the
  // musical plan; pressing it reports a read miss, the page bumps that
  // segment to the queue front, and within 60 s the note sounds.
  const peak = await pollUntil(
    async () => {
      await page.evaluate(() => (window as any).__ftsRig.noteOn(24, 110));
      await new Promise((r) => setTimeout(r, 700));
      const p = await masterPeak();
      await page.evaluate(() => (window as any).__ftsRig.noteOff(24));
      return p;
    },
    (p) => typeof p === 'number' && p > 0.001,
    60_000, 3000, 'masterPeak() > 0.001 after extreme noteOn(24) (miss-driven fetch)',
  );
  expect(peak).toBeGreaterThan(0.001);
});

test('everything reaches ready; resolution hits 100 and FULL RESOLUTION renders', async () => {
  test.setTimeout(420_000);
  const states = await pollUntil(
    packStates,
    (ps) => ps.length >= 9 && ps.every((p) => p.state === 'ready'),
    360_000, 1000, 'ALL packs ready (progressive included)',
  );
  for (const p of states) {
    expect(p.state, `${p.name} fully streamed`).toBe('ready');
    expect(p.bytes, `${p.name} byte-complete`).toBe(p.total);
  }
  const res = await pollUntil(
    resolution,
    (r) => r === 100,
    30_000, 500, 'resolution() to reach 100',
  );
  expect(res).toBe(100);
  await expect(page.getByTestId('rig-resolution')).toHaveText(/FULL RESOLUTION/);
});

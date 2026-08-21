import { test, expect, type BrowserContext, type Page } from '@playwright/test';

// The three smallest Worship proxies — waiting for these keeps the boot test
// fast; the full set is ~2.4 GB and must NOT be waited on.
const SMALL_PACKS = ['Choir Women', 'Big Berthas', 'Prophet 5'];

type PackState = { name: string; state: string; bytes: number; total: number };

test.describe.configure({ mode: 'serial' });

let context: BrowserContext;
let page: Page;

test.beforeAll(async ({ browser }) => {
  // ONE context for the whole suite: the refresh-resume test relies on
  // OPFS/IDB persisting across a reload in the same context.
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

async function rigState(): Promise<string | null> {
  return page.evaluate(() => (window as any).__ftsRig?.state() ?? null);
}

async function rigAudioState(): Promise<string | null> {
  return page.evaluate(() => (window as any).__ftsRig?.audioState() ?? null);
}

async function masterPeak(): Promise<number | null> {
  return page.evaluate(() => {
    const p = (window as any).__ftsRig?.masterPeak();
    return typeof p === 'number' ? p : null;
  });
}

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

function smallPacksOf(states: PackState[]): PackState[] {
  return SMALL_PACKS.map((wanted) =>
    states.find((p) => p.name.toLowerCase().includes(wanted.toLowerCase())),
  ).filter((p): p is PackState => p !== undefined);
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

test('boot: rig starts, streams ALL packs to ready — none deferred', async () => {
  // W6 (attach-by-handle): every pack in the Worship set attaches — the
  // bytes live on the worklet's JS heap, so nothing defers on wasm memory.
  // Dolceola (841 MB) + Clavichord (574 MB) stream from local disk; the
  // full set is ~2.4 GB. Since W7 the big packs go PROGRESSIVE (playable
  // in seconds, then per-segment detail fill), which trades whole-file
  // throughput for playability — the all-ready wait gets a bigger budget.
  test.setTimeout(720_000);
  await page.goto('/rigs/keys/worship');
  await page.getByTestId('rig-start').click();

  // idle → starting → running → ready. 'running' can be brief; accept
  // either running or ready as proof we got past 'starting'.
  await pollUntil(
    rigState,
    (s) => s === 'running' || s === 'ready',
    60_000, 500, "state() to reach 'running'",
  );

  await pollUntil(
    rigState,
    (s) => s === 'ready',
    600_000, 1000, "state() to reach 'ready'",
  );

  // EVERY pack row must be fully resident — none deferred, none failed.
  const states = await pollUntil(
    packStates,
    (ps) => ps.length > 0 && ps.every((p) => p.state === 'ready'),
    600_000, 1000, 'ALL packs to be ready',
  );
  // The Worship program references nine packed soundsources (PHAT Bass is
  // synthesis-mode — no pack); every one must attach.
  expect(states.length).toBeGreaterThanOrEqual(9);
  for (const p of states) {
    expect(p.state, `${p.name} attached (not deferred/failed)`).toBe('ready');
    expect(p.bytes, `${p.name} fully streamed`).toBe(p.total);
  }
});

test('audio out: notes and the demo player move the master peak', async () => {
  const audioState = await rigAudioState();
  expect(audioState).toBe('running');

  // A chord through the rig's own JS hook.
  await page.evaluate(() => {
    const rig = (window as any).__ftsRig;
    rig.noteOn(60, 100);
    rig.noteOn(64, 100);
    rig.noteOn(67, 100);
  });
  const peak = await pollUntil(
    masterPeak,
    (p) => typeof p === 'number' && p > 0.001,
    5_000, 100, 'masterPeak() > 0.001 after noteOn chord',
  );
  expect(peak).toBeGreaterThan(0.001);

  await page.evaluate(() => {
    const rig = (window as any).__ftsRig;
    rig.noteOff(60);
    rig.noteOff(64);
    rig.noteOff(67);
  });

  // Wait for the master to fall back to (near) silence so the next
  // assertions measure their own notes, not this chord's tail.
  await pollUntil(
    masterPeak,
    (p) => typeof p === 'number' && p < 0.0005,
    15_000, 200, 'masterPeak() to decay after the chord',
  );

  // A HIGH note (C7) — outside the middle-out preload coverage the wasm
  // decoded-PCM budget allows on the big pianos, so this exercises
  // warm-on-note: the worklet decodes the sample on demand before queueing
  // the note-on. First press may land a beat late; the retrigger loop
  // below absorbs that.
  await page.evaluate(() => (window as any).__ftsRig.noteOn(96, 110));
  const highPeak = await pollUntil(
    async () => {
      // Retrigger — a first press that raced the warm decode plays nothing.
      await page.evaluate(() => (window as any).__ftsRig.noteOn(96, 110));
      return masterPeak();
    },
    (p) => typeof p === 'number' && p > 0.001,
    20_000, 500, 'masterPeak() > 0.001 after high noteOn(96) (warm-on-note)',
  );
  expect(highPeak).toBeGreaterThan(0.001);
  await page.evaluate(() => (window as any).__ftsRig.noteOff(96));
  await pollUntil(
    masterPeak,
    (p) => typeof p === 'number' && p < 0.0005,
    15_000, 200, 'masterPeak() to decay after the high note',
  );

  // Second proof through the UI path: the demo MIDI player in the
  // soundsource popover.
  await page.getByTestId('ssm-button').click();
  await expect(page.getByTestId('demo-play-0')).toBeVisible({ timeout: 10_000 });
  await page.screenshot({
    path: test.info().outputPath('ssm-popover.png'),
    fullPage: true,
  });
  await page.getByTestId('demo-play-0').click();
  const demoPeak = await pollUntil(
    masterPeak,
    (p) => typeof p === 'number' && p > 0.001,
    10_000, 100, 'masterPeak() > 0.001 while demo-play-0 plays',
  );
  expect(demoPeak).toBeGreaterThan(0.001);
  await page.getByTestId('demo-stop').click();
});

test('refresh-resume: cached packs return to ready from OPFS, no re-stream', async () => {
  await page.reload();
  await page.getByTestId('rig-start').click();

  // Cache hit: the small packs must come back 'ready' fast, with
  // bytes === total from the stored copy rather than re-streaming.
  const states = await pollUntil(
    packStates,
    (ps) => {
      const small = smallPacksOf(ps);
      return small.length === SMALL_PACKS.length && small.every((p) => p.state === 'ready');
    },
    // Observed: re-ready lands at ~14-15s — the rig re-boots the worklet
    // before re-attaching, which dominates; the actual cache-hit proof is
    // bytes === total below (no re-stream), not wall time. 30s de-flakes.
    30_000, 250, `cached packs [${SMALL_PACKS.join(', ')}] ready after reload`,
  );
  for (const p of smallPacksOf(states)) {
    expect(p.bytes, `${p.name} served whole from cache`).toBe(p.total);
  }
});

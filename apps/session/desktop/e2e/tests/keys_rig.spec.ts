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
  // OPFS/IDB persisting across a reload in the same context. MIDI is
  // granted so the W11 hot-plug plumbing assertions see an armed
  // statechange listener instead of a permission denial.
  // (This chromium only grants Web MIDI when BOTH names are present —
  // 'midi' alone still rejects requestMIDIAccess.)
  context = await browser.newContext({ permissions: ['midi', 'midi-sysex'] });
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

// The rig AUTO-STARTS on mount now, so `rig-start` only appears on the
// failed-boot retry path. Click it if it happens to be there; otherwise
// the boot is already under way.
async function ensureStarted(): Promise<void> {
  const btn = page.getByTestId('rig-start');
  if (await btn.isVisible().catch(() => false)) {
    await btn.click();
  }
}

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
  await ensureStarted();

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

  // ── W8: render-load + latency are measured and sane while the demo
  // plays. The load window turns over every ~0.7 s (250 quanta) and the
  // page polls it at 2 Hz, so give it a few seconds to show a non-zero
  // completed window. < 0.9 = the rig fits the budget on this box.
  const load = await pollUntil(
    () => page.evaluate(() => (window as any).__ftsRig?.renderLoad() ?? null),
    (l) => typeof l === 'number' && l > 0,
    15_000, 250, 'renderLoad() > 0 while the demo plays',
  );
  expect(load).toBeGreaterThan(0);
  // W11: the default hint is now `low`, where the browser wakes the
  // worklet per 1–2 quanta instead of in batches — and the processor's
  // ~1 ms clock rounds each isolated render call up, inflating the read
  // by up to ~0.4 (1 ms against a 2.67 ms quantum). Observed 0.8–1.1 on
  // a healthy box; a genuinely overloaded rig reads several×, so 1.5
  // still catches runaway CPU without flaking on clock quantization.
  expect(load).toBeLessThan(1.5);
  const latency = await page.evaluate(() => (window as any).__ftsRig?.latencyMs() ?? null);
  expect(latency).toBeGreaterThan(0);
  const statsJson = await page.evaluate(() => (window as any).__ftsRig?.audioStats() ?? '');
  const stats = JSON.parse(statsJson);
  expect(stats.quanta).toBeGreaterThan(0);
  expect(stats.sampleRate).toBeGreaterThan(0);

  // The audio panel popover, as a visual artifact (close the ssm popover
  // first so the two don't overlap in the shot).
  await page.getByTestId('ssm-button').click();
  await page.getByTestId('audio-button').click();
  await expect(page.getByTestId('audio-popover')).toBeVisible({ timeout: 5_000 });
  await expect(page.getByTestId('audio-load')).toBeVisible();
  await page.screenshot({
    path: test.info().outputPath('audio-popover.png'),
    fullPage: true,
  });
  await page.getByTestId('audio-button').click();

  await page.getByTestId('ssm-button').click();
  await page.getByTestId('demo-stop').click();
  await page.getByTestId('ssm-button').click();
});

test('realtime: cold-note warms never glitch the audio thread', async () => {
  // ── W12, the headline regression test ────────────────────────────────
  // A cold note used to decode SYNCHRONOUSLY in the worklet's message
  // handler — which runs on the audio rendering thread — starving
  // process() and dropping every sounding voice ("goes silent every time
  // I play a new note"). Decode now lives in the decoder worker; the
  // processor counts real underruns via currentFrame discontinuities.
  // While a chord SUSTAINS, hammer notes far outside preload coverage:
  // the chord must keep sounding and the underrun counter must not move.
  test.setTimeout(120_000);
  const stats = async () => JSON.parse(
    await page.evaluate(() => (window as any).__ftsRig.audioStats()),
  );
  // Measure THIS phase, not boot. `worstHandlerMs` is monotonic since the
  // worklet started, and boot legitimately runs long handlers (building
  // lane instruments as packs attach — bounded, but tens of ms). What this
  // test is about is whether PLAYING glitches, so zero the counters and
  // judge only what follows.
  await page.evaluate(() => (window as any).__ftsRig.resetAudioStats?.());
  await page.waitForTimeout(500);
  const before = await stats();

  // Sustain a chord for the whole hammering phase.
  await page.evaluate(() => {
    const rig = (window as any).__ftsRig;
    rig.noteOn(60, 100); rig.noteOn(64, 100); rig.noteOn(67, 100);
  });
  await pollUntil(
    masterPeak,
    (p) => typeof p === 'number' && p > 0.001,
    5_000, 100, 'chord sounding before the cold-note hammering',
  );

  // Cold extremes, alternating ends of the keyboard, varied velocities
  // (velocity layers are distinct samples). Each press both queues a warm
  // for the decoder worker AND must leave the sounding chord untouched.
  const cold = [21, 103, 23, 105, 25, 99, 27, 101, 30, 107, 33, 97];
  for (let i = 0; i < cold.length; i += 1) {
    await page.evaluate(([n, v]) => (window as any).__ftsRig.noteOn(n, v),
      [cold[i], 40 + ((i * 17) % 80)] as [number, number]);
    await page.waitForTimeout(150);
    const p = await masterPeak();
    // The CHORD keeps sounding through every cold press — this is the
    // exact field failure mode (silence on each new note).
    expect(p, `chord audible during cold press ${cold[i]}`).toBeGreaterThan(0.0005);
    await page.evaluate((n) => (window as any).__ftsRig.noteOff(n), cold[i]);
  }

  // Give the decoder worker a beat to land its PCM, then read the meters.
  await page.waitForTimeout(2_000);
  const after = await stats();
  expect(after.glitches - before.glitches, 'zero underruns while hammering').toBe(0);
  // No message handler may sit on the audio thread for long WHILE PLAYING —
  // the decode is gone; what remains is a bounded PCM memcpy (~1 MB
  // pieces). 20 ms is already ~8 quanta; the old decode path measured
  // hundreds. Boot-time instrument building is excluded by the reset above
  // and tracked separately (see browser-keys-rig.md W13).
  expect(after.worstHandlerMs, 'audio-thread handlers stay bounded while playing')
    .toBeLessThan(20);
  // The worker actually decoded something for those cold notes.
  expect(after.pcmInserts, 'decoder worker delivered PCM').toBeGreaterThan(0);

  await page.evaluate(() => {
    const rig = (window as any).__ftsRig;
    rig.noteOff(60); rig.noteOff(64); rig.noteOff(67);
  });
  await pollUntil(
    masterPeak,
    (p) => typeof p === 'number' && p < 0.0005,
    15_000, 200, 'masterPeak() to decay after the realtime test',
  );
});

test('refresh-resume: cached packs return to ready from OPFS, no re-stream', async () => {
  await page.reload();
  await ensureStarted();

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

test('latency hint: flipping to playback re-boots the audio path and sound survives', async () => {
  // W8: the selector stores fts.keys-latency-hint and re-runs the whole
  // boot (new AudioContext with the hint, cached program, OPFS packs) —
  // this is also a regression test of the in-page re-boot path. (W11
  // added a second flip — through the numeric `low` hint — to the same
  // test, hence the wider budget.)
  test.setTimeout(300_000);
  await pollUntil(
    rigState,
    (s) => s === 'running' || s === 'ready',
    60_000, 500, "state() running before the flip",
  );
  await page.getByTestId('audio-button').click();
  const hint = page.getByTestId('audio-latency-hint');
  await expect(hint).toBeVisible({ timeout: 5_000 });
  await expect(hint).toBeEnabled({ timeout: 30_000 });
  await hint.selectOption('playback');

  // The pref landed…
  const stored = await page.evaluate(() => localStorage.getItem('fts.keys-latency-hint'));
  expect(stored).toBe('playback');

  // …and the rig comes back: state recovers past the restart, the cached
  // small packs re-attach, the context runs, and a note still sounds.
  await pollUntil(
    rigState,
    (s) => s === 'running' || s === 'ready',
    60_000, 500, "state() to recover after the latency-hint flip",
  );
  await pollUntil(
    packStates,
    (ps) => {
      const small = smallPacksOf(ps);
      return small.length === SMALL_PACKS.length && small.every((p) => p.state === 'ready');
    },
    60_000, 500, 'cached packs ready after the latency-hint re-boot',
  );
  const audioState = await pollUntil(
    rigAudioState,
    (s) => s === 'running',
    15_000, 250, "audioState() 'running' after the flip",
  );
  expect(audioState).toBe('running');
  const peak = await pollUntil(
    async () => {
      await page.evaluate(() => (window as any).__ftsRig.noteOn(60, 100));
      return masterPeak();
    },
    (p) => typeof p === 'number' && p > 0.001,
    20_000, 500, 'masterPeak() > 0.001 after the latency-hint re-boot',
  );
  expect(peak).toBeGreaterThan(0.001);
  await page.evaluate(() => (window as any).__ftsRig.noteOff(60));

  // ── W11: flip through the numeric `low` hint the same way — stored
  // pref + full recovery. (Absolute latency numbers are machine-dependent
  // and NOT asserted; this proves the hint plumbing end to end.)
  await pollUntil(
    async () => page.getByTestId('audio-latency-hint').isEnabled(),
    (e) => e === true,
    30_000, 500, 'hint selector enabled again before the low flip',
  );
  await page.getByTestId('audio-latency-hint').selectOption('low');
  const storedLow = await page.evaluate(() => localStorage.getItem('fts.keys-latency-hint'));
  expect(storedLow).toBe('low');
  await pollUntil(
    rigState,
    (s) => s === 'running' || s === 'ready',
    60_000, 500, "state() to recover after the low flip",
  );
  const audioStateLow = await pollUntil(
    rigAudioState,
    (s) => s === 'running',
    15_000, 250, "audioState() 'running' after the low flip",
  );
  expect(audioStateLow).toBe('running');
  const peakLow = await pollUntil(
    async () => {
      await page.evaluate(() => (window as any).__ftsRig.noteOn(60, 100));
      return masterPeak();
    },
    (p) => typeof p === 'number' && p > 0.001,
    20_000, 500, 'masterPeak() > 0.001 after the low re-boot',
  );
  expect(peakLow).toBeGreaterThan(0.001);
  await page.evaluate(() => (window as any).__ftsRig.noteOff(60));
});

test('webmidi: hot-plug listener armed, port gate defaults to omni', async () => {
  // No hardware in CI — assert the W11 plumbing: the statechange listener
  // is installed (hot-plugged controllers will be picked up), the live
  // input list is served, and a fresh page forwards from EVERY input.
  const armed = await pollUntil(
    () => page.evaluate(() => (window as any).__ftsRig?.midiHotplugArmed?.() ?? null),
    (a) => a === true,
    30_000, 500, 'midiHotplugArmed() === true',
  );
  expect(armed).toBe(true);
  const inputsJson = await page.evaluate(() => (window as any).__ftsRig.midiInputs());
  expect(Array.isArray(JSON.parse(inputsJson))).toBe(true);
  const omni = await page.evaluate(() => (window as any).__ftsRig.midiOmni());
  expect(omni).toBe(true);
});

test('viewport fit: no horizontal scroll at 1366x768 and 1920x1080', async () => {
  // W11: the rig scales to fit narrow viewports (transform, top-left
  // origin) and fills wide ones — never a horizontal page scrollbar,
  // never a rig pushed off the left edge.
  const original = page.viewportSize();
  try {
    for (const size of [{ width: 1366, height: 768 }, { width: 1920, height: 1080 }]) {
      await page.setViewportSize(size);
      await pollUntil(
        () => page.evaluate(() =>
          document.documentElement.scrollWidth - window.innerWidth),
        (d) => typeof d === 'number' && d <= 1,
        10_000, 250, `no horizontal overflow at ${size.width}x${size.height}`,
      );
      const box = await page.evaluate(() => {
        const el = document.getElementById('keys-fit-inner');
        if (!el) return null;
        const r = el.getBoundingClientRect();
        return { left: r.left, right: r.right, width: r.width };
      });
      expect(box, 'rig content mounted').not.toBeNull();
      expect(box!.left, 'rig starts inside the viewport').toBeGreaterThanOrEqual(0);
      expect(box!.left, 'no dead space pushing the rig out').toBeLessThan(size.width / 2);
      expect(box!.right, 'rig fits the viewport').toBeLessThanOrEqual(size.width + 1);
    }
  } finally {
    if (original) await page.setViewportSize(original);
  }
});

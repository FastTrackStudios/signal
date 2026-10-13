# Browser guitar rig — `/rigs/guitar`

The guitar rig, whole, in a browser tab: every profile and patch the desktop
rig has, NAM models and IRs included, played from a guitar on the audio
input (or a looped reference DI) with no added latency.

## Pieces

| piece | where | runs on |
|---|---|---|
| chain runner | `features/rigs/guitar/worklet/src/runner.rs` | AudioWorklet (render thread) |
| NAM worker | `NamWorker` in `worklet/src/web.rs` + `nam_worker.js` | one Web Worker per remote model |
| planner | `worklet/src/plan.rs` (`planChain` in wasm) | page and worklet — same code, same answer |
| page orchestrator | `worklet/guitar_rig.js` | main thread |
| bundle | `signal rig web-bundle <dir>` (`features/rigs/guitar/src/web_bundle.rs`) | build time |

One wasm module serves both the worklet (`GuitarWorklet`) and the workers
(`NamWorker`). Chains are built by signal-sampler's own `prepare_chain`, so a
patch sounds as it does natively: same blocks, same NAM calibration, same
dual-amp blend (`amp_blend`), same trims.

## Where each model runs

An A2 model costs ~310 µs of a 2667 µs quantum (128 frames @ 48 kHz) in
wasm with simd128 — after the pure engine's kernel work in
neural-amp-modeler-rs (it was 1550 µs). A patch's models are mostly in series
(boost → drives → amp), so they cannot run side by side within a quantum.
The plan, per model:

1. **Inline**, on the render thread, while the budget lasts — no latency.
   The budget is 80% of the quantum less what the patch's *playing*
   non-model blocks cost (reverbs are ~500 µs each), measured once per
   block on a profiler worker and cached.
2. **Amp R** (a dual-amp patch) runs on a worker *at the same time as Amp L*:
   both hear the dry guitar, so its input is posted when Amp L starts. No
   latency. Amp L is pinned inline: the two are summed, and a lag between
   them would comb.
3. A serial model that does not fit runs on a worker **one quantum behind**:
   2.7 ms of latency per such model, while it is switched on.
4. A **bypassed** model waits on a standby worker, fed its input whenever
   the worker is idle (output discarded) — so stomping it on is seamless (no
   silent quantum, no cold model; bit-exact in `stomping_a_standby_model_on_is_seamless`)
   and it costs the render thread nothing while off.

Measured in Chrome on the M-series MacBook: all 53 shipped patches plan to
**zero added latency**, render load 44% average / 62% worst, no worker
misses. The page re-plans with a smaller budget if a worker starts missing,
so a slower machine trades latency, never dropouts.

## The worker protocol

One `SharedArrayBuffer` per remote model (layout in `web.rs`): the worklet
writes the input and bumps `SEQ_IN` + `Atomics.notify`; the worker, parked in
`Atomics.wait`, runs the model and publishes `SEQ_OUT`. The worklet never
blocks (AudioWorklet cannot `Atomics.wait`) — it spins, bounded, and plays
silence for a late quantum (counted as a miss). To message a serving worker
the page raises `CMD`, which returns it to its event loop.

Browsers render quanta in bursts (two back to back for a 5.3 ms device
buffer), so "a quantum later" can be well under a quantum of wall time —
which is why standby feeding never waits.

**Needs cross-origin isolation** — `Cross-Origin-Opener-Policy: same-origin`
and `Cross-Origin-Embedder-Policy: require-corp` on the page — for
`SharedArrayBuffer`. Without it the page runs every model inline.

## The bundle

`signal rig web-bundle <dir>` writes `rig.json` (every profile's patches,
resolved exactly as the live rig resolves them) and `assets/<blake3>.nam|wav`.
Content addressing ships a shared capture once, lets assets be cached
forever, and keeps local paths out of a public bundle. Plugin blocks (no
plugin host in a browser) and placeholders without DSP (Pitch, for now) are
left out. The shipped rig: 4 profiles, 53 patches, 38 assets, 10.5 MB.

## Try it

```bash
just guitar-web-harness
```

stages everything under `target/guitar-web` and serves it with the isolation
headers on :8765. `node profile.mjs <profile> <patch>` there prints each
block's cost.

## Not yet

- The page is a harness; the real one is the Signal guitar UI on
  `apps/web`'s `/rigs/guitar`, with its GPU panels on `vello_hybrid` canvases
  (the keyflow-web pattern).
- A patch switch builds its chain in a worklet message handler — on the
  render thread — so the switch itself can drop a quantum.
- The two reverbs are now the heaviest blocks (~1 ms together). Their wet
  path could run a quantum late on a worker with the dry path on time — the
  mix law is linear (`dry·(1−mix) + wet·mix`) — freeing the budget for
  more inline models.
- `set_slimmable_size` below 1 does not change the pure engine's output in
  the wasm bench; the adaptive fallback to slimmer models needs that fixed.

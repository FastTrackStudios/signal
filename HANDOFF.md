# Handoff — the rig in a browser (2026-09-22)

Written at the end of a long session. Everything below is **committed
locally and nothing is pushed**, in three repos on `/Volumes/dev-drive`:
`signal`, `processor`, and a new clone `neural-amp-modeler-rs`.

The user's standing rule: commit locally, never push, PR or tag unless
asked. Two pushes are queued behind their decision — see **Waiting on the
user**.

## What this session did

Four threads, in order:

1. **The guitar rig plays in a browser** — `signal-guitar-worklet`, a NAM
   worker pool, and `signal rig web-bundle`. Design doc:
   `crates/signal/docs/browser-guitar-rig.md` (read it first; it is the
   map for the audio half).
2. **The NAM engine got 5× faster** in wasm, bit-identically.
3. **Three faults fixed** in the desktop rig: a 100 GB overnight memory
   leak, the vox error on an EQ drag, and a 1 GB/night log.
4. **The painted widgets run in the browser** — the real ones, with their
   shaders, on WebGPU. The browser's second EQ is deleted.

### 1. The rig plays in a browser

`features/rigs/guitar/worklet` (`signal-guitar-worklet`): an AudioWorklet
chain runner plus a NAM Web Worker pool over `SharedArrayBuffer`.

Where each model runs is planned per patch (`src/plan.rs`): inline while
the render budget lasts, Amp R on a worker beside Amp L (zero latency,
because both hear the dry guitar), anything else one quantum behind, and
bypassed models on standby workers that stay warm so a stomp is seamless
(bit-exact test: `stomping_a_standby_model_on_is_seamless`).

Measured in Chrome, M-series MacBook: **all 53 shipped patches plan to
zero added latency**, render load 44% average / 62% worst, no misses.

- `just guitar-web-harness` — builds, exports the rig, serves the harness
  with the COOP/COEP headers `SharedArrayBuffer` needs.
- `signal rig web-bundle <dir>` — every patch resolved as the live rig
  resolves it; models and IRs content-addressed (no local paths in a
  public bundle). Shipped rig: 4 profiles, 53 patches, 38 assets, 10.5 MB.

### 2. The NAM engine

`/Volumes/dev-drive/neural-amp-modeler-rs` (cloned this session, local
commit `289e8ce`). An A2 model went **1.55 ms → ~310 µs** per 128-frame
quantum in wasm+simd128, output bit-identical (all parity and bit-exact
tests against the C++ core pass):

- register-blocked kernels for the square/1→C shapes with the channel
  count a compile-time constant (A2 layers are 8 channels, 3 when slim);
- the history roll was a `Vec` per column per block — now one memmove;
- activations matched once per buffer, not per element;
- flat passes for `z = conv + mixin`, the residual, the skip sum.

`signal/Cargo.toml` has a `[patch]` to that clone. **It must be tagged
before signal can be pushed** (signal's git dep still points at
`v0.1.0`).

### 3. The three desktop faults

All in `features/rigs/guitar/ui`:

- **Memory.** `Callback::new` in a component body is owned by the scope
  and freed only on unmount (dioxus-core says so). The Control view makes
  fourteen, re-renders on every meter tick, never unmounts, and two
  captured the whole block list: 2.8 M live allocations, ~5 MB/s, 100 GB
  overnight. `src/stable.rs` gives a component one callback per call site,
  re-pointed each render — usable inside `if`/`match`/`map`, where
  `use_callback` cannot go. Idle RSS now flat (203 MB over 3.5 min).
- **The vox error.** A drag sent one spawned RPC per pointer event per
  field and saturated vox's 64 in-flight limit; the server closes the
  connection on the 65th. Reproduction kept, ignored:
  `tests/param_flood.rs::an_unbounded_drag_flood_breaks_the_link` (1926
  of 2000 writes failed). Every write now goes through
  `src/param_writer.rs`: newest value per (block, param), ≤8 on the wire,
  never two for one parameter. Verified native (real backend, real vox)
  **and** on wasm (`wasm-bindgen-test`), and by hand in the browser: 400
  rapid moves, link alive, values landed.
- **The log.** The gate visualiser drew zero-height bars, which usvg
  rejects with a warning *per repaint* — 7.3 M lines (1 GB) in one night.

### 4. The painted widgets, in the browser

The browser used to draw hand-made SVG imitations of the painted panels.
It now runs **the widgets themselves**:

- `processor/libs/ui/fts-audio-ui/src/scene_canvas.rs` — a `<canvas>` with
  vello on **WebGPU**, driven through anyrender's window renderer, whose
  painter is both a `PaintScene` and a `RenderContext`. That is the whole
  trick: `renderer_specific_context()` hands the widget a real wgpu device
  and `try_register_custom_resource` takes the texture its shader drew
  into, so the browser makes the same two calls Blitz does
  (`can_create_surfaces`, then `paint`) against the same struct — shaders
  included.
- `CompWidget`, `DelayWidget`, `ReverbWidget`, `ModWidget` and
  `EqGraphWidget` each implement `CanvasPanel` and the Blitz/nice-plug
  trait over one inherent `create_surfaces` / `paint_frame`.
- **eq-ui is now portable**: `graph-paint` (painters, no plugin host)
  under `graph`; the component and popup take dioxus rather than the
  nice-plug prelude; `nice-plug-dioxus` moved to a native-only table, so
  baseview and nice-log's filesystem logging stay out of the browser.
  `spectrum-analyzer-ui`'s painters are portable too; its settings panel
  is behind `panel` (reached by `spectrum-analyzer/ui-panel`, which
  eq-ui's `native` turns on).
- **`eq_surface` is deleted** and the `eq-vello` feature with it, at the
  user's request ("i never want to see it again"). There is one EQ.

Three wasm32 traps found the hard way, all the same shape — *a thread or
a std clock is not a slow path in wasm32, it is a panic that takes the
whole subtree with it*:

- `use_repaint_clock` spawned an OS thread (all four viz crates);
- the EQ graph's own 120 Hz tick did too;
- `std::time::Instant::now()` / `SystemTime::now()` in the widgets' clocks
  → `web-time`.

If a browser panel ever comes up empty with "RuntimeError: unreachable"
in the console, look for one of those three before anything else.

## State right now

- **Verified in the browser** (the app at `127.0.0.1:8080` against the
  engine on `:4040`): the rig loads, 14 canvases live, no console errors,
  the real EQ graph paints, an EQ node drags and the curve follows, and a
  400-move flood keeps the link.
- **Native**: `rig_shot` renders correctly (no regressions from the
  callback refactor); signal-guitar-ui 26 tests, signal-guitar 72,
  signal-guitar-worklet 13, processor's viz crates all pass.
- Running background processes from this session (kill when done):
  `signal-desktop --engine` (holds the audio device), `python3 -m
  http.server 8080` in `apps/desktop/web-dist`, and a harness server on
  `:8765`.
- `apps/desktop/web-dist/index.html` has a **hand-injected console-capture
  script** (`window.__caught`) for debugging. It is not in the build —
  it disappears on the next stage. Do not commit it.

## Environment gotchas (these cost hours)

- **Building for wasm32 needs a clang that targets wasm** — Apple clang
  cannot, and `ring` fails in its build script:
  `CC_wasm32_unknown_unknown=/nix/store/6adskryjj6g2p508xjxp2x4iwyy15gsr-clang-21.1.8/bin/clang`
  `AR_wasm32_unknown_unknown=/nix/store/8w308r33lb6d0ijmypa802v6dvyswkcj-llvm-21.1.8/bin/llvm-ar`
- **wasm-bindgen CLI on PATH is 0.2.114; the lockfile is 0.2.126.** The
  dev shell has the right one but rebuilds `dx` from source (slow).
  Workaround used:
  `cargo install wasm-bindgen-cli --version 0.2.126 --locked --root <scratch>/tools`
  and prepend its `bin` to PATH.
- **`just web-stage`'s tailwind step fails** (no node_modules for
  tailwindcss in apps/desktop). Run the `dx build --platform web` line
  directly; the committed `assets/tailwind-signal.css` is enough.
- Release wasm hides panic messages. For a message, build without
  `--release` (dioxus installs the panic hook) or capture
  `window.onerror` as above.
- `cargo` in `processor/` needs `+1.94.0` (const `mul_add`); signal has a
  rustup override.

## Waiting on the user

1. **Push + tag `neural-amp-modeler-rs` `v0.1.1`**, then repin signal off
   the local `[patch]` and push signal. Asked, not yet answered.
2. Whether to push processor (it carries the canvas host, the eq-ui
   split, and the wasm fixes; signal's local `[patch]` points at it).

## Next, in the order I would do it

1. **One GPU device for many canvases.** Each `SceneCanvas` builds its own
   `VelloWindowRenderer`, and the Control view mounts 14 — that is 14 wgpu
   devices. It works, but `anyrender_vello` has no way to share a
   `WGPUContext` (`VelloRendererOptions` has no slot for one). Either
   patch/vendor it to accept an existing context, or render several panels
   into one canvas.
2. **The engine wedged** after ~30 minutes and ~10 browser reloads: HTTP
   on `:4040` stopped answering (curl timed out), 0% CPU, main thread
   parked in `block_on`, ~10 established connections from the page.
   Restarting fixed it. Not diagnosed — reproduce by reloading the browser
   remote repeatedly and watch `curl -m 5 http://127.0.0.1:4040/`. This is
   the most suspicious loose thread left.
3. **The browser rig's own page.** `apps/web`'s `/rigs/guitar` is still a
   placeholder (`apps/web/src/routes/rig.rs`); the worklet stack is
   driven only by the harness at `features/rigs/guitar/worklet/harness/`.
   Stage 4 in `browser-guitar-rig.md`.
4. **Patch switches build their chain on the render thread** (a worklet
   message handler), so a switch can drop a quantum. Pre-build resident
   chains the way native `GuitarRig` does.
5. **`set_slimmable_size` does not change the pure engine's output** in
   the wasm bench — the adaptive slim fallback is therefore dead code
   until that is fixed.
6. The two reverbs are the heaviest blocks left (~1 ms together). Their
   wet path could run a quantum late on a worker with the dry on time; the
   mix law is linear (`dry·(1−mix) + wet·mix`).
7. Small: the compressor gain-reduction meter is a global slot the limiter
   can overwrite; the pitch shifter has no DSP; `localhost:4040/account/
   callback` is still unregistered in starcommand.

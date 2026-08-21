# Browser keys rig — `/rigs/keys/worship`

The keys rig playable entirely in the browser: samples streamed in as ogg
proxy packs, MIDI from WebMIDI (or bundled demo files), audio rendered in an
AudioWorklet by the same engine the native workstation uses. Reachable over
tailscale from the engine box (`fasttrackstudio --engine` on :4040,
`embed-web`); the public `fasttrackstudio.app/rigs/keys/worship` mapping is a
later serving concern.

## Architecture (decided)

Reuse, don't reinvent:

- **Renderer**: the existing `WebRenderer` worklet
  (`daw-standalone/src/audio_engine/web.rs`, features `decode,web`). The
  keys rig is `Hosting::Lanes` = N daw tracks each with one `KeysInstrument`
  in its fx slot, and `KeysInstrument` already implements the
  `daw::plugin::PluginInstance` trait the wasm `ProjectRenderer` FX stage
  calls. Seed the lane project, `insert_plugin_instance`, render.
- **Sample format**: kind-6 ogg-vorbis proxy packs (decode = symphonia,
  pure Rust, wasm-clean; encode stays native-only). The full Worship proxy
  set is built (~2.4 GB; synth/choir lanes are 3–20 MB each, Keyscape
  pianos 330–840 MB).
- **Transport**: the existing `PackLibrary` RPC over the same `/vox`
  WebSocket the remote UI opens — `read(name, variant, start)` already does
  chunked transfer + resume-from-offset.
- **MIDI file playback**: preset SMFs parsed with the daw-proto SMF parser
  and laid as MIDI items on the lane tracks; the wasm-clean
  `render/midi.rs` transport path plays them — same code path as the
  native workstation, sample-accurate, loop/stop for free.

## Phases

- **W1** — `fts-sample` split: `engine-core` (wasm-clean: open pack from
  bytes via `PackBytes`, decode flac/ogg/pcm, budget) vs `engine-native`
  (mmap, rayon preload, encoders, prefetch threads); `engine` aliases
  native so no consumer changes. IN FLIGHT.
- **W2** — headless rig: `KeysRig::open_headless(sample_rate, &LaneProgram)`
  (no cpal / `AudioIoPrefs`), signal-sampler's native-only deps cfg-gated
  (or a `signal-sampler-core` leaf if gating sprawls); wasm live-MIDI queue
  in `render/mod.rs` (plain `RefCell<VecDeque<_>>` — the worklet scope is
  single-threaded) + `#[wasm_bindgen] noteOn/noteOff/cc/pitchBend/panic` on
  `WebRenderer`; worklet messages `open_lanes { program }`,
  `attach_pack { name, bytes }`, `midi`. DONE — target-gating (no core
  crate needed): signal-sampler / signal-rig-host / signal-plugin-host
  compile for wasm32 as-is (native halves in
  `[target.'cfg(not(wasm32))'.dependencies]` + `#[cfg]`), packs-from-bytes
  via `signal_sampler::pack_registry`, and the worklet-with-keys entry is
  the NEW crate `signal-keys-worklet` (features/rigs/keys/worklet —
  daw-standalone can't dep signal-sampler, the arrow runs the other way):
  `KeysWorklet` = `WebRenderer` + `KeysRig::open_headless_on` +
  `attachPack/openLanes/reloadLanes/noteOn/…`. The wasm queue is a
  `Mutex<VecDeque<_>>` on `Standalone` (not `RefCell` — keeps `Standalone:
  Sync`; uncontended in the single-threaded worklet), drained per block.
- **W3** — browser app phase (the fts dx web build):
  - Route: a `/rigs/keys/:profile` URL branch in `launch_app` (alongside
    `collection_browser`'s pathname sniffing), mounting the existing
    wasm-clean `signal_keys_ui::KeysRigRemote` backed by a LOCAL client
    talking to the worklet (same UI as the remote rig; only the transport
    differs).
  - **Soundsource Manager**: a TOP-BAR BUTTON, out of the way — icon with
    an aggregate progress badge (spinner/`n/m` while streaming, quiet when
    ready), opening a popover of per-pack rows: name, size, progress bar,
    state `queued → streaming → verifying → ready / failed`, retry/delete
    actions, total footprint. Lanes light up as their pack turns ready
    (synths playable in seconds; pianos fill in behind).
  - **Persistence**: pack BYTES in OPFS (`packs/<name>.<variant>.signalpack`
    + `.part` in flight — mirror of native `pack_client.rs`: resume +
    sha256 verify); download LEDGER in IndexedDB (expected size, sha,
    bytes landed, state). Both survive refresh; on boot the ledger drives
    auto-resume via `read(start = bytes_landed)`. Ask
    `navigator.storage.persist()` so packs aren't evicted.
  - **Demo MIDI player**: in the same popover — 2–3 bundled preset SMFs
    (authored programmatically; mind add_notes quarter-note units), each
    with play/loop/stop and a target (whole rig or one lane). The no-hardware
    smoke test.
  - **WebMIDI**: `navigator.requestMIDIAccess()` via web-sys (add
    MidiAccess/MidiInput/MidiMessageEvent features), raw 3-byte messages
    forwarded to the worklet port. Converges with the demo player on the
    same input seam.
- **W4** — staging/serving: a `just` target that wasm-bindgen-builds the
  keys worklet and stages it into the dx web bundle (`web-stage`), then
  `--features embed-web` so ONE engine binary serves UI + packs + vox on
  :4040 → tailnet.

- **W5 — Playwright end-to-end test** (+ interactive browser-tools
  verification during development):
  - A real-browser test that proves the whole chain: launch the engine
    binary on an ephemeral port serving the embedded web bundle → open
    `/rigs/keys/worship` in chromium
    (`--autoplay-policy=no-user-gesture-required` so the AudioContext can
    start) → wait for a FIXTURE pack to stream and turn `ready` in the
    Soundsource Manager → drive the demo MIDI player → assert audio.
  - **Fixture pack**: a tiny bundled `.signalpack` (few hundred KB, built
    by the fts-sample test encoder) served by a test pack root, so CI never
    needs AudioHaven or a 500 MB piano.
  - **Audio assertion**: the page exposes rig state + master peak to JS
    (e.g. `window.__ftsRig = { state, packStates, masterPeak() }` fed by
    the same status the UI renders) — the test polls `masterPeak() > 0`
    while the demo file plays; plus screenshots of the manager popover
    states (streaming / ready) as visual artifacts.
  - Also covers refresh-resume: reload mid-download, assert the ledger
    resumes from the stored offset rather than restarting.
  - Lives in `apps/fasttrackstudio/e2e/` (package.json + playwright
    config); its own `just` target (e.g. `just keys-web-e2e`) that builds
    web-stage + the worklet, starts the engine on a scratch port, runs
    playwright, tears down. CI job is separate from the cargo gates (needs
    node + chromium).
  - This imposes on W3: stable `data-testid` attributes on the top-bar
    button, per-pack rows, and demo player controls; and the JS state hook
    above.

## Open questions / later

- Piano-lane weight on travel links: consider a lower-quality travel
  variant (`transcode --quality 0.4`) or per-sample range streaming (the
  pack index gives entry offsets; `read(start)` can fetch ranges — DFD over
  the network).
- `Worship PHAT Bass` lane has no pack anywhere (the `.prt_omn` gap) — the
  lane is silent natively too; browser matches native until that pack is
  built.
- COOP/COEP headers on the engine's axum server if SharedArrayBuffer
  streaming into the worklet is adopted later; v1 transfers assembled pack
  bytes into the worklet via postMessage (transferable).

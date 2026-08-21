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
  `attach_pack { name, bytes }`, `midi`.
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

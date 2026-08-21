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
- **W4** — staging/serving. DONE — ONE engine binary serves the whole rig:
  - `just keys-worklet-wasm` (Justfile): release wasm build of
    `signal-keys-worklet` + `wasm-bindgen --target web --out-name
    signal_keys_worklet` into `apps/fasttrackstudio/web-dist/worklet/`,
    plus daw-standalone's `examples/web_worklet/processor.js` copied
    verbatim as `keys_processor.js` (it is already keys-aware — `entry:
    'keys'` + the keys message kinds — and the glue URL arrives in the
    init message, so nothing is hardcoded). Yields exactly the three URLs
    `web_keys_rig.rs` expects: `/worklet/keys_processor.js`,
    `/worklet/signal_keys_worklet.js`, `/worklet/signal_keys_worklet_bg.wasm`.
  - `just web-stage` now runs the worklet staging after the dx copy, so
    every staged bundle carries the worklet.
  - `just keys-web` = web-stage, then `cargo build --release -p
    fasttrackstudio --features embed-web` (staging FIRST — embed-web
    `include_dir!`s `web-dist/` at compile time). Default features stay
    on, so the engine has the keys backend + PackLibrary.
  - No serving fixes were needed: `architect::host`'s embedded fallback
    already handles nested paths (`include_dir::get_file`), serves
    `application/wasm` for `.wasm`, and falls back to `index.html` for
    SPA routes like `/rigs/keys/worship`. PackLibrary scans
    `/run/media/AudioHaven/Signal/Libraries` by default
    (`FTS_PACK_LIBRARY` overrides) — the Worship proxy tree included.
  - Run it: `just keys-web`, then `target/release/fasttrackstudio
    --engine` (binds `0.0.0.0:4040`; `SIGNAL_ENGINE_ADDR` overrides) and
    open `http://<tailnet-host>:4040/rigs/keys/worship`.
  - Smoke-tested headless (scratch port via
    `SIGNAL_ENGINE_ADDR=127.0.0.1:14041`): `/health` → ok;
    `/rigs/keys/worship` → 200 SPA index (`text/html`);
    `/worklet/signal_keys_worklet_bg.wasm` → 200 `application/wasm`,
    5.75 MB; the processor + glue js → 200 `text/javascript`; `/vox` →
    101 Switching Protocols; pack library scanned 8433 packs.

- **W5 — Playwright end-to-end test** — DONE (+ interactive browser-tools
  verification during development):
  - Lives at `apps/fasttrackstudio/e2e/` (`@playwright/test` ~1.59, pinned
    to the chromium-1217 revision the flake's `PLAYWRIGHT_BROWSERS_PATH`
    nix store carries). Run: `just keys-web-e2e` (expects
    `target/release/fasttrackstudio` to exist — `just keys-web` builds it;
    needs the real pack library or `FTS_PACK_LIBRARY`). A globalSetup
    picks a free scratch port, spawns the release binary with
    `SIGNAL_ENGINE_ADDR`, waits on `/health`, and teardown kills that
    exact pid — never port 4040.
  - Three serial tests, ONE browser context (so OPFS/IDB persist for the
    reload): **boot** (click `rig-start`, `state()` → running → ready,
    the three smallest Worship proxies — Choir Women / Big Berthas /
    Prophet 5 — stream to `ready` with bytes == total; never waits on the
    full ~2.4 GB set); **audio out** (`audioState()==='running'`,
    `noteOn` chord then `demo-play-0` via the soundsource popover, each
    drives `masterPeak() > 0.001`; popover screenshot saved to
    test-results); **refresh-resume** (reload + restart: the cached packs
    return `ready` with bytes == total from OPFS, no re-stream — observed
    ~15 s, dominated by the worklet re-boot, not streaming).
  - Chromium launches with `--autoplay-policy=no-user-gesture-required` —
    the page-side AudioContext runs without a gesture, so `masterPeak()`
    is a real audio-out proof (CDP input can't grant user activation).
  - The fixture-pack idea below stays open for CI; the suite currently
    runs against the real library (the three proxies total ~19 MB, boot
    ≈ 1.8 min, whole suite ≈ 2 min).
  - Original sketch:
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

- **W6 — packs out of wasm memory (attach-by-handle) + warm-on-note** —
  DONE. All NINE Worship packs attach; `PackPhase::Deferred` no longer
  occurs with the stock set.
  - **`PackBytes::External { id, len }`** (fts-sample): pack bytes that
    live OUTSIDE the process, reachable only through a pluggable
    process-global reader (`cache::set_external_pack_reader`,
    `Box<dyn Fn(u32, u64, &mut [u8]) -> bool + Send + Sync>` — fts-sample
    never deps wasm-bindgen). Every pack read goes through
    `PackBytes::read_range(offset, len) -> Cow<[u8]>` (borrowed for
    Mapped/Owned, copied through the reader for External); `Deref` panics
    on External by design, and `Pcm::sample`/`warm`/`StreamedSample::open`
    carry non-trapping guards. `SignalPcmPack::open_external(id, len)`
    parses header + index through the reader; audio entries **materialize
    per entry** at decode time (`load_pack_sample` copies just the entry's
    span to an Owned buffer, so streaming/mapped windows never see
    External). Native `Pcm::Mapped`/mmap paths byte-for-byte unchanged.
  - **Worklet protocol**: `attach_pack` no longer copies bytes into wasm —
    keys_processor.js keeps the transferred buffer in `this.packs`
    (id → Uint8Array, JS heap, outside the 4 GB), serves
    `globalThis.__ftsPackRead(id, offset, len) → subarray|null`, and calls
    `attachPackExternal(key, id, byteLength)`
    (`pack_registry::install_external`). The wasm-side reader closure
    captures nothing (re-resolves the hook per call), so it satisfies
    `Send + Sync`. A re-attach under the same key frees the superseded
    buffer. The old byte-copy `attachPack` stays for compatibility.
  - **Page guard**: the 1.5 GB `WASM_PACK_RESIDENT_CAP` refusal is
    replaced by a 6 GB `JS_PACK_RESIDENT_SANITY_CAP` (tab-memory realism —
    pack bytes cost wasm linear memory ~0 now); `Deferred` stays in the
    enum for pathological sets, relabelled "past the tab-memory sanity
    cap".
  - **Warm-on-note**: the decoded-PCM budget (768 MB on wasm) still bounds
    preload coverage, and the audio render DROPS a voice whose sample is
    not resident. `KeysRig::warm_note(note, vel)` walks every lane's
    render tree (`RenderNode::warm_note_samples`, zone-filtered) and
    decodes the needed sample on the control side; `KeysWorklet::midi`/
    `noteOn` call it on note-on **before** queueing the event (between
    render quanta — a cold key's first press may decode audibly late
    once). Warm charges the budget past its ceiling; well past it
    (limit + 1/8) the warmed engine sheds largest-first back to the limit.
  - e2e: the boot test now waits for **all** pack rows `ready` (≥ 9,
    none deferred/failed, 300 s budget — observed ~1.7 min for the full
    ~2.4 GB set from local disk); the audio test adds a high C7
    `noteOn(96)` assertion exercising warm-on-note beyond preload
    coverage. Suite green 3×, ~2.2 min.

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

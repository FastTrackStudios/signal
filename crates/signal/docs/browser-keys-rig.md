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

- **W7 — network range streaming with musical prioritization** — DONE.
  A pack becomes PLAYABLE in seconds and gains detail as it streams;
  misses driven by actual playing jump the queue.
  - **Proto** (`signal-packs-proto`): `read_range(name, variant,
    range: String /* "start+len", PackRange's Display/FromStr */, tx)`
    streams exactly one range (absolute offsets, contiguous,
    stream-close = range done); `pack_plan(name, variant, start, tx)`
    streams the plan as the UTF-8 bytes of facet-json
    `Vec<PackSegment>` where each `PackSegment{start, len, rank: u64,
    label}` tiles the file exactly once (total + sha256 come from the
    `packs()` listing). `read` additionally accepts VIRTUAL names —
    `"plan:<name>"` and `"range:<start>+<len>:<name>"` — that route to
    the same two operations over `read`'s exact signature; the browser
    client uses those. (The dedicated methods are exercised natively —
    `pack-library/tests/plan_roundtrip.rs` and the `pack_probe`
    example's `plan:`/`planfirst:` modes.)
  - **The 4040 trap (a whole debugging arc, recorded so nobody repeats
    it)**: the wasm `server_url()` used to redirect ANY local page not
    on :4040 to `ws://127.0.0.1:4040/vox` (a dx-dev-server
    convenience) — so the e2e's page silently drove the LIVE studio
    engine, and every new RPC failed with phantom phon schema-compat
    errors ("writer and reader schema kinds differ" = an old binary
    without the methods; later `NotFound` = its `read` seeing the
    virtual name). The heuristic now redirects only the known dx dev
    ports (8080/8087), and both e2e suites pin
    `fts.signal-engine-ws-url` in localStorage to their scratch engine.
    The streamed/virtual wire shapes were kept anyway — proven, and
    resume-friendly.
  - **Planner** (`signal_sampler::pack_plan`, served by
    `signal-pack-library`, memoized per pack): rank 0 = the exact bytes
    `SignalPcmPack::open` reads — the 64-byte header plus the index span
    (`SignalPcmPack::index_span()`, new) with its embedded spec — TWO
    segments, since the index sits at the file end. Ranks 1.. are one
    audio entry each: velocity-layer distance from the middle layer
    first, then |key − 60| middle-out, then rr (0 first); zone mode reads
    the embedded spec's zones, convention mode parses filenames; unknown
    entries go last in file order; defensive `gap` segments keep coverage
    exact. Unit-tested natively (rank-0-only bytes literally `open_bytes`
    successfully; ordering; exact tiling).
  - **Client** (`web_packs.rs` + `web_keys_rig.rs`): packs ≤ 32 MB keep
    the W6 whole-file path; larger packs go progressive — plan + a plan
    LEDGER in IndexedDB (`plan:<name>.<variant>`, received-segment
    bitmap) + sparse bytes at true offsets in OPFS
    (`….signalpack.sparse`, commit-every-16MB, ledger marks segments only
    after a commit). Rank-0 fetches first, then
    `attach_pack_progressive` → the row turns `playable` ("playable —
    loading detail n%") and the lanes reload; detail segments stream in
    rank order over ONE reused connection, CONCURRENT with the small
    packs' whole-file streams. At 100%: whole-file sha256 (when known),
    rename into place, ordinary ready ledger — the next boot attaches it
    as a normal cached pack. Refresh-resume replays committed segments
    from the sparse file (contiguous runs, few reads) and fetches the
    rest.
  - **Worklet** (`keys_processor.js`): `this.packs` entries are either a
    whole Uint8Array (W6) or a sparse `{len, segs}` store fed by
    fire-and-forget `pack_segment {id, start, bytes}` (page-allocated ids
    from 2^20; exactly-adjacent segments coalesce). `__ftsPackRead`
    serves covered ranges (plan segments are whole entries, so
    single-segment hits are the norm), records uncovered ones in a
    bounded deduped miss list, and returns null — the engine drops that
    voice silently and retries next press (fts-sample never
    negative-caches a failed load, so NO reload is needed when the bytes
    arrive). `take_misses` drains the list; the page polls ~1 Hz and
    bumps the covering plan segments to the queue front.
  - **Resolution**: one rig-wide number — delivered sample segments over
    total sample segments across every referenced pack (rank-0 excluded;
    whole-file packs all-or-nothing). Prominent in the top bar
    (`data-testid="rig-resolution"`, amber `RESOLUTION n%` bar →
    quiet green `FULL RESOLUTION`), and `__ftsRig.resolution()`.
  - **e2e** (`keys_rig_progressive.spec.ts`, its own cleared context):
    piano playable < 30 s with bytes ≪ total; the HEADLINE — middle C
    sounds (other lanes muted via the new `lane-row-*`/`lane-mute-*`
    testids) while the pack is still `playable` and resolution < 100;
    an extreme note (24) silent at first then sounding within 60 s via
    the miss path; everything `ready`, resolution 100, FULL RESOLUTION
    rendered. The W6 suite runs unchanged before it.

- **W8 — the Audio panel: latency visibility + render-load tracing** —
  DONE.
  - **Render load, measured in the processor** (keys_processor.js):
    `this.renderer.render(...)` in `process()` is timed with the
    polyfill's Date.now-backed clock (~1 ms resolution vs a ~2.67 ms
    quantum — a single call's reading is 0-or-1 quantization noise), so
    the numbers are AGGREGATED: render ms summed over a 250-quantum
    window (~0.7 s @ 48 kHz) and divided by the window's AUDIO time
    (quanta × 128000/sampleRate ms) — never by wall time between
    `process()` calls, which includes the browser's callback pacing and
    would understate load. Per-window worst single-quantum cost (coarse
    but spike-catching) and a monotonic quantum counter ride along. All
    preallocated numbers — nothing allocates per quantum. New message
    `audio_stats { replyTo }` → `{ load, worstMs, quanta, sampleRate,
    voices }`.
  - **Voice count**: `SampleEngine::active_voices()` already existed
    (`VoicePool::active_count`, a `Vec::len`); W8 added the shallow walk
    up — `RenderNode::active_voices()` (sums `SamplerInstrument` leaves;
    synth backends keep private voice vecs and are not counted) and
    `KeysRig::active_voices()` (per-lane `edit_lane`, same seam as
    `warm_note`), exported as `KeysWorklet::activeVoices`.
  - **Latency, page-side**: `ctx.baseLatency` + `ctx.outputLatency` read
    via Reflect (the getters aren't in the app's web-sys feature set),
    refreshed by a dedicated 2 Hz `audio_stats` poll (the load window
    only turns over every ~0.7 s; 10 Hz would re-read the same window).
  - **latencyHint** (interactive | balanced | playback): stored in
    localStorage `fts.keys-latency-hint`, applied at `boot_worklet` via
    `AudioContextOptions` (the option object Reflect-set — the web-sys
    union setter shape varies by version; feature `AudioContextOptions`
    added for `new_with_context_options`). Changing it runs an IN-PAGE
    re-boot, not a reload: close the old context, drop the worklet,
    re-run `boot_rig` — cached lane program (IndexedDB) + OPFS packs
    mean no network. Poll loops carry the worklet signal and park
    themselves when superseded. (Known benign race: a pack attach still
    in flight from the old boot can stamp its row Ready in the new
    list — same pack, same totals.)
  - **Panel**: a top-bar "Audio" button (`audio-button`) opens
    `audio-popover`: sample rate, quantum, base/output/total latency ms,
    the render-load bar (`audio-load`; green < 60%, amber < 90%, red
    past), worst-quantum ms, voices (when ≥ 0), the hint selector
    (`audio-latency-hint`), quanta/uptime. `__ftsRig` gains
    `renderLoad()`, `latencyMs()`, `audioStats()` (JSON) — reading the
    thread-local hook state, so they survive the re-boot uninstalled.
  - **e2e**: the audio test asserts `renderLoad()` ∈ (0, 0.9) and
    `latencyMs() > 0` while the demo plays and screenshots the popover;
    a fourth serial test flips the hint to `playback` and proves the rig
    recovers (state, cached packs, `audioState() === 'running'`, a note
    still moves the master peak) — the re-boot path's regression test.

- **W9 — full UI parity: the page body is `signal_keys_ui::KeysRigRemote`**
  — DONE. The same remote component the desktop (`rig_view.rs`) and phone
  mount, backed by REAL `KeysRigClient`/`KeysRigStreamClient` vox clients
  — served locally, in-process, in the tab.
  - **Transport (choice (a))**: architect's `LocalServer` works on wasm —
    it carries its own in-memory `Link` (`architect/src/memory_link.rs`,
    wasm-only, built exactly for "the in-process browser engine's `!Send`
    backend") and the `#[architect::rpc]` derive relaxes backend bounds to
    `MaybeSendSync` (empty on wasm). So no seam in signal-keys-ui at all:
    `web_keys_backend.rs` implements the `KeysRig` trait +
    `KeysRigStreamSource` + `Services` (`layers![Service, StreamService]`,
    `CurrentThreadDispatcher`), `LocalServer::serve(into_router(), scope)`
    establishes the two typed clients, and the page provides them in
    context precisely as `rig_view.rs` does over the network. The UI
    cannot tell it isn't remote.
  - **`WebKeysBackend`** (apps/fasttrackstudio/src/web_keys_backend.rs):
    state page-side, audio in the worklet. Real: status (peaks/voices/ctx
    state), mixer (engines/lanes from the lane program; gains, mutes,
    solos with native solo semantics, master trim — dB → linear onto the
    worklet's folder/lane tracks), tree, engine order, trigger/pitch
    bend/mod wheel (raw MIDI to the worklet), `midi_ports`/`set_midi_port`
    (WebMIDI input selection, checked per message), `midi_recent` (a ring
    fed by the ONE `Worklet::midi` seam — WebMIDI, demo player, on-screen
    keys, `trigger`), start/stop (AudioContext resume/suspend),
    `lane_program_wire` (the cached program). Honest stubs (engine-side
    state only): preset library (ONE row — the resolved profile),
    `load_preset`/`set_layer_patch`/`set_layer_variant`/`clear_layer`
    (`last_error` says why), macros/Global Controls (empty lists — they
    drive engine DSP rebuilds), per-module gain/enable (lanes run one
    fused instrument in-tab), stacks (empty; `perform_mode` stored),
    drones. The full table is in the module docs.
  - **Events**: a local `PubSub::sliding(64)` hub. The 10 Hz peak poll is
    the meter pump (Status + queued Midi each tick); Mixer/Tree/Library/
    Perform publish on change — the UI's reactive paths light up as packs
    attach (`update_row` mirrors pack usability onto lane `live` flags).
  - **Chrome/layout**: the W3–W8 header (RESOLUTION, Soundsources popover
    incl. demo player, Audio panel) stays; `fts_chrome::provide_chrome()`
    + a `PanelRail` at the remote's right edge make the rig's Routing and
    MIDI-monitor panels openable; below the remote sits a compact footer
    with the on-screen octave and the **compat lane strip** — kept VISIBLE
    and functional (per-lane volume/mute now routed through the backend so
    the remote mixer and the strip stay coherent; strip state re-syncs
    from the backend every peak tick). All `lane-row-*`/`lane-mute-*`
    testids (and `data-pack-name`) live there unchanged; every other
    testid + the `__ftsRig` hook untouched.
  - **Clients survive the latency-hint re-boot**: the backend is stable;
    `install` refreshes its shared state, `set_worklet(None → new)` swaps
    the audio path, the mounted remote never re-establishes.
  - Verified: both e2e suites 8/8 green twice; a manual playwright probe
    confirms the remote mounts (perform strip, engine cards, patches) and
    that a mute clicked in the remote mixer round-trips through the
    in-process RPC to the worklet and back out to the strip.
  - Parity scope, per region: **full** — mixer zooms' faders/mutes/solos/
    meters, tree/routing panel, MIDI monitor + keyboard lights, perform
    mode buttons, on-screen input; **partial** — layer/engine zoom render
    with empty macro panels and one module slot (no Global Controls DSP
    in-tab), browser lists the single resolved profile; **absent by
    honest stub** — preset loading, stacks, drones, module editing (all
    engine-side state; the methods answer with current state and
    `last_error` text where a click would otherwise lie).

### W10 — full telemetry (DONE)

Every FTS process exports logs/traces/metrics via OTLP to the live
collector (the Task observability stack: collector → Tempo/Loki,
Grafana on top), the same `architect_telemetry` pattern as task-server.

- **The env contract**: export is doubly opt-in — the `otel` cargo
  feature on `architect-telemetry` carries the dependency weight, and at
  runtime nothing initializes unless `OTEL_EXPORTER_OTLP_ENDPOINT` is
  set. The exporter speaks **http/protobuf**, so the endpoint is the
  collector's HTTP port: `http://127.0.0.1:4318` (NOT gRPC 4317).
  Unsetting the var silently disables all export; console logs are
  unaffected. `RUST_LOG` keeps working exactly as before (EnvFilter with
  the same per-binary defaults).
- **Service names**: `fts-engine` (`fasttrackstudio --engine`, incl. the
  systemd `signal-engine` unit, which now sets the endpoint in its
  `Environment=`), `fts-app` (desktop GUI), `fts-patchbay`
  (`fts-patchbay`), `fts-cli` (`fts`). The name is passed to
  `architect_telemetry` at init and wins over `OTEL_SERVICE_NAME`.
- **Wide events**: architect's per-RPC span is the wide event
  (`TransportRpc/get_state`, `KeysRigSvc/lane_program_wire`, … appear as
  root traces in Tempo). Enrichments ride `architect_telemetry::wide::set`:
  the pack library sets `pack.name` / `pack.variant` /
  `pack.range_start` / `pack.range_len` / `pack.plan_cached`; the keys
  backend sets `keys.profile` / `keys.pack_count` / `keys.lane_count` on
  `lane_program_wire`.
- **Finding FTS in Grafana** (Explore):
  - Loki: `{service_name="fts-engine"}`
  - Tempo (TraceQL): `{ resource.service.name = "fts-engine" }`, e.g.
    `{ resource.service.name = "fts-engine" && span.pack.name != "" }`
- Browser-side telemetry (wasm page errors → engine) is a follow-up;
  `architect_telemetry` is no-op stubs on wasm today.

### W11 — field fixes: output latency, layout fit, the app shell, MIDI hot-plug (DONE)

Two tailnet field reports ("needs to be live playable, like a 256 buffer";
"dead space left + horizontal scroll on my screen") plus two mid-pass
addenda (use the app's real chrome; hot-plugged controllers never work).

- **Output latency**: the render path was already quantum-tight; the felt
  latency was the OUTPUT pipeline (observed base 13.7 ms + output
  32–45 ms under `interactive`). The hint set grew NUMERIC choices —
  `low` → `AudioContextOptions.latencyHint = 0.005` (seconds, double) and
  `ultra` → `0.0027` (~one quantum) — same Reflect path, same
  `fts.keys-latency-hint` key; the page DEFAULTS to `low` now
  (`interactive` proved conservative). The Audio panel keeps the achieved
  base/output readout and adds a one-line tip when outputLatency > 20 ms
  (`audio-latency-tip`): "launch the browser with
  `--audio-buffer-size=256`" — past that point the browser's output
  buffer is the wall and only launching it smaller moves it. (A
  sampleRate-vs-device mismatch readout was skipped — not cheaply
  detectable.) Note for the e2e suite: under `low` the browser wakes the
  worklet per 1–2 quanta instead of batches, and the processor's ~1 ms
  clock rounds isolated calls up — `renderLoad()` reads 0.8–1.1 on a
  healthy box, so the load assertion is `< 1.5` (runaway CPU still reads
  several×).
- **Layout**: `KeysRigRemote`'s natural width is ~2360–2420 px (the
  mixer's engine-card band: `flex: 0 0 auto` cards whose min-content
  escapes through flex ancestors, so the page grew a horizontal scrollbar
  on anything narrower and the body scrolled the rig off the left edge).
  Fix is WEB-SIDE ONLY: a `ScaleToFit` wrapper (`#keys-fit-outer` /
  `#keys-fit-inner`) measures the unscaled layout via `scrollWidth`
  (transform-independent) against the frame's room and applies
  `transform: scale(room/natural)` with `transform-origin: top left`,
  inner width pinned at the natural width and inner height grown by 1/s
  so the scaled box exactly fills the outer (overflow hidden — the page
  NEVER scrolls sideways). Wider-than-natural viewports keep scale 1 and
  the fluid layout fills edge to edge. Two traps encoded in comments:
  style declarations are patched per-property, so BOTH branches must name
  the transform (a stale `scale(0.52)` survived the wide branch until
  made symmetric); and content growth widens `scrollWidth` without
  resizing any observed box, so a 1 s `setInterval` backs up the
  ResizeObserver.
- **The app shell**: the page's hand-rolled top bar is gone. The page
  mounts `fts_chrome::AppFrame` + `TopBar` + `IconRail` + `PanelRail` —
  the SAME shell the desktop app builds in main.rs — and registers its
  widgets through the chrome seams: crumbs `Keys ▸ {profile}`; the
  Resolution pill (`rig-resolution`, amber `RESOLUTION n%` → green
  `FULL RESOLUTION`) and the master meter as status items; Soundsources
  (incl. demo player + MIDI section) and Audio as PANELS on the right
  rail beside the rig's own Routing/MIDI. Two additive seams were added
  to fts-chrome for the e2e contract: `PanelSpec::testid(..)` (a
  `data-testid` on the rail's toggle button — `ssm-button` /
  `audio-button` live there now) and `StatusItem::tagged(testid, item)`
  (a wrapping variant rendered as a testid'd span). Publishing happens
  from a LEAF component (`KeysChrome`) so the 10 Hz meter tick re-renders
  only it and the TopBar, not the whole page tree. Every prior testid and
  the `__ftsRig` hook survive.
- **WebMIDI hot-plug**: `init_webmidi` used to enumerate once — a
  controller connected after page load was invisible. Now the MIDIAccess
  is kept, `statechange` re-walks the live input map (new ports get the
  forwarding handler, gone ports drop out; the raw JS closure only pokes
  an mpsc channel — the re-walk and Signal writes run in the spawned
  task, inside the runtime). A rejected `requestMIDIAccess` is SAID:
  "MIDI permission blocked — allow it in the address bar"
  (`midi-status`), distinct from "no MIDI devices". The port gate
  defaults to OMNI (`midi_port: None`). e2e proofs (no hardware in CI):
  `__ftsRig.midiInputs()` / `midiHotplugArmed()` / `midiOmni()`; note
  this chromium only grants Web MIDI with BOTH playwright permissions
  `['midi', 'midi-sysex']`.
- **e2e**: the latency test also flips through `low` (stored pref +
  full recovery); new serial tests assert the WebMIDI plumbing and the
  viewport fit at 1366×768 and 1920×1080 (no horizontal overflow,
  `#keys-fit-inner` starts inside the viewport and fits it).

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

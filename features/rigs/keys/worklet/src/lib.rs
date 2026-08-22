//! Browser keys rig — the AudioWorklet entry.
//!
//! Wraps daw-standalone's [`WebRenderer`] (the wasm worklet renderer) and
//! signal-sampler's headless [`KeysRig`]: the rig's `Hosting::Lanes`
//! topology (rig folder → engine folders → layer tracks, one
//! [`KeysInstrument`] per lane) is seeded into the SAME `Standalone` the
//! worklet renders, packs arrive as transferred bytes
//! ([`signal_sampler::pack_registry`]), and live MIDI rides the wasm
//! live-MIDI queue (`Standalone::push_live_midi` → per-block drain).
//!
//! This crate exists because the dependency arrow is one-way —
//! signal-sampler depends on the `daw` facade (which contains
//! daw-standalone), so daw-standalone can never depend on signal-sampler.
//! The worklet-with-keys entry lives here, on top of both.
//!
//! [`WebRenderer`]: daw_standalone::audio_engine::web::WebRenderer
//! [`KeysRig`]: signal_sampler::KeysRig
//! [`KeysInstrument`]: signal_sampler::KeysInstrument

// ── The LaneProgram wire shape ──────────────────────────────────────────────
//
// The Facet mirror (`WireProgram` and friends) lives in
// `signal_sampler::keys_rig` — the one crate both this worklet and the
// engine backend (signal-keys, which SERIALIZES the same JSON from
// `KeysRig::lane_program_wire`) depend on. Re-exported so existing
// consumers keep their import path.

pub use signal_sampler::keys_rig::{WireEngine, WireLayer, WireProgram};

/// Parse a lane program from its JSON wire form (the payload of the
/// worklet's `open_lanes` message — the engine's `lane_program_wire` RPC
/// produces it).
pub fn wire_program_from_json(text: &str) -> eyre::Result<WireProgram> {
    facet_json::from_str(text).map_err(|e| eyre::eyre!("lane program JSON: {e}"))
}

// ── The wasm worklet entry ──────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use wasm_bindgen::prelude::*;

    use daw_standalone::audio_engine::web::WebRenderer;
    use signal_sampler::KeysRig;
    use signal_sampler::keys_rig::LaneProgram;

    use crate::wire_program_from_json;

    fn js_err(e: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&e.to_string())
    }

    /// Install fts-sample's process-wide external pack reader, bridged to
    /// the processor's `globalThis.__ftsPackRead(id, offset, len)` (defined
    /// once in keys_processor.js over its JS-held pack buffers). The
    /// closure captures NOTHING (it re-resolves the hook per call), so it
    /// satisfies the reader's `Send + Sync` bound — the worklet scope is
    /// single-threaded anyway.
    fn install_external_reader_once() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            signal_sampler::engine::cache::set_external_pack_reader(Box::new(
                |id, offset, dst| {
                    use wasm_bindgen::JsCast as _;
                    let Ok(hook) =
                        js_sys::Reflect::get(&js_sys::global(), &"__ftsPackRead".into())
                    else {
                        return false;
                    };
                    let Ok(hook) = hook.dyn_into::<js_sys::Function>() else {
                        return false;
                    };
                    let Ok(value) = hook.call3(
                        &JsValue::NULL,
                        &JsValue::from_f64(f64::from(id)),
                        &JsValue::from_f64(offset as f64),
                        &JsValue::from_f64(dst.len() as f64),
                    ) else {
                        return false;
                    };
                    let Ok(bytes) = value.dyn_into::<js_sys::Uint8Array>() else {
                        return false; // null / undefined: id unknown or range refused
                    };
                    if bytes.length() as usize != dst.len() {
                        return false;
                    }
                    bytes.copy_to(dst);
                    true
                },
            ));
        });
    }

    /// The keys-rig worklet: a [`WebRenderer`] plus (once `openLanes` ran) a
    /// headless [`KeysRig`] seeded into the same `Standalone`. Construct one
    /// per `AudioWorkletProcessor` on the worklet thread; `RefCell` is fine —
    /// the AudioWorkletGlobalScope is single-threaded.
    #[wasm_bindgen]
    pub struct KeysWorklet {
        renderer: WebRenderer,
        rig: RefCell<Option<KeysRig>>,
        /// The last opened program — kept so `reloadLanes` (after a late
        /// `attachPack`) can re-install instruments as a same-shape,
        /// glitch-free per-track swap.
        program: RefCell<Option<LaneProgram>>,
        /// `(layer, sample path)` pairs a note-on needed but found
        /// non-resident — drained by the processor (`takeWarmRequests`) and
        /// shipped to the page's decoder worker. Deduped; bounded (a held
        /// chord retrying cold keys must not grow this without limit).
        warm_out: RefCell<Vec<(String, String)>>,
        /// Partially-received PCM from `insertPcmChunk`, keyed
        /// `layer\u{1}path` — a sample is published only once its last
        /// piece lands, so no single audio-thread call copies more than a
        /// chunk.
        pending_pcm: RefCell<std::collections::HashMap<String, Vec<f32>>>,
        /// Monotonic counters for the audio panel: samples inserted via
        /// `insertPcm`, and inserts refused by the budget ceiling.
        pcm_inserted: std::cell::Cell<u32>,
        pcm_refused: std::cell::Cell<u32>,
        /// Lanes rebuilt by the last scoped reload, and how many times that
        /// scoping matched NOTHING and fell back to the full rebuild.
        reload_lanes: std::cell::Cell<u32>,
        reload_full: std::cell::Cell<u32>,
        /// Zones handed to the streamer workers to open (W13's shared-memory
        /// path) — the counterpart of `pcm_inserted` on the copy path.
        opens_queued: std::cell::Cell<u32>,
    }

    thread_local! {
        /// The last Rust panic's message (see the hook in `KeysWorklet::new`).
        static LAST_PANIC: std::cell::RefCell<String> = std::cell::RefCell::default();
    }

    /// The most recent panic message, for the processor's error replies —
    /// after a panic the failing call surfaces only as
    /// `RuntimeError: unreachable`; this names it.
    #[wasm_bindgen(js_name = lastPanicMessage)]
    pub fn last_panic_message() -> String {
        LAST_PANIC.with(|p| p.borrow().clone())
    }

    // ── Streamer threads (W13) ──────────────────────────────────────────
    //
    // With shared memory these run in page-spawned Web Workers over the
    // SAME wasm heap the audio thread renders from, which is what makes
    // them the browser's version of the native streamer pool: a worker
    // decodes a chunk straight into memory the audio thread already reads.
    // No postMessage, no copy, nothing for the audio thread to do.

    /// Whether this build has the thread ABI (shared memory + atomics). The
    /// page checks it to decide between spawning streamer workers and
    /// falling back to the single-threaded decoder-worker protocol.
    #[wasm_bindgen(js_name = threadsAvailable)]
    pub fn threads_available() -> bool {
        cfg!(target_feature = "atomics")
    }

    /// Byte address of the word a streamer worker parks on, and its current
    /// value. The worker calls `Atomics.wait(i32View, addr/4, value, ms)`
    /// and then [`streamerDrain`](streamer_drain) — parking lives in JS
    /// because Rust's wait/notify intrinsics are nightly-only and this code
    /// is shared with stable native builds (see `stream_wasm`).
    #[wasm_bindgen(js_name = streamerWakeAddr)]
    pub fn streamer_wake_addr() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            signal_sampler::engine::stream_wasm::wake_addr()
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    #[wasm_bindgen(js_name = streamerWakeValue)]
    pub fn streamer_wake_value() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            signal_sampler::engine::stream_wasm::wake_value()
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    /// One non-blocking drain of the streamer queue — the fallback for
    /// hosts that cannot park (and a test seam). Returns samples filled.
    #[wasm_bindgen(js_name = streamerDrain)]
    pub fn streamer_drain() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            // Free what the audio thread retired. It cannot free itself —
            // the allocator lock is shared with these workers, and the
            // worklet thread is realtime priority (see `built_lanes`).
            let reaped = signal_sampler::built_lanes::reap() as u32;
            signal_sampler::engine::stream_wasm::drain() as u32 + reaped
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    /// Streamer queue depth / samples dropped because it was full — the
    /// "are the decoders keeping up?" numbers for the audio panel.
    #[wasm_bindgen(js_name = streamerDepth)]
    pub fn streamer_depth() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            signal_sampler::engine::stream_wasm::depth() as u32
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    /// Zones OPENED by the streamer workers since boot — the shared-memory
    /// path's throughput number (`pcmInserts` counts the copy path).
    #[wasm_bindgen(js_name = streamerOpened)]
    pub fn streamer_opened() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            signal_sampler::engine::stream_wasm::opened() as u32
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    /// Open jobs a worker dequeued but could not complete, and the open
    /// ring's depth. `opened == 0 && failed == 0 && depth > 0` means no
    /// worker is draining at all; failures climbing means they are running
    /// but cannot do the work.
    #[wasm_bindgen(js_name = streamerOpenFailed)]
    pub fn streamer_open_failed() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            signal_sampler::engine::stream_wasm::open_failed() as u32
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    #[wasm_bindgen(js_name = streamerOpenDepth)]
    pub fn streamer_open_depth() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            signal_sampler::engine::stream_wasm::open_depth() as u32
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    #[wasm_bindgen(js_name = streamerDropped)]
    pub fn streamer_dropped() -> u32 {
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            signal_sampler::engine::stream_wasm::dropped() as u32
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        0
    }

    #[wasm_bindgen]
    impl KeysWorklet {
        /// `sample_rate` is the worklet's actual `sampleRate`.
        #[wasm_bindgen(constructor)]
        pub fn new(sample_rate: u32) -> KeysWorklet {
            // Panics otherwise surface as a bare `RuntimeError: unreachable`
            // with no message — route them to the worklet console so a
            // failure names itself.
            {
                use std::sync::Once;
                static HOOK: Once = Once::new();
                HOOK.call_once(|| {
                    std::panic::set_hook(Box::new(|info| {
                        // Stash the message where `lastPanicMessage()` can
                        // fetch it — the worklet scope's console is invisible
                        // to page-side tooling, and the trap that follows a
                        // panic reads as a bare `RuntimeError: unreachable`.
                        LAST_PANIC.with(|p| *p.borrow_mut() = format!("{info}"));
                        // Also onto the worklet scope's globalThis: after the
                        // panic the whole wasm instance traps, so JS is the
                        // only place the message survives.
                        let _ = js_sys::Reflect::set(
                            &js_sys::global(),
                            &"__ftsPanic".into(),
                            &format!("{info}").into(),
                        );
                        use wasm_bindgen::JsCast as _;
                        let _ = js_sys::eval("console").ok().and_then(|c| {
                            let f = js_sys::Reflect::get(&c, &"error".into()).ok()?;
                            let f: js_sys::Function = f.dyn_into().ok()?;
                            f.call1(&c, &format!("keys-worklet panic: {info}").into())
                                .ok()
                        });
                    }));
                });
            }
            KeysWorklet {
                renderer: WebRenderer::new(sample_rate),
                rig: RefCell::new(None),
                program: RefCell::new(None),
                warm_out: RefCell::new(Vec::new()),
                pending_pcm: RefCell::new(std::collections::HashMap::new()),
                pcm_inserted: std::cell::Cell::new(0),
                pcm_refused: std::cell::Cell::new(0),
                reload_lanes: std::cell::Cell::new(0),
                reload_full: std::cell::Cell::new(0),
                opens_queued: std::cell::Cell::new(0),
            }
        }

        /// Resolve what a note-on needs and get it opened OFF this thread.
        ///
        /// With shared memory (W13) the streamer workers open zones
        /// directly into the caches this rig reads — one enqueue per
        /// missing zone and nothing crosses a thread boundary. Without it
        /// (single-threaded build, or a page that is not cross-origin
        /// isolated) the paths go to the decoder worker instead, which
        /// decodes in its own heap and ships PCM back as copies.
        fn queue_warm_requests(&self, rig: &KeysRig, note: u8, velocity: u8) {
            // BOTH paths, deliberately — they are not yet interchangeable.
            //
            // The shared-memory path enqueues the zone for a streamer
            // worker, which is where this is going. But it cannot COMPLETE
            // yet: pack bytes live on the WORKLET's JS heap (W6
            // attach-by-handle, reachable only through the
            // `__ftsPackRead` hook installed in this scope), so a worker
            // sharing the wasm heap still cannot read them and its
            // `cache.get` fails. Until pack bytes move to a
            // SharedArrayBuffer every thread can read, the decoder worker
            // remains the path that actually makes a cold note sound.
            //
            // Taking only the new path made far-out cold notes silent —
            // enqueued, never opened. Never trade a working path for an
            // unfinished one.
            #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
            {
                let queued = rig.queue_note_opens(note, velocity);
                self.opens_queued
                    .set(self.opens_queued.get().wrapping_add(queued as u32));
            }
            self.queue_warm_requests_via_worker(rig, note, velocity);
        }

        /// Hand the paths to the page's decoder worker (deduped, bounded at
        /// 64 pending). Still the path that actually completes — see the
        /// note in `queue_warm_requests`.
        fn queue_warm_requests_via_worker(&self, rig: &KeysRig, note: u8, velocity: u8) {
            let missing = rig.missing_note_samples(note, velocity);
            if missing.is_empty() {
                return;
            }
            let mut out = self.warm_out.borrow_mut();
            for (layer, path) in missing {
                if out.len() >= 64 {
                    break;
                }
                let path = path.to_string_lossy().into_owned();
                if !out.iter().any(|(l, p)| *l == layer && *p == path) {
                    out.push((layer, path));
                }
            }
        }

        /// Install a `.signalpack`'s bytes under the spec-path key the lane
        /// trees reference (`sample_block(_, key)`). Call before `openLanes`
        /// for lanes to sound immediately; a pack attached later needs one
        /// `reloadLanes` to reach the running instruments.
        ///
        /// This copies the whole pack into wasm linear memory — kept for
        /// compatibility; the processor now attaches by handle instead
        /// ([`attach_pack_external`](Self::attach_pack_external)).
        #[wasm_bindgen(js_name = attachPack)]
        pub fn attach_pack(&self, key: &str, bytes: &[u8]) -> Result<(), JsValue> {
            signal_sampler::pack_registry::install(key, bytes.to_vec()).map_err(js_err)
        }

        /// Install a pack whose BYTES STAY ON THE JS HEAP, outside wasm
        /// linear memory: the processor keeps the transferred buffer in its
        /// own `Map` keyed by `id` and serves ranged reads through
        /// `globalThis.__ftsPackRead(id, offset, len) → Uint8Array | null`.
        /// This is what lets every Worship pack attach — multi-GB packs cost
        /// the 4 GB wasm address space nothing; only decoded PCM (bounded by
        /// the wasm preload budget) lives in linear memory.
        #[wasm_bindgen(js_name = attachPackExternal)]
        pub fn attach_pack_external(&self, key: &str, id: u32, len: f64) -> Result<(), JsValue> {
            install_external_reader_once();
            signal_sampler::pack_registry::install_external(key, id, len as u64)
                .map_err(js_err)
        }

        /// Build the headless keys rig from a lane-program JSON (see
        /// `WireProgram`), select its project, and roll the transport so
        /// `render` produces the lanes. Returns the number of layer lanes.
        #[wasm_bindgen(js_name = openLanes)]
        pub fn open_lanes(&self, program_json: &str) -> Result<u32, JsValue> {
            let program = wire_program_from_json(program_json)
                .map_err(js_err)?
                .into_lane_program();
            let lanes: u32 = program
                .engines
                .iter()
                .map(|e| e.layers.len() as u32)
                .sum();
            let rig = KeysRig::open_headless_on(
                self.renderer.standalone(),
                self.renderer.output_sample_rate(),
                &program,
            )
            .map_err(js_err)?;
            self.renderer.select_project(rig.project_guid());
            self.renderer.play();
            *self.rig.borrow_mut() = Some(rig);
            *self.program.borrow_mut() = Some(program);
            Ok(lanes)
        }

        /// Re-install the current program's lane instruments — same shape ⇒
        /// glitch-free per-track swap. Call after `attachPack` landed a pack
        /// the running lanes were still missing.
        #[wasm_bindgen(js_name = reloadLanes)]
        pub fn reload_lanes(&self) -> Result<(), JsValue> {
            let program = self.program.borrow();
            let Some(program) = program.as_ref() else {
                return Err(js_err("no lane program open"));
            };
            let mut rig = self.rig.borrow_mut();
            let Some(rig) = rig.as_mut() else {
                return Err(js_err("no lane program open"));
            };
            rig.load_lanes(program).map_err(js_err)
        }

        /// Re-install ONLY the lanes that reference `key` — the bounded
        /// alternative to `reloadLanes` after a pack attach. Rebuilding all
        /// nine Worship lanes measured >500 ms ON THE AUDIO THREAD (this
        /// handler runs there), which stalled the render after every pack
        /// landed; a pack only affects the lanes that name it. Returns how
        /// many lanes were rebuilt.
        #[wasm_bindgen(js_name = reloadLanesForPack)]
        pub fn reload_lanes_for_pack(&self, key: &str) -> Result<u32, JsValue> {
            let program = self.program.borrow();
            let Some(program) = program.as_ref() else {
                return Err(js_err("no lane program open"));
            };
            let mut rig = self.rig.borrow_mut();
            let Some(rig) = rig.as_mut() else {
                return Err(js_err("no lane program open"));
            };
            let n = rig.reload_lanes_for_pack(program, key);
            if n == 0 {
                // No lane claims this pack key. Either the pack genuinely
                // belongs to no lane, or the key the page attached under is
                // not spelled the way the lane trees spell it — and in that
                // second case a scoped reload would leave every instrument
                // without its samples, i.e. a SILENT rig. Correctness first:
                // fall back to the full reload (the slow path this scoping
                // exists to avoid) and record it, so `reloadFull` in the
                // audio stats says plainly that the fast path never applied.
                self.reload_full.set(self.reload_full.get().wrapping_add(1));
                rig.load_lanes(program).map_err(js_err)?;
            }
            self.reload_lanes.set(n as u32);
            Ok(n as u32)
        }

        /// Lanes rebuilt by the last scoped reload (0 ⇒ the key matched
        /// nothing and the full reload ran — see `pcmReloadFull`).
        #[wasm_bindgen(js_name = reloadLaneCount)]
        pub fn reload_lane_count(&self) -> u32 {
            self.reload_lanes.get()
        }

        /// How many times the scoped reload fell back to rebuilding every
        /// lane. Should be 0; anything else means the pack keys and the lane
        /// trees disagree about how a library is named.
        #[wasm_bindgen(js_name = reloadFullCount)]
        pub fn reload_full_count(&self) -> u32 {
            self.reload_full.get()
        }

        /// Render one block into the worklet's output channels.
        pub fn render(&self, out_left: &mut [f32], out_right: &mut [f32]) {
            self.renderer.render(out_left, out_right);
        }

        // ── Live MIDI (dispatched to every layer track — each lane's zone
        // filters its own notes, exactly like the native rig) ────────────

        #[wasm_bindgen(js_name = noteOn)]
        pub fn note_on(&self, key: u8, velocity: u8) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                // NEVER decode here: in an AudioWorkletGlobalScope this
                // message handler runs on the audio rendering thread, and a
                // synchronous ogg decode starves `process()` — every
                // sounding voice drops out (the W12 field bug). Instead,
                // resolve what the note needs and queue the misses for the
                // page's decoder worker; the render drops just THIS voice
                // until its PCM arrives via `insertPcm`, then the next
                // press sounds.
                self.queue_warm_requests(rig, key, velocity);
                rig.note_on(key, velocity);
            }
        }

        #[wasm_bindgen(js_name = noteOff)]
        pub fn note_off(&self, key: u8) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.note_off(key);
            }
        }

        pub fn cc(&self, controller: u8, value: u8) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.cc(controller, value);
            }
        }

        /// Pitch wheel (14-bit raw, 8192 = center).
        #[wasm_bindgen(js_name = pitchBend)]
        pub fn pitch_bend(&self, raw: u16) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.pitch_bend(raw);
            }
        }

        /// Raw 3-byte MIDI (e.g. straight from WebMIDI) to every lane.
        pub fn midi(&self, status: u8, data1: u8, data2: u8) {
            // Note-on (velocity > 0): queue any missing samples for the
            // decoder worker — never decode on this thread; see `noteOn`.
            if status & 0xf0 == 0x90 && data2 > 0 {
                if let Some(rig) = self.rig.borrow().as_ref() {
                    self.queue_warm_requests(rig, data1, data2);
                }
            }
            self.renderer.midi_to_all_tracks(status, data1, data2);
        }

        /// All Notes Off (CC 123).
        #[wasm_bindgen(js_name = allNotesOff)]
        pub fn all_notes_off(&self) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.all_notes_off();
            }
        }

        /// Panic — All Sound Off (CC 120).
        pub fn panic(&self) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.panic();
            }
        }

        // ── Transport (the lane project rolls after `openLanes`; these
        // mirror the plain worklet's messages) ───────────────────────────

        pub fn play(&self) {
            self.renderer.play();
        }
        pub fn pause(&self) {
            self.renderer.pause();
        }
        pub fn stop(&self) {
            self.renderer.stop();
        }

        // ── Track mixer (indices follow `trackPeaks` order: rig folder
        // first, then engines/layers in project order) ───────────────────

        /// Set one project track's volume (linear, 1.0 = unity).
        #[wasm_bindgen(js_name = setTrackVolume)]
        pub fn set_track_volume(&self, index: u32, volume: f64) {
            self.renderer.set_track_volume(index, volume);
        }

        /// Mute / unmute one project track.
        #[wasm_bindgen(js_name = setTrackMute)]
        pub fn set_track_mute(&self, index: u32, muted: bool) {
            self.renderer.set_track_mute(index, muted);
        }

        // ── Read-side ────────────────────────────────────────────────────

        /// Post-fader peaks per project track (rig folder first, then
        /// engines/layers in project order) — read off the live meter bank.
        #[wasm_bindgen(js_name = trackPeaks)]
        pub fn track_peaks(&self) -> Vec<f32> {
            self.renderer.track_peaks()
        }

        /// The keys of every installed in-memory pack.
        #[wasm_bindgen(js_name = installedPacks)]
        pub fn installed_packs(&self) -> Vec<JsValue> {
            signal_sampler::pack_registry::keys()
                .into_iter()
                .map(|k| JsValue::from_str(&k))
                .collect()
        }

        /// Whether a lane program is open.
        #[wasm_bindgen(js_name = isOpen)]
        pub fn is_open(&self) -> bool {
            self.rig.borrow().is_some()
        }

        /// Live-MIDI events queued but not yet rendered. 0 (or a couple)
        /// is healthy; a depth that stays up means pressed notes are
        /// waiting on the renderer and will arrive late.
        #[wasm_bindgen(js_name = midiQueueDepth)]
        pub fn midi_queue_depth(&self) -> u32 {
            self.renderer.standalone().live_midi_depth() as u32
        }

        /// WORKER SIDE: compile the lanes that play `key`, off the audio
        /// thread, and publish them for the worklet to install.
        ///
        /// This is the 62 ms that used to run in `reload_lanes` ON the
        /// audio thread. The worker needs no rig — only the program — so
        /// it takes the program JSON directly.
        #[cfg(target_feature = "atomics")]
        #[wasm_bindgen(js_name = buildLanesForPack)]
        pub fn build_lanes_for_pack(
            program_json: &str,
            key: &str,
            sample_rate: u32,
        ) -> Result<u32, JsValue> {
            let program = wire_program_from_json(program_json)
                .map_err(js_err)?
                .into_lane_program();
            Ok(KeysRig::build_lanes_for_pack(&program, key, sample_rate) as u32)
        }

        /// Single-threaded build: there is no worker to compile on, so the
        /// caller keeps using `reloadLanesForPack` on the audio thread.
        #[cfg(not(target_feature = "atomics"))]
        #[wasm_bindgen(js_name = buildLanesForPack)]
        pub fn build_lanes_for_pack(
            _program_json: &str,
            _key: &str,
            _sample_rate: u32,
        ) -> Result<u32, JsValue> {
            Ok(0)
        }

        /// AUDIO SIDE: install whatever a worker has finished compiling.
        /// Cheap (`begin_swap` is two moves) and GAPLESS — the tree being
        /// replaced keeps sounding until its voices release.
        #[wasm_bindgen(js_name = installBuiltLanes)]
        pub fn install_built_lanes(&self) -> u32 {
            #[cfg(target_feature = "atomics")]
            {
                self.rig
                    .borrow()
                    .as_ref()
                    .map(|rig| rig.install_built_lanes() as u32)
                    .unwrap_or(0)
            }
            #[cfg(not(target_feature = "atomics"))]
            0
        }

        /// Whether any compiled lane is waiting (a single atomic load, so
        /// the processor can check it every quantum).
        #[wasm_bindgen(js_name = hasBuiltLanes)]
        pub fn has_built_lanes(&self) -> bool {
            #[cfg(target_feature = "atomics")]
            {
                signal_sampler::built_lanes::has_pending()
            }
            #[cfg(not(target_feature = "atomics"))]
            false
        }

        /// Lanes compiled off-thread / installed, since boot.
        #[wasm_bindgen(js_name = lanesBuilt)]
        pub fn lanes_built(&self) -> u32 {
            #[cfg(target_feature = "atomics")]
            {
                signal_sampler::built_lanes::built() as u32
            }
            #[cfg(not(target_feature = "atomics"))]
            0
        }

        #[wasm_bindgen(js_name = lanesInstalled)]
        pub fn lanes_installed(&self) -> u32 {
            #[cfg(target_feature = "atomics")]
            {
                signal_sampler::built_lanes::installed() as u32
            }
            #[cfg(not(target_feature = "atomics"))]
            0
        }

        /// Drop every queued live-MIDI event and silence what is sounding.
        ///
        /// Called when the renderer notices it has been away (a big
        /// `currentFrame` jump): the queue then holds notes the player
        /// pressed seconds ago, and playing them now is worse than not
        /// playing them at all. Returns how many were discarded.
        #[wasm_bindgen(js_name = flushMidiQueue)]
        pub fn flush_midi_queue(&self) -> u32 {
            let dropped = self.renderer.standalone().flush_live_midi() as u32;
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.all_notes_off();
            }
            dropped
        }

        /// Notes the audio thread dropped for want of a resident sample —
        /// pressed keys that made NO sound.
        #[wasm_bindgen(js_name = notesDropped)]
        pub fn notes_dropped(&self) -> u32 {
            signal_sampler::engine::notes_dropped() as u32
        }

        /// Voices currently alive across every lane's sampler sources
        /// (0 before `openLanes`) — polled by the processor's `audio_stats`
        /// reply for the page's audio panel. Cheap: a `Vec::len` per
        /// sampler leaf, single-threaded in the worklet scope.
        #[wasm_bindgen(js_name = activeVoices)]
        pub fn active_voices(&self) -> u32 {
            self.rig
                .borrow()
                .as_ref()
                .map(|rig| rig.active_voices() as u32)
                .unwrap_or(0)
        }

        // ── The decoder-worker seam (W12) ────────────────────────────────
        //
        // The worklet side NEVER decodes: note-ons queue `(layer, path)`
        // warm requests (drained below), the page's decoder WORKER — a
        // second instance of this same wasm module in a plain Worker —
        // decodes them off-thread and ships raw PCM back in through
        // `insertPcm`. The worker uses `decodePathPcm`/`coveragePaths` on
        // ITS instance; the worklet uses the rest on its own.

        /// Whether any warm requests are queued — checked per quantum by
        /// the processor, so it must stay allocation-free.
        #[wasm_bindgen(js_name = hasWarmRequests)]
        pub fn has_warm_requests(&self) -> bool {
            !self.warm_out.borrow().is_empty()
        }

        /// Drain the queued warm requests as a JS array of
        /// `{ layer, path }` objects.
        #[wasm_bindgen(js_name = takeWarmRequests)]
        pub fn take_warm_requests(&self) -> js_sys::Array {
            let out = js_sys::Array::new();
            for (layer, path) in self.warm_out.borrow_mut().drain(..) {
                let o = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&o, &"layer".into(), &layer.into());
                let _ = js_sys::Reflect::set(&o, &"path".into(), &path.into());
                out.push(&o);
            }
            out
        }

        /// Number of pending warm requests (diagnostic).
        #[wasm_bindgen(js_name = warmQueueDepth)]
        pub fn warm_queue_depth(&self) -> u32 {
            self.warm_out.borrow().len() as u32
        }

        /// Insert PCM the decoder worker produced: `pcm` is interleaved
        /// f32 (`num_frames × channels`). Returns whether the lane accepted
        /// it — `false` with `charge_past_ceiling: false` means the
        /// decoded-PCM budget is full (background fill should pause).
        ///
        /// Cost on the audio thread: one memcpy of the PCM across the
        /// wasm-bindgen boundary plus a map insert — no decode. A typical
        /// entry is 1–10 MB (≲ 1 ms); the worst-quantum stat will show it.
        #[wasm_bindgen(js_name = insertPcm)]
        #[allow(clippy::too_many_arguments)]
        pub fn insert_pcm(
            &self,
            layer: &str,
            path: &str,
            channels: u16,
            sample_rate: u32,
            pcm: &[f32],
            charge_past_ceiling: bool,
        ) -> bool {
            let num_frames = if channels == 0 { 0 } else { pcm.len() / channels as usize };
            let data = std::sync::Arc::new(
                signal_sampler::engine::cache::SampleData::from_f32(
                    pcm.to_vec(),
                    channels,
                    sample_rate,
                    num_frames,
                ),
            );
            let accepted = self
                .rig
                .borrow()
                .as_ref()
                .map(|rig| {
                    rig.insert_decoded(
                        layer,
                        std::path::Path::new(path),
                        data,
                        charge_past_ceiling,
                    )
                })
                .unwrap_or(false);
            if accepted {
                self.pcm_inserted.set(self.pcm_inserted.get().wrapping_add(1));
            } else {
                self.pcm_refused.set(self.pcm_refused.get().wrapping_add(1));
            }
            accepted
        }

        /// Insert PCM in BOUNDED PIECES. A whole decoded sample can be tens
        /// of MB, and copying that across the wasm boundary measured 28–34 ms
        /// on the audio thread — ten-plus render quanta, i.e. the very
        /// dropout this design exists to prevent. The worker therefore sends
        /// ~1 MB pieces: each handler call costs a fraction of a quantum,
        /// they accumulate OFF the render path (a plain Vec grow), and only
        /// the final piece publishes the sample to the engine.
        ///
        /// `offset` is in f32 samples and must match what has accumulated so
        /// far; a mismatch drops the partial (the worker re-sends).
        #[wasm_bindgen(js_name = insertPcmChunk)]
        #[allow(clippy::too_many_arguments)]
        pub fn insert_pcm_chunk(
            &self,
            layer: &str,
            path: &str,
            channels: u16,
            sample_rate: u32,
            offset: u32,
            pcm: &[f32],
            is_last: bool,
            charge_past_ceiling: bool,
        ) -> bool {
            let key = format!("{layer}\u{1}{path}");
            let mut pending = self.pending_pcm.borrow_mut();
            let buf = pending.entry(key.clone()).or_default();
            if buf.len() != offset as usize {
                // Out of order / a resend after a drop: restart cleanly.
                if offset != 0 {
                    pending.remove(&key);
                    return false;
                }
                buf.clear();
            }
            buf.extend_from_slice(pcm);
            if !is_last {
                return true;
            }
            let frames = pending.remove(&key).unwrap_or_default();
            drop(pending);
            let num_frames = if channels == 0 { 0 } else { frames.len() / channels as usize };
            let data = std::sync::Arc::new(
                signal_sampler::engine::cache::SampleData::from_f32(
                    frames,
                    channels,
                    sample_rate,
                    num_frames,
                ),
            );
            let accepted = self
                .rig
                .borrow()
                .as_ref()
                .map(|rig| {
                    rig.insert_decoded(
                        layer,
                        std::path::Path::new(path),
                        data,
                        charge_past_ceiling,
                    )
                })
                .unwrap_or(false);
            if accepted {
                self.pcm_inserted.set(self.pcm_inserted.get().wrapping_add(1));
            } else {
                self.pcm_refused.set(self.pcm_refused.get().wrapping_add(1));
            }
            accepted
        }

        /// Set the decoded-PCM ceiling in MB (0 = unlimited). Must be called
        /// BEFORE anything decodes — the processor calls it immediately after
        /// construction, from the page's stored preference. Returns whether
        /// it took (a later call is ignored; see `budget::set_limit_mb`).
        #[wasm_bindgen(js_name = setPcmBudgetMb)]
        pub fn set_pcm_budget_mb(&self, mb: f64) -> bool {
            signal_sampler::engine::budget::set_limit_mb(mb.max(0.0) as u64)
        }

        /// Decoded PCM currently resident, in MB — the number that says
        /// whether the ceiling is the thing limiting how much of the rig can
        /// sound without a fetch.
        #[wasm_bindgen(js_name = pcmUsedMb)]
        pub fn pcm_used_mb(&self) -> f64 {
            signal_sampler::engine::budget::used_bytes() as f64 / (1024.0 * 1024.0)
        }

        /// The decoded-PCM ceiling in MB (-1 when unlimited).
        #[wasm_bindgen(js_name = pcmLimitMb)]
        pub fn pcm_limit_mb(&self) -> f64 {
            let limit = signal_sampler::engine::budget::limit_bytes();
            if limit == u64::MAX {
                -1.0
            } else {
                limit as f64 / (1024.0 * 1024.0)
            }
        }

        /// Zones this thread handed to the streamer workers (W13). Paired
        /// with `streamerOpened`: queued-but-never-opened means the workers
        /// are not draining; zero queued means the note path never asked.
        #[wasm_bindgen(js_name = opensQueued)]
        pub fn opens_queued(&self) -> u32 {
            self.opens_queued.get()
        }

        /// Samples inserted via `insertPcm` since boot (diagnostic).
        #[wasm_bindgen(js_name = pcmInsertCount)]
        pub fn pcm_insert_count(&self) -> u32 {
            self.pcm_inserted.get()
        }

        /// Inserts refused by the budget ceiling since boot (diagnostic).
        #[wasm_bindgen(js_name = pcmRefusedCount)]
        pub fn pcm_refused_count(&self) -> u32 {
            self.pcm_refused.get()
        }

        /// DECODER-WORKER SIDE: decode `path` in `layer`'s instrument on
        /// the calling thread and return `{ channels, sampleRate, frames,
        /// pcm: Float32Array }` — the PCM is taken back out of this
        /// instance's cache afterwards, so the worker stays memory-flat.
        /// Returns `undefined` when the path is unknown or its pack bytes
        /// are not readable yet (a miss was recorded through
        /// `__ftsPackRead`; fetch and retry).
        #[wasm_bindgen(js_name = decodePathPcm)]
        pub fn decode_path_pcm(&self, layer: &str, path: &str) -> JsValue {
            let rig = self.rig.borrow();
            let Some(rig) = rig.as_ref() else {
                return JsValue::UNDEFINED;
            };
            let Some(data) = rig.decode_sample_take(layer, std::path::Path::new(path)) else {
                return JsValue::UNDEFINED;
            };
            // A STREAMED sample decodes through `decode_all`, which stops
            // early (silently) at the first chunk it cannot read — exactly
            // what happens when the pack bytes for that span have not been
            // fetched yet. Shipping that would install a TRUNCATED sample
            // the rig would then treat as complete forever, so a short
            // decode is reported as "not ready": the caller drains its read
            // misses, fetches, and retries.
            let expected = data.num_frames * data.channels.max(1) as usize;
            let pcm_f32 = data.to_f32();
            if expected > 0 && pcm_f32.len() + 64 < expected {
                return JsValue::UNDEFINED;
            }
            let o = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&o, &"channels".into(), &f64::from(data.channels).into());
            let _ =
                js_sys::Reflect::set(&o, &"sampleRate".into(), &f64::from(data.sample_rate).into());
            let _ =
                js_sys::Reflect::set(&o, &"frames".into(), &(data.num_frames as f64).into());
            let pcm = js_sys::Float32Array::from(pcm_f32.as_ref());
            let _ = js_sys::Reflect::set(&o, &"pcm".into(), &pcm.into());
            o.into()
        }

        /// DECODER-WORKER SIDE: every lane's coverage-first sample list
        /// (playable order, middle-out from `center`, lanes interleaved) as
        /// a JS array of `{ layer, path }` — the background fill plan.
        #[wasm_bindgen(js_name = coveragePaths)]
        pub fn coverage_paths(&self, center: u8) -> js_sys::Array {
            let out = js_sys::Array::new();
            if let Some(rig) = self.rig.borrow().as_ref() {
                for (layer, path) in rig.coverage_samples(center) {
                    let o = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&o, &"layer".into(), &layer.into());
                    let _ = js_sys::Reflect::set(
                        &o,
                        &"path".into(),
                        &path.to_string_lossy().into_owned().into(),
                    );
                    out.push(&o);
                }
            }
            out
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::KeysWorklet;

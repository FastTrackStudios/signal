//! `/rigs/keys/:profile` — the browser keys rig (W3 of
//! crates/signal/docs/browser-keys-rig.md).
//!
//! A standalone web surface (launched by the wasm entry when the URL
//! matches, like `collection_browser`): boots the keys AudioWorklet from
//! the staged bundle, fetches the engine's resolved lane program over the
//! shared `/vox` target (`KeysRig::lane_program_wire`), opens the lanes,
//! then streams each referenced pack (OPFS-first — see `web_packs.rs`),
//! attaching bytes to the worklet as they turn ready. MIDI comes from
//! WebMIDI inputs and the bundled demo SMFs; a top-bar **Soundsource
//! Manager** popover shows per-pack streaming state.
//!
//! Since W9 the page body is the REAL remote UI —
//! `signal_keys_ui::KeysRigRemote`, the same component the desktop and
//! phone mount — backed by real `KeysRigClient`/`KeysRigStreamClient`
//! vox clients served in-process by `web_keys_backend.rs` (architect's
//! `LocalServer` over its wasm memory link). The W3–W8 top-bar chrome
//! (Resolution, Soundsources + demo player, Audio panel) stays around it.
//! The remote UI's own keyboard and mixer are the input and per-lane
//! controls; the e2e suite reaches lanes through `__ftsRig.lanes()` /
//! `setLaneMute()` (the visible compat strip is gone since W9.5).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use dioxus::prelude::*;
use js_sys::{Array, Object, Reflect};
use signal_keys_proto::KeysLaneProgram;
use signal_keys_proto::keys::{KeysRigClient, KeysRigStreamClient};
use signal_keys_ui::KeysRigRemote;
use signal_packs_proto::PackInfo;
use signal_packs_proto::packs::PackLibraryClient;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, AudioWorkletNode, AudioWorkletNodeOptions, MessagePort};

use crate::remote::EngineTarget;
use crate::web_packs::{self, PackEvent, PackWant};

// ── The staged worklet bundle (the W4 contract — one place) ────────────────

/// The AudioWorkletProcessor module (daw-standalone's `processor.js`).
pub(crate) const WORKLET_PROCESSOR_URL: &str = "/worklet/keys_processor.js";
/// The wasm-bindgen glue for `signal-keys-worklet`.
pub(crate) const WORKLET_GLUE_URL: &str = "/worklet/signal_keys_worklet.js";
/// The `signal-keys-worklet` wasm binary.
pub(crate) const WORKLET_WASM_URL: &str = "/worklet/signal_keys_worklet_bg.wasm";

/// IndexedDB key caching the last `lane_program_wire` reply, so a fully
/// offline boot still opens the lanes (packs then come from OPFS).
const PROGRAM_CACHE_KEY: &str = "lane-program";

/// localStorage key for the AudioContext `latencyHint` choice (W8). Read
/// at worklet boot; changing it in the audio panel re-runs the whole boot
/// path (new AudioContext, packs re-attach from OPFS).
const LATENCY_HINT_KEY: &str = "fts.keys-latency-hint";

/// A stored hint choice is one of these. `interactive`/`balanced`/
/// `playback` are the spec's category strings; `low` and `ultra` (W11) are
/// NUMERIC hints — the spec allows a double in seconds, and Chrome honours
/// it down to about one render quantum, which is what "live playable"
/// needs on top of an output pipeline that `interactive` leaves at 30+ ms.
fn valid_hint(v: &str) -> bool {
    matches!(v, "low" | "ultra" | "interactive" | "balanced" | "playback")
}

/// The seconds value for a numeric hint choice, if it is one.
fn hint_seconds(v: &str) -> Option<f64> {
    match v {
        "low" => Some(0.005),
        "ultra" => Some(0.0027), // ~one 128-sample quantum @ 48 kHz
        _ => None,
    }
}

/// The stored `latencyHint` (validated), defaulting to `low` (W11) — the
/// rig is a playable instrument first, and `interactive` proved
/// conservative in the field (32–45 ms of output latency on top of the
/// base). `low` asks for 5 ms and lets the browser clamp to what the
/// device can do.
fn latency_hint_pref() -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(LATENCY_HINT_KEY).ok().flatten())
        .filter(|v| valid_hint(v))
        .unwrap_or_else(|| "low".into())
}

/// The demo SMFs are authored at this tempo (they carry no tempo meta;
/// see examples/demo_midi_gen.rs).
const DEMO_BPM: f64 = 74.0;

/// Bundled demo SMFs: (label, bytes). Regenerate with
/// `cargo run -p fasttrackstudio --example demo_midi_gen`.
const DEMO_FILES: [(&str, &[u8]); 3] = [
    ("Pads (I–V–vi–IV)", include_bytes!("../assets/demo-midi/pads.mid")),
    ("Piano figure", include_bytes!("../assets/demo-midi/piano.mid")),
    ("Arp line", include_bytes!("../assets/demo-midi/arp.mid")),
];

// ── Routing ────────────────────────────────────────────────────────────────

/// `/rigs/keys/{profile}` → the profile segment.
fn profile_from_path() -> Option<String> {
    let path = web_sys::window()?.location().pathname().ok()?;
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        ["rigs", "keys", profile] => Some((*profile).to_string()),
        _ => None,
    }
}

/// True when the wasm entry should launch [`KeysWebRig`] instead of the
/// app shell.
pub(crate) fn route_matches() -> bool {
    profile_from_path().is_some()
}

// ── The worklet handle (main-thread side of processor.js's protocol) ───────

fn js_str(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

type Pending = Rc<RefCell<HashMap<u32, futures_channel::oneshot::Sender<JsValue>>>>;

/// One booted keys worklet: the `AudioWorkletNode` + the tiny
/// replyTo/value RPC over its port (mirroring `web_worklet/index.html`).
#[derive(Clone)]
pub(crate) struct Worklet {
    ctx: AudioContext,
    _node: AudioWorkletNode,
    port: MessagePort,
    pending: Pending,
    next_id: Rc<Cell<u32>>,
    _onmessage: Rc<Closure<dyn FnMut(web_sys::MessageEvent)>>,
}

impl PartialEq for Worklet {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.next_id, &other.next_id)
    }
}

impl Worklet {
    fn obj(kind: &str) -> Object {
        let o = Object::new();
        let _ = Reflect::set(&o, &"kind".into(), &kind.into());
        o
    }

    fn set(o: &Object, key: &str, v: &JsValue) {
        let _ = Reflect::set(o, &key.into(), v);
    }

    /// Fire-and-forget message.
    fn fire(&self, kind: &str, fields: &[(&str, JsValue)]) {
        let o = Self::obj(kind);
        for (k, v) in fields {
            Self::set(&o, k, v);
        }
        let _ = self.port.post_message(&o);
    }

    /// Fire-and-forget message with a transferred buffer.
    fn fire_transfer(&self, kind: &str, fields: &[(&str, JsValue)], transfer: &JsValue) {
        let o = Self::obj(kind);
        for (k, v) in fields {
            Self::set(&o, k, v);
        }
        let _ = self
            .port
            .post_message_with_transferable(&o, &Array::of1(transfer));
    }

    /// One streamed plan segment for a progressive pack (fire-and-forget;
    /// the buffer is TRANSFERRED to the worklet's JS heap).
    fn pack_segment(&self, id: u32, start: u64, bytes: &[u8]) {
        let arr = js_sys::Uint8Array::from(bytes);
        let buf = arr.buffer();
        self.fire_transfer(
            "pack_segment",
            &[
                ("id", id.into()),
                ("start", JsValue::from_f64(start as f64)),
                ("bytes", buf.clone().into()),
            ],
            buf.as_ref(),
        );
    }

    /// Attach a sparse (progressive) pack by id — its rank-0 segments must
    /// already be pushed via [`pack_segment`](Self::pack_segment).
    async fn attach_progressive(&self, key: &str, id: u32, len: u64) -> Result<(), String> {
        let v = self
            .rpc(
                "attach_pack_progressive",
                &[
                    ("key", key.into()),
                    ("id", id.into()),
                    ("len", JsValue::from_f64(len as f64)),
                ],
                None,
            )
            .await?;
        if v.as_bool() == Some(true) {
            Ok(())
        } else {
            Err(format!("attach_pack_progressive: {}", js_str(v)))
        }
    }

    /// Drain the worklet's read-miss list: `(pack id, start, len)` of
    /// ranges the engine wanted but the sparse store did not hold.
    async fn take_misses(&self) -> Vec<(u32, u64, u64)> {
        let Ok(v) = self.rpc("take_misses", &[], None).await else {
            return Vec::new();
        };
        let Ok(arr) = v.dyn_into::<Array>() else {
            return Vec::new();
        };
        arr.iter()
            .filter_map(|m| {
                let id = Reflect::get(&m, &"id".into()).ok()?.as_f64()? as u32;
                let start = Reflect::get(&m, &"start".into()).ok()?.as_f64()? as u64;
                let len = Reflect::get(&m, &"len".into()).ok()?.as_f64()? as u64;
                Some((id, start, len))
            })
            .collect()
    }

    /// The processor's render-load window (`audio_stats`), plus the page
    /// side's AudioContext latency figures (Reflect — the getters aren't in
    /// this crate's web-sys feature set, and a number is all we need).
    async fn audio_stats(&self) -> Option<AudioStats> {
        let v = self.rpc("audio_stats", &[], None).await.ok()?;
        let f = |key: &str| Reflect::get(&v, &key.into()).ok().and_then(|x| x.as_f64());
        let lat = |key: &str| {
            Reflect::get(self.ctx.as_ref(), &key.into())
                .ok()
                .and_then(|x| x.as_f64())
                .unwrap_or(0.0)
        };
        Some(AudioStats {
            load: f("load")?,
            worst_ms: f("worstMs").unwrap_or(0.0),
            quanta: f("quanta").unwrap_or(0.0),
            sample_rate: f("sampleRate").unwrap_or_else(|| f64::from(self.ctx.sample_rate())),
            voices: f("voices").unwrap_or(-1.0) as i32,
            base_ms: lat("baseLatency") * 1000.0,
            output_ms: lat("outputLatency") * 1000.0,
        })
    }

    /// Raw 3-byte MIDI to every lane. The ONE seam every source goes
    /// through (WebMIDI, demo player, on-screen keys, the backend's
    /// `trigger`), so it also feeds the monitor's recent-MIDI ring.
    pub(crate) fn midi(&self, status: u8, d1: u8, d2: u8) {
        crate::web_keys_backend::midi_seen(status, d1, d2);
        let arr = Array::of3(&status.into(), &d1.into(), &d2.into());
        self.fire("midi", &[("bytes", arr.into())]);
    }

    /// Resume the AudioContext (the backend's `start`; a no-op while
    /// already running).
    pub(crate) fn resume(&self) {
        let _ = self.ctx.resume();
    }

    /// Suspend the AudioContext (the backend's `stop` — audio pauses,
    /// packs and the lane program stay).
    pub(crate) fn suspend(&self) {
        let _ = self.ctx.suspend();
    }

    pub(crate) fn all_notes_off(&self) {
        self.fire("all_notes_off", &[]);
    }

    pub(crate) fn set_track_volume(&self, index: u32, volume: f64) {
        self.fire(
            "set_track_volume",
            &[("index", index.into()), ("volume", volume.into())],
        );
    }

    pub(crate) fn set_track_mute(&self, index: u32, muted: bool) {
        self.fire(
            "set_track_mute",
            &[("index", index.into()), ("muted", muted.into())],
        );
    }

    /// Await-able request (`replyTo`/`value` over the port).
    async fn rpc(
        &self,
        kind: &str,
        fields: &[(&str, JsValue)],
        transfer: Option<&JsValue>,
    ) -> Result<JsValue, String> {
        let id = self.next_id.get();
        self.next_id.set(id.wrapping_add(1));
        let (tx, rx) = futures_channel::oneshot::channel();
        self.pending.borrow_mut().insert(id, tx);
        let o = Self::obj(kind);
        Self::set(&o, "replyTo", &id.into());
        for (k, v) in fields {
            Self::set(&o, k, v);
        }
        let posted = match transfer {
            Some(t) => self
                .port
                .post_message_with_transferable(&o, &Array::of1(t)),
            None => self.port.post_message(&o),
        };
        posted.map_err(|e| format!("worklet post: {}", js_str(e)))?;
        rx.await.map_err(|_| "worklet reply dropped".to_string())
    }
}

/// Create the AudioContext, load the processor module, construct the node
/// (`entry: 'keys'`), and wait for its `ready`. Must run from a user
/// gesture (autoplay policy).
async fn boot_worklet() -> Result<Worklet, String> {
    let window = web_sys::window().ok_or("no window")?;
    // The stored latencyHint rides AudioContextOptions (Reflect-set onto
    // the options dict — it is a plain JS object, and the union-typed
    // setter shape varies across web-sys versions). Category choices go
    // over as strings; `low`/`ultra` go over as NUMERIC seconds (the
    // spec's double form), which the browser clamps to the device floor.
    let opts = web_sys::AudioContextOptions::new();
    let pref = latency_hint_pref();
    let hint: JsValue = match hint_seconds(&pref) {
        Some(secs) => JsValue::from_f64(secs),
        None => pref.as_str().into(),
    };
    let _ = Reflect::set(&opts, &"latencyHint".into(), &hint);
    let ctx = AudioContext::new_with_context_options(&opts)
        .map_err(|e| format!("AudioContext: {}", js_str(e)))?;
    // Autoplay insurance: a context created outside a real user gesture
    // starts 'suspended' and our own resume() call is then rejected too —
    // silence with everything else working. Any genuine pointer press on the
    // page is a resume opportunity; keep the listener for the page's life
    // (resume() on a running context is a no-op).
    {
        let ctx2 = ctx.clone();
        let on_pointer = Closure::<dyn FnMut()>::new(move || {
            let _ = ctx2.resume();
        });
        if let Some(doc) = window.document() {
            let _ = doc.add_event_listener_with_callback(
                "pointerdown",
                on_pointer.as_ref().unchecked_ref(),
            );
        }
        on_pointer.forget();
    }
    // AudioWorklet (and OPFS) exist only in SECURE contexts — https or
    // localhost. Over plain http on a LAN/tailnet IP, `ctx.audioWorklet`
    // is undefined and the raw failure is an unreadable TypeError deep in
    // addModule. Name the actual problem and the fix instead.
    if !window.is_secure_context() {
        return Err(
            "this page needs a SECURE context for audio (AudioWorklet): open it via \
             https (e.g. the tailscale-served https://<machine>.ts.net URL) or \
             http://localhost — plain http on an IP cannot work"
                .to_string(),
        );
    }
    let worklet = ctx
        .audio_worklet()
        .map_err(|e| format!("audioWorklet: {}", js_str(e)))?;
    JsFuture::from(
        worklet
            .add_module(WORKLET_PROCESSOR_URL)
            .map_err(|e| format!("addModule: {}", js_str(e)))?,
    )
    .await
    .map_err(|e| format!("addModule({WORKLET_PROCESSOR_URL}): {}", js_str(e)))?;

    let opts = AudioWorkletNodeOptions::new();
    let counts = Array::of1(&2u32.into());
    opts.set_output_channel_count(&counts);
    // The name keys_processor.js registers — the keys-specific processor
    // with a STATIC glue import (AudioWorkletGlobalScope has no dynamic
    // import(), which is why the generic daw-standalone processor cannot
    // boot here).
    let node = AudioWorkletNode::new_with_options(&ctx, "fts-keys-rig", &opts)
        .map_err(|e| format!("AudioWorkletNode: {}", js_str(e)))?;
    node.connect_with_audio_node(&ctx.destination())
        .map_err(|e| format!("connect: {}", js_str(e)))?;
    let port = node.port().map_err(|e| format!("port: {}", js_str(e)))?;

    // One dispatcher for replies + the init 'ready'.
    let pending: Pending = Rc::new(RefCell::new(HashMap::new()));
    let ready: Rc<RefCell<Option<futures_channel::oneshot::Sender<()>>>> =
        Rc::new(RefCell::new(None));
    let (ready_tx, ready_rx) = futures_channel::oneshot::channel();
    *ready.borrow_mut() = Some(ready_tx);
    let p2 = pending.clone();
    let r2 = ready.clone();
    let onmessage = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
        move |ev: web_sys::MessageEvent| {
            let data = ev.data();
            if let Ok(reply_to) = Reflect::get(&data, &"replyTo".into())
                && let Some(id) = reply_to.as_f64()
            {
                if let Some(tx) = p2.borrow_mut().remove(&(id as u32)) {
                    let value = Reflect::get(&data, &"value".into()).unwrap_or(JsValue::UNDEFINED);
                    let _ = tx.send(value);
                }
                return;
            }
            if let Ok(kind) = Reflect::get(&data, &"kind".into())
                && kind.as_string().as_deref() == Some("ready")
                && let Some(tx) = r2.borrow_mut().take()
            {
                let _ = tx.send(());
            }
        },
    );
    port.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    // Fetch the worklet wasm on the main thread (the worklet scope can't
    // fetch) and hand it over with the init.
    let resp = JsFuture::from(window.fetch_with_str(WORKLET_WASM_URL))
        .await
        .map_err(|e| format!("fetch {WORKLET_WASM_URL}: {}", js_str(e)))?;
    let resp: web_sys::Response = resp
        .dyn_into()
        .map_err(|_| "fetch: not a Response".to_string())?;
    if !resp.ok() {
        return Err(format!(
            "worklet wasm missing ({} on {WORKLET_WASM_URL}) — staged by `just web-stage` (W4)",
            resp.status()
        ));
    }
    let wasm_bytes = JsFuture::from(
        resp.array_buffer()
            .map_err(|e| format!("arrayBuffer: {}", js_str(e)))?,
    )
    .await
    .map_err(|e| format!("wasm bytes: {}", js_str(e)))?;

    let init = Worklet::obj("init");
    Worklet::set(&init, "wasmBytes", &wasm_bytes);
    Worklet::set(&init, "glueUrl", &WORKLET_GLUE_URL.into());
    Worklet::set(&init, "sampleRate", &f64::from(ctx.sample_rate()).into());
    Worklet::set(&init, "entry", &"keys".into());
    port.post_message_with_transferable(&init, &Array::of1(&wasm_bytes))
        .map_err(|e| format!("init post: {}", js_str(e)))?;
    ready_rx
        .await
        .map_err(|_| "worklet never became ready".to_string())?;

    Ok(Worklet {
        ctx,
        _node: node,
        port,
        pending,
        next_id: Rc::new(Cell::new(1)),
        _onmessage: Rc::new(onmessage),
    })
}

// ── Page state ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Boot {
    /// Waiting on the user gesture the AudioContext needs.
    Idle,
    Starting(String),
    Running,
    Failed(String),
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum PackPhase {
    Queued,
    Streaming,
    Verifying,
    Ready,
    /// Attached and PLAYABLE (rank-0 landed; the worklet opens the pack
    /// and decodes whatever segments exist) while detail keeps streaming.
    /// `pct` = delivered sample segments / total sample segments.
    Playable { pct: u8 },
    /// Streamed + cached in OPFS but NOT attached: the total pack bytes
    /// held by the worklet would cross the tab-memory sanity cap.
    /// Deliberately distinct from `Failed` — the bytes are here and
    /// correct; only residency is deferred. Since W6 (attach-by-handle:
    /// pack bytes live on the worklet's JS heap, not in wasm linear
    /// memory) the cap is generous and this state should not occur with
    /// the stock Worship set.
    Deferred,
    Failed(String),
}

impl PackPhase {
    fn label(&self) -> &str {
        match self {
            Self::Queued => "queued",
            Self::Streaming => "streaming",
            Self::Verifying => "verifying",
            Self::Ready => "ready",
            Self::Playable { .. } => "playable",
            Self::Deferred => "deferred (past the tab-memory sanity cap)",
            Self::Failed(_) => "failed",
        }
    }

    /// Attached and sounding (fully or partially).
    fn usable(&self) -> bool {
        matches!(self, Self::Ready | Self::Playable { .. })
    }
}

/// Total pack-bytes sanity ceiling across every attached pack. Since W6,
/// packs attach BY HANDLE: the transferred buffers live on the worklet's JS
/// heap and cost wasm's 4 GB linear memory nothing (only decoded PCM enters
/// it, bounded by the 768 MB wasm preload budget), so this is no longer a
/// wasm-address-space guard — it is tab-memory realism. The full Worship
/// proxy set is ~2.4 GB; 6 GB leaves room for growth while still refusing a
/// pathological set before the tab OOMs. Past it, a pack parks as
/// `Deferred` (cached in OPFS, not resident).
const JS_PACK_RESIDENT_SANITY_CAP: u64 = 6_000_000_000;

/// One pack row in the Soundsource Manager.
#[derive(Clone, PartialEq)]
struct PackRow {
    /// The spec-path key the worklet installs under.
    key: String,
    name: String,
    variant: String,
    total: u64,
    bytes: u64,
    phase: PackPhase,
    /// Sample segments DELIVERED to the worklet (rank-0 index segments
    /// excluded) — the rig-wide Resolution numerator. Whole-file packs
    /// are all-or-nothing: 0 until `Ready`, then `seg_total`.
    seg_done: u32,
    /// Total sample segments (1 for whole-file packs).
    seg_total: u32,
}

/// Rig-wide Resolution (0–100): delivered sample segments over total
/// sample segments across every pack the profile references. A pack still
/// queued/streaming counts 0; whole-file packs flip all-at-once on ready.
fn resolution_pct(rows: &[PackRow]) -> u32 {
    let (done, total) = rows.iter().fold((0u64, 0u64), |(d, t), r| {
        (d + u64::from(r.seg_done), t + u64::from(r.seg_total.max(1)))
    });
    if total == 0 {
        return 100;
    }
    // Integer floor — only ALL segments delivered reads 100.
    ((done * 100) / total) as u32
}

/// One lane row (from `KeysLaneProgram::lanes`).
#[derive(Clone, PartialEq)]
struct LaneRow {
    engine: String,
    name: String,
    /// Pack key (empty = nothing to stream).
    key: String,
    /// Worklet project-track index (rig folder = 0; see the proto docs).
    track: u32,
    volume: f64,
    muted: bool,
    peak: f32,
}

/// Compute each lane's worklet track index: rig(0), then per engine its
/// folder track followed by its lanes.
fn lane_tracks(lanes: &[signal_keys_proto::KeysLaneRef]) -> Vec<u32> {
    let mut out = Vec::with_capacity(lanes.len());
    let mut track = 0u32; // rig folder
    let mut engine: Option<&str> = None;
    for lane in lanes {
        if engine != Some(lane.engine.as_str()) {
            engine = Some(lane.engine.as_str());
            track += 1; // the engine folder track
        }
        track += 1;
        out.push(track);
    }
    out
}

/// One `audio_stats` reading: the processor's render-load window plus the
/// page-side AudioContext latency figures (W8).
#[derive(Clone, Copy, PartialEq, Default)]
struct AudioStats {
    /// Render load 0..1+ — total measured render ms over the window's
    /// AUDIO ms (quanta × quantum-ms). See the processor's stats comment
    /// for the coarse-clock aggregation.
    load: f64,
    /// Worst single-quantum render cost in the last window (ms, ~1 ms
    /// resolution — coarse, but spikes show).
    worst_ms: f64,
    /// Monotonic quantum counter since the worklet booted.
    quanta: f64,
    sample_rate: f64,
    /// Active sampler voices, or -1 when unavailable.
    voices: i32,
    /// `ctx.baseLatency` in ms.
    base_ms: f64,
    /// `ctx.outputLatency` in ms (0 where the browser doesn't expose it).
    output_ms: f64,
}

impl AudioStats {
    fn latency_ms(&self) -> f64 {
        self.base_ms + self.output_ms
    }
}

/// The `window.__ftsRig` state hook W5's Playwright test polls.
#[derive(Default)]
struct RigHook {
    state: String,
    packs_json: String,
    /// Lane rows (engine/name/pack/track) for the e2e suite — set once at
    /// lane build; replaced the visible compat strip's DOM testids.
    lanes_json: String,
    master_peak: f64,
    /// Rig-wide Resolution 0–100 (see [`resolution_pct`]).
    resolution: f64,
    /// Last audio_stats reading (W8) — `renderLoad()` / `latencyMs()` /
    /// `audioStats()` read these.
    audio: AudioStats,
    /// Live WebMIDI input names, JSON array (`midiInputs()`).
    midi_inputs_json: String,
    /// The `statechange` hot-plug listener is armed (`midiHotplugArmed()`).
    midi_hotplug: bool,
}

thread_local! {
    static RIG_HOOK: RefCell<RigHook> = RefCell::default();
}

/// One row of the hook's `packStates()` JSON.
#[derive(facet::Facet, Default)]
struct HookPack {
    name: String,
    /// The pack's spec-path key — joins pack rows to `lanes()` rows.
    key: String,
    state: String,
    bytes: u64,
    total: u64,
    /// Failure text / playable pct — diagnostics for the e2e suite and
    /// remote debugging (the popover shows the same string).
    detail: String,
}

fn hook_set_state(state: &str) {
    RIG_HOOK.with(|h| h.borrow_mut().state = state.to_string());
}

/// One row of the hook's `lanes()` JSON (the e2e suite's lane access).
#[derive(facet::Facet, Default)]
struct HookLane {
    engine: String,
    name: String,
    /// The pack spec-path key (empty = no pack, silent natively too).
    key: String,
    /// The worklet track index this lane renders on.
    track: u32,
}

fn hook_set_lanes(rows: &[LaneRow]) {
    let rows: Vec<HookLane> = rows
        .iter()
        .map(|r| HookLane {
            engine: r.engine.clone(),
            name: r.name.clone(),
            key: r.key.clone(),
            track: r.track,
        })
        .collect();
    let json = facet_json::to_string(&rows).unwrap_or_default();
    RIG_HOOK.with(|h| h.borrow_mut().lanes_json = json);
}

fn hook_set_packs(rows: &[PackRow]) {
    let resolution = f64::from(resolution_pct(rows));
    let rows: Vec<HookPack> = rows
        .iter()
        .map(|r| HookPack {
            name: r.name.clone(),
            key: r.key.clone(),
            state: r.phase.label().to_string(),
            bytes: r.bytes,
            total: r.total,
            detail: match &r.phase {
                PackPhase::Failed(e) => e.clone(),
                PackPhase::Playable { pct } => format!("{pct}%"),
                _ => String::new(),
            },
        })
        .collect();
    let json = facet_json::to_string(&rows).unwrap_or_default();
    RIG_HOOK.with(|h| {
        let mut h = h.borrow_mut();
        h.packs_json = json;
        h.resolution = resolution;
    });
}

fn hook_set_peak(peak: f64) {
    RIG_HOOK.with(|h| h.borrow_mut().master_peak = peak);
}

fn hook_set_audio(stats: AudioStats) {
    RIG_HOOK.with(|h| h.borrow_mut().audio = stats);
}

/// Mirror the live WebMIDI input list (+ whether the hot-plug listener is
/// armed) onto the hook for the e2e suite.
fn hook_set_midi(names: &[String], hotplug: bool) {
    let json = facet_json::to_string(&names.to_vec()).unwrap_or_default();
    RIG_HOOK.with(|h| {
        let mut h = h.borrow_mut();
        h.midi_inputs_json = json;
        h.midi_hotplug = hotplug;
    });
}

/// The `audioStats()` JSON row (mirrors [`AudioStats`], camelCase keys for
/// the e2e suite / remote debugging).
#[derive(facet::Facet, Default)]
struct HookAudio {
    load: f64,
    #[facet(rename = "worstMs")]
    worst_ms: f64,
    quanta: f64,
    #[facet(rename = "sampleRate")]
    sample_rate: f64,
    voices: i32,
    #[facet(rename = "baseMs")]
    base_ms: f64,
    #[facet(rename = "outputMs")]
    output_ms: f64,
    #[facet(rename = "latencyMs")]
    latency_ms: f64,
}

/// Install `window.__ftsRig = { state, packStates, masterPeak }` once.
fn install_hook() {
    let Some(window) = web_sys::window() else { return };
    if Reflect::has(&window, &"__ftsRig".into()).unwrap_or(false) {
        return;
    }
    let obj = Object::new();
    let state = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_str(&h.borrow().state))
    });
    let packs = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_str(&h.borrow().packs_json))
    });
    let peak = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_f64(h.borrow().master_peak))
    });
    let resolution = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_f64(h.borrow().resolution))
    });
    // W8 audio diagnostics. These read the thread-local (not a captured
    // worklet), so they survive the latency-hint re-boot unchanged.
    let render_load = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_f64(h.borrow().audio.load))
    });
    let latency_ms = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_f64(h.borrow().audio.latency_ms()))
    });
    let audio_stats = Closure::<dyn FnMut() -> JsValue>::new(|| {
        let a = RIG_HOOK.with(|h| h.borrow().audio);
        let row = HookAudio {
            load: a.load,
            worst_ms: a.worst_ms,
            quanta: a.quanta,
            sample_rate: a.sample_rate,
            voices: a.voices,
            base_ms: a.base_ms,
            output_ms: a.output_ms,
            latency_ms: a.latency_ms(),
        };
        JsValue::from_str(&facet_json::to_string(&row).unwrap_or_default())
    });
    // Lane access for the e2e suite (replaced the visible compat strip).
    let lanes_fn = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_str(&h.borrow().lanes_json))
    });
    let set_lane_mute = Closure::<dyn FnMut(f64, bool) -> JsValue>::new(|i: f64, muted: bool| {
        JsValue::from_bool(crate::web_keys_backend::strip_set_mute(i as usize, muted))
    });
    let _ = Reflect::set(&obj, &"lanes".into(), lanes_fn.as_ref());
    let _ = Reflect::set(&obj, &"setLaneMute".into(), set_lane_mute.as_ref());
    lanes_fn.forget();
    set_lane_mute.forget();
    // W11 WebMIDI plumbing proofs for the e2e suite (no hardware in CI):
    // the live input-name list, whether the statechange hot-plug listener
    // is armed, and whether the port gate defaults to omni.
    let midi_inputs_fn = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_str(&h.borrow().midi_inputs_json))
    });
    let midi_hotplug_fn = Closure::<dyn FnMut() -> JsValue>::new(|| {
        RIG_HOOK.with(|h| JsValue::from_bool(h.borrow().midi_hotplug))
    });
    let midi_omni_fn = Closure::<dyn FnMut() -> JsValue>::new(|| {
        JsValue::from_bool(crate::web_keys_backend::midi_omni())
    });
    let _ = Reflect::set(&obj, &"midiInputs".into(), midi_inputs_fn.as_ref());
    let _ = Reflect::set(&obj, &"midiHotplugArmed".into(), midi_hotplug_fn.as_ref());
    let _ = Reflect::set(&obj, &"midiOmni".into(), midi_omni_fn.as_ref());
    midi_inputs_fn.forget();
    midi_hotplug_fn.forget();
    midi_omni_fn.forget();
    let _ = Reflect::set(&obj, &"state".into(), state.as_ref());
    let _ = Reflect::set(&obj, &"packStates".into(), packs.as_ref());
    let _ = Reflect::set(&obj, &"masterPeak".into(), peak.as_ref());
    let _ = Reflect::set(&obj, &"resolution".into(), resolution.as_ref());
    let _ = Reflect::set(&obj, &"renderLoad".into(), render_load.as_ref());
    let _ = Reflect::set(&obj, &"latencyMs".into(), latency_ms.as_ref());
    let _ = Reflect::set(&obj, &"audioStats".into(), audio_stats.as_ref());
    let _ = Reflect::set(&window, &"__ftsRig".into(), &obj);
    // Page-lifetime closures.
    state.forget();
    packs.forget();
    peak.forget();
    resolution.forget();
    render_load.forget();
    latency_ms.forget();
    audio_stats.forget();
}

// ── Engine plumbing ────────────────────────────────────────────────────────

async fn with_timeout<T>(
    ms: u64,
    fut: impl std::future::Future<Output = T>,
) -> Option<T> {
    use futures_util::FutureExt as _;
    futures_util::select! {
        v = fut.fuse() => Some(v),
        _ = architect::platform::sleep(std::time::Duration::from_millis(ms)).fuse() => None,
    }
}

/// Fetch the resolved lane program from the engine, falling back to (and
/// refreshing) the IndexedDB cache so an offline boot still works.
async fn fetch_lane_program(target: &EngineTarget) -> Result<KeysLaneProgram, String> {
    let fetched: Option<KeysLaneProgram> = match with_timeout(12_000, async {
        let client: KeysRigClient = crate::remote::establish_verbose(target).await.ok()?;
        client.lane_program_wire().await.ok()
    })
    .await
    {
        Some(v) => v,
        None => None,
    };
    if let Some(program) = fetched
        && !program.program_json.is_empty()
    {
        if let Ok(json) = facet_json::to_string(&program) {
            let _ = web_packs::idb_put(PROGRAM_CACHE_KEY, &json).await;
        }
        return Ok(program);
    }
    // Offline (or an empty engine): the cached copy.
    if let Ok(Some(json)) = web_packs::idb_get(PROGRAM_CACHE_KEY).await
        && let Ok(cached) = facet_json::from_str::<KeysLaneProgram>(&json)
        && !cached.program_json.is_empty()
    {
        return Ok(cached);
    }
    Err(format!(
        "engine unreachable at {} and no cached program",
        target.label()
    ))
}

/// The host's `PackLibrary` listing, if reachable (proxy + full variants).
async fn fetch_pack_listing(target: &EngineTarget) -> Option<Vec<PackInfo>> {
    with_timeout(12_000, async {
        let client: PackLibraryClient = crate::remote::establish_verbose(target).await.ok()?;
        client.packs().await.ok()
    })
    .await
    .flatten()
}

/// Pick the variant to fetch for `name`: proxy when the host offers it
/// (streaming tier), else full.
fn pick_variant(listing: &[PackInfo], name: &str) -> Option<PackWant> {
    let pick = listing
        .iter()
        .find(|p| p.name == name && p.variant == "proxy")
        .or_else(|| listing.iter().find(|p| p.name == name))?;
    Some(PackWant {
        name: pick.name.clone(),
        variant: pick.variant.clone(),
        size_bytes: pick.size_bytes,
        sha256: pick.sha256.clone(),
    })
}

// ── The page ───────────────────────────────────────────────────────────────

/// The compiled Tailwind sheet the signal UI components style themselves
/// with (additive — signal-keys-ui is inline-styles-first). The same file
/// `rig_view.rs` inlines for the desktop remotes.
const SIGNAL_TAILWIND: &str = include_str!("../assets/tailwind-signal.css");

/// The browser keys rig root. Reads the URL itself; no props.
#[component]
pub fn KeysWebRig() -> Element {
    let profile = profile_from_path().unwrap_or_default();
    let mut boot = use_signal(|| Boot::Idle);
    let packs = use_signal(Vec::<PackRow>::new);
    let lanes = use_signal(Vec::<LaneRow>::new);
    let worklet = use_signal(|| Option::<Worklet>::None);
    let master = use_signal(|| 0.0f32);
    let midi_inputs = use_signal(Vec::<String>::new);
    // WebMIDI availability text ("" = fine): permission denied / unsupported
    // reads very differently from "no devices plugged in" (W11).
    let midi_status = use_signal(String::new);
    let astats = use_signal(AudioStats::default);
    // The in-process vox clients KeysRigRemote consumes (established once
    // the backend is installed; survive the latency-hint re-boot).
    let clients = use_signal(|| Option::<(KeysRigClient, KeysRigStreamClient)>::None);
    // (index playing, looping, generation) — generation cancels schedulers.
    let demo = use_signal(|| (Option::<usize>::None, false, 0u64));

    // The remote UI publishes its readouts/panels into the app chrome —
    // provide one shared Chrome so its PanelHosts and our PanelRail agree.
    fts_chrome::provide_chrome();

    use_hook(|| {
        install_hook();
        hook_set_state("idle");
    });

    let start = use_callback(move |_: ()| {
        if !matches!(*boot.peek(), Boot::Idle | Boot::Failed(_)) {
            return;
        }
        boot.set(Boot::Starting("booting audio worklet…".into()));
        hook_set_state("starting");
        spawn(boot_rig(boot, packs, lanes, worklet, master, midi_inputs, midi_status, astats, clients));
    });

    // Full re-boot of the audio path (the latency-hint selector's change
    // path): close the old AudioContext, drop the worklet, and run the
    // ordinary boot again — the new context picks up the stored hint, the
    // lane program comes from the IndexedDB cache, and every pack
    // re-attaches from OPFS with no network. The old poll loops park
    // themselves when they see the worklet signal changed.
    let restart = use_callback(move |_: ()| {
        let mut boot = boot;
        let mut worklet = worklet;
        let mut packs = packs;
        let mut lanes = lanes;
        if let Some(w) = worklet.peek().clone() {
            w.all_notes_off();
            let _ = w.ctx.close();
        }
        worklet.set(None);
        crate::web_keys_backend::set_worklet(None);
        packs.set(Vec::new());
        lanes.set(Vec::new());
        boot.set(Boot::Starting("restarting audio…".into()));
        hook_set_state("starting");
        spawn(boot_rig(boot, packs, lanes, worklet, master, midi_inputs, midi_status, astats, clients));
    });

    let boot_running = matches!(boot(), Boot::Running);
    let rail_items = vec![fts_chrome::RailItem::new(
        "keys",
        "Keys",
        fts_chrome::Icon::Keys,
        true,
        Callback::new(|_| {}),
    )];

    rsx! {
        document::Style { {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;overflow:hidden;}*{box-sizing:border-box;}"} }
        fts_chrome::AppFrame {
            top: rsx! {
                fts_chrome::TopBar {
                    leading: rsx! {
                        span {
                            style: "font-weight: 700; letter-spacing: 1px; font-size: 12px; \
                                    color: #71717a; cursor: default; padding-right: 2px;",
                            "FTS"
                        }
                    },
                }
            },
            rail: rsx! {
                fts_chrome::IconRail { items: rail_items }
            },
            // The page's flyouts, exactly like the desktop app's
            // engines/settings: content rendered by PanelHost, opened from
            // the right rail.
            panel: rsx! {
                fts_chrome::PanelHost { id: "web.ssm".to_string(),
                    SoundsourceManager { packs, worklet, lanes, midi_inputs, midi_status, demo, boot_running }
                }
                fts_chrome::PanelHost { id: "web.audio".to_string(),
                    AudioPanel { astats, restart, boot_running }
                }
            },
            right: rsx! { fts_chrome::PanelRail {} },
            // Publishes the page's crumbs / status / panels into the chrome
            // from a LEAF component, so the 10 Hz master-meter tick
            // re-renders only this (and the TopBar) — never the whole page.
            KeysChrome { packs, master, profile: profile.clone() }
            main {
                style: if boot_running {
                    // The full remote UI wants the whole viewport.
                    "flex:1; min-width:0; min-height:0; display:flex; flex-direction:column;"
                } else {
                    "flex:1; min-width:0; display:flex; flex-direction:column; gap:16px; padding:20px;"
                },
                match boot() {
                    Boot::Idle => rsx! {
                        div { style: "display:flex; flex-direction:column; align-items:center; gap:12px; margin:auto; max-width:520px;",
                            span { style: "font-size:18px; font-weight:700;", "Keys rig — {profile}" }
                            span { style: "font-size:13px; color:#a1a1aa; text-align:center; max-width:420px;",
                                "Plays the rig entirely in this browser: packs stream from the engine and cache locally; MIDI from your keyboard (WebMIDI) or the demo player."
                            }
                            button {
                                "data-testid": "rig-start",
                                style: "padding:10px 26px; border-radius:8px; background:#22c55e; color:#052e16; font-weight:700; font-size:15px; border:none; cursor:pointer;",
                                onclick: move |_| start.call(()),
                                "Start"
                            }
                        }
                    },
                    Boot::Starting(msg) => rsx! {
                        div { style: "margin:auto; color:#a1a1aa; font-size:14px;", "{msg}" }
                    },
                    Boot::Failed(msg) => rsx! {
                        div { style: "display:flex; flex-direction:column; align-items:center; gap:10px; margin:auto; max-width:520px;",
                            span { style: "color:#ef4444; font-size:14px; font-weight:600;", "Could not start the rig" }
                            span { style: "color:#a1a1aa; font-size:13px; max-width:480px; text-align:center;", "{msg}" }
                            button {
                                style: "padding:6px 18px; border-radius:6px; background:#111113; color:#e4e4e7; border:1px solid #27272a; cursor:pointer;",
                                onclick: move |_| start.call(()),
                                "Retry"
                            }
                        }
                    },
                    Boot::Running => rsx! {
                        document::Style { {SIGNAL_TAILWIND} }
                        // The real remote UI over the in-process clients,
                        // inside the scale-to-fit wrapper (W11): a viewport
                        // narrower than the rig's natural width gets the
                        // WHOLE rig scaled down instead of a horizontal
                        // scrollbar. KeysRigRemote's own PanelHosts
                        // (Routing / MIDI) render inside it as always.
                        ScaleToFit {
                            match clients() {
                                Some((rig, stream)) => {
                                    let _ = provide_context(rig);
                                    let _ = provide_context(stream);
                                    rsx! { KeysRigRemote {} }
                                }
                                None => rsx! {
                                    div { style: "display:flex; align-items:center; justify-content:center; flex:1; color:#71717a; font-size:13px;",
                                        "Wiring the rig UI…"
                                    }
                                },
                            }
                        }
                    },
                }
            }
        }
    }
}

/// The page's contributions to the app chrome (W11): crumbs, the
/// Resolution pill + master meter as REGISTERED status items, and the
/// Soundsources/Audio panels on the right rail — the same seams the
/// desktop app and the rig views publish through, instead of a second,
/// hand-rolled top bar. A leaf component so the 10 Hz meter tick
/// re-renders only this and the TopBar.
#[component]
fn KeysChrome(
    packs: Signal<Vec<PackRow>>,
    master: Signal<f32>,
    profile: String,
) -> Element {
    // Usable = attached and sounding (ready OR playable-while-filling);
    // the count answers "can I play it?", the Resolution pill answers
    // "am I hearing all of it?".
    let ready_count = packs.read().iter().filter(|p| p.phase.usable()).count();
    let pack_count = packs.read().len();
    let streaming = pack_count > ready_count;
    let resolution = resolution_pct(&packs.read());

    let level = fts_chrome::use_chrome_level(1);
    level.crumbs(vec![
        fts_chrome::Crumb::here("Keys"),
        fts_chrome::Crumb::here(profile),
    ]);
    let mut status = Vec::new();
    if pack_count > 0 {
        // Rig-wide Resolution — PROMINENT: a player in the first minute
        // must see the sound is not final yet.
        let pill = if resolution < 100 {
            fts_chrome::StatusItem::pill(format!("RESOLUTION {resolution}%"), "#fbbf24", "#292312")
        } else {
            fts_chrome::StatusItem::pill("FULL RESOLUTION", "#22c55e", "#0f2417")
        };
        status.push(fts_chrome::StatusItem::tagged("rig-resolution", pill));
        if streaming {
            status.push(fts_chrome::StatusItem::text(format!(
                "{ready_count}/{pack_count} soundsources"
            )));
        }
    }
    status.push(fts_chrome::StatusItem::meter(master(), "#22c55e"));
    level.status(status);
    level.panels(vec![
        fts_chrome::PanelSpec::new("web.ssm", "Soundsources", fts_chrome::Icon::Browser)
            .width(400)
            .testid("ssm-button"),
        fts_chrome::PanelSpec::new("web.audio", "Audio", fts_chrome::Icon::Engine)
            .width(320)
            .testid("audio-button"),
    ]);
    rsx! {}
}

/// Scale-to-fit wrapper (W11): measures the content's UNSCALED layout
/// width (`scrollWidth` is transform-independent) against the room the
/// frame gives it, and applies `transform: scale(s)` with the layout box
/// compensated — inner width pinned at the natural width and inner height
/// grown by 1/s, so the scaled result exactly fills the outer box and the
/// page never scrolls sideways. A viewport wider than the natural width
/// leaves scale at 1 and the fluid layout fills it edge to edge.
#[component]
fn ScaleToFit(children: Element) -> Element {
    // (scale, natural width px, outer height px)
    let mut fit = use_signal(|| (1.0f64, 0.0f64, 0.0f64));
    use_future(move || async move {
        let mut ev = dioxus::document::eval(
            r#"
            const measure = () => {
                const outer = document.getElementById('keys-fit-outer');
                const inner = document.getElementById('keys-fit-inner');
                if (!outer || !inner) return;
                // scrollWidth reports the unscaled layout, so re-measuring
                // while scaled stays stable; only genuine content growth
                // or a viewport change moves it.
                const natural = inner.scrollWidth;
                const room = outer.clientWidth;
                const scale = natural > room + 1 ? room / natural : 1;
                dioxus.send([scale, natural, outer.clientHeight]);
            };
            window.addEventListener('resize', measure);
            const ro = new ResizeObserver(measure);
            const arm = () => {
                const outer = document.getElementById('keys-fit-outer');
                const inner = document.getElementById('keys-fit-inner');
                if (outer && inner) { ro.observe(outer); ro.observe(inner); measure(); }
                else { setTimeout(arm, 120); }
            };
            arm();
            // Content GROWTH (packs attaching, panels opening) widens
            // scrollWidth without resizing the observed boxes, so a slow
            // safety poll catches what the ResizeObserver cannot. The recv
            // side dedupes, so an unchanged reading costs nothing.
            setInterval(measure, 1000);
            "#,
        );
        while let Ok(v) = ev.recv::<(f64, f64, f64)>().await {
            if *fit.peek() != v {
                fit.set(v);
            }
        }
    });
    let (scale, natural, outer_h) = fit();
    // BOTH branches name every property the other uses: style declarations
    // are patched individually, so a property present only in the scaled
    // branch would survive the switch back to scale 1 (a stale transform
    // shrank the rig on wide viewports until this was made symmetric).
    let inner_style = if scale < 0.999 && natural > 0.0 {
        format!(
            "width:{natural}px; height:{:.1}px; transform:scale({scale:.5}); \
             transform-origin:top left; display:flex;",
            outer_h / scale,
        )
    } else {
        "width:100%; height:100%; transform:scale(1); transform-origin:top left; display:flex;"
            .to_string()
    };
    rsx! {
        div {
            id: "keys-fit-outer",
            style: "flex:1; min-width:0; min-height:0; overflow:hidden;",
            div { id: "keys-fit-inner", style: "{inner_style}", {children} }
        }
    }
}

/// The whole boot sequence, spawned from the Start gesture.
#[allow(clippy::too_many_arguments)]
async fn boot_rig(
    mut boot: Signal<Boot>,
    mut packs: Signal<Vec<PackRow>>,
    mut lanes: Signal<Vec<LaneRow>>,
    mut worklet_out: Signal<Option<Worklet>>,
    master: Signal<f32>,
    midi_inputs: Signal<Vec<String>>,
    midi_status: Signal<String>,
    astats: Signal<AudioStats>,
    mut clients: Signal<Option<(KeysRigClient, KeysRigStreamClient)>>,
) {
    let fail = |boot: &mut Signal<Boot>, msg: String| {
        hook_set_state("failed");
        boot.set(Boot::Failed(msg));
    };

    // 1. Audio worklet.
    let worklet = match boot_worklet().await {
        Ok(w) => w,
        Err(e) => return fail(&mut boot, e),
    };
    let _ = worklet.ctx.resume();

    // 2. The resolved lane program (engine, else cache).
    boot.set(Boot::Starting("fetching the lane program…".into()));
    let target = EngineTarget::current();
    let program = match fetch_lane_program(&target).await {
        Ok(p) => p,
        Err(e) => return fail(&mut boot, e),
    };

    // 3. Open the lanes (the rig renders silence until packs land).
    boot.set(Boot::Starting("opening lanes…".into()));
    match worklet
        .rpc("open_lanes", &[("program", program.program_json.as_str().into())], None)
        .await
    {
        Ok(v) if v.as_f64().is_some() => {}
        Ok(v) => return fail(&mut boot, format!("open_lanes: {}", js_str(v))),
        Err(e) => return fail(&mut boot, e),
    }
    let tracks = lane_tracks(&program.lanes);
    lanes.set(
        program
            .lanes
            .iter()
            .zip(tracks)
            .map(|(l, track)| LaneRow {
                engine: l.engine.clone(),
                name: l.name.clone(),
                key: l.key.clone(),
                track,
                volume: 1.0,
                muted: false,
                peak: 0.0,
            })
            .collect(),
    );
    hook_set_lanes(&lanes.read());
    worklet_out.set(Some(worklet.clone()));

    // W9: install (or refresh, on a re-boot) the in-tab backend and hand
    // it the worklet; establish the in-process vox clients KeysRigRemote
    // mounts over. Clients survive a re-boot — the backend is stable and
    // only its worklet handle swaps.
    let backend =
        crate::web_keys_backend::install(&profile_from_path().unwrap_or_default(), &program);
    crate::web_keys_backend::set_worklet(Some(worklet.clone()));
    if clients.peek().is_none() {
        match backend.clients().await {
            Ok(pair) => clients.set(Some(pair)),
            Err(e) => {
                tracing::warn!("in-process keys clients failed to establish: {e:?}");
            }
        }
    }

    boot.set(Boot::Running);
    hook_set_state("running");

    // Diagnostics onto the state hook now that the worklet exists:
    // `audioState()` (the AudioContext's state — 'suspended' is the autoplay
    // trap wearing its disguise) and `noteOn(note, vel)` / `noteOff(note)`
    // (raw injection past the UI, for the e2e test and remote debugging).
    if let Some(w) = web_sys::window() {
        if let Ok(hook) = Reflect::get(&w, &"__ftsRig".into()) {
            let ctx2 = worklet.ctx.clone();
            let audio_state = Closure::<dyn FnMut() -> JsValue>::new(move || {
                // Reflect, not the typed getter — `AudioContextState` isn't in
                // this crate's web-sys feature set and a string is all we need.
                Reflect::get(ctx2.as_ref(), &"state".into())
                    .unwrap_or_else(|_| JsValue::from_str("unknown"))
            });
            let _ = Reflect::set(&hook, &"audioState".into(), audio_state.as_ref());
            audio_state.forget();
            let wk = worklet.clone();
            let note_on = Closure::<dyn FnMut(f64, f64)>::new(move |n: f64, v: f64| {
                wk.midi(0x90, n as u8, v as u8);
            });
            let _ = Reflect::set(&hook, &"noteOn".into(), note_on.as_ref());
            note_on.forget();
            let wk = worklet.clone();
            let note_off = Closure::<dyn FnMut(f64)>::new(move |n: f64| {
                wk.midi(0x80, n as u8, 64);
            });
            let _ = Reflect::set(&hook, &"noteOff".into(), note_off.as_ref());
            note_off.forget();
        }
    }

    // 4. WebMIDI in the background.
    spawn(init_webmidi(worklet.clone(), midi_inputs, midi_status));

    // 5. Meter poll (~10 Hz) + audio-stats poll (2 Hz — the processor's
    // load window only turns over every ~0.7 s anyway).
    spawn(poll_peaks(worklet.clone(), worklet_out, lanes, master));
    spawn(poll_audio_stats(worklet.clone(), worklet_out, astats));

    // 6. Stream the packs, lane order. Small packs stream whole (the W6
    // path — seconds); packs past PROGRESSIVE_THRESHOLD go PROGRESSIVE:
    // plan + rank-0 + attach now (playable), detail afterwards. Phase A
    // gets EVERY pack attached before any pack's detail fills in.
    packs.set(
        program
            .packs
            .iter()
            .map(|p| PackRow {
                key: p.key.clone(),
                name: p.name.clone(),
                variant: String::new(),
                total: 0,
                bytes: 0,
                phase: PackPhase::Queued,
                seg_done: 0,
                seg_total: 1,
            })
            .collect(),
    );
    hook_set_packs(&packs.peek());
    let listing = fetch_pack_listing(&target).await.unwrap_or_default();
    // Progressive (big) packs FIRST: their phase-A cost is a plan + a few
    // MB of rank-0, and it turns a piano playable in seconds. Their
    // detail fill then runs CONCURRENTLY with the small packs' whole-file
    // streams (two vox connections — modest), so a key pressed in the
    // first minute pulls its segment forward immediately.
    let (big_idx, small_idx): (Vec<usize>, Vec<usize>) = {
        let rows = packs.peek();
        (0..rows.len()).partition(|&i| {
            pick_variant(&listing, &rows[i].name)
                .map(|w| w.size_bytes > web_packs::PROGRESSIVE_THRESHOLD)
                .unwrap_or(false)
        })
    };
    let mut jobs = Vec::new();
    for i in big_idx {
        if let Some(job) = stream_one_pack(&target, &listing, i, packs, &worklet).await {
            jobs.push(job);
        }
    }
    let fill = run_progressive_fill(&target, jobs, packs, &worklet);
    let whole = async {
        for i in small_idx {
            let _ = stream_one_pack(&target, &listing, i, packs, &worklet).await;
        }
    };
    futures_util::join!(fill, whole);
    hook_set_state("ready");
}

/// Update row `i` in place and mirror to the JS hook.
fn update_row(packs: &mut Signal<Vec<PackRow>>, i: usize, f: impl FnOnce(&mut PackRow)) {
    {
        let mut rows = packs.write();
        if let Some(row) = rows.get_mut(i) {
            f(row);
        }
    }
    hook_set_packs(&packs.peek());
    // Mirror pack usability onto the backend's lane `live` flags (the
    // remote UI's tree/mixer light up as packs attach).
    let usable: Vec<(String, bool)> = packs
        .peek()
        .iter()
        .map(|r| (r.key.clone(), r.phase.usable()))
        .collect();
    crate::web_keys_backend::on_packs(&usable);
}

/// OPFS-first fetch of one manifest pack, attach + lane reload on success.
/// Packs past [`web_packs::PROGRESSIVE_THRESHOLD`] return a
/// [`ProgressiveJob`]: attached PLAYABLE here (plan + rank-0), detail
/// filled by [`run_progressive_fill`].
async fn stream_one_pack(
    target: &EngineTarget,
    listing: &[PackInfo],
    i: usize,
    mut packs: Signal<Vec<PackRow>>,
    worklet: &Worklet,
) -> Option<ProgressiveJob> {
    let (key, name) = {
        let rows = packs.peek();
        let row = rows.get(i)?;
        (row.key.clone(), row.name.clone())
    };

    // Already cached (either variant)? Attach with no network at all.
    for variant in ["proxy", "full"] {
        if let Some(bytes) = web_packs::cached_pack(&name, variant).await {
            let total = bytes.byte_length() as u64;
            update_row(&mut packs, i, |r| {
                r.variant = variant.to_string();
                r.total = total;
                r.bytes = total;
            });
            attach_pack(worklet, &key, bytes, i, &mut packs).await;
            return None;
        }
    }

    let Some(want) = pick_variant(listing, &name) else {
        update_row(&mut packs, i, |r| {
            r.phase = PackPhase::Failed("not offered by the pack host".into());
        });
        return None;
    };
    if want.size_bytes > web_packs::PROGRESSIVE_THRESHOLD {
        return start_progressive(target, &want, &key, i, packs, worklet).await;
    }
    update_row(&mut packs, i, |r| {
        r.variant = want.variant.clone();
        r.total = want.size_bytes;
        r.phase = PackPhase::Streaming;
    });

    let progress = move |ev: PackEvent| {
        // `Signal` is Copy — take a local so the closure stays `Fn`.
        let mut packs = packs;
        match ev {
            PackEvent::Progress { bytes, total } => {
                update_row(&mut packs, i, |r| {
                    r.bytes = bytes;
                    if total > 0 {
                        r.total = total;
                    }
                });
            }
            PackEvent::Verifying => {
                update_row(&mut packs, i, |r| r.phase = PackPhase::Verifying);
            }
        }
    };
    match web_packs::ensure_pack(target, &want, &progress).await {
        Ok(bytes) => attach_pack(worklet, &key, bytes, i, &mut packs).await,
        Err(e) => update_row(&mut packs, i, |r| r.phase = PackPhase::Failed(e)),
    }
    None
}

/// Transfer the pack bytes into the worklet and reload the lanes so the
/// running instruments pick it up.
async fn attach_pack(
    worklet: &Worklet,
    key: &str,
    bytes: js_sys::ArrayBuffer,
    i: usize,
    packs: &mut Signal<Vec<PackRow>>,
) {
    // Refuse attaches past the tab-memory sanity cap (see the cap's doc) —
    // resident = every pack already Ready, plus this one. Pack bytes live
    // on the worklet's JS heap (attach-by-handle), not in wasm memory.
    let incoming = bytes.byte_length() as u64;
    let resident: u64 = packs
        .read()
        .iter()
        .filter(|r| r.phase.usable())
        .map(|r| r.total)
        .sum();
    if resident + incoming > JS_PACK_RESIDENT_SANITY_CAP {
        update_row(packs, i, |r| r.phase = PackPhase::Deferred);
        return;
    }
    let attach = worklet
        .rpc(
            "attach_pack",
            &[("key", key.into()), ("bytes", bytes.clone().into())],
            Some(bytes.as_ref()),
        )
        .await;
    match attach {
        Ok(v) if v.as_bool() == Some(true) => {
            let _ = worklet.rpc("reload_lanes", &[], None).await;
            update_row(packs, i, |r| {
                r.phase = PackPhase::Ready;
                r.seg_done = r.seg_total.max(1);
            });
        }
        Ok(v) => update_row(packs, i, |r| {
            r.phase = PackPhase::Failed(format!("attach: {}", js_str(v)));
        }),
        Err(e) => update_row(packs, i, |r| r.phase = PackPhase::Failed(e)),
    }
}

// ── Progressive streaming (W7) ─────────────────────────────────────────────

/// Page-allocated worklet pack ids for progressive attaches. Starts at
/// 2^20 so it can never collide with the processor's own whole-buffer
/// counter (which starts at 1 and counts packs, not segments).
fn next_prog_id() -> u32 {
    thread_local! {
        static NEXT: Cell<u32> = const { Cell::new(1 << 20) };
    }
    NEXT.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    })
}

/// One attached-but-still-filling pack: the plan, what the worklet has,
/// what OPFS has, and the fetch queue (misses jump the front).
struct ProgressiveJob {
    row: usize,
    name: String,
    variant: String,
    /// Worklet pack id (page-allocated, see [`next_prog_id`]).
    id: u32,
    total: u64,
    sha256: String,
    segments: Vec<web_packs::SegRow>,
    /// Parallel to `segments`: COMMITTED in the `.sparse` OPFS file
    /// (persisted — what a refresh resumes from).
    received: Vec<bool>,
    /// Parallel to `segments`: delivered to the worklet this session.
    delivered: Vec<bool>,
    /// Segment indices still to fetch, rank order; misses move to front.
    queue: std::collections::VecDeque<usize>,
    /// A miss bumped this job's queue — service it before round-robin.
    urgent: bool,
    /// Indices written to OPFS since the last commit (only a COMMIT makes
    /// them safe to mark `received` — OPFS writables persist on close).
    uncommitted: Vec<usize>,
    writer: web_packs::SparseWriter,
    /// Sample segments (rank > 0) — the Resolution denominator share.
    sample_total: u32,
}

impl ProgressiveJob {
    fn sample_done(&self) -> u32 {
        self.segments
            .iter()
            .zip(&self.delivered)
            .filter(|(s, d)| s.rank > 0 && **d)
            .count() as u32
    }

    fn playable_pct(&self) -> u8 {
        if self.sample_total == 0 {
            return 100;
        }
        ((u64::from(self.sample_done()) * 100) / u64::from(self.sample_total)) as u8
    }

    /// A worklet read-miss landed in `[start, start+len)` — move the plan
    /// segments covering it (not yet delivered) to the queue front.
    fn bump_range(&mut self, start: u64, len: u64) {
        let end = start.saturating_add(len);
        let hit: Vec<usize> = self
            .segments
            .iter()
            .enumerate()
            .filter(|(idx, s)| {
                !self.delivered[*idx] && s.start < end && s.start + s.len > start
            })
            .map(|(idx, _)| idx)
            .collect();
        if hit.is_empty() {
            return;
        }
        self.queue.retain(|i| !hit.contains(i));
        for idx in hit.into_iter().rev() {
            self.queue.push_front(idx);
        }
        self.urgent = true;
    }

    fn ledger(&self) -> web_packs::PlanLedger {
        web_packs::PlanLedger {
            name: self.name.clone(),
            variant: self.variant.clone(),
            total: self.total,
            sha256: self.sha256.clone(),
            segments: self.segments.clone(),
            received: self.received.clone(),
        }
    }
}

/// Mirror a job's delivery state onto its pack row (phase / bytes /
/// Resolution counters) and the JS hook.
fn sync_progressive_row(job: &ProgressiveJob, packs: &mut Signal<Vec<PackRow>>) {
    let bytes: u64 = job
        .segments
        .iter()
        .zip(&job.delivered)
        .filter(|(_, d)| **d)
        .map(|(s, _)| s.len)
        .sum();
    let (done, pct) = (job.sample_done(), job.playable_pct());
    update_row(packs, job.row, |r| {
        r.bytes = bytes;
        r.seg_done = done;
        r.seg_total = job.sample_total.max(1);
        r.phase = PackPhase::Playable { pct };
    });
}

/// Start one progressive pack: plan (engine, else the stored ledger),
/// resume delivered segments from OPFS, fetch any missing rank-0
/// segments, attach + reload — the pack turns PLAYABLE here. Detail is
/// [`run_progressive_fill`]'s job.
async fn start_progressive(
    target: &EngineTarget,
    want: &PackWant,
    key: &str,
    i: usize,
    mut packs: Signal<Vec<PackRow>>,
    worklet: &Worklet,
) -> Option<ProgressiveJob> {
    let fail = |packs: &mut Signal<Vec<PackRow>>, msg: String| {
        update_row(packs, i, |r| r.phase = PackPhase::Failed(msg));
    };
    update_row(&mut packs, i, |r| {
        r.variant = want.variant.clone();
        r.total = want.size_bytes;
        r.phase = PackPhase::Streaming;
    });
    web_packs::request_persist().await;

    // Plan: the engine's, validated against (or replaced by) the stored
    // ledger so a refresh resumes instead of restarting. Total + sha come
    // from the pack listing (`want`); the plan is only the segment tiling.
    let fetched =
        web_packs::fetch_pack_plan(target, &want.name, &want.variant, want.size_bytes).await;
    let stored = web_packs::plan_ledger_get(&want.name, &want.variant).await;
    let (total, sha256, segments, received) = match (fetched, stored) {
        (Ok(plan), stored) => {
            let segments: Vec<web_packs::SegRow> = plan
                .iter()
                .map(|s| web_packs::SegRow {
                    start: s.start,
                    len: s.len,
                    rank: u32::try_from(s.rank).unwrap_or(u32::MAX),
                })
                .collect();
            match stored {
                Some(s) if s.matches(want.size_bytes, &segments) => {
                    (want.size_bytes, want.sha256.clone(), segments, s.received)
                }
                _ => {
                    // Plan changed (or first visit): stale sparse bytes
                    // would fail the whole-file sha — start clean.
                    web_packs::discard_sparse(&want.name, &want.variant).await;
                    let n = segments.len();
                    (want.size_bytes, want.sha256.clone(), segments, vec![false; n])
                }
            }
        }
        (Err(_), Some(s)) => {
            // Offline: resume what the ledger knows; fetching waits for
            // the host to come back.
            let received = s.received.clone();
            (s.total, s.sha256.clone(), s.segments, received)
        }
        (Err(e), None) => {
            // Diagnostic: does read_range work where pack_plan failed?
            let probe = match crate::remote::establish_verbose::<PackLibraryClient>(target).await {
                Ok(c) => {
                    match web_packs::read_range_to_vec(&c, &want.name, &want.variant, 0, 64).await {
                        Ok(b) => format!("read_range probe OK ({} bytes)", b.len()),
                        Err(pe) => format!("read_range probe FAILED: {pe}"),
                    }
                }
                Err(ce) => format!("probe connect failed: {ce}"),
            };
            fail(&mut packs, format!("{e} [{probe}]"));
            return None;
        }
    };
    let sample_total = segments.iter().filter(|s| s.rank > 0).count() as u32;

    let writer = match web_packs::SparseWriter::open(&want.name, &want.variant).await {
        Ok(w) => w,
        Err(e) => {
            fail(&mut packs, format!("OPFS sparse file: {e}"));
            return None;
        }
    };
    let mut job = ProgressiveJob {
        row: i,
        name: want.name.clone(),
        variant: want.variant.clone(),
        id: next_prog_id(),
        total,
        sha256,
        delivered: vec![false; segments.len()],
        queue: std::collections::VecDeque::new(),
        urgent: false,
        uncommitted: Vec::new(),
        writer,
        sample_total,
        segments,
        received,
    };
    web_packs::plan_ledger_put(&job.ledger()).await;

    // Resume: push already-committed bytes from OPFS to the worklet as
    // CONTIGUOUS RUNS (the sparse store doesn't care about plan
    // boundaries; runs keep a half-streamed piano to a handful of reads).
    let sparse = web_packs::sparse_name(&job.name, &job.variant);
    const MAX_RUN: u64 = 64 * 1024 * 1024;
    let mut run: Option<(u64, u64, Vec<usize>)> = None; // (start, end, indices)
    let mut runs: Vec<(u64, u64, Vec<usize>)> = Vec::new();
    let mut order: Vec<usize> = (0..job.segments.len()).collect();
    order.sort_by_key(|&idx| job.segments[idx].start);
    for idx in order {
        if !job.received[idx] {
            continue;
        }
        let s = &job.segments[idx];
        match &mut run {
            Some((start, end, indices))
                if *end == s.start && (s.start + s.len) - *start <= MAX_RUN =>
            {
                *end = s.start + s.len;
                indices.push(idx);
            }
            _ => {
                if let Some(r) = run.take() {
                    runs.push(r);
                }
                run = Some((s.start, s.start + s.len, vec![idx]));
            }
        }
    }
    if let Some(r) = run.take() {
        runs.push(r);
    }
    for (start, end, indices) in runs {
        match web_packs::read_file_slice(&sparse, start, end - start).await {
            Ok(bytes) => {
                worklet.pack_segment(job.id, start, &bytes);
                for idx in indices {
                    job.delivered[idx] = true;
                }
            }
            Err(_) => {
                // The sparse file disagrees with the ledger — treat those
                // segments as missing and refetch them.
                for idx in indices {
                    job.received[idx] = false;
                }
            }
        }
    }

    // Rank-0 segments the resume did not cover come from the network NOW —
    // the attach cannot succeed without them.
    let rank0_missing: Vec<usize> = job
        .segments
        .iter()
        .enumerate()
        .filter(|(idx, s)| s.rank == 0 && !job.delivered[*idx])
        .map(|(idx, _)| idx)
        .collect();
    if !rank0_missing.is_empty() {
        let client: PackLibraryClient = match crate::remote::establish_verbose(target).await {
            Ok(c) => c,
            Err(e) => {
                fail(&mut packs, format!("pack host unreachable: {e}"));
                return None;
            }
        };
        for idx in rank0_missing {
            let seg = job.segments[idx];
            match web_packs::read_range_to_vec(&client, &job.name, &job.variant, seg.start, seg.len)
                .await
            {
                Ok(bytes) => {
                    worklet.pack_segment(job.id, seg.start, &bytes);
                    if let Err(e) = job.writer.write_at(seg.start, &bytes).await {
                        fail(&mut packs, format!("OPFS write: {e}"));
                        return None;
                    }
                    job.delivered[idx] = true;
                    job.uncommitted.push(idx);
                }
                Err(e) => {
                    fail(&mut packs, e);
                    return None;
                }
            }
        }
        // Commit rank-0 immediately — the open set is the one part a
        // refresh must never lose.
        if job.writer.commit().await.is_ok() {
            for idx in job.uncommitted.drain(..) {
                job.received[idx] = true;
            }
            web_packs::plan_ledger_put(&job.ledger()).await;
        }
    }

    // Attach (rank-0 is in the worklet's sparse store) + reload lanes.
    if let Err(e) = worklet.attach_progressive(key, job.id, job.total).await {
        fail(&mut packs, e);
        return None;
    }
    let _ = worklet.rpc("reload_lanes", &[], None).await;

    // Queue the rest in rank order.
    let mut pending: Vec<usize> =
        (0..job.segments.len()).filter(|&idx| !job.delivered[idx]).collect();
    pending.sort_by_key(|&idx| (job.segments[idx].rank, job.segments[idx].start));
    job.queue = pending.into();

    sync_progressive_row(&job, &mut packs);
    if job.queue.is_empty() {
        finalize_progressive_job(job, &mut packs).await;
        return None;
    }
    Some(job)
}

/// Phase B: fill in every progressive pack's detail, one segment at a
/// time over ONE shared connection, misses first (~1 Hz poll of the
/// worklet's read-miss list — a played hole jumps the queue).
async fn run_progressive_fill(
    target: &EngineTarget,
    mut jobs: Vec<ProgressiveJob>,
    mut packs: Signal<Vec<PackRow>>,
    worklet: &Worklet,
) {
    if jobs.is_empty() {
        return;
    }
    let mut client: Option<PackLibraryClient> = None;
    let mut last_miss_poll = 0.0f64;
    while !jobs.is_empty() {
        // ── Misses jump the queue (rate-limited ~1 Hz) ───────────────────
        let now = js_sys::Date::now();
        if now - last_miss_poll >= 1000.0 {
            last_miss_poll = now;
            for (id, start, len) in worklet.take_misses().await {
                if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
                    job.bump_range(start, len);
                }
            }
        }

        // ── Pick: urgent (miss-bumped) jobs first, else the first open ───
        let ji = jobs
            .iter()
            .position(|j| j.urgent && !j.queue.is_empty())
            .or_else(|| jobs.iter().position(|j| !j.queue.is_empty()));
        let Some(ji) = ji else {
            // Queues are empty but jobs remain — finalize stragglers.
            for job in jobs.drain(..) {
                finalize_progressive_job(job, &mut packs).await;
            }
            break;
        };
        let (idx, seg, row, name, variant) = {
            let job = &mut jobs[ji];
            job.urgent = false;
            let Some(idx) = job.queue.pop_front() else { continue };
            if job.delivered[idx] {
                continue;
            }
            (idx, job.segments[idx], job.row, job.name.clone(), job.variant.clone())
        };

        // ── Fetch: establish lazily, one reconnect, then give up ─────────
        let mut fetched: Result<Vec<u8>, String> = Err("not attempted".into());
        for _attempt in 0..2 {
            if client.is_none() {
                match crate::remote::establish_verbose(target).await {
                    Ok(c) => client = Some(c),
                    Err(e) => {
                        fetched = Err(format!("pack host unreachable: {e}"));
                        continue;
                    }
                }
            }
            let c = client.as_ref().expect("client established");
            match web_packs::read_range_to_vec(c, &name, &variant, seg.start, seg.len).await {
                Ok(bytes) => {
                    fetched = Ok(bytes);
                    break;
                }
                Err(e) => {
                    // Drop the (possibly dead) connection; the next
                    // attempt re-establishes.
                    client = None;
                    fetched = Err(e);
                }
            }
        }
        let bytes = match fetched {
            Ok(bytes) => bytes,
            Err(e) => {
                update_row(&mut packs, row, |r| r.phase = PackPhase::Failed(e));
                jobs.remove(ji);
                continue;
            }
        };
        let job = &mut jobs[ji];

        // ── Deliver to the worklet (audible immediately) + persist ───────
        worklet.pack_segment(job.id, seg.start, &bytes);
        job.delivered[idx] = true;
        match job.writer.write_at(seg.start, &bytes).await {
            Ok(committed) => {
                job.uncommitted.push(idx);
                if committed {
                    for u in job.uncommitted.drain(..) {
                        job.received[u] = true;
                    }
                    web_packs::plan_ledger_put(&job.ledger()).await;
                }
            }
            Err(e) => {
                // OPFS trouble: keep streaming into the worklet (audible >
                // resumable), but this session won't refresh-resume.
                tracing::warn!("sparse write failed for {}: {e}", job.name);
            }
        }
        sync_progressive_row(job, &mut packs);

        if job.queue.is_empty() {
            let job = jobs.remove(ji);
            finalize_progressive_job(job, &mut packs).await;
        }
    }
}

/// All segments delivered: final OPFS commit, whole-file sha verify,
/// rename into place, row → `Ready`.
async fn finalize_progressive_job(mut job: ProgressiveJob, packs: &mut Signal<Vec<PackRow>>) {
    if job.writer.commit().await.is_ok() {
        for u in job.uncommitted.drain(..) {
            job.received[u] = true;
        }
    }
    // Everything written is now committed; the ledger is about to be
    // dropped by finalize on success anyway.
    for r in job.received.iter_mut() {
        *r = true;
    }
    web_packs::plan_ledger_put(&job.ledger()).await;
    let ProgressiveJob { name, variant, total, sha256, row, writer, sample_total, .. } = job;
    let _ = writer.finish().await;
    match web_packs::finalize_progressive(&name, &variant, total, &sha256).await {
        Ok(()) => update_row(packs, row, |r| {
            r.bytes = total;
            r.seg_done = sample_total.max(1);
            r.seg_total = sample_total.max(1);
            r.phase = PackPhase::Ready;
        }),
        Err(e) => update_row(packs, row, |r| r.phase = PackPhase::Failed(e)),
    }
}

/// Walk the (live) MIDIInputMap: attach the forwarding handler to any
/// input not yet seen (by port id) and rebuild the current name list. The
/// map itself only holds currently-available ports, so a re-walk after a
/// `statechange` is the whole hot-plug story.
fn sync_midi_inputs(
    access: &web_sys::MidiAccess,
    worklet: &Worklet,
    seen: &Rc<RefCell<std::collections::HashSet<String>>>,
) -> Vec<String> {
    let mut names = Vec::new();
    let inputs = access.inputs();
    if let Ok(Some(iter)) = js_sys::try_iter(&inputs) {
        for entry in iter.flatten() {
            // Map iteration yields [key, value] pairs.
            let pair: Array = entry.into();
            let Ok(input) = pair.get(1).dyn_into::<web_sys::MidiInput>() else {
                continue;
            };
            let name = input.name().unwrap_or_else(|| "MIDI input".into());
            names.push(name.clone());
            let id = input.id();
            if !seen.borrow_mut().insert(id) {
                continue; // handler already attached to this port
            }
            let w = worklet.clone();
            let onmsg = Closure::<dyn FnMut(web_sys::MidiMessageEvent)>::new(
                move |ev: web_sys::MidiMessageEvent| {
                    // `set_midi_port` selects which input forwards (omni
                    // when none is selected) — checked per message so the
                    // choice applies live, no re-enumeration.
                    if !crate::web_keys_backend::midi_allows(&name) {
                        return;
                    }
                    if let Ok(data) = ev.data()
                        && !data.is_empty()
                    {
                        let d1 = data.get(1).copied().unwrap_or(0);
                        let d2 = data.get(2).copied().unwrap_or(0);
                        w.midi(data[0], d1, d2);
                    }
                },
            );
            input.set_onmidimessage(Some(onmsg.as_ref().unchecked_ref()));
            onmsg.forget(); // page-lifetime
        }
    }
    names
}

/// Enumerate WebMIDI inputs and forward every 3-byte message to the rig.
/// Keeps the MIDIAccess alive and listens to `statechange` (W11), so a
/// controller plugged in AFTER the page loaded starts working the moment
/// the browser sees it — and a denied permission is SAID, not shown as
/// the ambiguous "no MIDI devices".
async fn init_webmidi(
    worklet: Worklet,
    mut names_out: Signal<Vec<String>>,
    mut status_out: Signal<String>,
) {
    let Some(window) = web_sys::window() else { return };
    let Ok(promise) = window.navigator().request_midi_access() else {
        status_out.set("WebMIDI is not available in this browser".into());
        hook_set_midi(&[], false);
        return;
    };
    let access = match JsFuture::from(promise).await {
        Ok(a) => a,
        Err(_) => {
            // Rejected: permission denied (or the policy blocks it).
            status_out
                .set("MIDI permission blocked — allow it in the address bar".into());
            hook_set_midi(&[], false);
            return;
        }
    };
    let Ok(access) = access.dyn_into::<web_sys::MidiAccess>() else {
        status_out.set("WebMIDI is not available in this browser".into());
        hook_set_midi(&[], false);
        return;
    };
    status_out.set(String::new());

    let seen: Rc<RefCell<std::collections::HashSet<String>>> =
        Rc::new(RefCell::new(std::collections::HashSet::new()));
    let names = sync_midi_inputs(&access, &worklet, &seen);
    crate::web_keys_backend::set_webmidi_inputs(names.clone());
    hook_set_midi(&names, true);
    names_out.set(names);

    // Hot-plug: on any port state change, re-walk the map — new inputs get
    // the handler, gone inputs drop out of the list. The raw JS closure
    // only pokes a channel; the re-walk (and the Signal writes) happen in
    // THIS task, inside the Dioxus runtime like every other signal write.
    let (tx, mut rx) = futures_channel::mpsc::unbounded::<()>();
    let onstate = Closure::<dyn FnMut(web_sys::Event)>::new(move |_ev: web_sys::Event| {
        let _ = tx.unbounded_send(());
    });
    access.set_onstatechange(Some(onstate.as_ref().unchecked_ref()));
    onstate.forget(); // page-lifetime (keeps the sender alive too)
    use futures_util::StreamExt as _;
    while rx.next().await.is_some() {
        let names = sync_midi_inputs(&access, &worklet, &seen);
        crate::web_keys_backend::set_webmidi_inputs(names.clone());
        hook_set_midi(&names, true);
        names_out.set(names);
    }
}

/// True while `worklet` is still the page's live worklet — poll loops
/// park themselves once the latency-hint restart swapped it out.
fn still_current(worklet: &Worklet, current: &Signal<Option<Worklet>>) -> bool {
    current.peek().as_ref() == Some(worklet)
}

/// ~10 Hz meter poll off the worklet's `trackPeaks`.
async fn poll_peaks(
    worklet: Worklet,
    current: Signal<Option<Worklet>>,
    mut lanes: Signal<Vec<LaneRow>>,
    mut master: Signal<f32>,
) {
    loop {
        architect::platform::sleep(std::time::Duration::from_millis(100)).await;
        if !still_current(&worklet, &current) {
            return;
        }
        let Ok(v) = worklet.rpc("track_peaks", &[], None).await else {
            continue;
        };
        let arr: Array = match v.dyn_into() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let peaks: Vec<f32> = arr.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
        let m = peaks.first().copied().unwrap_or(0.0);
        master.set(m);
        hook_set_peak(f64::from(m));
        // The backend's meter tick: Status (+ any queued MIDI) onto the
        // remote UI's event stream.
        crate::web_keys_backend::on_peaks(&peaks);
        // Keep the compat strip coherent with mixer edits made through
        // KeysRigRemote (both index off the same program lane order).
        let strip = crate::web_keys_backend::strip_state();
        let mut rows = lanes.write();
        for (i, row) in rows.iter_mut().enumerate() {
            row.peak = peaks.get(row.track as usize).copied().unwrap_or(0.0);
            if let Some((volume, muted)) = strip.get(i) {
                row.volume = *volume;
                row.muted = *muted;
            }
        }
    }
}

/// 2 Hz audio-stats poll (W8): the processor's render-load window plus the
/// AudioContext latency figures, mirrored to the panel signal and the
/// `__ftsRig` hook.
async fn poll_audio_stats(
    worklet: Worklet,
    current: Signal<Option<Worklet>>,
    mut astats: Signal<AudioStats>,
) {
    loop {
        architect::platform::sleep(std::time::Duration::from_millis(500)).await;
        if !still_current(&worklet, &current) {
            return;
        }
        let Some(stats) = worklet.audio_stats().await else {
            continue;
        };
        astats.set(stats);
        hook_set_audio(stats);
        crate::web_keys_backend::on_voices(stats.voices);
    }
}

// ── UI pieces ──────────────────────────────────────────────────────────────

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// The Audio panel popover (W8): latency visibility + render-load tracing
/// + the latencyHint selector (whose change re-boots the audio path).
#[component]
fn AudioPanel(
    astats: Signal<AudioStats>,
    restart: Callback<()>,
    boot_running: bool,
) -> Element {
    let a = astats();
    let load_pct = (a.load * 100.0).max(0.0);
    let load_bar_pct = load_pct.min(100.0);
    let load_color = if load_pct < 60.0 {
        "#22c55e"
    } else if load_pct < 90.0 {
        "#fbbf24"
    } else {
        "#ef4444"
    };
    let uptime_s = if a.sample_rate > 0.0 {
        a.quanta * 128.0 / a.sample_rate
    } else {
        0.0
    };
    let hint = latency_hint_pref();
    let row_style = "display:flex; align-items:baseline; gap:8px;";
    let key_style = "font-size:11px; color:#71717a; flex:1;";
    let val_style = "font-size:12px; color:#e4e4e7;";

    rsx! {
        div {
            "data-testid": "audio-popover",
            style: "display:flex; flex-direction:column; gap:8px; padding:12px; font-family:sans-serif; color:#e4e4e7;",
            div { style: row_style,
                span { style: key_style, "sample rate" }
                span { style: val_style, "{a.sample_rate as u32} Hz" }
            }
            div { style: row_style,
                span { style: key_style, "quantum" }
                span { style: val_style, "128 samples" }
            }
            div { style: row_style,
                span { style: key_style, "latency (base + output)" }
                span { style: val_style,
                    "{a.base_ms:.1} + {a.output_ms:.1} = {a.latency_ms():.1} ms"
                }
            }
            // W11: past ~20 ms of OUTPUT latency the hint alone cannot help —
            // the browser's output buffer is the wall, and only launching it
            // smaller moves it.
            if a.output_ms > 20.0 {
                span {
                    "data-testid": "audio-latency-tip",
                    style: "font-size:11px; color:#fbbf24; line-height:1.4;",
                    "for lowest latency, launch the browser with --audio-buffer-size=256"
                }
            }
            div { style: "display:flex; flex-direction:column; gap:3px;",
                div { style: row_style,
                    span { style: key_style, "render load" }
                    span { style: "font-size:12px; color:{load_color};", "{load_pct:.0}%" }
                }
                div {
                    style: "height:6px; border-radius:3px; background:#18181b; border:1px solid #27272a; overflow:hidden;",
                    div {
                        "data-testid": "audio-load",
                        style: "height:100%; width:{load_bar_pct}%; background:{load_color};",
                    }
                }
            }
            div { style: row_style,
                span { style: key_style, "worst quantum" }
                span { style: val_style, "{a.worst_ms:.0} ms" }
            }
            if a.voices >= 0 {
                div { style: row_style,
                    span { style: key_style, "voices" }
                    span { style: val_style, "{a.voices}" }
                }
            }
            div { style: row_style,
                span { style: key_style, "latency hint" }
                select {
                    "data-testid": "audio-latency-hint",
                    disabled: !boot_running,
                    style: "background:#18181b; color:#e4e4e7; border:1px solid #27272a; border-radius:4px; font-size:12px; padding:2px 6px;",
                    value: "{hint}",
                    onchange: move |ev| {
                        let choice = ev.value();
                        if !valid_hint(&choice) {
                            return;
                        }
                        if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
                            let _ = s.set_item(LATENCY_HINT_KEY, &choice);
                        }
                        // Applying the hint needs a NEW AudioContext — run
                        // the full re-boot (cached program + OPFS packs, no
                        // network).
                        restart.call(());
                    },
                    option { value: "ultra", "ultra (2.7 ms)" }
                    option { value: "low", "low (5 ms)" }
                    option { value: "interactive", "interactive" }
                    option { value: "balanced", "balanced" }
                    option { value: "playback", "playback" }
                }
            }
            div { style: row_style,
                span { style: key_style, "quanta / uptime" }
                span { style: val_style, "{a.quanta as u64} · {uptime_s:.0} s" }
            }
        }
    }
}

/// The Soundsource Manager popover: per-pack rows + the demo MIDI player
/// + the WebMIDI indicator.
#[component]
fn SoundsourceManager(
    packs: Signal<Vec<PackRow>>,
    worklet: Signal<Option<Worklet>>,
    lanes: Signal<Vec<LaneRow>>,
    midi_inputs: Signal<Vec<String>>,
    midi_status: Signal<String>,
    demo: Signal<(Option<usize>, bool, u64)>,
    boot_running: bool,
) -> Element {
    let rows = packs.read().clone();
    let footprint: u64 = rows
        .iter()
        .map(|r| if r.phase == PackPhase::Ready { r.total } else { r.bytes })
        .sum();
    let inputs = midi_inputs.read().clone();
    let midi_msg = midi_status.read().clone();

    rsx! {
        div {
            "data-testid": "ssm-popover",
            style: "display:flex; flex-direction:column; gap:10px; padding:12px; font-family:sans-serif; color:#e4e4e7;",
            if rows.is_empty() {
                span { style: "font-size:12px; color:#52525b;", "No packs yet — start the rig." }
            }
            for (i, row) in rows.iter().enumerate() {
                PackRowView { index: i, row: row.clone(), packs, worklet }
            }
            if !rows.is_empty() {
                span { style: "font-size:11px; color:#52525b;", "Local footprint: {mb(footprint)}" }
            }
            // ── Demo MIDI player ───────────────────────────────────────
            span { style: "font-size:12px; font-weight:700; letter-spacing:1px; color:#a1a1aa; text-transform:uppercase; margin-top:4px;", "Demo player" }
            DemoPlayer { worklet, demo, enabled: boot_running }
            // ── MIDI indicator ─────────────────────────────────────────
            span { style: "font-size:12px; font-weight:700; letter-spacing:1px; color:#a1a1aa; text-transform:uppercase; margin-top:4px;", "MIDI" }
            // Denied/unsupported reads very differently from "nothing
            // plugged in" — say which one it is (W11). Hot-plugged devices
            // appear here live via the statechange listener.
            if !midi_msg.is_empty() {
                span {
                    "data-testid": "midi-status",
                    style: "font-size:12px; color:#ef4444;",
                    "{midi_msg}"
                }
            } else if inputs.is_empty() {
                span {
                    "data-testid": "midi-status",
                    style: "font-size:12px; color:#52525b;",
                    "no MIDI devices (hot-plug works — connect one any time)"
                }
            } else {
                for name in inputs {
                    span { style: "font-size:12px; color:#22c55e;", "● {name}" }
                }
            }
        }
    }
}

#[component]
fn PackRowView(
    index: usize,
    row: PackRow,
    packs: Signal<Vec<PackRow>>,
    worklet: Signal<Option<Worklet>>,
) -> Element {
    let pct = if row.total > 0 {
        ((row.bytes as f64 / row.total as f64) * 100.0).min(100.0)
    } else if row.phase == PackPhase::Ready {
        100.0
    } else {
        0.0
    };
    let (color, detail) = match &row.phase {
        PackPhase::Queued => ("#52525b", String::new()),
        PackPhase::Streaming => ("#fbbf24", format!("{} / {}", mb(row.bytes), mb(row.total))),
        PackPhase::Verifying => ("#38bdf8", "sha256…".into()),
        PackPhase::Ready => ("#22c55e", mb(row.total)),
        // Amber-green: sounding, still gaining detail.
        PackPhase::Playable { pct } => {
            ("#a3e635", format!("playable — loading detail {pct}%"))
        }
        PackPhase::Deferred => ("#a78bfa", "cached — past the tab-memory sanity cap".into()),
        PackPhase::Failed(e) => ("#ef4444", e.clone()),
    };
    let name = row.name.clone();
    let variant = row.variant.clone();
    let failed = matches!(row.phase, PackPhase::Failed(_));

    let retry = move |_| {
        let mut packs = packs;
        spawn(async move {
            // Reset the row; the boot task is gone by now, so re-run the
            // single-pack flow here (row state carries what it needs).
            update_row(&mut packs, index, |r| {
                r.phase = PackPhase::Queued;
                r.bytes = 0;
                r.seg_done = 0;
                r.seg_total = 1;
            });
            let target = EngineTarget::current();
            let listing = fetch_pack_listing(&target).await.unwrap_or_default();
            if let Some(w) = worklet.peek().clone() {
                if let Some(job) = stream_one_pack(&target, &listing, index, packs, &w).await {
                    run_progressive_fill(&target, vec![job], packs, &w).await;
                }
            }
        });
    };
    let delete = {
        let name = name.clone();
        let variant = variant.clone();
        move |_| {
            let name = name.clone();
            let variant = variant.clone();
            let mut packs = packs;
            spawn(async move {
                for v in [variant.as_str(), "proxy", "full"] {
                    if !v.is_empty() {
                        let _ = web_packs::delete_pack(&name, v).await;
                    }
                }
                update_row(&mut packs, index, |r| {
                    r.phase = PackPhase::Queued;
                    r.bytes = 0;
                    r.seg_done = 0;
                    r.seg_total = 1;
                });
            });
        }
    };

    rsx! {
        div {
            "data-testid": "ssm-row-{row.name}",
            style: "display:flex; flex-direction:column; gap:4px; padding:8px; border:1px solid #27272a; border-radius:8px; background:#0c0c0e;",
            div { style: "display:flex; align-items:baseline; gap:8px;",
                span { style: "font-size:13px; color:#e4e4e7; font-weight:500; flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{row.name}" }
                span {
                    "data-testid": "ssm-state-{row.name}",
                    style: "font-size:11px; color:{color};",
                    "{row.phase.label()}"
                }
            }
            div {
                style: "height:5px; border-radius:3px; background:#18181b; overflow:hidden;",
                div {
                    "data-testid": "ssm-progress-{row.name}",
                    style: "height:100%; width:{pct}%; background:{color};",
                }
            }
            div { style: "display:flex; align-items:center; gap:8px;",
                span { style: "font-size:11px; color:#52525b; flex:1;", "{detail}" }
                if failed {
                    button {
                        style: "padding:1px 8px; border-radius:4px; background:transparent; color:#a1a1aa; border:1px solid #27272a; font-size:11px; cursor:pointer;",
                        onclick: retry,
                        "Retry"
                    }
                }
                button {
                    style: "padding:1px 8px; border-radius:4px; background:transparent; color:#71717a; border:1px solid #27272a; font-size:11px; cursor:pointer;",
                    onclick: delete,
                    "Delete"
                }
            }
        }
    }
}

/// The bundled-SMF player: play/loop/stop, targeting the whole rig
/// (scheduler choice (b): a main-thread scheduler over `KeysWorklet.midi`
/// against the authored tempo — the worklet has no MIDI-item surface yet).
#[component]
fn DemoPlayer(
    worklet: Signal<Option<Worklet>>,
    demo: Signal<(Option<usize>, bool, u64)>,
    enabled: bool,
) -> Element {
    let (playing, looping, _) = demo();

    let play = move |n: usize, do_loop: bool| {
        let Some(w) = worklet.peek().clone() else { return };
        let mut demo = demo;
        let generation = {
            let (_, _, g) = *demo.peek();
            let g = g + 1;
            demo.set((Some(n), do_loop, g));
            g
        };
        spawn(run_demo(w, n, demo, generation));
    };

    let stop = move |_| {
        let mut demo = demo;
        let (_, _, g) = *demo.peek();
        demo.set((None, false, g + 1));
        if let Some(w) = worklet.peek().clone() {
            w.all_notes_off();
        }
    };

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:6px;",
            for (n, (label, _)) in DEMO_FILES.iter().enumerate() {
                div { style: "display:flex; align-items:center; gap:8px;",
                    span { style: "font-size:12px; color:#e4e4e7; flex:1;", "{label}" }
                    if playing == Some(n) {
                        span { style: "font-size:11px; color:#22c55e;", if looping { "looping" } else { "playing" } }
                    }
                    button {
                        "data-testid": "demo-play-{n}",
                        disabled: !enabled,
                        style: "padding:1px 10px; border-radius:4px; background:transparent; color:#e4e4e7; border:1px solid #27272a; font-size:11px; cursor:pointer;",
                        onclick: move |_| play(n, false),
                        "Play"
                    }
                    button {
                        disabled: !enabled,
                        style: "padding:1px 10px; border-radius:4px; background:transparent; color:#a1a1aa; border:1px solid #27272a; font-size:11px; cursor:pointer;",
                        onclick: move |_| play(n, true),
                        "Loop"
                    }
                }
            }
            button {
                "data-testid": "demo-stop",
                style: "padding:3px 10px; border-radius:4px; background:transparent; color:#ef4444; border:1px solid #27272a; font-size:11px; cursor:pointer; align-self:flex-start;",
                onclick: stop,
                "Stop"
            }
        }
    }
}

/// One timed MIDI message.
struct DemoEvent {
    at_ms: f64,
    bytes: [u8; 3],
}

/// Flatten a demo SMF into wall-clock MIDI at [`DEMO_BPM`].
fn demo_events(n: usize) -> (Vec<DemoEvent>, f64) {
    let Some(snap) = daw_proto::midi::smf::parse(DEMO_FILES[n].1, 0) else {
        return (Vec::new(), 0.0);
    };
    let ms_per_ppq = 60_000.0 / DEMO_BPM / snap.ppq;
    let mut events = Vec::new();
    for note in &snap.notes {
        events.push(DemoEvent {
            at_ms: note.start_ppq * ms_per_ppq,
            bytes: [0x90 | (note.channel & 0x0f), note.pitch, note.velocity.max(1)],
        });
        events.push(DemoEvent {
            at_ms: (note.start_ppq + note.length_ppq) * ms_per_ppq,
            bytes: [0x80 | (note.channel & 0x0f), note.pitch, 0],
        });
    }
    for cc in &snap.ccs {
        events.push(DemoEvent {
            at_ms: cc.position_ppq * ms_per_ppq,
            bytes: [0xb0 | (cc.channel & 0x0f), cc.controller, cc.value],
        });
    }
    for bend in &snap.pitch_bends {
        let raw = (i32::from(bend.value) + 8192).clamp(0, 16383);
        events.push(DemoEvent {
            at_ms: bend.position_ppq * ms_per_ppq,
            bytes: [
                0xe0 | (bend.channel & 0x0f),
                (raw & 0x7f) as u8,
                ((raw >> 7) & 0x7f) as u8,
            ],
        });
    }
    events.sort_by(|a, b| a.at_ms.partial_cmp(&b.at_ms).unwrap_or(std::cmp::Ordering::Equal));
    let len_ms = snap.length_ppq * ms_per_ppq;
    (events, len_ms)
}

/// The scheduler task: fires events against `Date.now()`, cancelled by a
/// generation bump; loops when the demo state says so.
async fn run_demo(
    worklet: Worklet,
    n: usize,
    mut demo: Signal<(Option<usize>, bool, u64)>,
    generation: u64,
) {
    let (events, len_ms) = demo_events(n);
    if events.is_empty() {
        return;
    }
    let mut origin = js_sys::Date::now();
    'outer: loop {
        for ev in &events {
            loop {
                let (playing, _, g) = *demo.peek();
                if g != generation || playing != Some(n) {
                    worklet.all_notes_off();
                    return;
                }
                let now = js_sys::Date::now();
                if now - origin >= ev.at_ms {
                    break;
                }
                let wait = (ev.at_ms - (now - origin)).clamp(1.0, 25.0);
                architect::platform::sleep(std::time::Duration::from_millis(wait as u64)).await;
            }
            worklet.midi(ev.bytes[0], ev.bytes[1], ev.bytes[2]);
        }
        // File done — loop or stop.
        let (playing, looping, g) = *demo.peek();
        if g != generation || playing != Some(n) || !looping {
            break 'outer;
        }
        origin += len_ms.max(1.0);
    }
    let (_, _, g) = *demo.peek();
    if g == generation {
        demo.set((None, false, g + 1));
    }
    worklet.all_notes_off();
}

// (The compat lane strip and on-screen octave that lived here until W9.5
// are gone: the remote UI's own keyboard and mixer are the input and
// per-lane controls. The e2e suite reaches lanes through `__ftsRig`'s
// `lanes()` / `setLaneMute()` instead of DOM testids.)

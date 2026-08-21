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
//! v1 is a MINIMAL rig page (lane list + volumes/mutes + on-screen keys +
//! master meter) rather than `signal_keys_ui::KeysRigRemote`: the remote
//! needs real `KeysRigClient`/`KeysRigStreamClient` vox clients, and a
//! local backend proxying 30+ service methods onto the worklet is a
//! follow-up, not a page.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use dioxus::prelude::*;
use js_sys::{Array, Object, Reflect};
use signal_keys_proto::KeysLaneProgram;
use signal_keys_proto::keys::KeysRigClient;
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

    /// Raw 3-byte MIDI to every lane.
    pub(crate) fn midi(&self, status: u8, d1: u8, d2: u8) {
        let arr = Array::of3(&status.into(), &d1.into(), &d2.into());
        self.fire("midi", &[("bytes", arr.into())]);
    }

    pub(crate) fn all_notes_off(&self) {
        self.fire("all_notes_off", &[]);
    }

    fn set_track_volume(&self, index: u32, volume: f64) {
        self.fire(
            "set_track_volume",
            &[("index", index.into()), ("volume", volume.into())],
        );
    }

    fn set_track_mute(&self, index: u32, muted: bool) {
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
    let ctx = AudioContext::new().map_err(|e| format!("AudioContext: {}", js_str(e)))?;
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
    let node = AudioWorkletNode::new_with_options(&ctx, "daw-standalone", &opts)
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
    Failed(String),
}

impl PackPhase {
    fn label(&self) -> &str {
        match self {
            Self::Queued => "queued",
            Self::Streaming => "streaming",
            Self::Verifying => "verifying",
            Self::Ready => "ready",
            Self::Failed(_) => "failed",
        }
    }
}

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

/// The `window.__ftsRig` state hook W5's Playwright test polls.
#[derive(Default)]
struct RigHook {
    state: String,
    packs_json: String,
    master_peak: f64,
}

thread_local! {
    static RIG_HOOK: RefCell<RigHook> = RefCell::default();
}

/// One row of the hook's `packStates()` JSON.
#[derive(facet::Facet, Default)]
struct HookPack {
    name: String,
    state: String,
    bytes: u64,
    total: u64,
}

fn hook_set_state(state: &str) {
    RIG_HOOK.with(|h| h.borrow_mut().state = state.to_string());
}

fn hook_set_packs(rows: &[PackRow]) {
    let rows: Vec<HookPack> = rows
        .iter()
        .map(|r| HookPack {
            name: r.name.clone(),
            state: r.phase.label().to_string(),
            bytes: r.bytes,
            total: r.total,
        })
        .collect();
    let json = facet_json::to_string(&rows).unwrap_or_default();
    RIG_HOOK.with(|h| h.borrow_mut().packs_json = json);
}

fn hook_set_peak(peak: f64) {
    RIG_HOOK.with(|h| h.borrow_mut().master_peak = peak);
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
    let _ = Reflect::set(&obj, &"state".into(), state.as_ref());
    let _ = Reflect::set(&obj, &"packStates".into(), packs.as_ref());
    let _ = Reflect::set(&obj, &"masterPeak".into(), peak.as_ref());
    let _ = Reflect::set(&window, &"__ftsRig".into(), &obj);
    // Page-lifetime closures.
    state.forget();
    packs.forget();
    peak.forget();
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
    let ssm_open = use_signal(|| false);
    // (index playing, looping, generation) — generation cancels schedulers.
    let demo = use_signal(|| (Option::<usize>::None, false, 0u64));

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
        spawn(boot_rig(boot, packs, lanes, worklet, master, midi_inputs));
    });

    let ready_count = packs.read().iter().filter(|p| p.phase == PackPhase::Ready).count();
    let pack_count = packs.read().len();
    let streaming = pack_count > ready_count;

    rsx! {
        document::Style { {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;}*{box-sizing:border-box;}"} }
        div {
            style: "min-height:100vh; background:#0a0a0a; color:#e4e4e7; font-family:sans-serif; display:flex; flex-direction:column;",
            // ── Top bar ────────────────────────────────────────────────
            header {
                style: "display:flex; align-items:center; gap:10px; padding:10px 14px; border-bottom:1px solid #27272a; position:relative;",
                span { style: "font-weight:700; letter-spacing:1px; font-size:12px; color:#a1a1aa;", "FASTTRACKSTUDIO" }
                span { style: "font-size:11px; letter-spacing:1px; color:#52525b; text-transform:uppercase;", "keys · {profile}" }
                div { style: "flex:1;" }
                MasterMeter { master }
                button {
                    "data-testid": "ssm-button",
                    style: "display:flex; align-items:center; gap:6px; padding:4px 10px; border-radius:6px; background:#111113; color:#e4e4e7; border:1px solid #27272a; font-size:12px; cursor:pointer;",
                    onclick: {
                        let mut ssm_open = ssm_open;
                        move |_| { let v = *ssm_open.peek(); ssm_open.set(!v); }
                    },
                    span { "🎛" }
                    if pack_count == 0 {
                        span { style: "color:#71717a;", "Soundsources" }
                    } else if streaming {
                        span { style: "color:#fbbf24;", "{ready_count}/{pack_count}" }
                    } else {
                        span { style: "color:#22c55e;", "{ready_count}/{pack_count}" }
                    }
                }
                if ssm_open() {
                    SoundsourceManager { packs, worklet, lanes, midi_inputs, demo, boot_running: matches!(boot(), Boot::Running) }
                }
            }
            // ── Body ───────────────────────────────────────────────────
            main { style: "flex:1; display:flex; flex-direction:column; gap:16px; padding:20px; max-width:860px; width:100%; margin:0 auto;",
                match boot() {
                    Boot::Idle => rsx! {
                        div { style: "display:flex; flex-direction:column; align-items:center; gap:12px; margin:auto;",
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
                        div { style: "display:flex; flex-direction:column; align-items:center; gap:10px; margin:auto;",
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
                        LaneList { lanes, packs, worklet }
                        OnScreenKeys { worklet }
                    },
                }
            }
        }
    }
}

/// The whole boot sequence, spawned from the Start gesture.
async fn boot_rig(
    mut boot: Signal<Boot>,
    mut packs: Signal<Vec<PackRow>>,
    mut lanes: Signal<Vec<LaneRow>>,
    mut worklet_out: Signal<Option<Worklet>>,
    master: Signal<f32>,
    midi_inputs: Signal<Vec<String>>,
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
    worklet_out.set(Some(worklet.clone()));
    boot.set(Boot::Running);
    hook_set_state("running");

    // 4. WebMIDI in the background.
    spawn(init_webmidi(worklet.clone(), midi_inputs));

    // 5. Meter poll.
    spawn(poll_peaks(worklet.clone(), lanes, master));

    // 6. Stream the packs, lane order (synth lanes are MBs — playable in
    // seconds; the pianos fill in behind and reload their lanes).
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
            })
            .collect(),
    );
    hook_set_packs(&packs.peek());
    let listing = fetch_pack_listing(&target).await.unwrap_or_default();
    for i in 0..program.packs.len() {
        stream_one_pack(&target, &listing, i, packs, &worklet).await;
    }
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
}

/// OPFS-first fetch of one manifest pack, attach + lane reload on success.
async fn stream_one_pack(
    target: &EngineTarget,
    listing: &[PackInfo],
    i: usize,
    mut packs: Signal<Vec<PackRow>>,
    worklet: &Worklet,
) {
    let (key, name) = {
        let rows = packs.peek();
        let Some(row) = rows.get(i) else { return };
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
            return;
        }
    }

    let Some(want) = pick_variant(listing, &name) else {
        update_row(&mut packs, i, |r| {
            r.phase = PackPhase::Failed("not offered by the pack host".into());
        });
        return;
    };
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
            update_row(packs, i, |r| r.phase = PackPhase::Ready);
        }
        Ok(v) => update_row(packs, i, |r| {
            r.phase = PackPhase::Failed(format!("attach: {}", js_str(v)));
        }),
        Err(e) => update_row(packs, i, |r| r.phase = PackPhase::Failed(e)),
    }
}

/// Enumerate WebMIDI inputs and forward every 3-byte message to the rig.
async fn init_webmidi(worklet: Worklet, mut names_out: Signal<Vec<String>>) {
    let Some(window) = web_sys::window() else { return };
    let Ok(promise) = window.navigator().request_midi_access() else {
        return;
    };
    let Ok(access) = JsFuture::from(promise).await else {
        return; // denied / unsupported — the indicator stays "no MIDI"
    };
    let Ok(access) = access.dyn_into::<web_sys::MidiAccess>() else {
        return;
    };
    let inputs = access.inputs();
    let mut names = Vec::new();
    if let Ok(Some(iter)) = js_sys::try_iter(&inputs) {
        for entry in iter.flatten() {
            // Map iteration yields [key, value] pairs.
            let pair: Array = entry.into();
            let Ok(input) = pair.get(1).dyn_into::<web_sys::MidiInput>() else {
                continue;
            };
            names.push(input.name().unwrap_or_else(|| "MIDI input".into()));
            let w = worklet.clone();
            let onmsg = Closure::<dyn FnMut(web_sys::MidiMessageEvent)>::new(
                move |ev: web_sys::MidiMessageEvent| {
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
    names_out.set(names);
}

/// ~10 Hz meter poll off the worklet's `trackPeaks`.
async fn poll_peaks(worklet: Worklet, mut lanes: Signal<Vec<LaneRow>>, mut master: Signal<f32>) {
    loop {
        architect::platform::sleep(std::time::Duration::from_millis(100)).await;
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
        let mut rows = lanes.write();
        for row in rows.iter_mut() {
            row.peak = peaks.get(row.track as usize).copied().unwrap_or(0.0);
        }
    }
}

// ── UI pieces ──────────────────────────────────────────────────────────────

#[component]
fn MasterMeter(master: Signal<f32>) -> Element {
    let pct = (master() .clamp(0.0, 1.0) * 100.0) as u32;
    rsx! {
        div {
            style: "width:90px; height:8px; border-radius:4px; background:#18181b; border:1px solid #27272a; overflow:hidden;",
            title: "master",
            div { style: "height:100%; width:{pct}%; background:#22c55e;" }
        }
    }
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// The Soundsource Manager popover: per-pack rows + the demo MIDI player
/// + the WebMIDI indicator.
#[component]
fn SoundsourceManager(
    packs: Signal<Vec<PackRow>>,
    worklet: Signal<Option<Worklet>>,
    lanes: Signal<Vec<LaneRow>>,
    midi_inputs: Signal<Vec<String>>,
    demo: Signal<(Option<usize>, bool, u64)>,
    boot_running: bool,
) -> Element {
    let rows = packs.read().clone();
    let footprint: u64 = rows
        .iter()
        .map(|r| if r.phase == PackPhase::Ready { r.total } else { r.bytes })
        .sum();
    let inputs = midi_inputs.read().clone();

    rsx! {
        div {
            "data-testid": "ssm-popover",
            style: "position:absolute; top:44px; right:12px; z-index:50; width:380px; max-height:70vh; overflow-y:auto; background:#111113; border:1px solid #27272a; border-radius:10px; padding:12px; display:flex; flex-direction:column; gap:10px; box-shadow:0 8px 30px rgba(0,0,0,.5);",
            span { style: "font-size:12px; font-weight:700; letter-spacing:1px; color:#a1a1aa; text-transform:uppercase;", "Soundsources" }
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
            if inputs.is_empty() {
                span { style: "font-size:12px; color:#52525b;", "no MIDI devices" }
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
            });
            let target = EngineTarget::current();
            let listing = fetch_pack_listing(&target).await.unwrap_or_default();
            if let Some(w) = worklet.peek().clone() {
                stream_one_pack(&target, &listing, index, packs, &w).await;
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

/// The lane list: per-lane ready state, volume + mute (daw track ops on
/// the worklet), and a small peak bar.
#[component]
fn LaneList(
    lanes: Signal<Vec<LaneRow>>,
    packs: Signal<Vec<PackRow>>,
    worklet: Signal<Option<Worklet>>,
) -> Element {
    let rows = lanes.read().clone();
    let pack_phase = |key: &str| -> Option<PackPhase> {
        packs.read().iter().find(|p| p.key == key).map(|p| p.phase.clone())
    };
    rsx! {
        div { style: "display:flex; flex-direction:column; gap:6px;",
            for (i, row) in rows.iter().enumerate() {
                {
                    let phase = if row.key.is_empty() { None } else { pack_phase(&row.key) };
                    let (dot, dot_title) = match &phase {
                        None if row.key.is_empty() => ("#52525b", "no pack (silent natively too)"),
                        None | Some(PackPhase::Queued) => ("#52525b", "waiting"),
                        Some(PackPhase::Streaming) | Some(PackPhase::Verifying) => ("#fbbf24", "streaming"),
                        Some(PackPhase::Ready) => ("#22c55e", "ready"),
                        Some(PackPhase::Failed(_)) => ("#ef4444", "failed"),
                    };
                    let peak_pct = (row.peak.clamp(0.0, 1.0) * 100.0) as u32;
                    let vol_pct = (row.volume * 80.0) as i64;
                    let track = row.track;
                    let muted = row.muted;
                    rsx! {
                        div {
                            key: "{row.engine}/{row.name}",
                            style: "display:flex; align-items:center; gap:10px; padding:8px 12px; border:1px solid #27272a; border-radius:8px; background:#111113;",
                            span { style: "width:8px; height:8px; border-radius:999px; background:{dot}; flex:none;", title: "{dot_title}" }
                            div { style: "display:flex; flex-direction:column; min-width:140px;",
                                span { style: "font-size:13px; color:#e4e4e7; font-weight:500;", "{row.name}" }
                                span { style: "font-size:10px; color:#52525b; text-transform:uppercase; letter-spacing:1px;", "{row.engine}" }
                            }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: "{vol_pct}",
                                style: "flex:1; accent-color:#22c55e;",
                                oninput: move |ev| {
                                    let v = ev.value().parse::<f64>().unwrap_or(80.0) / 80.0;
                                    if let Some(row) = lanes.write().get_mut(i) { row.volume = v; }
                                    if let Some(w) = worklet.peek().clone() { w.set_track_volume(track, v); }
                                },
                            }
                            button {
                                style: if muted {
                                    "padding:1px 8px; border-radius:4px; background:#7f1d1d; color:#fecaca; border:1px solid #7f1d1d; font-size:11px; cursor:pointer;"
                                } else {
                                    "padding:1px 8px; border-radius:4px; background:transparent; color:#a1a1aa; border:1px solid #27272a; font-size:11px; cursor:pointer;"
                                },
                                onclick: move |_| {
                                    let now = !muted;
                                    if let Some(row) = lanes.write().get_mut(i) { row.muted = now; }
                                    if let Some(w) = worklet.peek().clone() { w.set_track_mute(track, now); }
                                },
                                "M"
                            }
                            div { style: "width:60px; height:6px; border-radius:3px; background:#18181b; overflow:hidden; flex:none;",
                                div { style: "height:100%; width:{peak_pct}%; background:#22c55e;" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An on-screen octave (C4–C5) that plays the whole rig.
#[component]
fn OnScreenKeys(worklet: Signal<Option<Worklet>>) -> Element {
    // (midi note, label, is black)
    const KEYS: [(u8, &str, bool); 13] = [
        (60, "C", false),
        (61, "C#", true),
        (62, "D", false),
        (63, "D#", true),
        (64, "E", false),
        (65, "F", false),
        (66, "F#", true),
        (67, "G", false),
        (68, "G#", true),
        (69, "A", false),
        (70, "A#", true),
        (71, "B", false),
        (72, "C", false),
    ];
    rsx! {
        div { style: "display:flex; gap:3px; justify-content:center; padding:10px 0;",
            for (note, label, black) in KEYS {
                button {
                    style: if black {
                        "width:34px; height:90px; border-radius:0 0 6px 6px; background:#18181b; color:#a1a1aa; border:1px solid #27272a; font-size:10px; cursor:pointer; align-self:flex-start; display:flex; align-items:flex-end; justify-content:center; padding-bottom:6px;"
                    } else {
                        "width:44px; height:130px; border-radius:0 0 6px 6px; background:#e4e4e7; color:#3f3f46; border:1px solid #27272a; font-size:11px; cursor:pointer; display:flex; align-items:flex-end; justify-content:center; padding-bottom:8px;"
                    },
                    onpointerdown: move |_| {
                        if let Some(w) = worklet.peek().clone() { w.midi(0x90, note, 100); }
                    },
                    onpointerup: move |_| {
                        if let Some(w) = worklet.peek().clone() { w.midi(0x80, note, 0); }
                    },
                    onpointerleave: move |_| {
                        if let Some(w) = worklet.peek().clone() { w.midi(0x80, note, 0); }
                    },
                    "{label}"
                }
            }
        }
    }
}

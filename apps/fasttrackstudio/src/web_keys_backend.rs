//! W9 — the in-tab keys backend behind `signal_keys_ui::KeysRigRemote`.
//!
//! The browser keys page mounts the SAME remote UI the desktop and phone
//! use (`KeysRigRemote`, which takes `KeysRigClient` + `KeysRigStreamClient`
//! from Dioxus context). Here those are REAL generated vox clients — served
//! locally: this module implements the `#[architect::rpc]` `KeysRig` trait
//! plus its `#[subscribe]` stream source, builds the same `LayerRouter` the
//! engine mounts, and serves it over `architect::LocalServer`'s in-process
//! memory link (which exists on wasm precisely for this shape). The UI
//! cannot tell it isn't talking to the network engine; only the transport
//! differs.
//!
//! State lives page-side (this shared struct, written by the page's boot /
//! pack / poll code); audio lives in the worklet. Method-by-method
//! semantics (implemented = real effect on the in-tab rig; stub = honest
//! no-op documented here because the state it edits only exists on the
//! engine):
//!
//! | method | semantics |
//! |---|---|
//! | `start` | resume the AudioContext (boot itself needs the page's Start gesture) |
//! | `stop` | suspend the AudioContext (packs + program stay) |
//! | `status` | running/ctx state, profile as loaded preset, master peak + per-engine/lane meters from `trackPeaks`, voices from `audio_stats`, WebMIDI port, `last_error` |
//! | `presets` | ONE row — the engine profile this page resolved (`loaded: true`) |
//! | `rescan` / `load_preset` | stubs: the browser has no preset library; sets an honest `last_error` |
//! | `tree` | profile → engines → lanes from the lane program (`live` = pack attached) |
//! | `lane_program_wire` | the cached program this page booted from |
//! | `mixer` | engines/lanes with faders, mutes, solos, pack-as-patch |
//! | `set_layer_gain` / `set_engine_gain` / `set_master_gain` | real: dB → linear onto the worklet's lane/engine-folder/rig tracks |
//! | `set_layer_mute` / `set_engine_mute` / `set_layer_solo` | real: solo logic mirrors the native mixer (any solo silences un-soloed lanes) |
//! | `set_layer_exclude_global` | stored + rendered; no DSP effect (no Global Controls run in-tab) |
//! | `set_layer_patch` / `set_layer_variant` / `clear_layer` | stubs (module sources are resolved on the engine); honest `last_error` |
//! | `set_module_gain` / `set_module_enabled` | stubs — lanes run one fused instrument in-tab, there is no per-module bus |
//! | `set_drone` | stub — the lane program carries no drone engines |
//! | `set_engine_order` | real (layout only): reorders the mixer's engine cards |
//! | `layer_detail` / `engine_detail` | real shape, empty macro list (macros drive engine-side DSP rebuilds) |
//! | `rig_macros` / `set_*_global` / `set_layer_macro` | stubs, empty macros |
//! | `perform` / `press_stack` / `capture_stack` / `set_perform_mode` | mode is stored; the profile's stacks are engine-side state → empty list |
//! | `trigger` / `pitch_bend` / `mod_wheel` | real: raw MIDI into the worklet (and the monitor ring) |
//! | `midi_ports` | WebMIDI input names |
//! | `set_midi_port` | real: selects which WebMIDI input forwards (empty = omni) |
//! | `midi_recent` | the page's recent-MIDI ring (everything sent to the worklet: WebMIDI, demo player, on-screen keys, `trigger`) |
//! | `events` (stream) | a local `PubSub` hub — Status/Midi at the peak-poll rate, Mixer/Tree/Library/Perform on change — mirroring the native meter pump |

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use architect::dispatch::CurrentThreadDispatcher;
use architect::{layers, HasDispatcher, Layer, PubSub, Services};
use midicore_proto::MidiEvent;
use signal_keys_proto::keys::{
    KeysEvent, KeysRig as KeysRigSvc, KeysRigClient, KeysRigStreamClient, KeysRigStreamSource,
};
use signal_keys_proto::{
    KeysEngineDetail, KeysEngineModel, KeysLaneProgram, KeysLayerDetail, KeysLayerModel,
    KeysMacro, KeysMeter, KeysMixer, KeysModule, KeysNode, KeysPerform, KeysPreset, KeysStatus,
};

use crate::web_keys_rig::Worklet;

/// The monitor ring's depth (matches the native backend's order of
/// magnitude — enough for `held_notes` + the monitor panel).
const MIDI_RING: usize = 128;

// ── Shared page-side state ──────────────────────────────────────────────────

/// One mixer lane (a worklet layer track).
#[derive(Clone, Default)]
pub(crate) struct LaneStrip {
    pub name: String,
    pub engine: String,
    /// Pack spec-path key (empty = nothing to stream — silent natively too).
    pub key: String,
    /// The pack's library name (display).
    pub pack: String,
    /// Worklet project-track index.
    pub track: u32,
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    pub exclude_global: bool,
    /// The lane's pack is attached (ready or playable) — it can sound.
    pub live: bool,
    pub peak: f32,
}

/// One engine (a worklet folder track).
#[derive(Clone, Default)]
struct EngineStrip {
    name: String,
    /// Worklet folder-track index.
    track: u32,
    gain_db: f32,
    muted: bool,
    peak: f32,
}

#[derive(Default)]
struct WebRigShared {
    worklet: Option<Worklet>,
    profile: String,
    program: KeysLaneProgram,
    engines: Vec<EngineStrip>,
    lanes: Vec<LaneStrip>,
    master_db: f32,
    master_peak: f32,
    running: bool,
    voices: i32,
    /// Selected WebMIDI input (None = omni / all inputs forward).
    midi_port: Option<String>,
    midi_inputs: Vec<String>,
    midi_recent: VecDeque<MidiEvent>,
    midi_dirty: bool,
    perform_mode: u32,
    last_error: Option<String>,
}

type Shared = Rc<RefCell<WebRigShared>>;

thread_local! {
    /// The page's one backend (the wasm main thread is the only thread).
    static BACKEND: RefCell<Option<WebKeysBackend>> = const { RefCell::new(None) };
}

/// The installed backend, if the rig has booted.
pub(crate) fn backend() -> Option<WebKeysBackend> {
    BACKEND.with(|b| b.borrow().clone())
}

// ── The backend ─────────────────────────────────────────────────────────────

/// The in-tab keys backend handle. Cheap to clone (all state shared);
/// `CurrentThreadDispatcher` because the wasm page is single-threaded.
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub(crate) struct WebKeysBackend {
    shared: Shared,
    events: PubSub<KeysEvent>,
    /// Keeps the LocalServer acceptor tasks alive for the page's life.
    scope: Arc<architect::Scope>,
}

impl Services for WebKeysBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_keys_proto::keys::Service,
            signal_keys_proto::keys::StreamService
        ]
    }
}

impl KeysRigStreamSource for WebKeysBackend {
    fn events_hub(&self) -> &PubSub<KeysEvent> {
        &self.events
    }
}

/// Worklet track layout for a lane program: rig folder = 0, then per
/// engine its folder track followed by its lane tracks (the
/// `KeysLaneProgram` doc-comment contract, shared with `lane_tracks` in
/// web_keys_rig.rs).
fn build_strips(program: &KeysLaneProgram) -> (Vec<EngineStrip>, Vec<LaneStrip>) {
    let pack_name = |key: &str| {
        program
            .packs
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    };
    let mut engines: Vec<EngineStrip> = Vec::new();
    let mut lanes = Vec::new();
    let mut track = 0u32; // rig folder
    for lane in &program.lanes {
        if engines.last().map(|e| e.name.as_str()) != Some(lane.engine.as_str()) {
            track += 1;
            engines.push(EngineStrip {
                name: lane.engine.clone(),
                track,
                ..EngineStrip::default()
            });
        }
        track += 1;
        lanes.push(LaneStrip {
            name: lane.name.clone(),
            engine: lane.engine.clone(),
            key: lane.key.clone(),
            pack: pack_name(&lane.key),
            track,
            ..LaneStrip::default()
        });
    }
    (engines, lanes)
}

fn db_to_linear(db: f32) -> f64 {
    10f64.powf(f64::from(db) / 20.0)
}

/// Install (or, on the latency-hint re-boot, refresh) the page backend for
/// `program`. Keeps the existing hub + clients across a re-boot so the
/// mounted `KeysRigRemote` heals without a remount.
pub(crate) fn install(profile: &str, program: &KeysLaneProgram) -> WebKeysBackend {
    let (engines, lanes) = build_strips(program);
    if let Some(b) = backend() {
        {
            let mut s = b.shared.borrow_mut();
            s.profile = profile.to_string();
            s.program = program.clone();
            s.engines = engines;
            s.lanes = lanes;
        }
        b.publish_structure();
        return b;
    }
    let shared: Shared = Rc::new(RefCell::new(WebRigShared {
        profile: profile.to_string(),
        program: program.clone(),
        engines,
        lanes,
        perform_mode: 1,
        ..WebRigShared::default()
    }));
    // `architect::rig::events_hub()`'s strategy, spelled out — the `rig`
    // module itself is native-only (its pump is an OS thread).
    let b = WebKeysBackend {
        shared,
        events: PubSub::sliding(64),
        scope: architect::Scope::new(),
    };
    BACKEND.with(|slot| *slot.borrow_mut() = Some(b.clone()));
    b
}

/// Point the backend at the (re)booted worklet — or `None` while a re-boot
/// is in flight. Applies the current mixer to the new worklet's tracks.
pub(crate) fn set_worklet(worklet: Option<Worklet>) {
    let Some(b) = backend() else { return };
    let arrived = worklet.is_some();
    {
        let mut s = b.shared.borrow_mut();
        s.worklet = worklet;
        s.running = arrived;
    }
    if arrived {
        b.apply_mixer();
    }
    b.publish_status();
}

/// The page's pack rows changed: refresh each lane's `live` flag
/// (`(key, usable)` pairs) and publish Mixer + Tree when anything flipped.
pub(crate) fn on_packs(rows: &[(String, bool)]) {
    let Some(b) = backend() else { return };
    let changed = {
        let mut s = b.shared.borrow_mut();
        let mut changed = false;
        for lane in s.lanes.iter_mut() {
            let live = !lane.key.is_empty()
                && rows.iter().any(|(key, usable)| *usable && *key == lane.key);
            if lane.live != live {
                lane.live = live;
                changed = true;
            }
        }
        changed
    };
    if changed {
        b.publish_structure();
    }
}

/// One `trackPeaks` reading (rig folder first). Publishes Status (the
/// meter tick) and flushes the MIDI ring — together this IS the meter
/// pump, at the page's poll rate.
pub(crate) fn on_peaks(peaks: &[f32]) {
    let Some(b) = backend() else { return };
    {
        let mut s = b.shared.borrow_mut();
        s.master_peak = peaks.first().copied().unwrap_or(0.0);
        for e in s.engines.iter_mut() {
            e.peak = peaks.get(e.track as usize).copied().unwrap_or(0.0);
        }
        for l in s.lanes.iter_mut() {
            l.peak = peaks.get(l.track as usize).copied().unwrap_or(0.0);
        }
    }
    b.publish_status();
    let dirty = {
        let mut s = b.shared.borrow_mut();
        std::mem::take(&mut s.midi_dirty)
    };
    if dirty {
        let recent = b.midi_ring();
        b.events.publish(KeysEvent::Midi(recent));
    }
}

/// The audio-stats poll's voice count (−1 = unavailable).
pub(crate) fn on_voices(voices: i32) {
    if let Some(b) = backend() {
        b.shared.borrow_mut().voices = voices;
    }
}

/// Record one raw MIDI message into the monitor ring (called by
/// `Worklet::midi` — the single seam every source goes through).
pub(crate) fn midi_seen(status: u8, d1: u8, d2: u8) {
    let Some(b) = backend() else { return };
    let Ok((event, _)) = MidiEvent::decode(&[status, d1, d2]) else {
        return;
    };
    let mut s = b.shared.borrow_mut();
    if s.midi_recent.len() >= MIDI_RING {
        s.midi_recent.pop_front();
    }
    s.midi_recent.push_back(event);
    s.midi_dirty = true;
}

/// WebMIDI enumeration landed — the port list `midi_ports` serves.
pub(crate) fn set_webmidi_inputs(names: Vec<String>) {
    let Some(b) = backend() else { return };
    b.shared.borrow_mut().midi_inputs = names;
    b.publish_status();
}

/// Whether messages from WebMIDI input `name` forward to the rig (omni
/// unless `set_midi_port` picked one input).
pub(crate) fn midi_allows(name: &str) -> bool {
    match backend() {
        Some(b) => {
            let s = b.shared.borrow();
            s.midi_port.as_deref().is_none_or(|p| p == name)
        }
        None => true,
    }
}

/// Whether the MIDI port gate is OMNI (no specific input selected) — the
/// fresh-page default, proven by the e2e suite via `__ftsRig.midiOmni()`.
pub(crate) fn midi_omni() -> bool {
    backend().is_none_or(|b| b.shared.borrow().midi_port.is_none())
}

/// The compat lane strip's snapshot: `(linear volume, muted)` per lane, in
/// lane order — keeps the strip coherent with mixer edits made through
/// `KeysRigRemote`.
pub(crate) fn strip_state() -> Vec<(f64, bool)> {
    match backend() {
        Some(b) => b
            .shared
            .borrow()
            .lanes
            .iter()
            .map(|l| (db_to_linear(l.gain_db), l.muted))
            .collect(),
        None => Vec::new(),
    }
}

/// The compat strip's volume slider (linear 0..~1.25) for lane `i`.
#[allow(dead_code)] // the strip's fader path — kept for a future hook use
pub(crate) fn strip_set_volume(i: usize, linear: f64) {
    let Some(b) = backend() else { return };
    let db = 20.0 * linear.max(0.000_1).log10();
    let name = b.shared.borrow().lanes.get(i).map(|l| l.name.clone());
    if let Some(name) = name {
        KeysRigSvc::set_layer_gain(&b, name, db as f32);
    }
}

/// Set lane `i`'s mute explicitly (the `__ftsRig.setLaneMute` e2e hook —
/// the visible compat strip is gone; tests mute through this). Returns the
/// applied state.
pub(crate) fn strip_set_mute(i: usize, muted: bool) -> bool {
    let Some(b) = backend() else { return false };
    let name = b.shared.borrow().lanes.get(i).map(|l| l.name.clone());
    match name {
        Some(name) => {
            KeysRigSvc::set_layer_mute(&b, name, muted);
            muted
        }
        None => false,
    }
}

/// The compat strip's mute toggle for lane `i`. Returns the new state.
#[allow(dead_code)]
pub(crate) fn strip_toggle_mute(i: usize) -> bool {
    let Some(b) = backend() else { return false };
    let target = b
        .shared
        .borrow()
        .lanes
        .get(i)
        .map(|l| (l.name.clone(), !l.muted));
    match target {
        Some((name, muted)) => {
            KeysRigSvc::set_layer_mute(&b, name, muted);
            muted
        }
        None => false,
    }
}

impl WebKeysBackend {
    /// Establish the two typed clients over the in-process server — the
    /// exact objects `KeysRigRemote` consumes from context.
    pub(crate) async fn clients(
        &self,
    ) -> eyre::Result<(KeysRigClient, KeysRigStreamClient)> {
        let server =
            architect::LocalServer::serve(self.clone().into_router(), self.scope.clone());
        let rig: KeysRigClient = server.establish().await?;
        let stream: KeysRigStreamClient = server.establish().await?;
        Ok((rig, stream))
    }

    fn worklet(&self) -> Option<Worklet> {
        self.shared.borrow().worklet.clone()
    }

    /// Raw MIDI to the rig (the worklet records it into the monitor ring
    /// via [`midi_seen`]).
    fn send_midi(&self, status: u8, d1: u8, d2: u8) {
        if let Some(w) = self.worklet() {
            w.midi(status, d1, d2);
        }
    }

    /// Push the whole mixer state onto the worklet's tracks: master trim
    /// on the rig folder, engine trims/mutes on the folder tracks, lane
    /// gains + effective mutes (mute OR silenced-by-solo) on the lane
    /// tracks — the native `apply_mixer` shape over daw track ops.
    fn apply_mixer(&self) {
        let Some(w) = self.worklet() else { return };
        let s = self.shared.borrow();
        let any_solo = s.lanes.iter().any(|l| l.soloed);
        w.set_track_volume(0, db_to_linear(s.master_db));
        for e in &s.engines {
            w.set_track_volume(e.track, db_to_linear(e.gain_db));
            w.set_track_mute(e.track, e.muted);
        }
        for l in &s.lanes {
            w.set_track_volume(l.track, db_to_linear(l.gain_db));
            w.set_track_mute(l.track, l.muted || (any_solo && !l.soloed));
        }
    }

    fn set_error(&self, msg: &str) {
        self.shared.borrow_mut().last_error = Some(msg.to_string());
        self.publish_status();
    }

    fn midi_ring(&self) -> Vec<MidiEvent> {
        self.shared.borrow().midi_recent.iter().cloned().collect()
    }

    // ── model builders ───────────────────────────────────────────────────

    fn build_status(&self) -> KeysStatus {
        let s = self.shared.borrow();
        let mut meters = Vec::with_capacity(s.engines.len() + s.lanes.len());
        if s.running {
            for e in &s.engines {
                meters.push(KeysMeter {
                    kind: "engine".into(),
                    name: e.name.clone(),
                    peak: e.peak,
                });
            }
            for l in &s.lanes {
                meters.push(KeysMeter {
                    kind: "layer".into(),
                    name: l.name.clone(),
                    peak: l.peak,
                });
            }
        }
        KeysStatus {
            running: s.running,
            loaded_preset: (!s.program.program_json.is_empty())
                .then(|| s.profile.clone()),
            master_peak: s.master_peak,
            meters,
            voices: s.voices.max(0) as u32,
            midi_port: s.midi_port.clone(),
            last_error: s.last_error.clone(),
        }
    }

    fn build_mixer(&self) -> KeysMixer {
        let s = self.shared.borrow();
        KeysMixer {
            profile: s.profile.clone(),
            engines: s
                .engines
                .iter()
                .map(|e| KeysEngineModel {
                    name: e.name.clone(),
                    drone: None,
                    gain_db: e.gain_db,
                    muted: e.muted,
                    layers: s
                        .lanes
                        .iter()
                        .filter(|l| l.engine == e.name)
                        .map(lane_model)
                        .collect(),
                })
                .collect(),
            master_db: s.master_db,
        }
    }

    fn build_tree(&self) -> KeysNode {
        let s = self.shared.borrow();
        KeysNode {
            id: "keys".into(),
            label: s.profile.clone(),
            role: "preset".into(),
            live: s.lanes.iter().any(|l| l.live),
            children: s
                .engines
                .iter()
                .map(|e| KeysNode {
                    id: format!("keys/{}", e.name),
                    label: e.name.clone(),
                    role: "engine".into(),
                    live: s.lanes.iter().any(|l| l.engine == e.name && l.live),
                    children: s
                        .lanes
                        .iter()
                        .filter(|l| l.engine == e.name)
                        .map(|l| KeysNode {
                            id: format!("keys/{}/{}", e.name, l.name),
                            label: l.name.clone(),
                            role: "layer".into(),
                            live: l.live,
                            children: Vec::new(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn build_presets(&self) -> Vec<KeysPreset> {
        let s = self.shared.borrow();
        if s.program.program_json.is_empty() {
            return Vec::new();
        }
        let tags: Vec<String> = s.engines.iter().map(|e| e.name.clone()).collect();
        vec![KeysPreset {
            name: s.profile.clone(),
            kind: "Profile".into(),
            loaded: true,
            scope: "engine".into(),
            tags,
            variants: Vec::new(),
        }]
    }

    fn build_perform(&self) -> KeysPerform {
        let s = self.shared.borrow();
        KeysPerform {
            profile_name: s.profile.clone(),
            stacks: Vec::new(),
            active_stack: u32::MAX,
            perform_mode: s.perform_mode,
        }
    }

    // ── publishers (the native backend's publish_* trio) ─────────────────

    fn publish_status(&self) {
        self.events.publish(KeysEvent::Status(self.build_status()));
    }

    fn publish_mixer(&self) {
        self.events.publish(KeysEvent::Mixer(self.build_mixer()));
    }

    /// Everything shape-related: library, tree, mixer, perform.
    fn publish_structure(&self) {
        self.events
            .publish(KeysEvent::Library(self.build_presets()));
        self.events.publish(KeysEvent::Tree(self.build_tree()));
        self.publish_mixer();
        self.events
            .publish(KeysEvent::Perform(self.build_perform()));
        self.publish_status();
    }
}

fn lane_model(l: &LaneStrip) -> KeysLayerModel {
    KeysLayerModel {
        name: l.name.clone(),
        engine: l.engine.clone(),
        patch: l.pack.clone(),
        preset: String::new(),
        gain_db: l.gain_db,
        muted: l.muted,
        soloed: l.soloed,
        live: l.live,
        key_lo: 0,
        key_hi: 127,
        modules: vec![lane_module(l)],
        exclude_global: l.exclude_global,
    }
}

/// The lane's single fused module: in-tab a lane runs one `KeysInstrument`
/// (no per-module bus), so the zoom shows one slot holding the pack.
fn lane_module(l: &LaneStrip) -> KeysModule {
    KeysModule {
        index: 0,
        slot: "A".into(),
        patch: l.pack.clone(),
        variant: String::new(),
        live: l.live,
        gain_db: 0.0,
        enabled: true,
        ..KeysModule::default()
    }
}

/// The honest text for engine-only operations.
const ENGINE_ONLY: &str =
    "the in-tab rig plays the engine's resolved profile; preset/module edits need the engine";

impl KeysRigSvc for WebKeysBackend {
    fn start(&self) {
        if let Some(w) = self.worklet() {
            w.resume();
        }
        self.shared.borrow_mut().running = self.worklet().is_some();
        self.publish_status();
    }

    fn stop(&self) {
        if let Some(w) = self.worklet() {
            w.suspend();
        }
        self.shared.borrow_mut().running = false;
        self.publish_status();
    }

    fn status(&self) -> KeysStatus {
        self.build_status()
    }

    fn presets(&self) -> Vec<KeysPreset> {
        self.build_presets()
    }

    fn rescan(&self) {
        // No pack library to re-scan in-tab; re-publish what we serve.
        self.events
            .publish(KeysEvent::Library(self.build_presets()));
    }

    fn load_preset(&self, _index: u32) {
        self.set_error(ENGINE_ONLY);
    }

    fn tree(&self) -> KeysNode {
        self.build_tree()
    }

    fn lane_program_wire(&self) -> KeysLaneProgram {
        self.shared.borrow().program.clone()
    }

    fn mixer(&self) -> KeysMixer {
        self.build_mixer()
    }

    fn set_layer_gain(&self, layer: String, db: f32) {
        {
            let mut s = self.shared.borrow_mut();
            if let Some(l) = s.lanes.iter_mut().find(|l| l.name == layer) {
                l.gain_db = db;
            }
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_engine_gain(&self, engine: String, db: f32) {
        {
            let mut s = self.shared.borrow_mut();
            if let Some(e) = s.engines.iter_mut().find(|e| e.name == engine) {
                e.gain_db = db;
            }
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_master_gain(&self, db: f32) {
        self.shared.borrow_mut().master_db = db;
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_layer_mute(&self, layer: String, muted: bool) {
        {
            let mut s = self.shared.borrow_mut();
            if let Some(l) = s.lanes.iter_mut().find(|l| l.name == layer) {
                l.muted = muted;
            }
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_engine_mute(&self, engine: String, muted: bool) {
        {
            let mut s = self.shared.borrow_mut();
            if let Some(e) = s.engines.iter_mut().find(|e| e.name == engine) {
                e.muted = muted;
            }
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_layer_solo(&self, layer: String, soloed: bool) {
        {
            let mut s = self.shared.borrow_mut();
            if let Some(l) = s.lanes.iter_mut().find(|l| l.name == layer) {
                l.soloed = soloed;
            }
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_layer_exclude_global(&self, layer: String, excluded: bool) {
        {
            let mut s = self.shared.borrow_mut();
            if let Some(l) = s.lanes.iter_mut().find(|l| l.name == layer) {
                l.exclude_global = excluded;
            }
        }
        self.publish_mixer();
    }

    fn set_layer_patch(&self, _layer: String, _module: u32, _preset: u32) {
        self.set_error(ENGINE_ONLY);
    }

    fn set_layer_variant(&self, _layer: String, _module: u32, _preset: u32, _variant: u32) {
        self.set_error(ENGINE_ONLY);
    }

    fn set_drone(&self, _engine: String, _key: u32, _octave: i32, _playing: bool) {
        // The lane program carries no drone engines.
    }

    fn set_engine_order(&self, engines: Vec<String>) {
        {
            let mut s = self.shared.borrow_mut();
            // Promotion semantics: named engines first (list order), the
            // rest keep their relative order behind them.
            let mut ordered = Vec::with_capacity(s.engines.len());
            for name in &engines {
                if let Some(pos) = s.engines.iter().position(|e| &e.name == name) {
                    ordered.push(s.engines.remove(pos));
                }
            }
            ordered.append(&mut s.engines);
            s.engines = ordered;
        }
        self.publish_mixer();
    }

    fn clear_layer(&self, _layer: String, _module: u32) {
        self.set_error(ENGINE_ONLY);
    }

    fn set_module_gain(&self, _layer: String, _module: u32, _db: f32) {
        // One fused instrument per lane in-tab — no per-module bus.
    }

    fn set_module_enabled(&self, _layer: String, _module: u32, _on: bool) {
        // See set_module_gain.
    }

    fn layer_detail(&self, layer: String, module: u32) -> KeysLayerDetail {
        let s = self.shared.borrow();
        let Some(l) = s.lanes.iter().find(|l| l.name == layer) else {
            return KeysLayerDetail::default();
        };
        KeysLayerDetail {
            layer: l.name.clone(),
            engine: l.engine.clone(),
            modules: vec![lane_module(l)],
            // One fused module per lane — every module index views slot A.
            module: { let _ = module; 0 },
            patch: l.pack.clone(),
            preset: String::new(),
            gain_db: l.gain_db,
            muted: l.muted,
            key_lo: 0,
            key_hi: 127,
            macros: Vec::new(),
            layer_macros: Vec::new(),
            tree: KeysNode {
                id: format!("keys/{}/{}", l.engine, l.name),
                label: l.name.clone(),
                role: "layer".into(),
                live: l.live,
                children: Vec::new(),
            },
        }
    }

    fn set_layer_macro(&self, _layer: String, _module: u32, _id: String, _value: f32) {
        // Macros drive engine-side DSP rebuilds — none run in-tab.
    }

    fn set_layer_global(&self, _layer: String, _id: String, _value: f32) {
        // See set_layer_macro.
    }

    fn engine_detail(&self, engine: String) -> KeysEngineDetail {
        let s = self.shared.borrow();
        let Some(e) = s.engines.iter().find(|e| e.name == engine) else {
            return KeysEngineDetail::default();
        };
        let lanes: Vec<&LaneStrip> =
            s.lanes.iter().filter(|l| l.engine == e.name).collect();
        KeysEngineDetail {
            engine: e.name.clone(),
            gain_db: e.gain_db,
            muted: e.muted,
            macros: Vec::new(),
            live_layers: lanes.iter().filter(|l| l.live && !l.muted).count() as u32,
            layers: lanes.len() as u32,
        }
    }

    fn rig_macros(&self) -> Vec<KeysMacro> {
        Vec::new()
    }

    fn set_rig_global(&self, _id: String, _value: f32) {
        // See set_layer_macro.
    }

    fn set_engine_global(&self, _engine: String, _id: String, _value: f32) {
        // See set_layer_macro.
    }

    fn perform(&self) -> KeysPerform {
        self.build_perform()
    }

    fn press_stack(&self, _index: u32) {
        // Stacks are engine-side scenes; the in-tab profile has none.
    }

    fn set_perform_mode(&self, mode: u32) {
        self.shared.borrow_mut().perform_mode = mode;
        self.events
            .publish(KeysEvent::Perform(self.build_perform()));
    }

    fn capture_stack(&self, _index: u32) {
        // See press_stack.
    }

    fn trigger(&self, note: u32, velocity: u32) {
        let (note, velocity) = (note.min(127) as u8, velocity.min(127) as u8);
        if velocity == 0 {
            self.send_midi(0x80, note, 64);
        } else {
            self.send_midi(0x90, note, velocity);
        }
    }

    fn pitch_bend(&self, raw: u32) {
        let raw = raw.min(16383);
        self.send_midi(0xe0, (raw & 0x7f) as u8, ((raw >> 7) & 0x7f) as u8);
    }

    fn mod_wheel(&self, value: u32) {
        self.send_midi(0xb0, 1, value.min(127) as u8);
    }

    fn midi_ports(&self) -> Vec<String> {
        self.shared.borrow().midi_inputs.clone()
    }

    fn set_midi_port(&self, name: String) {
        self.shared.borrow_mut().midi_port =
            (!name.is_empty()).then_some(name);
        self.publish_status();
    }

    fn midi_recent(&self) -> Vec<MidiEvent> {
        self.midi_ring()
    }
}

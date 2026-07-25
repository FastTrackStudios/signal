//! Headless keys-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`KeysRig`] (composition-tree instrument on the shared engine),
//! scans the Keyscape library for presets, loads a single-instrument program
//! per preset, and plays it from hardware MIDI or UI notes. Implements the
//! [`signal_keys_proto::keys::KeysRig`] service + its `#[subscribe]` stream;
//! mount `router()` (`architect::rig::RigBackend`) on a vox transport.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::rig::RigBackend;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use daw_audio_io::AudioIoPrefs;
use midicore::MidiEvent;
use signal_keys_proto::keys::{KeysEvent, KeysRig as KeysRigSvc, KeysRigStreamSource};
use signal_keys_proto::{
    KeysEngineModel, KeysLayerDetail, KeysLayerModel, KeysMacro, KeysMixer, KeysNode, KeysPerform,
    KeysPreset, KeysStack, KeysStatus,
};

use crate::profile::{KeysProfile, worship_profile};
use signal_sampler::rig_node::{RigNode, Role};
use signal_sampler::{Container, MidiInputHandle};

use crate::KeysRig;

/// Root of the local Keyscape extraction (per-instrument dirs each holding a
/// `library.styx`). Used as a fallback when no packs are present. Override with
/// `FTS_KEYSCAPE_ROOT`.
const KEYSCAPE_ROOT: &str = "/run/media/AudioHaven/Sampled/Keys/Keyscape";
/// Root of the built `.signalpack` library (one self-contained pack per
/// instrument). Preferred over the raw extraction. Override with
/// `FTS_KEYSCAPE_PACKS`.
const KEYSCAPE_PACKS_ROOT: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs";
#[derive(Default)]
struct State {
    presets: Vec<KeysPreset>,
    /// Absolute `library.styx` spec path per preset (index-aligned).
    specs: Vec<PathBuf>,
    loaded: Option<usize>,
    /// The loaded composition tree (for the control-view structure).
    tree: Option<Container>,
    midi_port: Option<String>,
    midi_handle: Option<MidiInputHandle>,
    /// The last audio-open failure, for UIs with no log access (phones).
    last_error: Option<String>,
    /// The active profile — the engine/layer mixer shape + its stacks.
    profile: KeysProfile,
    /// Live mixer state per layer name (fader / mute / solo / patch).
    lanes: BTreeMap<String, LaneState>,
    /// Live mixer state per engine name.
    engines: BTreeMap<String, EngineState>,
    /// Master trim (dB).
    master_db: f32,
    /// Index of the last pressed stack.
    active_stack: Option<usize>,
    /// Grid mode: 0 Preset, 1 Profile (stacks), 2 Setlist.
    perform_mode: u32,
}

/// Live per-layer mixer state (the profile holds the authored defaults).
#[derive(Clone, Debug)]
struct LaneState {
    engine: String,
    gain_db: f32,
    muted: bool,
    soloed: bool,
    /// The lane's four modules (the engine instances). Module A is index 0.
    modules: Vec<ModuleState>,
}

/// One module: its Source Block's patch and its own macro values (each
/// module has its own filter, amp envelope and FX).
#[derive(Clone, Debug)]
struct ModuleState {
    /// The soundsource in the Source Block — what actually loads.
    patch: String,
    /// The module preset this module came from ("American Obesity"), when it
    /// was opened from one. This is the name the UI shows for the module.
    preset: String,
    macros: BTreeMap<String, f32>,
    gain_db: f32,
    enabled: bool,
}

impl Default for ModuleState {
    fn default() -> Self {
        Self {
            patch: String::new(),
            preset: String::new(),
            macros: BTreeMap::new(),
            gain_db: 0.0,
            enabled: true,
        }
    }
}

impl ModuleState {
    /// What this module IS: the module preset it was opened from, else the
    /// bare soundsource sitting in its Source Block.
    fn display(&self) -> String {
        if self.preset.is_empty() { self.patch.clone() } else { self.preset.clone() }
    }
}

impl LaneState {
    /// Module A's name — what the mixer shows for the lane.
    fn primary_patch(&self) -> String {
        self.modules.first().map(|m| m.display()).unwrap_or_default()
    }

    /// Any module sounding?
    fn any_live(&self) -> bool {
        self.modules.iter().any(|m| !m.patch.is_empty() && m.enabled)
    }

    /// The linear gain a module renders at (off / empty = silent).
    fn module_gain(&self, index: usize) -> f32 {
        match self.modules.get(index) {
            Some(m) if m.enabled && !m.patch.is_empty() => db_to_linear(m.gain_db),
            _ => 0.0,
        }
    }

    fn module(&self, index: usize) -> Option<&ModuleState> {
        self.modules.get(index)
    }
}

/// One macro's declaration: id, panel, display name, range, unit, and whether
/// its block has DSP yet (the engine's stack is placeholder-first — see
/// `signal_synth::engine`).
struct MacroDef {
    id: &'static str,
    name: &'static str,
    group: &'static str,
    default: f32,
    min: f32,
    max: f32,
    unit: &'static str,
    live: bool,
}

/// The canonical macro surface every Signal Engine layer exposes. Grouped
/// into the layer-zoom's panels; the order here is the render order.
///
/// `live` marks the ones that reach DSP today: the sampler's amp envelope and
/// unison are real block params, the rest wait on their block's
/// implementation (filters, vibrato, ambience, FX are placeholders that pass
/// audio through).
const MACROS: &[MacroDef] = &[
    // ── Source ──────────────────────────────────────────────────────────
    MacroDef { id: "source.level", name: "Level", group: "Source", default: 0.0, min: -24.0, max: 12.0, unit: "dB", live: true },
    MacroDef { id: "source.pan", name: "Pan", group: "Source", default: 0.0, min: -1.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "source.transpose", name: "Transpose", group: "Source", default: 0.0, min: -24.0, max: 24.0, unit: "st", live: false },
    MacroDef { id: "source.fine", name: "Fine", group: "Source", default: 0.0, min: -100.0, max: 100.0, unit: "c", live: false },
    MacroDef { id: "source.unison", name: "Unison", group: "Source", default: 1.0, min: 1.0, max: 8.0, unit: "v", live: true },
    MacroDef { id: "source.detune", name: "Detune", group: "Source", default: 0.1, min: 0.0, max: 2.0, unit: "", live: true },
    // ── Filter ──────────────────────────────────────────────────────────
    MacroDef { id: "filter.cutoff", name: "Cutoff", group: "Filter", default: 20000.0, min: 20.0, max: 20000.0, unit: "Hz", live: false },
    MacroDef { id: "filter.reso", name: "Resonance", group: "Filter", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "filter.env_amt", name: "Env Amt", group: "Filter", default: 0.0, min: -1.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "filter.keytrack", name: "Key Trk", group: "Filter", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "filter.drive", name: "Drive", group: "Filter", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "filter.mix", name: "Mix", group: "Filter", default: 1.0, min: 0.0, max: 1.0, unit: "", live: false },
    // ── Envelopes 1..4 ──────────────────────────────────────────────────
    // ENV 1 is bound to the Amp and ENV 2 to the Filter (the bindings the
    // engine assumes today; unbinding lands with the mod matrix). 3 and 4
    // are free — route them from the matrix.
    MacroDef { id: "env1.delay", name: "Delay", group: "Env 1", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env1.attack", name: "Attack", group: "Env 1", default: 0.0, min: 0.0, max: 5000.0, unit: "ms", live: true },
    MacroDef { id: "env1.hold", name: "Hold", group: "Env 1", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env1.decay", name: "Decay", group: "Env 1", default: 0.0, min: 0.0, max: 5000.0, unit: "ms", live: false },
    MacroDef { id: "env1.sustain", name: "Sustain", group: "Env 1", default: 1.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "env1.release", name: "Release", group: "Env 1", default: 120.0, min: 0.0, max: 8000.0, unit: "ms", live: true },
    MacroDef { id: "env2.delay", name: "Delay", group: "Env 2", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env2.attack", name: "Attack", group: "Env 2", default: 5.0, min: 0.0, max: 5000.0, unit: "ms", live: false },
    MacroDef { id: "env2.hold", name: "Hold", group: "Env 2", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env2.decay", name: "Decay", group: "Env 2", default: 300.0, min: 0.0, max: 5000.0, unit: "ms", live: false },
    MacroDef { id: "env2.sustain", name: "Sustain", group: "Env 2", default: 0.7, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "env2.release", name: "Release", group: "Env 2", default: 200.0, min: 0.0, max: 8000.0, unit: "ms", live: false },
    MacroDef { id: "env3.delay", name: "Delay", group: "Env 3", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env3.attack", name: "Attack", group: "Env 3", default: 20.0, min: 0.0, max: 5000.0, unit: "ms", live: false },
    MacroDef { id: "env3.hold", name: "Hold", group: "Env 3", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env3.decay", name: "Decay", group: "Env 3", default: 400.0, min: 0.0, max: 5000.0, unit: "ms", live: false },
    MacroDef { id: "env3.sustain", name: "Sustain", group: "Env 3", default: 0.5, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "env3.release", name: "Release", group: "Env 3", default: 300.0, min: 0.0, max: 8000.0, unit: "ms", live: false },
    MacroDef { id: "env4.delay", name: "Delay", group: "Env 4", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env4.attack", name: "Attack", group: "Env 4", default: 200.0, min: 0.0, max: 5000.0, unit: "ms", live: false },
    MacroDef { id: "env4.hold", name: "Hold", group: "Env 4", default: 0.0, min: 0.0, max: 2000.0, unit: "ms", live: false },
    MacroDef { id: "env4.decay", name: "Decay", group: "Env 4", default: 600.0, min: 0.0, max: 5000.0, unit: "ms", live: false },
    MacroDef { id: "env4.sustain", name: "Sustain", group: "Env 4", default: 0.6, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "env4.release", name: "Release", group: "Env 4", default: 500.0, min: 0.0, max: 8000.0, unit: "ms", live: false },
    // ── LFOs 1..4 ───────────────────────────────────────────────────────
    MacroDef { id: "lfo1.rate", name: "Rate", group: "LFO 1", default: 2.0, min: 0.01, max: 40.0, unit: "Hz", live: false },
    MacroDef { id: "lfo1.depth", name: "Depth", group: "LFO 1", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "lfo1.shape", name: "Shape", group: "LFO 1", default: 0.0, min: 0.0, max: 4.0, unit: "", live: false },
    MacroDef { id: "lfo1.fade", name: "Fade In", group: "LFO 1", default: 0.0, min: 0.0, max: 4000.0, unit: "ms", live: false },
    MacroDef { id: "lfo2.rate", name: "Rate", group: "LFO 2", default: 0.5, min: 0.01, max: 40.0, unit: "Hz", live: false },
    MacroDef { id: "lfo2.depth", name: "Depth", group: "LFO 2", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "lfo2.shape", name: "Shape", group: "LFO 2", default: 1.0, min: 0.0, max: 4.0, unit: "", live: false },
    MacroDef { id: "lfo2.fade", name: "Fade In", group: "LFO 2", default: 0.0, min: 0.0, max: 4000.0, unit: "ms", live: false },
    MacroDef { id: "lfo3.rate", name: "Rate", group: "LFO 3", default: 4.0, min: 0.01, max: 40.0, unit: "Hz", live: false },
    MacroDef { id: "lfo3.depth", name: "Depth", group: "LFO 3", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "lfo3.shape", name: "Shape", group: "LFO 3", default: 2.0, min: 0.0, max: 4.0, unit: "", live: false },
    MacroDef { id: "lfo3.fade", name: "Fade In", group: "LFO 3", default: 0.0, min: 0.0, max: 4000.0, unit: "ms", live: false },
    MacroDef { id: "lfo4.rate", name: "Rate", group: "LFO 4", default: 8.0, min: 0.01, max: 40.0, unit: "Hz", live: false },
    MacroDef { id: "lfo4.depth", name: "Depth", group: "LFO 4", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "lfo4.shape", name: "Shape", group: "LFO 4", default: 3.0, min: 0.0, max: 4.0, unit: "", live: false },
    MacroDef { id: "lfo4.fade", name: "Fade In", group: "LFO 4", default: 0.0, min: 0.0, max: 4000.0, unit: "ms", live: false },
    // ── Tone / Vibrato / Ambience / Effects (per module) ─────────────────
    MacroDef { id: "tone.warmth", name: "Warmth", group: "Tone", default: 0.5, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "tone.drive", name: "Drive", group: "Tone", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "tone.body", name: "Body", group: "Tone", default: 0.5, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "vib.rate", name: "Rate", group: "Vibrato", default: 5.0, min: 0.1, max: 12.0, unit: "Hz", live: false },
    MacroDef { id: "vib.depth", name: "Depth", group: "Vibrato", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "vib.delay", name: "Delay", group: "Vibrato", default: 300.0, min: 0.0, max: 3000.0, unit: "ms", live: false },
    MacroDef { id: "amb.size", name: "Size", group: "Ambience", default: 0.5, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "amb.mix", name: "Mix", group: "Ambience", default: 0.15, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "amb.predelay", name: "Pre-dly", group: "Ambience", default: 20.0, min: 0.0, max: 250.0, unit: "ms", live: false },
    MacroDef { id: "fx.chorus", name: "Chorus", group: "Effects", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "fx.delay", name: "Delay", group: "Effects", default: 0.0, min: 0.0, max: 1.0, unit: "", live: false },
    MacroDef { id: "fx.width", name: "Width", group: "Effects", default: 0.5, min: 0.0, max: 1.0, unit: "", live: false },
];


fn macro_def(id: &str) -> Option<&'static MacroDef> {
    MACROS.iter().find(|m| m.id == id)
}

fn default_macros() -> BTreeMap<String, f32> {
    MACROS.iter().map(|m| (m.id.to_string(), m.default)).collect()
}

#[derive(Clone, Debug, Default)]
struct EngineState {
    gain_db: f32,
    muted: bool,
}

impl State {
    /// Seed the live mixer from a profile's authored defaults.
    fn adopt_profile(&mut self, profile: KeysProfile) {
        self.lanes.clear();
        self.engines.clear();
        for engine in &profile.engines {
            self.engines.insert(
                engine.name.clone(),
                EngineState { gain_db: engine.gain_db, muted: false },
            );
            for layer in &engine.layers {
                self.lanes.insert(
                    layer.name.clone(),
                    LaneState {
                        engine: engine.name.clone(),
                        gain_db: layer.gain_db,
                        muted: false,
                        soloed: false,
                        modules: layer
                            .module_patches()
                            .into_iter()
                            .map(|patch| ModuleState {
                                patch,
                                macros: default_macros(),
                                ..ModuleState::default()
                            })
                            .collect(),
                    },
                );
            }
        }
        self.profile = profile;
    }

    /// Any lane soloed? (Solo silences every un-soloed lane.)
    fn any_solo(&self) -> bool {
        self.lanes.values().any(|l| l.soloed)
    }

    /// The linear gain a lane should be rendering at right now, folding in
    /// mute, solo-exclusion and its engine's mute. Engine *faders* are their
    /// own cell, so they're not folded in here.
    fn lane_gain(&self, name: &str) -> f32 {
        let Some(lane) = self.lanes.get(name) else { return 0.0 };
        let engine_muted = self.engines.get(&lane.engine).is_some_and(|e| e.muted);
        let solo_excluded = self.any_solo() && !lane.soloed;
        if lane.muted || engine_muted || solo_excluded || !lane.any_live() {
            0.0
        } else {
            db_to_linear(lane.gain_db)
        }
    }

    fn engine_gain(&self, name: &str) -> f32 {
        match self.engines.get(name) {
            Some(e) if !e.muted => db_to_linear(e.gain_db),
            Some(_) => 0.0,
            None => 1.0,
        }
    }
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

struct Inner {
    rig: Mutex<Option<KeysRig>>,
    state: Mutex<State>,
    events: PubSub<KeysEvent>,
    pump_started: AtomicBool,
}

/// The keys-rig backend handle. Cheap to clone (all state shared).
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct KeysRigBackend {
    inner: Arc<Inner>,
}

impl Default for KeysRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// The backend's own small runtime. The daw-standalone engine spawns
/// tokio tasks during open/load (prefetch, pumps); the backend drives
/// those from plain worker threads, which have no ambient runtime —
/// entering this one gives every spawn a reactor regardless of host
/// (in-process iOS app, engine mode, tests).
fn keys_runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("keys-rt")
            .enable_all()
            .build()
            .expect("keys runtime")
    })
}

impl KeysRigBackend {
    /// Build the backend and scan the Keyscape library. Does not open audio.
    pub fn new() -> Self {
        let (presets, specs) = scan_keyscape();
        // Default patch: the LA Custom Rhodes if present, else the first found.
        let default_idx = presets
            .iter()
            .position(|p| p.name == "Rhodes - LA Custom")
            .or_else(|| {
                presets.iter().position(|p| {
                    let n = p.name.to_ascii_lowercase();
                    n.contains("rhodes") && n.contains("la custom")
                })
            })
            // Whatever is installed beats nothing (a phone with only the
            // Wurli downloaded should auto-load the Wurli).
            .or_else(|| (!presets.is_empty()).then_some(0));
        tracing::info!(
            presets = presets.len(),
            default = default_idx.and_then(|i| presets.get(i)).map(|p| p.name.as_str()).unwrap_or("<first>"),
            "keys rig: scanned library"
        );
        // The rig boots as a PROFILE — engines, layers, stacks — not a single
        // patch. `FTS_KEYS_PROFILE` points at a `.styx` profile; the built-in
        // Worship profile is the default.
        let profile = load_profile();
        let mut state =
            State { presets, specs, loaded: default_idx, ..State::default() };
        state.adopt_profile(profile);
        // Lanes whose authored patch isn't in the library start empty rather
        // than silently pointing at a missing pack.
        let known: Vec<String> = state.presets.iter().map(|p| p.name.clone()).collect();
        for lane in state.lanes.values_mut() {
            for m in lane.modules.iter_mut() {
                if !m.patch.is_empty() && !known.contains(&m.patch) {
                    tracing::info!(patch = %m.patch, "keys rig: profile patch not in library — module starts empty");
                    m.patch.clear();
                }
            }
        }
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                state: Mutex::new(state),
                events: architect::rig::events_hub(),
                pump_started: AtomicBool::new(false),
            }),
        };
        backend.spawn_meter_pump("keys-meter-pump");
        backend
    }

    fn program_for(&self, index: usize) -> Option<Container> {
        let s = self.inner.state.lock().ok()?;
        let spec = s.specs.get(index)?.to_string_lossy().into_owned();
        let name = s.presets.get(index)?.name.clone();
        Some(keys_program(&name, spec))
    }

    /// Build the profile's full program: every engine, every layer, each
    /// lane's patch resolved to its pack/library spec.
    fn profile_program(&self) -> Option<Container> {
        let s = self.inner.state.lock().ok()?;
        if s.profile.engines.is_empty() {
            return None;
        }
        // Patch name → spec path, via the scanned library.
        let index: BTreeMap<&str, &PathBuf> = s
            .presets
            .iter()
            .zip(s.specs.iter())
            .map(|(p, spec)| (p.name.as_str(), spec))
            .collect();
        // Lanes carry the LIVE patch assignment (a stack recall or a browser
        // pick), which is what should be rendering — not the authored default.
        let mut profile = s.profile.clone();
        for engine in &mut profile.engines {
            for layer in &mut engine.layers {
                let Some(lane) = s.lanes.get(&layer.name) else { continue };
                let patches: Vec<String> = lane.modules.iter().map(|m| m.patch.clone()).collect();
                layer.patch = patches.first().cloned().unwrap_or_default();
                layer.extra_modules = patches.into_iter().skip(1).collect();
            }
        }
        Some(profile.build_tree(|patch| {
            index.get(patch).map(|p| p.to_string_lossy().into_owned())
        }))
    }

    /// Push every live fader / mute / solo into the running program's cells.
    /// Pure atomics — no rebuild, so this is safe at UI drag rate.
    fn apply_mixer(&self) {
        let Ok(rig) = self.inner.rig.lock() else { return };
        let Some(rig) = rig.as_ref() else { return };
        let cells = rig.gain_cells();
        let Ok(s) = self.inner.state.lock() else { return };
        for (name, lane) in s.lanes.iter() {
            cells.set(name, s.lane_gain(name));
            // Modules are named "<layer> <slot>" in the tree.
            for i in 0..lane.modules.len() {
                let module_name =
                    format!("{name} {}", signal_synth::engine::module_slot(i));
                cells.set(&module_name, lane.module_gain(i));
            }
        }
        for name in s.engines.keys() {
            cells.set(name, s.engine_gain(name));
        }
        rig.set_output_gain(db_to_linear(s.master_db));
    }

    /// Rebuild the playable program from the profile (patch assignment
    /// changed), then re-apply the mixer to the fresh cells.
    fn rebuild_program(&self) {
        // Breadcrumbs: a rebuild touches the state lock, the rig lock and the
        // audio device in that order, so a stall is only diagnosable if the
        // log says which step it reached.
        tracing::debug!("keys rig: rebuild — building program");
        let Some(tree) = self.profile_program() else { return };
        tracing::debug!("keys rig: rebuild — ensuring audio");
        if !self.ensure_open() {
            self.publish_all();
            return;
        }
        tracing::debug!("keys rig: rebuild — loading preset");
        if let Ok(mut rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_mut() {
                rig.load_preset(&tree);
            }
        }
        tracing::debug!("keys rig: rebuild — publishing");
        if let Ok(mut s) = self.inner.state.lock() {
            s.tree = Some(tree);
        }
        self.apply_mixer();
        self.publish_all();
        tracing::debug!("keys rig: rebuild — done");
    }

    /// Open an authored Omnisphere patch as a **module preset**: it lands on
    /// `start` and, if it has more than one layer, fills the modules after it
    /// — each carrying its own source, filter, envelopes and unison. Modules
    /// the patch doesn't reach are left alone, so a one-layer preset only
    /// touches the module you dropped it on.
    fn load_omni_patch(&self, layer: &str, start: usize, file: &std::path::Path) {
        let imported = match signal_synth::engine::import_omni_patch(file) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(?file, "keys rig: patch import failed: {e}");
                if let Ok(mut s) = self.inner.state.lock() {
                    s.last_error = Some(format!("patch import failed: {e}"));
                }
                self.publish_all();
                return;
            }
        };
        {
            let Ok(mut s) = self.inner.state.lock() else { return };
            // Soundsource names resolve against the same library the modules
            // load from; an unknown one leaves that module empty.
            let known: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
            let Some(lane) = s.lanes.get_mut(layer) else { return };
            for (i, m) in imported.modules.iter().enumerate() {
                let at = start + i;
                while lane.modules.len() <= at {
                    lane.modules.push(ModuleState::default());
                }
                let slot = &mut lane.modules[at];
                slot.patch = known
                    .iter()
                    .find(|k| k.eq_ignore_ascii_case(&m.source))
                    .cloned()
                    .unwrap_or_default();
                slot.preset = imported.name.clone();
                slot.gain_db = m.level_db.clamp(MIN_FADER_DB, MAX_FADER_DB);
                slot.enabled = true;
                let set = |macros: &mut BTreeMap<String, f32>, id: &str, v: f32| {
                    if let Some(def) = macro_def(id) {
                        macros.insert(id.to_string(), v.clamp(def.min, def.max));
                    }
                };
                set(&mut slot.macros, "filter.cutoff", m.cutoff_hz);
                set(&mut slot.macros, "filter.reso", m.resonance);
                set(&mut slot.macros, "filter.env_amt", m.filter_env_depth);
                set(&mut slot.macros, "source.unison", m.unison as f32);
                set(&mut slot.macros, "source.detune", m.detune);
                if let Some((a, d, sus, r)) = m.amp_env {
                    set(&mut slot.macros, "env1.attack", a);
                    set(&mut slot.macros, "env1.decay", d);
                    set(&mut slot.macros, "env1.sustain", sus);
                    set(&mut slot.macros, "env1.release", r);
                }
                if let Some((a, d, sus, r)) = m.filter_env {
                    set(&mut slot.macros, "env2.attack", a);
                    set(&mut slot.macros, "env2.decay", d);
                    set(&mut slot.macros, "env2.sustain", sus);
                    set(&mut slot.macros, "env2.release", r);
                }
                // Omnisphere's LFOs are per-part, so every module of the
                // patch gets the same four.
                for (n, (rate, depth, shape)) in imported.lfos.iter().enumerate() {
                    let id = format!("lfo{}", n + 1);
                    set(&mut slot.macros, &format!("{id}.rate"), *rate);
                    set(&mut slot.macros, &format!("{id}.depth"), *depth);
                    set(&mut slot.macros, &format!("{id}.shape"), *shape);
                }
            }
            tracing::info!(
                layer,
                patch = %imported.name,
                start,
                modules = imported.modules.len(),
                "keys rig: opened module preset"
            );
        }
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-patch-open".into())
            .spawn(move || {
                let _rt = keys_runtime().enter();
                b.rebuild_program();
            });
    }

    /// The wire mixer snapshot.
    fn mixer_model(&self) -> KeysMixer {
        let Ok(s) = self.inner.state.lock() else { return KeysMixer::default() };
        let engines = s
            .profile
            .engines
            .iter()
            .map(|engine| {
                let est = s.engines.get(&engine.name).cloned().unwrap_or_default();
                KeysEngineModel {
                    name: engine.name.clone(),
                    gain_db: est.gain_db,
                    muted: est.muted,
                    layers: engine
                        .layers
                        .iter()
                        .map(|layer| {
                            let lane = s.lanes.get(&layer.name);
                            KeysLayerModel {
                                name: layer.name.clone(),
                                engine: engine.name.clone(),
                                patch: lane.map(|l| l.primary_patch()).unwrap_or_default(),
                                gain_db: lane.map(|l| l.gain_db).unwrap_or(0.0),
                                muted: lane.is_some_and(|l| l.muted),
                                soloed: lane.is_some_and(|l| l.soloed),
                                live: lane.is_some_and(|l| l.any_live()),
                                key_lo: layer.key_lo as u32,
                                key_hi: layer.key_hi as u32,
                                modules: lane
                                    .map(|l| {
                                        l.modules
                                            .iter()
                                            .enumerate()
                                            .map(|(i, m)| signal_keys_proto::KeysModule {
                                                index: i as u32,
                                                slot: signal_synth::engine::module_slot(i),
                                                patch: m.display(),
                                                source: m.patch.clone(),
                                                live: !m.patch.is_empty(),
                                                gain_db: m.gain_db,
                                                enabled: m.enabled,
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            }
                        })
                        .collect(),
                }
            })
            .collect();
        KeysMixer { profile: s.profile.name.clone(), engines, master_db: s.master_db }
    }

    /// The wire performance snapshot.
    fn perform_model(&self) -> KeysPerform {
        let Ok(s) = self.inner.state.lock() else { return KeysPerform::default() };
        KeysPerform {
            profile_name: s.profile.name.clone(),
            stacks: s
                .profile
                .stacks
                .iter()
                .enumerate()
                .map(|(i, st)| KeysStack {
                    name: st.name.clone(),
                    blurb: st.blurb.clone(),
                    is_active: s.active_stack == Some(i),
                })
                .collect(),
            active_stack: s.active_stack.map(|i| i as u32).unwrap_or(u32::MAX),
            perform_mode: s.perform_mode,
        }
    }

    fn publish_mixer(&self) {
        self.inner.events.publish(KeysEvent::Mixer(self.mixer_model()));
    }

    fn publish_perform(&self) {
        self.inner.events.publish(KeysEvent::Perform(self.perform_model()));
    }

    /// Open audio with the given (or first) preset, if not already open.
    fn ensure_open(&self) -> bool {
        {
            if self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false) {
                return true;
            }
        }
        // The profile IS the program — engines, layers, every lane's patch.
        // (A profile-less build falls back to the single-patch program, which
        // is what the mobile shell's one-pack case wants.)
        let idx = self.inner.state.lock().ok().and_then(|s| s.loaded).unwrap_or(0);
        let Some(tree) = self.profile_program().or_else(|| self.program_for(idx)) else {
            if let Ok(mut s) = self.inner.state.lock() {
                s.last_error = Some("no patches downloaded yet".into());
            }
            return false;
        };
        let prefs = AudioIoPrefs {
            output_device: String::new(),
            sample_rate: 0,
            // 256 frames on desktop; on iOS the fixed-size request rides a
            // macOS-only CoreAudio property (AVAudioSession owns the IO
            // buffer there), so ask for the backend default instead.
            buffer_size: if cfg!(target_os = "ios") { 0 } else { 256 },
            ..Default::default()
        };
        // Brand the in-flight state and convert panics into a visible
        // error — phone UIs have no logs, and a silent hang and a
        // swallowed thread panic are otherwise indistinguishable from
        // "nothing happened".
        if let Ok(mut s) = self.inner.state.lock() {
            s.last_error = Some("opening audio device…".into());
        }
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
        let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            KeysRig::open(&prefs, &tree)
        }))
        .unwrap_or_else(|p| {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic>".into());
            Err(eyre::eyre!("audio open panicked: {msg}"))
        });
        match opened {
            Ok(r) => {
                {
                    let mut rig = self.inner.rig.lock().unwrap();
                    *rig = Some(r);
                }
                if let Ok(mut s) = self.inner.state.lock() {
                    if s.loaded.is_none() {
                        s.loaded = Some(idx);
                    }
                    for (i, p) in s.presets.iter_mut().enumerate() {
                        p.loaded = i == idx;
                    }
                    s.tree = Some(tree);
                    s.last_error = None;
                }
                true
            }
            Err(e) => {
                tracing::error!("keys rig: audio open failed: {e}");
                if let Ok(mut s) = self.inner.state.lock() {
                    s.last_error = Some(format!("audio open failed: {e}"));
                }
                self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
                false
            }
        }
    }

    fn do_load_preset(&self, index: usize) {
        let Some(tree) = self.program_for(index) else {
            if let Ok(mut s) = self.inner.state.lock() {
                s.last_error = Some(format!("preset {index}: spec missing (re-scan needed?)"));
            }
            self.publish_all();
            return;
        };
        if !self.ensure_open() {
            // ensure_open recorded last_error; make sure remotes see it.
            self.publish_all();
            return;
        }
        if let Ok(mut rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_mut() {
                rig.load_preset(&tree);
            }
        }
        if let Ok(mut s) = self.inner.state.lock() {
            for (i, p) in s.presets.iter_mut().enumerate() {
                p.loaded = i == index;
            }
            s.loaded = Some(index);
            s.tree = Some(tree);
        }
        self.publish_all();
    }

    fn reattach_midi(&self) {
        let port = self.inner.state.lock().ok().and_then(|s| s.midi_port.clone());
        midicore::attach::reattach(
            "keys rig",
            port.as_deref(),
            || {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = None;
                }
            },
            |sel| {
                // KeysRig isn't Clone (owns the audio engine), so attach
                // under the lock.
                let rig = self.inner.rig.lock().unwrap();
                match rig.as_ref() {
                    Some(rig) => rig.attach_midi(sel).map(Some),
                    None => Ok(None),
                }
            },
            |h| {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = Some(h);
                }
            },
        );
    }

    fn publish_all(&self) {
        self.inner.events.publish(KeysEvent::Library(KeysRigSvc::presets(self)));
        self.inner.events.publish(KeysEvent::Tree(KeysRigSvc::tree(self)));
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
        self.publish_mixer();
        self.publish_perform();
    }

}

// r[impl primitives.architect.rig-backend]
impl RigBackend for KeysRigBackend {
    type Event = KeysEvent;
    type Tick = ();

    fn events_hub(&self) -> &PubSub<KeysEvent> {
        &self.inner.events
    }

    fn is_running(&self) -> bool {
        self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false)
    }

    fn pump_started(&self) -> &AtomicBool {
        &self.inner.pump_started
    }

    fn on_running_edge(&self, _running: bool) {
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
        self.inner.events.publish(KeysEvent::Tree(KeysRigSvc::tree(self)));
    }

    fn on_running_tick(&self) {
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
        self.inner.events.publish(KeysEvent::Midi(KeysRigSvc::midi_recent(self)));
    }

    fn midi_ports(&self) -> Vec<String> {
        KeysRig::midi_input_ports()
    }

    fn on_midi_ports_changed(&self, ports: &[String]) {
        // Defensive twin of the pump's guard: never drop a live attachment
        // for an empty scan (transient JACK/ALSA enumeration failure).
        if ports.is_empty() {
            tracing::debug!("keys rig: empty MIDI scan ignored — keeping the current attachment");
            return;
        }
        // A keyboard plugged in after the rig started is merged into the
        // omni stream without touching the UI.
        tracing::info!(?ports, "keys rig: MIDI ports changed — re-attaching");
        self.reattach_midi();
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }
}

// ── service impl ─────────────────────────────────────────────────────────────

impl KeysRigSvc for KeysRigBackend {
    fn start(&self) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-open".into())
            .spawn(move || {
                let _rt = keys_runtime().enter();
                if b.ensure_open() {
                    b.reattach_midi();
                    // Lanes start at their profile/scene levels, not unity.
                    b.apply_mixer();
                }
                b.publish_all();
            });
    }

    fn stop(&self) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_handle = None;
        }
        if let Ok(mut rig) = self.inner.rig.lock() {
            *rig = None;
        }
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }

    fn status(&self) -> KeysStatus {
        let running = self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
        let s = self.inner.state.lock().unwrap();
        let loaded_preset = s.loaded.and_then(|i| s.presets.get(i)).map(|p| p.name.clone());
        let master_peak = if running {
            self.inner.rig.lock().ok().and_then(|r| r.as_ref().map(|r| r.output_peak())).unwrap_or(0.0)
        } else {
            0.0
        };
        KeysStatus {
            running,
            loaded_preset,
            master_peak,
            voices: 0,
            midi_port: s.midi_port.clone(),
            last_error: s.last_error.clone(),
        }
    }

    fn presets(&self) -> Vec<KeysPreset> {
        self.inner.state.lock().map(|s| s.presets.clone()).unwrap_or_default()
    }

    fn rescan(&self) {
        let (presets, specs) = scan_keyscape();
        if let Ok(mut s) = self.inner.state.lock() {
            // Keep the loaded preset marked if it survived the rescan.
            let loaded_name =
                s.loaded.and_then(|i| s.presets.get(i)).map(|p| p.name.clone());
            s.loaded = loaded_name
                .as_deref()
                .and_then(|n| presets.iter().position(|p| p.name == n))
                // Nothing loaded yet — same default as `new()`, so the
                // first download lands on the LA Custom Rhodes; failing
                // that, whatever arrived first is better than nothing.
                .or_else(|| presets.iter().position(|p| p.name == "Rhodes - LA Custom"))
                .or_else(|| (!presets.is_empty()).then_some(0));
            s.presets = presets;
            s.specs = specs;
            if let Some(i) = s.loaded {
                s.presets[i].loaded = true;
            }
        }
        self.publish_all();
    }

    fn load_preset(&self, index: u32) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-load".into())
            .spawn(move || {
                let _rt = keys_runtime().enter();
                b.do_load_preset(index as usize)
            });
    }

    fn tree(&self) -> KeysNode {
        self.inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.tree.as_ref().map(|t| node_of(t, "")))
            .unwrap_or_default()
    }

    fn trigger(&self, note: u32, velocity: u32) {
        let (note, velocity) = (note as u8, velocity as u8);
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                if velocity > 0 {
                    rig.note_on(note, velocity);
                    rig.midi_monitor().record(&note_ev(note, velocity));
                } else {
                    rig.note_off(note);
                }
            }
        }
    }

    fn midi_ports(&self) -> Vec<String> {
        KeysRig::midi_input_ports()
    }

    fn set_midi_port(&self, name: String) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_port = if name.is_empty() { None } else { Some(name) };
        }
        self.reattach_midi();
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }

    fn midi_recent(&self) -> Vec<MidiEvent> {
        self.inner
            .rig
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(|r| r.midi_monitor().recent()))
            .unwrap_or_default()
    }

    // ── Mixer ────────────────────────────────────────────────────────────

    fn mixer(&self) -> KeysMixer {
        self.mixer_model()
    }

    fn set_layer_gain(&self, layer: String, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            lane.gain_db = db.clamp(MIN_FADER_DB, MAX_FADER_DB);
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_engine_gain(&self, engine: String, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(e) = s.engines.get_mut(&engine) else { return };
            e.gain_db = db.clamp(MIN_FADER_DB, MAX_FADER_DB);
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_master_gain(&self, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.master_db = db.clamp(MIN_FADER_DB, MAX_FADER_DB);
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_layer_mute(&self, layer: String, muted: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            lane.muted = muted;
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_engine_mute(&self, engine: String, muted: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(e) = s.engines.get_mut(&engine) else { return };
            e.muted = muted;
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_layer_solo(&self, layer: String, soloed: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            lane.soloed = soloed;
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_layer_patch(&self, layer: String, module: u32, preset: u32) {
        // An authored patch (.prt_omn) is a MODULE PRESET, not a bare source:
        // it carries filter, envelopes and unison, and a multi-layer patch
        // spills onto the modules after the one you dropped it on.
        let patch_file = self
            .inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.specs.get(preset as usize).cloned())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("prt_omn")));
        if let Some(file) = patch_file {
            self.load_omni_patch(&layer, module as usize, &file);
            return;
        }
        {
            let Ok(mut s) = self.inner.state.lock() else { return };
            let Some(name) = s.presets.get(preset as usize).map(|p| p.name.clone()) else {
                return;
            };
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            let Some(m) = lane.modules.get_mut(module as usize) else { return };
            if m.patch == name && m.preset.is_empty() {
                return;
            }
            m.patch = name;
            m.preset.clear();
        }
        // A new sample source in the lane — the program must recompile.
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-layer-patch".into())
            .spawn(move || {
                let _rt = keys_runtime().enter();
                b.rebuild_program();
            });
    }

    fn layer_detail(&self, layer: String, module: u32) -> KeysLayerDetail {
        let Ok(s) = self.inner.state.lock() else { return KeysLayerDetail::default() };
        let Some(lane) = s.lanes.get(&layer) else { return KeysLayerDetail::default() };
        let slot = (module as usize).min(lane.modules.len().saturating_sub(1));
        let (key_lo, key_hi) = s
            .profile
            .layer(&layer)
            .map(|(_, l)| (l.key_lo as u32, l.key_hi as u32))
            .unwrap_or((0, 127));
        let modules = lane
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| signal_keys_proto::KeysModule {
                index: i as u32,
                slot: signal_synth::engine::module_slot(i),
                patch: m.display(),
                source: m.patch.clone(),
                live: !m.patch.is_empty(),
                gain_db: m.gain_db,
                enabled: m.enabled,
            })
            .collect();
        let here = lane.module(slot);
        let macros = MACROS
            .iter()
            .map(|def| KeysMacro {
                id: def.id.to_string(),
                name: def.name.to_string(),
                group: def.group.to_string(),
                value: here
                    .and_then(|m| m.macros.get(def.id).copied())
                    .unwrap_or(def.default),
                min: def.min,
                max: def.max,
                unit: def.unit.to_string(),
                live: def.live && here.is_some_and(|m| !m.patch.is_empty()),
            })
            .collect();
        // The selected MODULE's slice of the live program.
        let module_name = format!("{layer} {}", signal_synth::engine::module_slot(slot));
        let tree = s
            .tree
            .as_ref()
            .and_then(|t| t.find(&module_name).map(|c| node_of(c, "")))
            .unwrap_or_default();
        KeysLayerDetail {
            layer: layer.clone(),
            engine: lane.engine.clone(),
            modules,
            module: slot as u32,
            patch: here.map(|m| m.display()).unwrap_or_default(),
            source: here.map(|m| m.patch.clone()).unwrap_or_default(),
            gain_db: lane.gain_db,
            muted: lane.muted,
            key_lo,
            key_hi,
            macros,
            tree,
        }
    }

    fn set_layer_macro(&self, layer: String, module: u32, id: String, value: f32) {
        let Some(def) = macro_def(&id) else { return };
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            let Some(m) = lane.modules.get_mut(module as usize) else { return };
            m.macros.insert(id, value.clamp(def.min, def.max));
        }
        // `source.level` rides the lane fader, which is a live cell.
        if def.id == "source.level" {
            self.apply_mixer();
        }
        self.publish_mixer();
    }

    fn clear_layer(&self, layer: String, module: u32) {
        {
            let Ok(mut s) = self.inner.state.lock() else { return };
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            let Some(m) = lane.modules.get_mut(module as usize) else { return };
            if m.patch.is_empty() && m.preset.is_empty() {
                return;
            }
            m.patch.clear();
            m.preset.clear();
        }
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-layer-clear".into())
            .spawn(move || {
                let _rt = keys_runtime().enter();
                b.rebuild_program();
            });
    }

    fn set_module_gain(&self, layer: String, module: u32, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            let Some(m) = lane.modules.get_mut(module as usize) else { return };
            m.gain_db = db.clamp(MIN_FADER_DB, MAX_FADER_DB);
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_module_enabled(&self, layer: String, module: u32, on: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else { return };
            let Some(m) = lane.modules.get_mut(module as usize) else { return };
            m.enabled = on;
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    // ── Performance ──────────────────────────────────────────────────────

    fn perform(&self) -> KeysPerform {
        self.perform_model()
    }

    fn press_stack(&self, index: u32) {
        let needs_rebuild = {
            let Ok(mut s) = self.inner.state.lock() else { return };
            let Some(stack) = s.profile.stack(index as usize).cloned() else { return };
            let mut rebuild = false;
            for slot in &stack.slots {
                let Some(lane) = s.lanes.get_mut(&slot.layer) else { continue };
                lane.muted = slot.muted;
                lane.gain_db = slot.gain_db;
                // An empty scene patch keeps whatever the lane holds — the
                // scene rides levels, it doesn't force a reload.
                if !slot.patch.is_empty()
                    && lane.modules.first().is_some_and(|m| m.patch != slot.patch)
                {
                    if let Some(m) = lane.modules.first_mut() {
                        m.patch = slot.patch.clone();
                        m.preset.clear();
                    }
                    rebuild = true;
                }
            }
            s.active_stack = Some(index as usize);
            rebuild
        };
        if needs_rebuild {
            let b = self.clone();
            let _ = std::thread::Builder::new()
                .name("keys-stack".into())
                .spawn(move || {
                    let _rt = keys_runtime().enter();
                    b.rebuild_program();
                });
        } else {
            // Level-only recall: instant, no audio gap.
            self.apply_mixer();
            self.publish_mixer();
            self.publish_perform();
        }
    }

    fn set_perform_mode(&self, mode: u32) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.perform_mode = mode;
        }
        self.publish_perform();
    }

    fn capture_stack(&self, index: u32) {
        if let Ok(mut s) = self.inner.state.lock() {
            let lanes = s.lanes.clone();
            let Some(stack) = s.profile.stacks.get_mut(index as usize) else { return };
            stack.slots = lanes
                .iter()
                .map(|(name, lane)| crate::profile::SceneSlot {
                    layer: name.clone(),
                    patch: lane.primary_patch(),
                    gain_db: lane.gain_db,
                    muted: lane.muted,
                })
                .collect();
        }
        self.publish_perform();
    }
}

impl KeysRigStreamSource for KeysRigBackend {
    fn events_hub(&self) -> &PubSub<KeysEvent> {
        &self.inner.events
    }
}

impl Services for KeysRigBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_keys_proto::keys::Service,
            signal_keys_proto::keys::StreamService
        ]
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn note_ev(note: u8, velocity: u8) -> MidiEvent {
    use midicore::{Channel, KeyNumber, Velocity};
    MidiEvent::NoteOn { channel: Channel::new(0), key: KeyNumber::new(note), velocity: Velocity::new(velocity) }
}

/// Fader travel — matches a console strip (−∞…+6 dB, clamped at −60).
const MIN_FADER_DB: f32 = -60.0;
const MAX_FADER_DB: f32 = 6.0;

/// Load the active profile: `FTS_KEYS_PROFILE` (a `.styx` file) if set and
/// parseable, else the built-in Worship profile.
fn load_profile() -> KeysProfile {
    let Ok(path) = std::env::var("FTS_KEYS_PROFILE") else {
        return worship_profile();
    };
    match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| KeysProfile::from_styx_str(&t)) {
        Ok(p) => {
            tracing::info!(path, profile = %p.name, "keys rig: loaded profile");
            p
        }
        Err(e) => {
            tracing::error!(path, "keys rig: profile load failed ({e}); using Worship");
            worship_profile()
        }
    }
}

/// Build a single-instrument keys program: Preset → Keys engine → Layer A →
/// Piano Source (Sampler block realized by `spec`).
fn keys_program(name: &str, spec: String) -> Container {
    Container::preset(name).add(
        Container::engine("Keys").add(
            Container::layer("Layer A").add(
                Container::module("Piano Source").sample_block("Piano", spec),
            ),
        ),
    )
}

/// Discover Keyscape instruments to load. Prefers the `.signalpack` library
/// (self-contained packs — faster load, the intended distribution format) and
/// falls back to the raw `library.styx` extraction if no packs are found.
/// The stored spec path (`.signalpack` or `library.styx`) is handed to the
/// sample block; `rig.rs` picks the loader by extension.
fn scan_keyscape() -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let packs_root =
        std::env::var("FTS_KEYSCAPE_PACKS").unwrap_or_else(|_| KEYSCAPE_PACKS_ROOT.into());
    let (mut packs, mut pack_specs) = scan_packs(&packs_root);
    // One engine, one library: the Omnisphere soundsources are loadable into
    // any lane exactly like a Keyscape pack (they're both just sources for
    // the Signal Engine's Soundsource block).
    let omni_root =
        std::env::var("FTS_OMNISPHERE_PACKS").unwrap_or_else(|_| OMNISPHERE_PACKS_ROOT.into());
    let (omni, omni_specs) = scan_packs_recursive(&omni_root);
    packs.extend(omni);
    pack_specs.extend(omni_specs);
    // Authored Omnisphere patches — these open into a whole layer.
    let patch_root = std::env::var("FTS_OMNISPHERE_PATCHES")
        .unwrap_or_else(|_| OMNISPHERE_PATCH_ROOT.into());
    let (patches, patch_specs) = scan_omni_patches(&patch_root);
    tracing::info!(patches = patches.len(), "keys rig: omnisphere patches");
    packs.extend(patches);
    pack_specs.extend(patch_specs);
    if !packs.is_empty() {
        return (packs, pack_specs);
    }
    let root = std::env::var("FTS_KEYSCAPE_ROOT").unwrap_or_else(|_| KEYSCAPE_ROOT.into());
    let mut presets = Vec::new();
    let mut specs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            let styx = dir.join("library.styx");
            if styx.exists() {
                let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if name.is_empty() {
                    continue;
                }
                presets.push(KeysPreset { kind: kind_of(&name), name, loaded: false });
                specs.push(styx);
            }
        }
    }
    (presets, specs)
}

/// Root of the Omnisphere patch library (`.prt_omn` presets — the authored
/// patches, as opposed to raw soundsources). Override with
/// `FTS_OMNISPHERE_PATCHES`.
const OMNISPHERE_PATCH_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere/Settings Library/Patches";

/// Enumerate `.prt_omn` patches under `root` — the **module presets**: an
/// authored voice (source + filter + envelopes + unison) that loads onto a
/// module, spilling onto the next ones when the patch has several layers.
fn scan_omni_patches(root: &str) -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let mut presets = Vec::new();
    let mut specs = Vec::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut found: Vec<PathBuf> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("prt_omn")) {
                found.push(p);
            }
        }
        found.sort();
        for patch in found {
            let Some(name) = patch.file_stem().and_then(|s| s.to_str()) else { continue };
            // Factory names carry a library prefix ("KEY │ American Obesity").
            let display = name.rsplit('│').next().unwrap_or(name).trim().to_string();
            if display.is_empty() {
                continue;
            }
            presets.push(KeysPreset { kind: "Module".into(), name: display, loaded: false });
            specs.push(patch);
        }
    }
    (presets, specs)
}

/// Root of the built Omnisphere soundsource packs — the synth half of the
/// shared library. Override with `FTS_OMNISPHERE_PACKS`.
const OMNISPHERE_PACKS_ROOT: &str =
    "/run/media/AudioHaven/Signal/Libraries/Keys/Omnisphere/Packs";

/// Enumerate `*.signalpack` files under `root`, at any depth (the Omnisphere
/// library nests by family). The file stem is the source name.
fn scan_packs_recursive(root: &str) -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let mut presets = Vec::new();
    let mut specs = Vec::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut found: Vec<PathBuf> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("signalpack")) {
                found.push(p);
            }
        }
        found.sort();
        for pack in found {
            let Some(name) = pack.file_stem().and_then(|s| s.to_str()) else { continue };
            if name.is_empty() {
                continue;
            }
            presets.push(KeysPreset {
                kind: "Synth".into(),
                name: name.to_string(),
                loaded: false,
            });
            specs.push(pack);
        }
    }
    (presets, specs)
}

/// Enumerate `*.signalpack` files in the packs root; the file stem is the
/// instrument name.
fn scan_packs(root: &str) -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let mut presets = Vec::new();
    let mut specs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut packs: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("signalpack")))
            .collect();
        packs.sort();
        for pack in packs {
            let name = pack.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            presets.push(KeysPreset { kind: kind_of(&name), name, loaded: false });
            specs.push(pack);
        }
    }
    (presets, specs)
}

/// Broad category for grouping in the browser.
fn kind_of(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    if n.contains("grand") || (n.contains("piano") && !n.contains("e piano")) { "Grand".into() }
    else if n.contains("rhodes") { "Rhodes".into() }
    else if n.contains("wurl") { "Wurlitzer".into() }
    else if n.contains("clav") { "Clav".into() }
    else if n.contains("mks") || n.contains("mk-80") || n.contains("e piano") || n.contains("electric") { "Electric".into() }
    else if n.contains("toy") || n.contains("celeste") || n.contains("glock") || n.contains("bell") { "Toy/Bell".into() }
    else { "Other".into() }
}

fn slug(s: &str) -> String {
    s.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Convert a composition [`Container`] into a wire [`KeysNode`] tree.
fn node_of(c: &Container, parent: &str) -> KeysNode {
    let id = if parent.is_empty() { slug(&c.name) } else { format!("{parent}/{}", slug(&c.name)) };
    let mut children = Vec::new();
    let mut any_live = false;
    for n in &c.children {
        let child = match n {
            RigNode::Container { container } => node_of(container, &id),
            RigNode::Block { block } => KeysNode {
                id: format!("{id}/{}", slug(&block.name)),
                label: block.name.clone(),
                role: "block".into(),
                live: block.has_backend(),
                children: Vec::new(),
            },
        };
        any_live |= child.live;
        children.push(child);
    }
    KeysNode { id, label: c.name.clone(), role: role_tag(c.role), live: any_live, children }
}

fn role_tag(role: Role) -> String {
    match role {
        Role::Preset => "preset",
        Role::Engine => "engine",
        Role::Layer => "layer",
        Role::Module => "module",
    }
    .to_string()
}
